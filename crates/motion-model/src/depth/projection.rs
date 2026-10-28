//! Explicit conversion from camera-Z metres into the actual camera projection.
use super::{
    CalibratedDepth, DepthDescriptor, MAX_DEPTH_DIMENSION, MAX_DEPTH_PIXELS,
    MAX_DEPTH_RESOURCE_BYTES,
};
use crate::{ensure, CameraPose, Result};
use glam::{DMat4, DVec3, DVec4};

#[derive(Clone, Debug)]
pub struct DepthProjection {
    camera: CameraPose,
    forward: DVec3,
    inverse: DMat4,
    pixels_per_meter: f64,
}
impl DepthProjection {
    pub fn new(camera: CameraPose, pixels_per_meter: f64) -> Result<Self> {
        ensure(
            camera.eye.is_finite()
                && camera.target.is_finite()
                && camera.view_projection.is_finite()
                && pixels_per_meter.is_finite()
                && pixels_per_meter > 0.,
            "invalid depth projection camera or world units",
        )?;
        let forward = camera.target.as_dvec3() - camera.eye.as_dvec3();
        ensure(
            forward.length_squared().is_finite() && forward.length_squared() > 0.,
            "depth camera has no finite viewing direction",
        )?;
        let matrix = camera.view_projection.as_dmat4();
        let inverse = matrix.inverse();
        ensure(
            inverse.is_finite(),
            "depth camera projection is not invertible",
        )?;
        Ok(Self {
            camera,
            forward: forward.normalize(),
            inverse,
            pixels_per_meter,
        })
    }
    pub fn camera(&self) -> CameraPose {
        self.camera
    }
    pub fn pixels_per_meter(&self) -> f64 {
        self.pixels_per_meter
    }
    pub fn matches_camera(&self, camera: CameraPose) -> bool {
        self.camera.eye == camera.eye
            && self.camera.target == camera.target
            && self.camera.projection == camera.projection
            && self.camera.view_projection == camera.view_projection
    }

    /// Top-left image UV, in continuous edges. Out-of-view samples are normal
    /// misses; no world-distance or clip-depth clamp is introduced.
    pub fn project_camera_z(&self, uv: [f64; 2], meters: f64) -> Result<Option<f32>> {
        ensure(
            uv.iter().all(|v| v.is_finite()) && meters.is_finite() && meters > 0.,
            "invalid projected depth sample",
        )?;
        if uv.iter().any(|v| !(0. ..=1.).contains(v)) {
            return Ok(None);
        }
        let world = self.unproject_camera_z(uv, meters)?;
        let clip = self.camera.view_projection.as_dmat4() * world.extend(1.);
        ensure(
            clip.is_finite() && clip.w > 0.,
            "depth projection is behind its camera",
        )?;
        let projected = clip.z / clip.w;
        ensure(projected.is_finite(), "nonfinite projected depth")?;
        if !(0. ..=1.).contains(&projected) {
            return Ok(None);
        }
        Ok(Some(projected as f32))
    }

    /// Reconstruct a point at explicit camera-Z metres. Offscreen rays are
    /// allowed: a transformed image can extend past the reference viewport.
    /// Near/far clipping belongs to the eventual rendering camera.
    pub fn unproject_camera_z(&self, uv: [f64; 2], meters: f64) -> Result<DVec3> {
        ensure(uv.iter().all(|v| v.is_finite()) && meters.is_finite() && meters > 0.,
            "invalid unprojected depth sample")?;
        let unproject = |z| -> Result<DVec3> {
            let p = self.inverse * DVec4::new(uv[0] * 2. - 1., 1. - uv[1] * 2., z, 1.);
            ensure(
                p.is_finite() && p.w != 0.,
                "depth camera unprojection overflow",
            )?;
            let p = p.truncate() / p.w;
            ensure(p.is_finite(), "nonfinite depth camera ray")?;
            Ok(p)
        };
        // Mid-depth stays finite for an infinite-far perspective projection.
        let near = unproject(0.)?;
        let next = unproject(0.5)?;
        let ray = next - near;
        let denominator = ray.dot(self.forward);
        ensure(
            denominator.is_finite() && denominator > 0.,
            "depth camera ray contradicts viewing direction",
        )?;
        let z = meters * self.pixels_per_meter;
        ensure(z.is_finite(), "depth world-unit conversion overflow")?;
        let distance = (near - self.camera.eye.as_dvec3()).dot(self.forward);
        let world = near + ray * ((z - distance) / denominator);
        ensure(world.is_finite(), "depth world position overflow")?;
        Ok(world)
    }
}

/// Native zero-to-one clip depth plus explicit validity, prepared without GPU IO.
/// The source descriptor and exact camera remain attached, preventing unrelated
/// frames/cameras from silently consuming a previously projected field.
#[derive(Clone, Debug)]
pub struct ProjectedDepth {
    projection: DepthProjection,
    source: DepthDescriptor,
    size: [u32; 2],
    samples: Vec<[f32; 2]>,
}
impl ProjectedDepth {
    /// Nearest texel sampling after the declared source crop/rotation/mirror.
    /// Confidence is not a validity mask: no confidence threshold is invented.
    /// Out-of-crop, invalid and near/far-clipped samples retain explicit validity0.
    pub fn from_calibrated(
        depth: &CalibratedDepth<'_>,
        projection: DepthProjection,
        size: [u32; 2],
    ) -> Result<Self> {
        let [w, h] = size;
        ensure(
            w > 0 && h > 0 && w <= MAX_DEPTH_DIMENSION && h <= MAX_DEPTH_DIMENSION,
            "projected depth dimensions exceed limits",
        )?;
        let count = (w as usize)
            .checked_mul(h as usize)
            .filter(|&n| n <= MAX_DEPTH_PIXELS)
            .ok_or_else(|| crate::Error::Invalid("projected depth pixel budget exceeded".into()))?;
        ensure(
            count <= MAX_DEPTH_RESOURCE_BYTES / std::mem::size_of::<[f32; 2]>(),
            "projected depth byte budget exceeded",
        )?;
        let mut samples = Vec::new();
        samples
            .try_reserve_exact(count)
            .map_err(|_| crate::Error::Invalid("projected depth allocation failed".into()))?;
        let descriptor = depth.resource().descriptor();
        let [dw, dh] = descriptor.size;
        for y in 0..h {
            for x in 0..w {
                let uv = [(x as f64 + 0.5) / w as f64, (y as f64 + 0.5) / h as f64];
                let sample = (|| -> Result<Option<f32>> {
                    let Some([u, v]) = descriptor.geometry.source_uv_to_depth_uv(uv)? else {
                        return Ok(None);
                    };
                    // An exact outer edge still denotes its adjacent last texel.
                    let dx = ((u * dw as f64) as usize).min(dw as usize - 1);
                    let dy = ((v * dh as f64) as usize).min(dh as usize - 1);
                    let Some(z) = depth.camera_z_meters(dy * dw as usize + dx)? else {
                        return Ok(None);
                    };
                    projection.project_camera_z(uv, z)
                })()?;
                samples.push(sample.map_or([1., 0.], |z| [z, 1.]));
            }
        }
        Ok(Self {
            projection,
            source: descriptor.clone(),
            size,
            samples,
        })
    }
    pub fn source(&self) -> &DepthDescriptor {
        &self.source
    }
    pub fn projection(&self) -> &DepthProjection {
        &self.projection
    }
    pub fn size(&self) -> [u32; 2] {
        self.size
    }
    pub fn samples(&self) -> &[[f32; 2]] {
        &self.samples
    }
}
