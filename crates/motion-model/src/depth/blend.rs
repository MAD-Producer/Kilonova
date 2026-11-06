//! Float depth mixing after explicit image registration and metric calibration.
//! Image registration never estimates motion or changes a camera's depth basis.
use super::{
    CalibratedDepth, DepthAxis, DepthDescriptor, DepthGeometry, DepthMeasure, DepthModel,
    DepthProjection, DepthResource, DepthSource, RelativeDepthCalibration, MAX_DEPTH_DIMENSION,
    MAX_DEPTH_PIXELS, MAX_DEPTH_RESOURCE_BYTES,
};
use crate::{ensure, Error, Result};
use glam::DMat3;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DepthImageFrame {
    pub source: DepthSource,
    pub extent: [u32; 2],
}
impl DepthImageFrame {
    pub fn from_depth(depth: &DepthResource) -> Self {
        Self {
            source: depth.descriptor().source.clone(),
            extent: depth.descriptor().geometry.source_size,
        }
    }
    fn validate(&self) -> Result<()> {
        self.source.pts.validate()?;
        ensure(
            self.extent.iter().all(|v| *v > 0),
            "empty depth registration frame",
        )
    }
    fn matches(&self, other: &Self) -> bool {
        self.source.same_sample(&other.source) && self.extent == other.extent
    }
}

/// A supplied target-to-reference image-pixel map, bound to both exact samples.
/// Different PTS require an explicit new registration even if its matrix is
/// identity. This does not infer camera motion, repair parallax or remove flicker.
#[derive(Clone, Debug, Serialize)]
pub struct DepthRegistration {
    target: DepthImageFrame,
    reference: DepthImageFrame,
    matrix: [[f64; 3]; 3],
}
impl DepthRegistration {
    pub fn same_sample(target: DepthImageFrame, reference: DepthImageFrame) -> Result<Self> {
        ensure(
            target.matches(&reference),
            "different depth frames require explicit registration",
        )?;
        Self::new(
            target,
            reference,
            [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
        )
    }
    pub fn new(
        target: DepthImageFrame,
        reference: DepthImageFrame,
        mut matrix: [[f64; 3]; 3],
    ) -> Result<Self> {
        target.validate()?;
        reference.validate()?;
        ensure(
            matrix.iter().flatten().all(|v| v.is_finite()),
            "nonfinite depth registration matrix",
        )?;
        let magnitude = matrix.iter().flatten().fold(0_f64, |m, v| m.max(v.abs()));
        ensure(magnitude > 0., "zero depth registration matrix")?;
        for value in matrix.iter_mut().flatten() {
            *value /= magnitude;
        }
        ensure(
            DMat3::from_cols_array_2d(&matrix).inverse().is_finite(),
            "singular depth registration matrix",
        )?;
        let [w, h] = target.extent.map(f64::from);
        let denominators = [[0., 0.], [w, 0.], [w, h], [0., h]]
            .map(|[x, y]| matrix[2][0] * x + matrix[2][1] * y + matrix[2][2]);
        ensure(
            denominators.iter().all(|v| v.is_finite() && *v > 0.)
                || denominators.iter().all(|v| v.is_finite() && *v < 0.),
            "depth registration crosses projective infinity",
        )?;
        Ok(Self {
            target,
            reference,
            matrix,
        })
    }
    fn bind(&self, target: &DepthImageFrame, reference: &DepthImageFrame) -> Result<()> {
        ensure(
            self.target.matches(target) && self.reference.matches(reference),
            "depth registration does not match source, revision, PTS or extent",
        )
    }
    fn map(&self, uv: [f64; 2]) -> Result<[f64; 2]> {
        let [x, y] = [
            uv[0] * f64::from(self.target.extent[0]),
            uv[1] * f64::from(self.target.extent[1]),
        ];
        let point = self.matrix.map(|row| row[0] * x + row[1] * y + row[2]);
        let uv = [
            point[0] / point[2] / f64::from(self.reference.extent[0]),
            point[1] / point[2] / f64::from(self.reference.extent[1]),
        ];
        ensure(
            uv.iter().all(|v| v.is_finite()),
            "depth registration coordinate overflow",
        )?;
        Ok(uv)
    }
}

/// Validated physical values in a declared reference camera. Raw relative depth
/// requires a bound calibration. Ray-distance inputs additionally supply their
/// source-grid ray cosines; no camera rays or units are guessed.
pub struct DepthBlendInput<'a> {
    resource: &'a DepthResource,
    calibrated: Option<CalibratedDepth<'a>>,
    projection: &'a DepthProjection,
    ray_cosines: Option<&'a [f32]>,
}
impl<'a> DepthBlendInput<'a> {
    pub fn new(
        resource: &'a DepthResource,
        calibration: Option<&'a RelativeDepthCalibration>,
        projection: &'a DepthProjection,
        ray_cosines: Option<&'a [f32]>,
    ) -> Result<Self> {
        let calibrated = if matches!(resource.descriptor().measure, DepthMeasure::Relative { .. }) {
            Some(
                calibration
                    .ok_or_else(|| {
                        Error::Invalid("depth blend requires explicit relative calibration".into())
                    })?
                    .bind(resource)?,
            )
        } else {
            ensure(
                calibration.is_none(),
                "metric depth must not use a relative calibration",
            )?;
            None
        };
        let ray_distance = matches!(
            resource.descriptor().measure,
            DepthMeasure::Metric {
                axis: DepthAxis::RayDistance,
                ..
            } | DepthMeasure::InverseMetric {
                axis: DepthAxis::RayDistance,
                ..
            }
        );
        if ray_distance {
            let cosines = ray_cosines.ok_or_else(|| {
                Error::Invalid("ray depth blend requires explicit source-grid ray cosines".into())
            })?;
            ensure(
                cosines.len() == resource.values().len()
                    && cosines.iter().all(|v| v.is_finite() && *v > 0. && *v <= 1.),
                "invalid depth blend ray cosine plane",
            )?;
        } else {
            ensure(
                ray_cosines.is_none(),
                "ray cosines supplied for non-ray depth",
            )?;
        }
        Ok(Self {
            resource,
            calibrated,
            projection,
            ray_cosines,
        })
    }
    fn sample(
        &self,
        index: usize,
        attempts: &mut u64,
        metrics: &mut DepthBlendMetrics,
    ) -> Result<Option<f64>> {
        *attempts += 1;
        metrics.validity_reads += u64::from(self.resource.validity().is_some());
        if let Some(depth) = &self.calibrated {
            depth.camera_z_meters(index)
        } else {
            metrics.ray_cosine_reads += u64::from(self.ray_cosines.is_some());
            self.resource
                .camera_z_meters(index, self.ray_cosines.map(|v| f64::from(v[index])))
        }
    }
}

/// Linear scalar weights, independent of colour encoding, RGBA alpha and depth
/// confidence. Invalid mask texels are missing weights, not invented zeroes.
pub struct DepthWeightMask {
    frame: DepthImageFrame,
    size: [u32; 2],
    geometry: DepthGeometry,
    values: Vec<f32>,
    validity: Option<Vec<u8>>,
}
impl DepthWeightMask {
    pub fn new(
        source: DepthSource,
        size: [u32; 2],
        geometry: DepthGeometry,
        values: Vec<f32>,
        validity: Option<Vec<u8>>,
    ) -> Result<Self> {
        source.pts.validate()?;
        geometry.validate()?;
        let count = pixel_count(size)?;
        ensure(
            values.len() == count,
            "depth mask sample count does not match size",
        )?;
        if let Some(v) = &validity {
            ensure(
                v.len() == count && v.iter().all(|x| *x <= 1),
                "invalid depth mask validity plane",
            )?;
        }
        ensure(
            count * (4 + usize::from(validity.is_some())) <= MAX_DEPTH_RESOURCE_BYTES,
            "depth mask byte budget exceeded",
        )?;
        ensure(
            values.iter().enumerate().all(|(i, v)| {
                validity.as_ref().is_some_and(|m| m[i] == 0)
                    || (v.is_finite() && (0. ..=1.).contains(v))
            }),
            "invalid linear depth mask weight",
        )?;
        Ok(Self {
            frame: DepthImageFrame {
                source,
                extent: geometry.source_size,
            },
            size,
            geometry,
            values,
            validity,
        })
    }
    pub fn frame(&self) -> &DepthImageFrame {
        &self.frame
    }
    fn sample(&self, uv: [f64; 2], metrics: &mut DepthBlendMetrics) -> Result<Option<f64>> {
        let Some(index) = sample_index(&self.geometry, self.size, uv)? else {
            return Ok(None);
        };
        metrics.mask_reads += 1;
        metrics.validity_reads += u64::from(self.validity.is_some());
        if self.validity.as_ref().is_some_and(|m| m[index] == 0) {
            return Ok(None);
        }
        Ok(Some(f64::from(self.values[index])))
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct DepthBlendMetrics {
    pub output_pixels: u64,
    pub invalid_pixels: u64,
    pub ai_reads: u64,
    pub reference_reads: u64,
    pub mask_reads: u64,
    pub validity_reads: u64,
    pub ray_cosine_reads: u64,
    /// Logical output payload, not allocator/RSS or GPU traffic measurements.
    pub output_bytes: u64,
}
#[derive(Clone, Debug, Serialize)]
pub struct DepthBlendProvenance {
    pub ai: DepthDescriptor,
    pub reference: DepthDescriptor,
    pub ai_calibration: Option<RelativeDepthCalibration>,
    pub reference_calibration: Option<RelativeDepthCalibration>,
    pub registration: DepthRegistration,
    pub mask_frame: Option<DepthImageFrame>,
    pub mask_size: Option<[u32; 2]>,
    pub mask_geometry: Option<DepthGeometry>,
    pub mask_registration: Option<DepthRegistration>,
    pub weight: f64,
    pub camera: crate::CameraPose,
    pub pixels_per_meter: f64,
}
pub struct DepthBlendResult {
    pub resource: DepthResource,
    pub metrics: DepthBlendMetrics,
    pub provenance: DepthBlendProvenance,
}
pub struct DepthBlendOptions {
    pub weight: f64,
    /// Caller-supplied identity of the authored derived procedure, not either
    /// input model's hash. Original input identities remain in provenance.
    pub output_model: DepthModel,
    pub byte_budget: usize,
}

/// D = mix(reference, ai, global_weight * linear_mask), in camera-Z metres.
/// Output follows the AI grid; reference/mask use explicit nearest sampling.
/// Only contributing operands are sampled, so an unused invalid operand cannot
/// poison zero/one endpoints. Intermediate weights require both valid depths.
pub fn blend_depth(
    ai: DepthBlendInput<'_>,
    reference: DepthBlendInput<'_>,
    registration: &DepthRegistration,
    mask: Option<(&DepthWeightMask, &DepthRegistration)>,
    options: DepthBlendOptions,
) -> Result<DepthBlendResult> {
    ensure(
        options.weight.is_finite() && (0. ..=1.).contains(&options.weight),
        "invalid global depth blend weight",
    )?;
    options.output_model.validate()?;
    ensure(
        ai.projection.matches_camera(reference.projection.camera())
            && ai.projection.pixels_per_meter() == reference.projection.pixels_per_meter(),
        "depth blend requires a common camera and world-unit basis before registration",
    )?;
    let target = DepthImageFrame::from_depth(ai.resource);
    registration.bind(&target, &DepthImageFrame::from_depth(reference.resource))?;
    if let Some((mask, registration)) = mask {
        registration.bind(&target, &mask.frame)?;
    }
    let source = ai.resource.descriptor();
    let count = pixel_count(source.size)?;
    let bytes = count
        .checked_mul(5)
        .ok_or_else(|| Error::Invalid("depth blend size overflow".into()))?;
    ensure(
        bytes <= options.byte_budget.min(MAX_DEPTH_RESOURCE_BYTES),
        "depth blend output byte budget exceeded",
    )?;
    let mut values = Vec::new();
    let mut validity = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| Error::Invalid("depth blend value allocation failed".into()))?;
    validity
        .try_reserve_exact(count)
        .map_err(|_| Error::Invalid("depth blend validity allocation failed".into()))?;
    let mut metrics = DepthBlendMetrics {
        output_pixels: count as u64,
        output_bytes: bytes as u64,
        ..Default::default()
    };
    for index in 0..count {
        let uv = [
            (index % source.size[0] as usize) as f64 + 0.5,
            (index / source.size[0] as usize) as f64 + 0.5,
        ];
        let uv = [
            uv[0] / f64::from(source.size[0]),
            uv[1] / f64::from(source.size[1]),
        ];
        let uv = source.geometry.depth_uv_to_source_uv(uv)?.ok_or_else(|| {
            Error::Invalid("depth blend output coordinate escaped its plane".into())
        })?;
        let mut ai_reads = 0;
        let mut reference_reads = 0;
        let value = (|| -> Result<Option<f64>> {
            let weight = if options.weight == 0. {
                0.
            } else if let Some((mask, registration)) = mask {
                let Some(value) = mask.sample(registration.map(uv)?, &mut metrics)? else {
                    return Ok(None);
                };
                options.weight * value
            } else {
                options.weight
            };
            let a = if weight > 0. {
                ai.sample(index, &mut ai_reads, &mut metrics)?
            } else {
                None
            };
            let b = if weight < 1. {
                let uv = registration.map(uv)?;
                if let Some(index) = sample_index(
                    &reference.resource.descriptor().geometry,
                    reference.resource.descriptor().size,
                    uv,
                )? {
                    reference.sample(index, &mut reference_reads, &mut metrics)?
                } else {
                    None
                }
            } else {
                None
            };
            Ok(if weight == 0. {
                b
            } else if weight == 1. {
                a
            } else {
                a.zip(b)
                    .map(|(a, b)| motion_effects::scalar::lerp(b, a, weight))
            })
        })()
        .map_err(|cause| Error::Invalid(format!("depth blend pixel={index}: {cause}")))?;
        metrics.ai_reads += ai_reads;
        metrics.reference_reads += reference_reads;
        if let Some(value) = value {
            let value = value as f32;
            ensure(
                value.is_finite() && value > 0.,
                &format!("depth blend pixel={index}: metres not representable in R32Float"),
            )?;
            values.push(value);
            validity.push(1);
        } else {
            values.push(0.);
            validity.push(0);
            metrics.invalid_pixels += 1;
        }
    }
    let provenance = DepthBlendProvenance {
        ai: source.clone(),
        reference: reference.resource.descriptor().clone(),
        ai_calibration: ai.calibrated.as_ref().map(|d| d.calibration().clone()),
        reference_calibration: reference
            .calibrated
            .as_ref()
            .map(|d| d.calibration().clone()),
        registration: registration.clone(),
        mask_frame: mask.map(|(m, _)| m.frame.clone()),
        mask_size: mask.map(|(m, _)| m.size),
        mask_geometry: mask.map(|(m, _)| m.geometry.clone()),
        mask_registration: mask.map(|(_, r)| r.clone()),
        weight: options.weight,
        camera: ai.projection.camera(),
        pixels_per_meter: ai.projection.pixels_per_meter(),
    };
    let mut descriptor = source.clone();
    descriptor.measure = DepthMeasure::Metric {
        meters_per_unit: 1.,
        axis: DepthAxis::CameraZ,
    };
    descriptor.scale = 1.;
    descriptor.offset = 0.;
    descriptor.valid_range = None;
    descriptor.model = options.output_model;
    Ok(DepthBlendResult {
        resource: DepthResource::new(descriptor, values, Some(validity), None)?,
        metrics,
        provenance,
    })
}

fn pixel_count([w, h]: [u32; 2]) -> Result<usize> {
    ensure(
        w > 0 && h > 0 && w <= MAX_DEPTH_DIMENSION && h <= MAX_DEPTH_DIMENSION,
        "depth blend dimensions exceed limits",
    )?;
    let count = w as usize * h as usize;
    ensure(
        count <= MAX_DEPTH_PIXELS,
        "depth blend pixel budget exceeded",
    )?;
    Ok(count)
}
fn sample_index(geometry: &DepthGeometry, size: [u32; 2], uv: [f64; 2]) -> Result<Option<usize>> {
    let Some([u, v]) = geometry.source_uv_to_depth_uv(uv)? else {
        return Ok(None);
    };
    let x = ((u * f64::from(size[0])) as usize).min(size[0] as usize - 1);
    let y = ((v * f64::from(size[1])) as usize).min(size[1] as usize - 1);
    Ok(Some(y * size[0] as usize + x))
}
