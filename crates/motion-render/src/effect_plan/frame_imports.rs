//! Current-sample numeric images. These source values describe real native
//! uploads/raster outputs, without charging a fictitious shader draw to imports.
use super::*;
use motion_effects::{
    execution_plan::*,
    image_contract::{coverage_storage, DataChannels, ImageContract, ImageMeaning, ImageStorage},
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum FrameImageSource {
    /// Native sidecar token, one-based; zero means package defaults.
    InstanceData(u32),
    Curve(u32),
    /// Draw index used by SOURCE_TOKEN - draw, not an asset index.
    Mask(usize),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub enum ImportedImage {
    Package(PackageImageImport),
    Frame(FrameImageImport),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct FrameImageImport {
    source: FrameImageSource,
    value_type: ImageValueType,
    storage: ImageStorage,
    encoding: ResourceEncoding,
}
impl FrameImageImport {
    pub fn source(&self) -> FrameImageSource {
        self.source
    }
    pub fn value_type(&self) -> ImageValueType {
        self.value_type
    }
    pub fn storage(&self) -> ImageStorage {
        self.storage
    }
    pub fn encoding(&self) -> ResourceEncoding {
        self.encoding
    }

    pub(crate) fn validate_native(&self, texture: &wgpu::Texture) -> Result<(), RenderError> {
        let actual = [texture.width(), texture.height()];
        if actual != self.value_type.rect.size
            || texture.format() != crate::image_formats::native_format(self.storage)
        {
            return Err(format!(
                "phase=frame_import source={:?}: actual={actual:?}/{:?} expected={:?}/{:?}",
                self.source,
                texture.format(),
                self.value_type.rect.size,
                self.storage
            )
            .into());
        }
        Ok(())
    }

    pub(crate) fn instance(
        token: u32,
        values: &motion_effects::shader_data::ShaderDataValues,
        declaration: &PackageImageImport,
    ) -> Result<Self, RenderError> {
        let ResourceEncoding::LittleEndianF32 { scalars } = declaration.encoding() else {
            return Err(
                "phase=frame_import: instance data requires a declared f32 byte container".into(),
            );
        };
        if token == 0
            || values.texture.len() != scalars as usize
            || values.texture_extent() != declaration.value_type().rect.size
        {
            return Err(format!("phase=frame_import instance={token}: private texture layout differs from its declaration").into());
        }
        Ok(Self {
            source: FrameImageSource::InstanceData(token),
            value_type: declaration.value_type(),
            storage: declaration.storage(),
            encoding: declaration.encoding(),
        })
    }

    pub(crate) fn curve(index: u32, precision: ChannelPrecision) -> Self {
        let (precision, width, storage) = if precision == ChannelPrecision::Unorm8 {
            (ChannelPrecision::Unorm8, 256, ImageStorage::Rgba8Unorm)
        } else {
            (
                ChannelPrecision::Float32,
                motion_model::HIGH_PRECISION_CURVE_SAMPLES as u32,
                ImageStorage::Rgba32Float,
            )
        };
        Self {
            source: FrameImageSource::Curve(index),
            storage,
            encoding: ResourceEncoding::NumericChannels,
            value_type: ImageValueType {
                image: ImageContract::new(
                    ImageMeaning::Data {
                        channels: DataChannels::Vector4,
                    },
                    precision,
                ),
                rect: PixelRect {
                    origin: [0; 2],
                    size: [width, 1],
                },
                coordinates: CoordinateDomain::Resource,
                depth: None,
            },
        }
    }

    pub(crate) fn mask(
        draw: usize,
        layer: u64,
        size: [u32; 2],
        precision: ChannelPrecision,
    ) -> Self {
        Self {
            source: FrameImageSource::Mask(draw),
            storage: coverage_storage(precision),
            encoding: ResourceEncoding::NumericChannels,
            value_type: ImageValueType {
                image: ImageContract::new(ImageMeaning::Coverage {}, precision),
                rect: PixelRect {
                    origin: [0; 2],
                    size,
                },
                coordinates: CoordinateDomain::Layer { layer },
                depth: None,
            },
        }
    }
}

fn append_source(
    nodes: &mut Vec<ExecutionNode>,
    bindings: &mut Vec<EffectOperationBinding>,
    import: FrameImageImport,
    draw: usize,
    scene: &Scene,
) -> Result<ImageValueId, RenderError> {
    if nodes.len() >= MAX_EXECUTION_NODES {
        return Err(ExecutionPlanError::Budget {
            node: None,
            resource: "nodes",
            required: nodes.len() as u64 + 1,
            maximum: MAX_EXECUTION_NODES as u64,
        }
        .into());
    }
    let node = ExecutionNodeId(nodes.len() as u32);
    let value = ImageValueId(node.0);
    let size = import.value_type.rect.size;
    nodes.push(ExecutionNode {
        id: node,
        output: value,
        output_type: import.value_type,
        origin: OperationOrigin {
            composition: scene.composition_id.clone(),
            layer: scene.layers[draw].id,
            effect: None,
        },
        inputs: vec![],
        semantics: OperationSemantics {
            domain: ExecutionDomain::Source,
            time: TimeDependency::CurrentSample,
            halo: Some([0; 4]),
            stateful: false,
            side_effects: false,
        },
        cost: OperationCost {
            shader_bytes: 0,
            static_loop_work: 0,
            samples_per_fragment: Some(0),
            passes: 0,
        },
        code_identity: [0; 32],
        uniform_cells: 0,
    });
    bindings.push(EffectOperationBinding {
        node,
        draw,
        scene_effect: None,
        original_passes: 0..0,
        output_region: [0., 0., size[0] as f32, size[1] as f32],
        resource_tokens: vec![],
        lookup_tables: vec![],
        instance_data: vec![],
        imported_image: Some(ImportedImage::Frame(import)),
    });
    Ok(value)
}

pub(super) fn lower_frame_images(
    nodes: &mut Vec<ExecutionNode>,
    bindings: &mut Vec<EffectOperationBinding>,
    frame: &EffectFramePlan,
    programs: &[EffectProgram],
    scene: &Scene,
) -> Result<(), RenderError> {
    let operations = bindings.len();
    let mut sources = BTreeMap::new();
    // Preserve native preparation of every scene LUT, including unreferenced
    // tables. An unreferenced source is not counted as a shader pass or sample.
    if frame.precision != ChannelPrecision::Unorm8
        && scene.curve_sources.len() != scene.curve_luts.len()
    {
        return Err(format!(
            "effect high-precision curve sources={} do not match LUTs={}",
            scene.curve_sources.len(),
            scene.curve_luts.len()
        )
        .into());
    }
    if let Some(first) = bindings.first() {
        let draw = first.draw;
        for index in 0..scene.curve_luts.len() {
            let index = u32::try_from(index)
                .map_err(|_| RenderError::Invalid("curve import index exceeds u32".into()))?;
            let import = FrameImageImport::curve(index, frame.precision);
            let value = append_source(nodes, bindings, import.clone(), draw, scene)?;
            sources.insert(import.source, (value, import));
        }
    }
    for group in frame.masks.chunk_by(|a, b| a.layer == b.layer) {
        let first = &group[0];
        let layer = scene
            .layers
            .get(first.layer)
            .ok_or("mask import draw is absent")?;
        let import = FrameImageImport::mask(
            first.layer,
            layer.id,
            [first.width, first.height],
            frame.precision,
        );
        let value = append_source(nodes, bindings, import.clone(), first.layer, scene)?;
        sources.insert(import.source, (value, import));
    }
    for operation in 0..operations {
        for pass in &frame.passes[bindings[operation].original_passes.clone()] {
            let mut edges = Vec::with_capacity(4);
            if pass.instance_data != 0 {
                if pass.resource_input != 0 || pass.sprite {
                    return Err(
                        "instance shader data cannot override image-input or sprite resources"
                            .into(),
                    );
                }
                let program = programs
                    .get(pass.program as usize)
                    .ok_or("instance import program is absent")?;
                let declaration = program
                    .imports
                    .first()
                    .ok_or("instance import resource zero is absent")?;
                let values = frame
                    .instance_data
                    .get((pass.instance_data - 1) as usize)
                    .ok_or("instance shader data index is invalid")?;
                let import = FrameImageImport::instance(pass.instance_data, values, declaration)?;
                if let Some((_, old)) = sources.get(&import.source) {
                    if old != &import {
                        return Err(
                            "phase=frame_import: shared instance data declarations differ".into(),
                        );
                    }
                } else {
                    let draw = bindings[operation].draw;
                    let value = append_source(nodes, bindings, import.clone(), draw, scene)?;
                    sources.insert(import.source, (value, import));
                }
                edges.push((0, FrameImageSource::InstanceData(pass.instance_data)));
            }
            if pass.lut >= 0 {
                edges.push((4, FrameImageSource::Curve(pass.lut as u32)));
            }
            for (slot, token) in [(5, pass.input), (6, pass.source)] {
                if token <= crate::mask_plan::SOURCE_TOKEN {
                    let draw =
                        (i64::from(crate::mask_plan::SOURCE_TOKEN) - i64::from(token)) as usize;
                    edges.push((slot, FrameImageSource::Mask(draw)));
                }
            }
            for (index, source) in edges {
                let (value, import) = sources.get(&source).ok_or_else(|| {
                    RenderError::Invalid(format!("phase=frame_import: source {source:?} is absent"))
                })?;
                let role = ImagePortRole::Resource { index };
                if !nodes[operation]
                    .inputs
                    .iter()
                    .any(|p| p.role == role && p.value == *value)
                {
                    nodes[operation].inputs.push(ImageInputPort {
                        role,
                        value: *value,
                        expected: import.value_type,
                    });
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
