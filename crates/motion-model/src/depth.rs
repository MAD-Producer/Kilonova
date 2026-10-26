//! Owned scalar depth, independent of inference engines and GPU APIs.
//!
//! Relative predictions carry ordering, never an implicit world-space calibration.
//! A renderer must explicitly convert metric depth into its camera projection.

use crate::{ensure, Error, Result};
use serde::{Deserialize, Serialize};

pub const MAX_DEPTH_DIMENSION: u32 = 4096;
pub const MAX_DEPTH_PIXELS: usize = 4096 * 4096;
pub const MAX_DEPTH_RESOURCE_BYTES: usize = 192 << 20;

/// Exact source presentation time, including negative timestamps before an edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepthTime {
    pub ticks: i64,
    pub timescale: u32,
}

impl DepthTime {
    pub fn validate(self) -> Result<()> {
        ensure(self.timescale > 0, "depth source time has a zero timescale")
    }

    pub fn same_instant(self, other: Self) -> bool {
        self.timescale != 0
            && other.timescale != 0
            && i128::from(self.ticks) * i128::from(other.timescale)
                == i128::from(other.ticks) * i128::from(self.timescale)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepthSource {
    pub asset: u64,
    /// Changes when the source bytes or interpretation change, even at the same PTS.
    pub revision: u64,
    pub pts: DepthTime,
}

impl DepthSource {
    pub fn same_sample(&self, other: &Self) -> bool {
        self.asset == other.asset
            && self.revision == other.revision
            && self.pts.same_instant(other.pts)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NearDirection {
    Increasing,
    Decreasing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DepthAxis {
    CameraZ,
    RayDistance,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DepthMeasure {
    /// Unitless, not necessarily in 0..1; no conversion to metres is defined.
    Relative { near: NearDirection },
    Metric {
        meters_per_unit: f64,
        axis: DepthAxis,
    },
    InverseMetric {
        inverse_meters_per_unit: f64,
        axis: DepthAxis,
    },
}

impl DepthMeasure {
    pub fn near_direction(self) -> NearDirection {
        match self {
            Self::Relative { near } => near,
            Self::Metric { .. } => NearDirection::Decreasing,
            Self::InverseMetric { .. } => NearDirection::Increasing,
        }
    }

    fn validate(self) -> Result<()> {
        let unit = match self {
            Self::Relative { .. } => return Ok(()),
            Self::Metric {
                meters_per_unit, ..
            } => meters_per_unit,
            Self::InverseMetric {
                inverse_meters_per_unit,
                ..
            } => inverse_meters_per_unit,
        };
        ensure(
            unit.is_finite() && unit > 0.0,
            "invalid physical depth unit",
        )
    }

    fn distance_meters(self, value: f64) -> Result<f64> {
        let distance = match self {
            Self::Relative { .. } => {
                return Err(Error::Invalid(
                    "relative depth requires explicit calibration".into(),
                ))
            }
            Self::Metric {
                meters_per_unit, ..
            } => value * meters_per_unit,
            Self::InverseMetric {
                inverse_meters_per_unit,
                ..
            } => 1.0 / (value * inverse_meters_per_unit),
        };
        ensure(
            distance.is_finite() && distance > 0.0,
            "nonpositive or nonfinite metric depth",
        )?;
        Ok(distance)
    }
}

/// Crop is in unrotated source pixels: x, y, width, height. Rotation is clockwise.
/// The output is a resampling of this crop after rotation and optional mirroring.
/// UV coordinates denote continuous image edges; texel centres use (i + .5) / size.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepthGeometry {
    pub source_size: [u32; 2],
    pub crop: [u32; 4],
    pub quarter_turns: u8,
    pub mirror_x: bool,
}

impl DepthGeometry {
    pub fn validate(&self) -> Result<()> {
        let [width, height] = self.source_size;
        let [x, y, w, h] = self.crop;
        ensure(width > 0 && height > 0, "empty depth source geometry")?;
        ensure(
            w > 0
                && h > 0
                && u64::from(x) + u64::from(w) <= u64::from(width)
                && u64::from(y) + u64::from(h) <= u64::from(height),
            "depth crop exceeds source bounds",
        )?;
        ensure(self.quarter_turns < 4, "unsupported depth rotation")
    }

    /// A normal miss outside the crop; does not clamp unrelated source pixels.
    pub fn source_uv_to_depth_uv(&self, uv: [f64; 2]) -> Result<Option<[f64; 2]>> {
        self.validate()?;
        ensure(uv.iter().all(|v| v.is_finite()), "nonfinite source UV")?;
        let [x, y, w, h] = self.crop;
        let u = (uv[0] * f64::from(self.source_size[0]) - f64::from(x)) / f64::from(w);
        let v = (uv[1] * f64::from(self.source_size[1]) - f64::from(y)) / f64::from(h);
        if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            return Ok(None);
        }
        let [mut u, v] = match self.quarter_turns {
            0 => [u, v],
            1 => [1.0 - v, u],
            2 => [1.0 - u, 1.0 - v],
            3 => [v, 1.0 - u],
            _ => return Err(Error::Invalid("unsupported depth rotation".into())),
        };
        if self.mirror_x {
            u = 1.0 - u;
        }
        Ok(Some([u, v]))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepthModel {
    pub id: String,
    pub revision: String,
    pub sha256: [u8; 32],
    /// Identifies the exact colour/layout/resize/normalisation/output procedure.
    pub processing: String,
}

impl DepthModel {
    pub fn validate(&self) -> Result<()> {
        for name in [&self.id, &self.revision, &self.processing] {
            ensure(
                !name.is_empty() && name.len() <= 256 && !name.chars().any(char::is_control),
                "invalid bounded depth model identity",
            )?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepthDescriptor {
    pub size: [u32; 2],
    pub source: DepthSource,
    pub geometry: DepthGeometry,
    pub model: DepthModel,
    pub measure: DepthMeasure,
    /// Apply decoded = stored * scale + offset before interpreting the measure.
    pub scale: f64,
    pub offset: f64,
    /// Optional valid decoded interval, in the measure's units. These are not
    /// renderer clip planes; None makes no bound claim for a relative model.
    pub valid_range: Option<[f64; 2]>,
}

impl DepthDescriptor {
    pub fn validate(&self) -> Result<usize> {
        let [w, h] = self.size;
        let pixels = u64::from(w) * u64::from(h);
        ensure(
            w > 0
                && h > 0
                && w <= MAX_DEPTH_DIMENSION
                && h <= MAX_DEPTH_DIMENSION
                && pixels <= MAX_DEPTH_PIXELS as u64,
            "depth resource exceeds dimension/pixel budget",
        )?;
        self.source.pts.validate()?;
        self.geometry.validate()?;
        self.model.validate()?;
        self.measure.validate()?;
        ensure(
            self.scale.is_finite() && self.scale > 0.0 && self.offset.is_finite(),
            "invalid depth scale/offset",
        )?;
        if let Some([low, high]) = self.valid_range {
            ensure(
                low.is_finite() && high.is_finite() && low < high,
                "invalid decoded depth range",
            )?;
        }
        Ok(pixels as usize)
    }
}

/// Immutable, validated R32Float scalar samples. R8 is not an internal format.
/// Inference runtimes, models, GPU handles and weight files are deliberately absent.
#[derive(Clone, Debug)]
pub struct DepthResource {
    descriptor: DepthDescriptor,
    values: Vec<f32>,
    validity: Option<Vec<u8>>,
    confidence: Option<Vec<f32>>,
}

impl DepthResource {
    pub fn new(
        descriptor: DepthDescriptor,
        values: Vec<f32>,
        validity: Option<Vec<u8>>,
        confidence: Option<Vec<f32>>,
    ) -> Result<Self> {
        let pixels = descriptor.validate()?;
        ensure(
            values.len() == pixels,
            "depth sample count does not match dimensions",
        )?;
        if let Some(mask) = &validity {
            ensure(
                mask.len() == pixels && mask.iter().all(|v| *v <= 1),
                "invalid depth validity plane",
            )?;
        }
        if let Some(scores) = &confidence {
            ensure(
                scores.len() == pixels
                    && scores
                        .iter()
                        .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
                "invalid depth confidence plane",
            )?;
        }
        let bytes =
            pixels * (4 + usize::from(validity.is_some()) + 4 * usize::from(confidence.is_some()));
        ensure(
            bytes <= MAX_DEPTH_RESOURCE_BYTES,
            "depth resource exceeds byte budget",
        )?;
        for (index, stored) in values.iter().enumerate() {
            if validity.as_ref().is_some_and(|mask| mask[index] == 0) {
                continue;
            }
            let decoded = f64::from(*stored) * descriptor.scale + descriptor.offset;
            ensure(decoded.is_finite(), "nonfinite valid depth sample")?;
            if let Some([low, high]) = descriptor.valid_range {
                ensure(
                    (low..=high).contains(&decoded),
                    "valid depth sample outside declared range",
                )?;
            }
            if !matches!(descriptor.measure, DepthMeasure::Relative { .. }) {
                descriptor.measure.distance_meters(decoded)?;
            }
        }
        Ok(Self {
            descriptor,
            values,
            validity,
            confidence,
        })
    }

    pub fn descriptor(&self) -> &DepthDescriptor {
        &self.descriptor
    }
    pub fn values(&self) -> &[f32] {
        &self.values
    }
    pub fn validity(&self) -> Option<&[u8]> {
        self.validity.as_deref()
    }
    pub fn confidence(&self) -> Option<&[f32]> {
        self.confidence.as_deref()
    }

    pub fn sample(&self, index: usize) -> Option<f64> {
        let value = *self.values.get(index)?;
        if self.validity.as_ref().is_some_and(|mask| mask[index] == 0) {
            return None;
        }
        Some(f64::from(value) * self.descriptor.scale + self.descriptor.offset)
    }

    /// Missing/invalid samples remain normal misses. Relative depth is an error,
    /// even when it happens to span 0..1. Ray distance additionally needs a ray.
    pub fn camera_z_meters(&self, index: usize, ray_cosine: Option<f64>) -> Result<Option<f64>> {
        let Some(value) = self.sample(index) else {
            return Ok(None);
        };
        let distance = self.descriptor.measure.distance_meters(value)?;
        let axis = match self.descriptor.measure {
            DepthMeasure::Metric { axis, .. } | DepthMeasure::InverseMetric { axis, .. } => axis,
            DepthMeasure::Relative { .. } => {
                return Err(Error::Invalid(
                    "relative depth requires explicit calibration".into(),
                ))
            }
        };
        let z = match axis {
            DepthAxis::CameraZ => distance,
            DepthAxis::RayDistance => {
                let cosine = ray_cosine
                    .ok_or_else(|| Error::Invalid("ray depth needs a camera ray cosine".into()))?;
                ensure(
                    cosine.is_finite() && cosine > 0.0 && cosine <= 1.0,
                    "invalid camera ray cosine",
                )?;
                distance * cosine
            }
        };
        ensure(z.is_finite() && z > 0.0, "invalid camera Z depth")?;
        Ok(Some(z))
    }
}
