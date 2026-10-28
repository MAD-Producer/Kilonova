//! A calibrated image is geometry in its authored reference camera, not a
//! full-viewport depth seed. Vertices stay in layer-local space so subsequent
//! layer/parent edits and independent camera animation use ordinary transforms.
use super::{CalibratedDepth, DepthProjection, MAX_DEPTH_RESOURCE_BYTES};
use crate::{ensure, CameraPose, Error, Result};
use glam::{Mat4, Vec3, Vec4};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepthReference {
    pub camera: CameraPose,
    pub world: Mat4,
    pub size: [f32; 2],
}
impl DepthReference {
    pub fn validate(&self) -> Result<()> {
        DepthProjection::new(self.camera, 1.)?;
        ensure(
            self.world.is_finite()
                && self.world.row(3) == Vec4::W
                && self.world.inverse().is_finite()
                && self.size.iter().all(|n| n.is_finite() && *n > 0.),
            "depth reference requires invertible affine geometry and positive size",
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DepthVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    /// Homogeneous correction preserving the source plane's image projection
    /// in the reference camera, even across vertices at different distances.
    pub texture_weight: f32,
}
#[derive(Clone, Debug)]
pub struct DepthSurface {
    vertices: Vec<DepthVertex>,
    reference_size: [f32; 2],
}
impl DepthSurface {
    /// Triangle grid at inference texel centres, with exact crop and source
    /// edges. Invalid and out-of-crop vertices retain their authored flat
    /// position; confidence never becomes an invented validity threshold.
    /// No resolution reduction or silent dropping occurs on budget refusal.
    pub fn new(
        depth: &CalibratedDepth<'_>,
        reference: &DepthReference,
        pixels_per_metre: f32,
    ) -> Result<Self> {
        Self::with_byte_budget(depth, reference, pixels_per_metre, MAX_DEPTH_RESOURCE_BYTES)
    }
    /// Scene owners pass their remaining retained-vertex budget before any
    /// full grid allocation. The resource-wide hard limit still applies.
    pub fn with_byte_budget(
        depth: &CalibratedDepth<'_>,
        reference: &DepthReference,
        pixels_per_metre: f32,
        byte_budget: usize,
    ) -> Result<Self> {
        reference.validate()?;
        let projection = DepthProjection::new(reference.camera, f64::from(pixels_per_metre))?;
        let descriptor = depth.resource().descriptor();
        let g = &descriptor.geometry;
        let [dw, dh] = descriptor.size;
        let [nx, ny] = if g.quarter_turns % 2 == 0 {
            [dw, dh]
        } else {
            [dh, dw]
        };
        let edges = |n: u32, origin: u32, extent: u32, full: u32| {
            let mut result = Vec::with_capacity(n as usize + 4);
            result.push(0.);
            if origin != 0 {
                result.push(f64::from(origin) / f64::from(full));
            }
            for i in 0..n {
                result.push(
                    (f64::from(origin) + (f64::from(i) + 0.5) / f64::from(n) * f64::from(extent))
                        / f64::from(full),
                );
            }
            if u64::from(origin) + u64::from(extent) != u64::from(full) {
                result.push((f64::from(origin) + f64::from(extent)) / f64::from(full));
            }
            result.push(1.);
            result
        };
        let xs = edges(nx, g.crop[0], g.crop[2], g.source_size[0]);
        let ys = edges(ny, g.crop[1], g.crop[3], g.source_size[1]);
        let count = (xs.len() - 1)
            .checked_mul(ys.len() - 1)
            .and_then(|n| n.checked_mul(6))
            .filter(|n| {
                *n <= byte_budget.min(MAX_DEPTH_RESOURCE_BYTES) / std::mem::size_of::<DepthVertex>()
            })
            .ok_or_else(|| Error::Invalid("depth surface vertex byte budget exceeded".into()))?;
        let mut vertices = Vec::new();
        vertices
            .try_reserve_exact(count)
            .map_err(|_| Error::Invalid("depth surface allocation failed".into()))?;
        let world = reference.world.as_dmat4();
        let inverse = world.inverse();
        let vertex = |uv: [f64; 2]| -> Result<DepthVertex> {
            let flat = glam::DVec3::new(
                (uv[0] - 0.5) * f64::from(reference.size[0]),
                (0.5 - uv[1]) * f64::from(reference.size[1]),
                0.,
            );
            let z = if let Some([u, v]) = g.source_uv_to_depth_uv(uv)? {
                let x = ((u * f64::from(dw)) as usize).min(dw as usize - 1);
                let y = ((v * f64::from(dh)) as usize).min(dh as usize - 1);
                depth.camera_z_meters(y * dw as usize + x)?
            } else {
                None
            };
            let local = if let Some(z) = z {
                let clip = reference.camera.view_projection.as_dmat4() * world * flat.extend(1.);
                ensure(
                    clip.is_finite() && clip.w > 0.,
                    "depth reference plane projects behind its camera",
                )?;
                let ray_uv = [0.5 + 0.5 * clip.x / clip.w, 0.5 - 0.5 * clip.y / clip.w];
                inverse.transform_point3(projection.unproject_camera_z(ray_uv, z)?)
            } else {
                flat
            };
            let position: Vec3 = local.as_vec3();
            ensure(
                position.is_finite(),
                "depth surface layer-local position overflow",
            )?;
            let matrix = reference.camera.view_projection.as_dmat4() * world;
            let weight = ((matrix * local.extend(1.)).w / (matrix * flat.extend(1.)).w) as f32;
            ensure(
                weight.is_finite() && weight > 0.,
                "depth surface texture projection overflow",
            )?;
            Ok(DepthVertex {
                position: position.to_array(),
                uv: [uv[0] as f32, uv[1] as f32],
                texture_weight: weight,
            })
        };
        // Two bounded rows avoid retaining another full-resolution position grid.
        let row = |y| {
            xs.iter()
                .map(|&x| vertex([x, y]))
                .collect::<Result<Vec<_>>>()
        };
        let mut previous = row(ys[0])?;
        for &y in &ys[1..] {
            let next = row(y)?;
            for x in 0..xs.len() - 1 {
                vertices.extend_from_slice(&[
                    previous[x],
                    next[x],
                    next[x + 1],
                    previous[x],
                    next[x + 1],
                    previous[x + 1],
                ]);
            }
            previous = next;
        }
        Ok(Self {
            vertices,
            reference_size: reference.size,
        })
    }
    pub fn vertices(&self) -> &[DepthVertex] {
        &self.vertices
    }
    /// Authored XY extent. A later layer size edit scales these local X/Y
    /// coordinates; metric Z stays governed by the calibration and transforms.
    pub fn reference_size(&self) -> [f32; 2] {
        self.reference_size
    }
    pub fn bytes(&self) -> usize {
        self.vertices.len() * std::mem::size_of::<DepthVertex>()
    }
}
