//! Explicit physical scratch descriptors shared by admission, native allocation
//! and the packed plan. Semantic slots remain local names while constructing
//! effects; they are never physical allocation identities.
use super::{scratch_graph as graph, scratch_lifetimes as placement, EffectFramePlan};
use crate::RenderError;
use serde::Serialize;
use std::ops::Range;

pub const MAX_SCRATCH_PASSES: usize = motion_model::MAX_LAYERS
    * (motion_effects::MAX_EFFECTS_PER_LAYER * (motion_effects::MAX_PASSES + 3) + 1);
pub const SCRATCH_RECORD_BYTES: usize = 32;
/// Portable usage bits: attachment, sampled, copy source, copy destination.
pub const SCRATCH_USAGE: u32 = 15;

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ScratchFormat {
    Rgba8UnormSrgb = 0,
    Rgba8Unorm = 1,
    Rgba16Float = 2,
    Rgba16Unorm = 3,
    Rgba32Float = 4,
}
impl ScratchFormat {
    pub fn storage(self) -> motion_effects::image_contract::ImageStorage {
        use motion_effects::image_contract::ImageStorage;
        match self {
            Self::Rgba8UnormSrgb => ImageStorage::Rgba8UnormSrgb,
            Self::Rgba8Unorm => ImageStorage::Rgba8Unorm,
            Self::Rgba16Float => ImageStorage::Rgba16Float,
            Self::Rgba16Unorm => ImageStorage::Rgba16Unorm,
            Self::Rgba32Float => ImageStorage::Rgba32Float,
        }
    }
    pub fn native(self) -> wgpu::TextureFormat {
        crate::image_formats::native_format(self.storage())
    }
    pub fn bytes_per_pixel(self) -> u32 {
        self.storage().bytes_per_pixel() as u32
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ScratchClass {
    pub format: ScratchFormat,
    /// Bit 0 = linear, bit 1 = sRGB numeric values used in separate lifetimes.
    pub working_spaces: u32,
    /// Bit 0 = premultiplied, bit 1 = straight, separated by full clears.
    pub alpha_modes: u32,
    pub usage: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LogicalClass {
    format: ScratchFormat,
    working_space: u32,
    alpha: u32,
    usage: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StorageClass {
    format: ScratchFormat,
    usage: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ScratchAllocation {
    pub width: u32,
    pub height: u32,
    pub class: ScratchClass,
}
impl ScratchAllocation {
    pub fn bytes(self) -> u64 {
        u64::from(self.width)
            * u64::from(self.height)
            * u64::from(self.class.format.bytes_per_pixel())
    }
    pub fn words(self) -> [u32; 8] {
        [
            self.width,
            self.height,
            self.class.format as u32,
            self.class.working_spaces,
            self.class.alpha_modes,
            self.class.usage,
            self.class.format.bytes_per_pixel(),
            0,
        ]
    }
    pub fn same_storage(self, other: Self) -> bool {
        self.width == other.width
            && self.height == other.height
            && self.class.format == other.class.format
            && self.class.usage == other.class.usage
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScratchBinding {
    pub input: i32,
    pub source: i32,
    pub output: u32,
}

#[derive(Default, Debug)]
pub(super) struct Layout {
    pub allocations: Vec<ScratchAllocation>,
    pub bindings: Vec<ScratchBinding>,
    pub outputs: Vec<Option<u32>>,
    pub bytes: u64,
}

/// Derive execution lifetimes for atomic draw/replay units. Each unit includes
/// its terminal snapshot copy or composition consumer. Third image-input
/// bindings refer to independently owned images, never scratch from that draw.
pub(super) fn layout(
    frame: &EffectFramePlan,
    current: Option<Range<usize>>,
) -> Result<Layout, RenderError> {
    if frame.passes.len() > MAX_SCRATCH_PASSES {
        return Err(format!("effect scratch pass limit exceeded ({MAX_SCRATCH_PASSES})").into());
    }
    let mut passes = Vec::with_capacity(frame.passes.len());
    for (index, p) in frame.passes.iter().enumerate() {
        let default_format = match p.output {
            0 | 4..=6 => ScratchFormat::Rgba8UnormSrgb,
            1..=3 => ScratchFormat::Rgba8Unorm,
            7 => ScratchFormat::Rgba16Float,
            other => {
                return Err(
                    format!("effect pass {index} semantic output {other} is invalid").into(),
                )
            }
        };
        let format = match frame.precision {
            motion_effects::image_contract::ChannelPrecision::Unorm8 => default_format,
            motion_effects::image_contract::ChannelPrecision::Unorm16 => ScratchFormat::Rgba16Unorm,
            motion_effects::image_contract::ChannelPrecision::Float16 => ScratchFormat::Rgba16Float,
            motion_effects::image_contract::ChannelPrecision::Float32 => ScratchFormat::Rgba32Float,
        };
        let semantic = |value: f32, name: &str| -> Result<u32, RenderError> {
            if value == 0. {
                Ok(0)
            } else if value == 1. {
                Ok(1)
            } else {
                Err(format!("effect pass {index} {name} semantic {value} is invalid").into())
            }
        };
        // Host raster/masked-raster utilities return linear premultiplied
        // pixels directly. Their inherited output_mode.xy is not evaluated by
        // the utility shader; it describes the following plugin conversion.
        let (working_space, alpha) = if matches!(p.program, 0 | 2) {
            (0, 0)
        } else {
            (
                semantic(p.uniform.output_mode[0], "working-space")?,
                semantic(p.uniform.output_mode[1], "alpha")?,
            )
        };
        passes.push(graph::Pass {
            input: p.input,
            source: p.source,
            output: p.output as usize,
            descriptor: placement::Descriptor {
                class: LogicalClass {
                    format,
                    working_space,
                    alpha,
                    usage: SCRATCH_USAGE,
                },
                width: p.width,
                height: p.height,
                bytes_per_pixel: format.bytes_per_pixel(),
            },
            // Native effect passes clear the entire attachment, including
            // sprite passes. Blending inside one sprite pass is not a later load.
            load: graph::Load::Clear,
        });
    }
    let mut draws: Vec<_> = frame
        .draws
        .iter()
        .map(|d| d.pass_start..d.pass_end)
        .collect();
    if let Some(current) = current {
        draws.push(current);
    }
    let graph = graph::build(&passes, &draws, 8, MAX_SCRATCH_PASSES)
        .map_err(|error| RenderError::Invalid(error.to_string()))?;
    // Logical colors/alpha are immutable per value, including attachment loads.
    // All production writes clear the whole attachment. Once the last consumer
    // ends, that clear ends the old semantic lifetime, so identical storage can
    // hold a new value without conversion or reinterpretation of retained data.
    let resources: Vec<_> = graph
        .resources
        .iter()
        .map(|r| placement::Resource {
            descriptor: placement::Descriptor {
                class: StorageClass {
                    format: r.descriptor.class.format,
                    usage: r.descriptor.class.usage,
                },
                width: r.descriptor.width,
                height: r.descriptor.height,
                bytes_per_pixel: r.descriptor.bytes_per_pixel,
            },
            first_use: r.first_use,
            last_use: r.last_use,
        })
        .collect();
    let placement = placement::place(
        &resources,
        placement::Limits {
            resources: MAX_SCRATCH_PASSES,
            allocations: MAX_SCRATCH_PASSES,
            bytes: u64::MAX,
        },
    )
    .map_err(|error| RenderError::Invalid(error.to_string()))?;
    let input = |value| match value {
        graph::Input::External(token) => token,
        graph::Input::Resource(id) => placement.assignments[id] as i32,
    };
    let mut allocations: Vec<_> = placement
        .allocations
        .iter()
        .map(|d| ScratchAllocation {
            width: d.width,
            height: d.height,
            class: ScratchClass {
                format: d.class.format,
                usage: d.class.usage,
                working_spaces: 0,
                alpha_modes: 0,
            },
        })
        .collect();
    for (index, resource) in graph.resources.iter().enumerate() {
        let class = &mut allocations[placement.assignments[index]].class;
        class.working_spaces |= 1 << resource.descriptor.class.working_space;
        class.alpha_modes |= 1 << resource.descriptor.class.alpha;
    }
    Ok(Layout {
        allocations,
        bindings: graph
            .passes
            .iter()
            .map(|p| ScratchBinding {
                input: input(p.input),
                source: input(p.source),
                output: placement.assignments[p.output] as u32,
            })
            .collect(),
        outputs: graph
            .draw_outputs
            .iter()
            .map(|id| id.map(|id| placement.assignments[id] as u32))
            .collect(),
        bytes: placement.bytes,
    })
}
