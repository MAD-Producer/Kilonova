//! Explicit bounded color lookup transforms. A LUT is not an OCIO configuration.
//! Inputs clamp to the declared domain; outputs retain negative/HDR values.
use crate::AlphaMode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;

mod cube;

pub const MAX_CUBE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_LUT_1D_SIZE: u32 = 65_536;
pub const MAX_LUT_3D_SIZE: u32 = 65;
/// Margin keeps float32 interpolation differences and sums finite on the GPU.
pub const MAX_LUT_VALUE: f32 = f32::MAX / 16.;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LutKind {
    OneD,
    ThreeD,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LutInterpolation {
    Nearest,
    /// Linear for 1D; trilinear for 3D.
    Linear,
    /// Valid only for 3D.
    Tetrahedral,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LutPurpose {
    Input,
    Working,
    Display,
    Output,
    Look,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LutDescriptor {
    pub kind: LutKind,
    pub size: u32,
    pub interpolation: LutInterpolation,
    pub purpose: LutPurpose,
    pub domain_min: [f32; 3],
    pub domain_max: [f32; 3],
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LutError {
    #[error("color LUT invalid {field}: {reason}")]
    Descriptor {
        field: &'static str,
        reason: &'static str,
    },
    #[error("color LUT capacity requested={requested} limit={limit}")]
    Capacity { requested: usize, limit: usize },
    #[error("color LUT entries expected={expected} actual={actual}")]
    EntryCount { expected: usize, actual: usize },
    #[error("color LUT entry outside finite arithmetic range entry={entry} channel={channel}")]
    Entry { entry: usize, channel: usize },
    #[error("color LUT nonfinite pixel={pixel} channel={channel}")]
    Pixel { pixel: usize, channel: usize },
    #[error("color LUT alpha outside [0,1] pixel={pixel}")]
    Alpha { pixel: usize },
    #[error("color LUT Cube line={line}: {reason}")]
    Cube { line: usize, reason: &'static str },
}

impl LutDescriptor {
    pub fn entry_count(self) -> Result<usize, LutError> {
        let limit = match self.kind {
            LutKind::OneD => MAX_LUT_1D_SIZE,
            LutKind::ThreeD => MAX_LUT_3D_SIZE,
        };
        if !(2..=limit).contains(&self.size) {
            return Err(LutError::Descriptor {
                field: "size",
                reason: "outside supported range",
            });
        }
        if self.kind == LutKind::OneD && self.interpolation == LutInterpolation::Tetrahedral {
            return Err(LutError::Descriptor {
                field: "interpolation",
                reason: "tetrahedral requires 3D",
            });
        }
        for channel in 0..3 {
            let min = self.domain_min[channel];
            let max = self.domain_max[channel];
            let span = max - min;
            if !min.is_finite() || !max.is_finite() || !span.is_finite() || span < f32::MIN_POSITIVE
            {
                return Err(LutError::Descriptor {
                    field: "domain",
                    reason: "requires finite increasing normal-width intervals",
                });
            }
        }
        let size = self.size as usize;
        Ok(match self.kind {
            LutKind::OneD => size,
            LutKind::ThreeD => size * size * size,
        })
    }
}

#[derive(Clone, Debug)]
pub struct ColorLut {
    descriptor: LutDescriptor,
    /// Cube ordering: red varies fastest, then green, then blue.
    values: Arc<[[f32; 3]]>,
    key: [u8; 32],
}

impl ColorLut {
    pub fn new(descriptor: LutDescriptor, values: Vec<[f32; 3]>) -> Result<Self, LutError> {
        let count = descriptor.entry_count()?;
        if values.len() != count {
            return Err(LutError::EntryCount {
                expected: count,
                actual: values.len(),
            });
        }
        for (entry, value) in values.iter().enumerate() {
            for (channel, sample) in value.iter().enumerate() {
                if !sample.is_finite() || sample.abs() > MAX_LUT_VALUE {
                    return Err(LutError::Entry { entry, channel });
                }
            }
        }
        let mut hash = Sha256::new();
        hash.update(b"motion-color-lut-v1\0");
        hash.update([
            descriptor.kind as u8,
            descriptor.interpolation as u8,
            descriptor.purpose as u8,
        ]);
        hash.update(descriptor.size.to_le_bytes());
        for value in descriptor
            .domain_min
            .into_iter()
            .chain(descriptor.domain_max)
            .chain(values.iter().flatten().copied())
        {
            hash.update(value.to_bits().to_le_bytes());
        }
        Ok(Self {
            descriptor,
            values: values.into(),
            key: hash.finalize().into(),
        })
    }
    pub fn descriptor(&self) -> LutDescriptor {
        self.descriptor
    }
    pub fn values(&self) -> &[[f32; 3]] {
        &self.values
    }
    pub fn key(&self) -> [u8; 32] {
        self.key
    }

    /// All inputs are validated before writing any pixel. Premultiplied RGB is
    /// unassociated before mapping and reassociated afterward. Zero alpha has
    /// zero RGB; straight-alpha input retains its authored RGB at zero alpha.
    pub fn apply_rgba(&self, pixels: &mut [[f32; 4]], alpha: AlphaMode) -> Result<(), LutError> {
        for (pixel, value) in pixels.iter().enumerate() {
            for (channel, sample) in value.iter().enumerate() {
                if !sample.is_finite() {
                    return Err(LutError::Pixel { pixel, channel });
                }
            }
            if !(0. ..=1.).contains(&value[3]) {
                return Err(LutError::Alpha { pixel });
            }
        }
        for pixel in pixels {
            let a = pixel[3];
            let rgb = if alpha == AlphaMode::Premultiplied {
                if a == 0. {
                    [0.; 3]
                } else {
                    [pixel[0] / a, pixel[1] / a, pixel[2] / a]
                }
            } else {
                [pixel[0], pixel[1], pixel[2]]
            };
            let mapped = self.sample(rgb);
            for channel in 0..3 {
                pixel[channel] = if alpha == AlphaMode::Premultiplied {
                    mapped[channel] * a
                } else {
                    mapped[channel]
                };
            }
        }
        Ok(())
    }

    fn sample(&self, rgb: [f32; 3]) -> [f32; 3] {
        let d = self.descriptor;
        let max = d.size - 1;
        let coordinate: [f32; 3] = std::array::from_fn(|i| {
            (rgb[i].clamp(d.domain_min[i], d.domain_max[i]) - d.domain_min[i])
                / (d.domain_max[i] - d.domain_min[i])
                * max as f32
        });
        let low = coordinate.map(|v| v.floor() as usize);
        let high = low.map(|v| (v + 1).min(max as usize));
        let fraction: [f32; 3] = std::array::from_fn(|i| coordinate[i] - low[i] as f32);
        if d.kind == LutKind::OneD {
            return std::array::from_fn(|i| {
                if d.interpolation == LutInterpolation::Nearest {
                    self.values[(coordinate[i] + 0.5).floor() as usize][i]
                } else {
                    let a = self.values[low[i]][i];
                    a + fraction[i] * (self.values[high[i]][i] - a)
                }
            });
        }
        let fetch =
            |p: [usize; 3]| self.values[p[0] + d.size as usize * (p[1] + d.size as usize * p[2])];
        if d.interpolation == LutInterpolation::Nearest {
            return fetch(coordinate.map(|v| (v + 0.5).floor() as usize));
        }
        if d.interpolation == LutInterpolation::Tetrahedral {
            let mut axes = [0, 1, 2];
            axes.sort_by(|&a, &b| fraction[b].total_cmp(&fraction[a]).then(a.cmp(&b)));
            let mut p = low;
            let mut previous = fetch(p);
            let mut output = previous;
            for axis in axes {
                p[axis] = high[axis];
                let next = fetch(p);
                for i in 0..3 {
                    output[i] += fraction[axis] * (next[i] - previous[i]);
                }
                previous = next;
            }
            output
        } else {
            let mut output = [0.; 3];
            for corner in 0..8 {
                let mut weight = 1.;
                let p = std::array::from_fn(|i| {
                    if corner & (1 << i) == 0 {
                        weight *= 1. - fraction[i];
                        low[i]
                    } else {
                        weight *= fraction[i];
                        high[i]
                    }
                });
                let value = fetch(p);
                for i in 0..3 {
                    output[i] += value[i] * weight;
                }
            }
            output
        }
    }
}
