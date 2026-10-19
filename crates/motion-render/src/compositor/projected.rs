//! A common normalized-depth space for planes sampled through different cameras.
//! Split attributes in reciprocal-W space, then recover each draw's own world
//! coordinates. The existing vertex ABI and per-draw projection remain intact.
use super::{Plane, PlaneVertex, Vertex};
use crate::Scene;
use glam::{DMat4, DVec2, DVec3, DVec4};
use motion_model::{Error, Result};

const MAX_CLIPPED_VERTICES: usize = 16;
#[derive(Clone, Copy)]
struct ClipVertex {
    position: DVec4,
    uv: DVec2,
}
#[derive(Clone, Copy, Default)]
struct SourceQuad {
    object: u64,
    model: DMat4,
    size: [f32; 2],
    offset: [f32; 2],
}

#[derive(Default)]
pub(super) struct ProjectedGeometry {
    pub active: bool,
    sources: Vec<SourceQuad>,
    clip: Vec<ClipVertex>,
    spare: Vec<ClipVertex>,
    vertices: Vec<Vertex>,
}
impl ProjectedGeometry {
    pub fn prepare(&mut self, scene: &Scene, overlays: &[bool]) -> Result<()> {
        self.active = scene.layers.iter().enumerate().any(|(i, l)| {
            l.three_d
                && !overlays.get(i).copied().unwrap_or(l.screen_overlay)
                && l.view_projection != scene.camera.view_projection
        });
        self.sources.clear();
        if !self.active {
            return Ok(());
        }
        for (i, layer) in scene.layers.iter().enumerate() {
            let overlay = overlays.get(i).copied().unwrap_or(layer.screen_overlay);
            let projection = crate::scene::plane_projection(scene, layer, overlay).as_dmat4();
            let inverse = projection.inverse();
            if !projection.is_finite() || !inverse.is_finite() {
                return Err(failure(layer.id, "nonfinite or singular camera"));
            }
            self.sources.push(SourceQuad {
                object: layer.id,
                ..Default::default()
            });
        }
        Ok(())
    }
    pub fn quad(
        &mut self,
        scene: &Scene,
        layer: usize,
        size: [f32; 2],
        offset: [f32; 2],
        overlay: bool,
    ) -> Result<&[Vertex]> {
        let l = &scene.layers[layer];
        self.sources[layer] = SourceQuad {
            object: l.id,
            model: crate::scene::plane_model(l, overlay).as_dmat4(),
            size,
            offset,
        };
        let matrix = crate::scene::plane_projection(scene, l, overlay).as_dmat4()
            * self.sources[layer].model;
        self.clip.clear();
        self.spare.clear();
        self.vertices.clear();
        for [x, y] in [[-0.5, 0.5], [-0.5, -0.5], [0.5, -0.5], [0.5, 0.5]] {
            let position = matrix
                * DVec4::new(
                    x * f64::from(size[0]) + f64::from(offset[0]),
                    y * f64::from(size[1]) + f64::from(offset[1]),
                    0.,
                    1.,
                );
            if !position.is_finite() {
                return Err(failure(l.id, "invalid plane geometry"));
            }
            self.clip.push(ClipVertex {
                position,
                uv: DVec2::new(x + 0.5, 0.5 - y),
            });
        }
        // Clip in homogeneous coordinates before dividing, including the eye
        // plane. This retains the correct UV parameter along every original edge.
        for boundary in 0..7 {
            if self.clip.is_empty() {
                break;
            }
            self.spare.clear();
            for i in 0..self.clip.len() {
                let a = self.clip[i];
                let b = self.clip[(i + 1) % self.clip.len()];
                let da = distance(a.position, boundary);
                let db = distance(b.position, boundary);
                if da >= 0. {
                    push(&mut self.spare, a, l.id)?;
                }
                if (da < 0. && db > 0.) || (da > 0. && db < 0.) {
                    let t = da / (da - db);
                    push(
                        &mut self.spare,
                        ClipVertex {
                            position: a.position.lerp(b.position, t),
                            uv: a.uv.lerp(b.uv, t),
                        },
                        l.id,
                    )?;
                }
            }
            std::mem::swap(&mut self.clip, &mut self.spare);
        }
        if self.clip.len() < 3 {
            return Ok(&self.vertices);
        }
        for v in &self.clip {
            if v.position.w <= 0. {
                return Err(failure(l.id, "invalid clipped W"));
            }
            let reciprocal_w = 1. / v.position.w;
            let position = v.position.truncate() * reciprocal_w;
            let uv = v.uv * reciprocal_w;
            if !position.is_finite() || !uv.is_finite() || !reciprocal_w.is_finite() {
                return Err(failure(l.id, "projective division overflow"));
            }
            self.vertices.push(Vertex {
                position,
                uv,
                reciprocal_w,
            });
        }
        Ok(&self.vertices)
    }
    pub fn back_project(&self, layer: usize, vertex: Vertex) -> Result<(DVec3, DVec2)> {
        let source = self.sources[layer];
        if !vertex.reciprocal_w.is_finite() || vertex.reciprocal_w <= 0. {
            return Err(failure(source.object, "invalid inverse W"));
        }
        let uv = vertex.uv / vertex.reciprocal_w;
        self.world_vertex(layer, uv)
    }
    pub fn original(&self, layer: usize, corner: usize) -> Result<PlaneVertex> {
        let uv = [[0., 0.], [0., 1.], [1., 1.], [1., 0.]][corner];
        let (position, uv) = self.world_vertex(layer, DVec2::from_array(uv))?;
        Ok(PlaneVertex {
            position: position.as_vec3().to_array(),
            uv: uv.as_vec2().to_array(),
        })
    }
    fn world_vertex(&self, layer: usize, uv: DVec2) -> Result<(DVec3, DVec2)> {
        let source = self.sources[layer];
        let position = source.model.transform_point3(DVec3::new(
            (uv.x - 0.5) * f64::from(source.size[0]) + f64::from(source.offset[0]),
            (0.5 - uv.y) * f64::from(source.size[1]) + f64::from(source.offset[1]),
            0.,
        ));
        if !position.is_finite() || !uv.is_finite() {
            return Err(failure(source.object, "inverse overflow"));
        }
        // The public vertex is f32. Refuse representational overflow explicitly.
        if !position.as_vec3().is_finite() || !uv.as_vec2().is_finite() {
            return Err(failure(source.object, "exceeds vertex range"));
        }
        Ok((position, uv))
    }
}
fn distance(p: DVec4, boundary: usize) -> f64 {
    match boundary {
        0 => p.w - 1e-12,
        1 => p.x + p.w,
        2 => p.w - p.x,
        3 => p.y + p.w,
        4 => p.w - p.y,
        5 => p.z,
        _ => p.w - p.z,
    }
}
fn push(vertices: &mut Vec<ClipVertex>, v: ClipVertex, layer: u64) -> Result<()> {
    if vertices.len() >= MAX_CLIPPED_VERTICES {
        return Err(failure(layer, "clipping vertex budget exceeded"));
    }
    vertices.push(v);
    Ok(())
}
fn failure(object: u64, cause: &str) -> Error {
    Error::Invalid(format!("layer {object} phase=exposure_projection: {cause}"))
}
pub(super) fn plane(vertices: &[Vertex]) -> Option<Plane> {
    let first = vertices.first()?.position;
    for edge in vertices.get(1..)?.windows(2) {
        let normal = (edge[0].position - first).cross(edge[1].position - first);
        if normal.length_squared() > 1e-30 {
            let normal = normal.normalize();
            return Some(Plane {
                normal,
                distance: normal.dot(first),
            });
        }
    }
    None
}
