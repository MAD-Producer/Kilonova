//! Independently authored camera math from Khronos glTF 2.0 §3.10. Native
//! projection uses [0,1] depth rather than the specification's OpenGL [-1,1].
use crate::{invalid, schema, unsupported, AnimationSample, Result, Scene};
use glam::{Mat4, Vec3, Vec4};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Camera {
    Perspective {
        yfov: f32,
        aspect_ratio: Option<f32>,
        znear: f32,
        zfar: Option<f32>,
    },
    Orthographic {
        xmag: f32,
        ymag: f32,
        znear: f32,
        zfar: f32,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraKind {
    Perspective,
    Orthographic,
}
#[derive(Clone, Copy, Debug)]
pub struct CameraView {
    pub node: usize,
    pub camera: usize,
    pub kind: CameraKind,
    pub eye: Vec3,
    pub target: Vec3,
    pub view_projection: Mat4,
    /// Native pixels, x/y/width/height. Letterboxing preserves authored aspect.
    pub viewport: [f32; 4],
}

pub(crate) fn decode(
    root: &schema::Root,
    budget: &mut crate::accessors::Budget,
    limits: &crate::ImportLimits,
) -> Result<Vec<Camera>> {
    budget.reserve(root.cameras.len(), std::mem::size_of::<Camera>(), limits)?;
    root.cameras
        .iter()
        .enumerate()
        .map(|(index, camera)| {
            let valid = |v: f32| v.is_finite() && v > 0.;
            match (
                camera.kind.as_str(),
                &camera.perspective,
                &camera.orthographic,
            ) {
                ("perspective", Some(p), None)
                    if valid(p.yfov)
                        && p.yfov < std::f32::consts::PI
                        && valid(p.znear)
                        && p.aspect_ratio.is_none_or(valid)
                        && p.zfar.is_none_or(|far| valid(far) && far > p.znear) =>
                {
                    Ok(Camera::Perspective {
                        yfov: p.yfov,
                        aspect_ratio: p.aspect_ratio,
                        znear: p.znear,
                        zfar: p.zfar,
                    })
                }
                ("orthographic", None, Some(o))
                    if valid(o.xmag)
                        && valid(o.ymag)
                        && o.znear.is_finite()
                        && o.znear >= 0.
                        && o.zfar.is_finite()
                        && o.zfar > o.znear =>
                {
                    Ok(Camera::Orthographic {
                        xmag: o.xmag,
                        ymag: o.ymag,
                        znear: o.znear,
                        zfar: o.zfar,
                    })
                }
                _ => Err(invalid(
                    "camera",
                    index,
                    "invalid projection type, fields or clipping interval",
                )),
            }
        })
        .collect()
}

impl Scene {
    /// Select a camera *node* reachable from the selected scene. No automatic
    /// activation, looping, or mutation of a composition's authored camera.
    /// Unit conversion affects positions/clipping planes, not field of view.
    pub fn camera_view(
        &self,
        scene: usize,
        node: usize,
        pixels_per_metre: f32,
        sample: Option<AnimationSample>,
        viewport: [u32; 2],
    ) -> Result<CameraView> {
        if viewport.contains(&0) || viewport.iter().any(|&v| v > (1 << 24)) {
            return Err(invalid(
                "camera_view",
                node,
                "viewport dimensions must be positive and exactly representable",
            ));
        }
        let mut selected = None;
        self.visit_node_instances_at(scene, pixels_per_metre, sample, |index, world| {
            if index == node {
                selected = Some(world);
            }
        })?;
        let world = selected.ok_or_else(|| {
            invalid(
                "camera_view",
                node,
                "camera node is absent from selected scene",
            )
        })?;
        let index = self.nodes[node].camera.ok_or_else(|| {
            invalid(
                "camera_view",
                node,
                "selected node does not instantiate a camera",
            )
        })?;
        let camera = self.cameras[index];
        // Ignore global scale as the format requires. A sheared/non-invertible
        // hierarchy has no unambiguous orthonormal frame in this profile; refuse
        // it rather than silently change its view or manufacture a look-at axis.
        let axes = [world.x_axis, world.y_axis, world.z_axis].map(|axis| {
            axis.truncate()
                .try_normalize()
                .ok_or_else(|| invalid("camera_view", node, "collapsed camera basis"))
        });
        let [x, y, z] = axes;
        let (x, y, z) = (x?, y?, z?);
        if x.dot(y).abs() > 1e-4
            || x.dot(z).abs() > 1e-4
            || y.dot(z).abs() > 1e-4
            || x.cross(y).dot(z) <= 0.
        {
            return Err(unsupported(
                "camera_view",
                node,
                "sheared or reflected camera hierarchy",
            ));
        }
        let eye = world.w_axis.truncate();
        let rigid = Mat4::from_cols(x.extend(0.), y.extend(0.), z.extend(0.), eye.extend(1.));
        let view = rigid.inverse();
        let width = viewport[0] as f32;
        let height = viewport[1] as f32;
        let viewport_aspect = width / height;
        let scale = pixels_per_metre;
        let (kind, projection, aspect) = match camera {
            Camera::Perspective {
                yfov,
                aspect_ratio,
                znear,
                zfar,
            } => {
                let aspect = aspect_ratio.unwrap_or(viewport_aspect);
                let near = znear * scale;
                if !near.is_finite()
                    || near <= 0.
                    || zfar.is_some_and(|far| !(far * scale).is_finite() || far * scale <= near)
                {
                    return Err(invalid(
                        "camera_view",
                        node,
                        "perspective clipping conversion overflow",
                    ));
                }
                let projection = if let Some(far) = zfar {
                    Mat4::perspective_rh(yfov, aspect, near, far * scale)
                } else {
                    Mat4::perspective_infinite_rh(yfov, aspect, near)
                };
                (CameraKind::Perspective, projection, aspect)
            }
            Camera::Orthographic {
                xmag,
                ymag,
                znear,
                zfar,
            } => {
                if [xmag * scale, ymag * scale, zfar * scale]
                    .iter()
                    .any(|v| !v.is_finite() || *v <= 0.)
                    || !(znear * scale).is_finite()
                    || zfar * scale <= znear * scale
                {
                    return Err(invalid(
                        "camera_view",
                        node,
                        "orthographic clipping conversion overflow",
                    ));
                }
                (
                    CameraKind::Orthographic,
                    Mat4::orthographic_rh(
                        -xmag * scale,
                        xmag * scale,
                        -ymag * scale,
                        ymag * scale,
                        znear * scale,
                        zfar * scale,
                    ),
                    xmag / ymag,
                )
            }
        };
        let (w, h) = if viewport_aspect > aspect {
            (height * aspect, height)
        } else {
            (width, width / aspect)
        };
        let viewport = [(width - w) * 0.5, (height - h) * 0.5, w, h];
        let view_projection = projection * view;
        let determinant = view_projection.determinant();
        let target = eye - z;
        if !view_projection.is_finite()
            || !determinant.is_finite()
            || determinant == 0.
            || !target.is_finite()
            || target == eye
            || viewport.iter().any(|v| !v.is_finite())
            || w <= 0.
            || h <= 0.
        {
            return Err(invalid(
                "camera_view",
                node,
                "camera conversion overflow or precision loss",
            ));
        }
        // Also reject a collapsed/infinite clipping conversion before native IO.
        let near_probe = projection
            * Vec4::new(
                0.,
                0.,
                -match camera {
                    Camera::Perspective { znear, .. } | Camera::Orthographic { znear, .. } => {
                        znear * scale
                    }
                },
                1.,
            );
        if !near_probe.is_finite() {
            return Err(invalid("camera_view", node, "clipping conversion overflow"));
        }
        Ok(CameraView {
            node,
            camera: index,
            kind,
            eye,
            target,
            view_projection,
            viewport,
        })
    }
}
