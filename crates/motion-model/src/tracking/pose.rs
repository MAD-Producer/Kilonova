//! Explicit external world/camera basis and uniform-unit conversion.
//! Column vectors: R_target = B_world * R_source * inverse(B_camera).
//! Source poses remain untouched; converted poses interpret the validated
//! near-unit quaternion as a rigid rotation and emit a unit quaternion.
use super::{
    asset::{self, Pose},
    plate::{self, Calibration},
};
use glam::{DMat3, DQuat, DVec3};
use std::fmt;

#[derive(Debug)]
pub enum Error {
    Basis,
    Units,
    Handedness,
    NonFinitePoint,
    NumericRange,
    Pose(asset::Error),
    Projection(plate::Error),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Basis => f.write_str("tracking basis must be a signed axis permutation"),
            Self::Units => {
                f.write_str("tracking source/target world units must be finite and positive")
            }
            Self::Handedness => f.write_str("tracking world/camera bases produce a reflected pose"),
            Self::NonFinitePoint => {
                f.write_str("tracking point/origin contains a non-finite coordinate")
            }
            Self::NumericRange => {
                f.write_str("tracking coordinate conversion exceeded finite range")
            }
            Self::Pose(cause) => write!(f, "tracking coordinate conversion: {cause}"),
            Self::Projection(cause) => write!(f, "tracking world reprojection: {cause}"),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Pose(cause) => Some(cause),
            Self::Projection(cause) => Some(cause),
            _ => None,
        }
    }
}
/// Rows select signed source components, e.g. [1,-2,-3] flips Y and Z.
/// World and camera bases are independent: a Z-up scene need not have a Z-up
/// camera-local frame. Both bases must have the same determinant for a rigid pose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Basis([i8; 3]);
impl Basis {
    pub const RIGHT_DOWN_FORWARD: Self = Self([1, 2, 3]);
    pub const RIGHT_UP_BACK: Self = Self([1, -2, -3]);
    pub const RIGHT_UP_FORWARD: Self = Self([1, -2, 3]);
    pub fn new(rows: [i8; 3]) -> Result<Self, Error> {
        let mut seen = [false; 3];
        for row in rows {
            let index = row.unsigned_abs();
            if !(1..=3).contains(&index) || seen[(index - 1) as usize] {
                return Err(Error::Basis);
            }
            seen[(index - 1) as usize] = true;
        }
        Ok(Self(rows))
    }
    pub fn rows(self) -> [i8; 3] {
        self.0
    }
    fn matrix(self) -> DMat3 {
        let mut columns = [[0.; 3]; 3];
        for (output, component) in self.0.into_iter().enumerate() {
            columns[(component.unsigned_abs() - 1) as usize][output] =
                if component > 0 { 1. } else { -1. };
        }
        DMat3::from_cols_array_2d(&columns)
    }
    pub fn inverse(self) -> Self {
        let mut result = [0; 3];
        for (output, component) in self.0.into_iter().enumerate() {
            result[(component.unsigned_abs() - 1) as usize] =
                (output as i8 + 1) * component.signum();
        }
        Self(result)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Conversion {
    world: Basis,
    camera: Basis,
    scale: f64,
    origin: DVec3,
}
impl Conversion {
    /// Source-world units to target-world units, followed by target-world origin.
    /// No unit/basis is guessed from a filename, importer name or image extent.
    pub fn new(
        world: Basis,
        camera: Basis,
        source_meters_per_unit: f64,
        target_meters_per_unit: f64,
        target_origin: [f64; 3],
    ) -> Result<Self, Error> {
        if !source_meters_per_unit.is_finite()
            || !target_meters_per_unit.is_finite()
            || source_meters_per_unit <= 0.
            || target_meters_per_unit <= 0.
        {
            return Err(Error::Units);
        }
        if world.matrix().determinant() != camera.matrix().determinant() {
            return Err(Error::Handedness);
        }
        let scale = source_meters_per_unit / target_meters_per_unit;
        if !scale.is_finite() || scale <= 0. {
            return Err(Error::NumericRange);
        }
        let origin = DVec3::from_array(target_origin);
        if !origin.is_finite() {
            return Err(Error::NonFinitePoint);
        }
        Ok(Self {
            world,
            camera,
            scale,
            origin,
        })
    }
    pub fn point(self, point: [f64; 3]) -> Result<[f64; 3], Error> {
        let point = DVec3::from_array(point);
        if !point.is_finite() {
            return Err(Error::NonFinitePoint);
        }
        finite(self.world.matrix() * point * self.scale + self.origin)
    }
    pub fn pose(self, source: Pose) -> Result<Pose, Error> {
        let rotation = DMat3::from_quat(rigid_rotation(source)?);
        let rotation = self.world.matrix() * rotation * self.camera.matrix().transpose();
        let orientation = DQuat::from_mat3(&rotation).normalize();
        if !orientation.is_finite() {
            return Err(Error::NumericRange);
        }
        Ok(Pose {
            position: self.point(source.position)?,
            orientation_xyzw: orientation.to_array(),
        })
    }
    pub fn inverse(self) -> Result<Self, Error> {
        let world = self.world.inverse();
        let scale = self.scale.recip();
        if !scale.is_finite() || scale <= 0. {
            return Err(Error::NumericRange);
        }
        let origin = -(world.matrix() * self.origin) * scale;
        if !origin.is_finite() {
            return Err(Error::NumericRange);
        }
        Ok(Self {
            world,
            camera: self.camera.inverse(),
            scale,
            origin,
        })
    }
}
fn rigid_rotation(pose: Pose) -> Result<DQuat, Error> {
    pose.validate().map_err(Error::Pose)?;
    Ok(DQuat::from_array(pose.orientation_xyzw).normalize())
}
fn finite(point: DVec3) -> Result<[f64; 3], Error> {
    if point.is_finite() {
        Ok(point.to_array())
    } else {
        Err(Error::NumericRange)
    }
}
/// World point and camera translation must use the same explicit world unit.
/// The returned point is canonical camera space (+X right,+Y down,+Z forward).
pub fn camera_point(camera_to_world: Pose, world_point: [f64; 3]) -> Result<[f64; 3], Error> {
    let rotation = rigid_rotation(camera_to_world)?;
    let world_point = DVec3::from_array(world_point);
    if !world_point.is_finite() {
        return Err(Error::NonFinitePoint);
    }
    finite(rotation.conjugate() * (world_point - DVec3::from_array(camera_to_world.position)))
}
pub fn world_residual(
    camera_to_world: Pose,
    calibration: Calibration,
    world_point: [f64; 3],
    observed_source_pixel: [f64; 2],
) -> Result<f64, Error> {
    calibration
        .residual(
            camera_point(camera_to_world, world_point)?,
            observed_source_pixel,
        )
        .map_err(Error::Projection)
}
#[cfg(test)]
#[path = "pose_tests.rs"]
mod tests;
