use crate::{to_world, Layer};
use glam::{EulerRot, Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};

/// Composition of the layer's sampled axis angles. Missing fields keep legacy poses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RotationOrder {
    #[default]
    LocalXyz,
    FixedXyz,
}
pub fn default_rotation_order(value: &RotationOrder) -> bool {
    *value == RotationOrder::LocalXyz
}
pub fn rotation_quaternion(rotation: [f32; 3], order: RotationOrder) -> Quat {
    let [x, y, z] = [rotation[0].to_radians(), -rotation[1].to_radians(), -rotation[2].to_radians()];
    match order {
        RotationOrder::LocalXyz => Quat::from_euler(EulerRot::XYZ, x, y, z),
        // Rz * Ry * Rx applies fixed parent axes X, then Y, then Z to a vector.
        RotationOrder::FixedXyz => Quat::from_euler(EulerRot::ZYX, z, y, x),
    }
}

pub(crate) fn pivot_matrix(layer: &Layer, frame: f64, width: u32, height: u32) -> crate::Result<Mat4> {
    let frame = layer.local_frame(frame);
    let t = &layer.transform;
    let mut rotation = t.rotation.sample(frame)?;
    let mut position = t.position.sample(frame)?;
    let mut scale = t.scale.sample(frame)?;
    if !layer.three_d {
        rotation[0] = 0.0;
        rotation[1] = 0.0;
        position[2] = 0.0;
        scale[2] = 100.0;
    }
    let quaternion = rotation_quaternion(rotation, layer.rotation_order);
    let scale = Vec3::from_array(scale) / 100.0;
    Ok(Mat4::from_scale_rotation_translation(scale, quaternion, to_world(position, width, height)))
}

/// Project parent motion into the composition plane for a flat child.
pub fn flat_matrix(m: Mat4) -> Mat4 {
    Mat4::from_cols(
        glam::Vec4::new(m.x_axis.x, m.x_axis.y, 0.0, 0.0),
        glam::Vec4::new(m.y_axis.x, m.y_axis.y, 0.0, 0.0),
        glam::Vec4::Z,
        glam::Vec4::new(m.w_axis.x, m.w_axis.y, 0.0, 1.0),
    )
}
