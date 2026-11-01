//! Original-PTS calibrated camera poses with independent authored corrections.
//! Canonical source camera axes are right/down/forward; engine axes right/up/back.
use super::{
    asset::{Kind, Observation, Pose, Rate, Visibility},
    plate::{Calibration, Extent, PlateTransform, Rotation},
    point::{Interpolation, Missing},
    pose::{Basis, Conversion},
    time::{Location, Mapping, Speed, Time, Timeline},
};
use crate::{ensure, Camera, CameraPose, CameraSample, Error, Project, ProjectionKind, Result};
use glam::{DMat3, DMat4, DQuat, DVec3, Mat4};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorrectionReference {
    /// Local engine-world rigid pose, before the camera's authored spatial parent.
    pub world: Pose,
    pub vertical_fov_degrees: f64,
}
impl CorrectionReference {
    pub fn capture(camera: &Camera, frame: f64, size: [u32; 2]) -> Result<Self> {
        ensure(
            camera.imported.is_none(),
            "cannot capture correction reference from imported node",
        )?;
        let sampled = crate::camera::authored_sample(camera, frame, size, Mat4::IDENTITY)?;
        let m = sampled.world.as_dmat4();
        let result = Self {
            world: Pose {
                position: m.w_axis.truncate().to_array(),
                orientation_xyzw: DQuat::from_mat3(&DMat3::from_mat4(m))
                    .normalize()
                    .to_array(),
            },
            vertical_fov_degrees: f64::from(camera.fov.sample(frame)?.clamp(10., 120.)),
        };
        result.validate()?;
        Ok(result)
    }
    fn validate(&self) -> Result<()> {
        self.world.validate().map_err(invalid)?;
        ensure(
            self.world.position.iter().all(|v| v.abs() <= 1e7),
            "camera correction reference exceeds world range",
        )?;
        ensure(
            self.vertical_fov_degrees.is_finite()
                && (10. ..=120.).contains(&self.vertical_fov_degrees),
            "camera correction reference FOV must be 10..120 degrees",
        )
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraBinding {
    pub asset: u64,
    pub track: String,
    pub anchor_local_frame: i32,
    pub source_anchor_pts: i64,
    pub speed_numerator: i32,
    pub speed_denominator: u32,
    /// Signed source-world component permutation into engine-world axes.
    pub world_basis: [i8; 3],
    pub target_meters_per_unit: f64,
    pub target_origin: [f64; 3],
    pub correction_reference: CorrectionReference,
    pub crop: [f64; 4],
    pub rotation: Rotation,
    pub near: f64,
    pub far: f64,
    pub interpolation: Interpolation,
    pub max_gap_ticks: u64,
    pub minimum_confidence: f64,
    pub missing: Missing,
}
fn invalid(cause: impl std::fmt::Display) -> Error {
    Error::Invalid(cause.to_string())
}
fn rigid(pose: Pose) -> DMat4 {
    DMat4::from_rotation_translation(
        DQuat::from_array(pose.orientation_xyzw).normalize(),
        DVec3::from_array(pose.position),
    )
}
impl CameraBinding {
    pub fn validate_parameters(&self) -> Result<()> {
        ensure(
            self.asset != 0
                && !self.track.trim().is_empty()
                && self.track.len() <= 128
                && !self.track.chars().any(char::is_control),
            "invalid tracking camera identity",
        )?;
        ensure(
            self.anchor_local_frame.unsigned_abs() <= crate::MAX_FRAMES,
            "camera anchor frame out of range",
        )?;
        Speed::new(self.speed_numerator, self.speed_denominator).map_err(invalid)?;
        self.correction_reference.validate()?;
        // The canonical file already fixes camera-local RDF axes. Their engine
        // conversion is explicit and fixed; only source-world axes are selected.
        Conversion::new(
            Basis::new(self.world_basis).map_err(invalid)?,
            Basis::RIGHT_UP_BACK,
            1.,
            self.target_meters_per_unit,
            self.target_origin,
        )
        .map_err(invalid)?;
        ensure(
            self.target_origin.iter().all(|v| v.abs() <= 1e7),
            "tracking camera origin exceeds world range",
        )?;
        ensure(
            self.minimum_confidence.is_finite() && (0. ..=1.).contains(&self.minimum_confidence),
            "invalid camera confidence threshold",
        )?;
        validate_clip(self.near, self.far)
    }
    pub fn validate(&self, project: &Project, camera: &Camera) -> Result<()> {
        self.validate_parameters()?;
        ensure(
            camera.created && camera.imported.is_none(),
            "tracking requires an explicit non-imported camera",
        )?;
        let asset = project
            .tracking_assets
            .iter()
            .find(|a| a.id == self.asset)
            .ok_or_else(|| invalid("camera tracking asset missing"))?;
        asset.validate_source_digest()?;
        let track = asset
            .data
            .tracks
            .iter()
            .find(|t| t.id == self.track)
            .ok_or_else(|| invalid("camera track missing"))?;
        ensure(
            track.kind == Kind::Camera,
            "camera binding requires a camera track",
        )?;
        self.conversion(asset.data.meters_per_world_unit)?;
        PlateTransform::new(
            asset.data.source_extent,
            self.crop,
            self.rotation,
            Extent::new(project.width, project.height).map_err(invalid)?,
        )
        .map_err(invalid)?;
        Ok(())
    }
    fn conversion(&self, source: Option<f64>) -> Result<Conversion> {
        Conversion::new(
            Basis::new(self.world_basis).map_err(invalid)?,
            Basis::RIGHT_UP_BACK,
            source.ok_or_else(|| invalid("camera source world units missing"))?,
            self.target_meters_per_unit,
            self.target_origin,
        )
        .map_err(invalid)
    }
    pub fn sample(
        &self,
        project: &Project,
        camera: &Camera,
        object: u64,
        local_frame: f64,
        prefix: Mat4,
    ) -> Result<CameraSample> {
        self.sample_inner(project, camera, local_frame, prefix)
            .map_err(|source| Error::TrackingCamera {
                object,
                asset: self.asset,
                track: self.track.clone(),
                local_frame,
                source: Box::new(source),
            })
    }
    fn sample_inner(
        &self,
        project: &Project,
        camera: &Camera,
        frame: f64,
        prefix: Mat4,
    ) -> Result<CameraSample> {
        let asset = project
            .tracking_assets
            .iter()
            .find(|a| a.id == self.asset)
            .ok_or_else(|| invalid("camera tracking asset missing"))?;
        let data = &asset.data;
        let track = data
            .tracks
            .iter()
            .find(|t| t.id == self.track)
            .ok_or_else(|| invalid("camera track missing"))?;
        let rate = Rate::new(project.fps, 1).map_err(invalid)?;
        let mapping = Mapping::new(
            Time::frame(i64::from(self.anchor_local_frame), rate),
            self.source_anchor_pts,
            Speed::new(self.speed_numerator, self.speed_denominator).map_err(invalid)?,
        );
        let location = Timeline::from_shared(data, mapping)
            .locate(Time::fractional_frame(frame, rate).map_err(invalid)?)
            .map_err(invalid)?;
        let available = |index| {
            let i = track
                .samples
                .binary_search_by_key(&index, |s| s.source_index)
                .map_err(|_| invalid(format!("camera observation absent source_index={index}")))?;
            let s = &track.samples[i];
            if s.visibility != Visibility::Visible {
                return Err(invalid(format!(
                    "camera observation unavailable source_index={index} visibility={:?}",
                    s.visibility
                )));
            }
            if s.confidence < self.minimum_confidence {
                return Err(invalid(format!("camera observation unavailable source_index={index} confidence={} threshold={}",s.confidence,self.minimum_confidence)));
            }
            Ok(s)
        };
        let selected = (|| match location {
            Location::At(i) => Ok((available(i)?, None, 0.)),
            Location::Between {
                before,
                after,
                numerator,
                denominator,
            } => {
                let gap = i128::from(data.source_pts[after as usize])
                    - i128::from(data.source_pts[before as usize]);
                if gap > i128::from(self.max_gap_ticks) {
                    return Err(invalid(format!("camera interpolation gap exceeds policy before={before} after={after} gap_ticks={gap} max_gap_ticks={}",self.max_gap_ticks)));
                }
                let a = available(before)?;
                if self.interpolation == Interpolation::Hold {
                    Ok((a, None, 0.))
                } else {
                    Ok((
                        a,
                        Some(available(after)?),
                        numerator as f64 / denominator as f64,
                    ))
                }
            }
            _ => Err(invalid(format!(
                "camera observation outside source PTS range location={location:?}"
            ))),
        })();
        let (a, b, t) = match selected {
            Ok(v) => v,
            Err(_) if self.missing == Missing::AuthoredOnly => {
                return crate::camera::authored_sample(
                    camera,
                    frame,
                    [project.width, project.height],
                    prefix,
                )
            }
            Err(cause) => return Err(cause),
        };
        let observation = |s: &super::asset::Sample| match s.observation {
            Some(Observation::Camera {
                camera_to_world,
                calibration,
            }) => Ok((camera_to_world, calibration)),
            _ => Err(invalid(format!(
                "camera payload kind mismatch source_index={}",
                s.source_index
            ))),
        };
        let (mut pose, mut calibration) = observation(a)?;
        if let Some(b) = b {
            let (other, lens) = observation(b)?;
            pose.position = std::array::from_fn(|i| {
                pose.position[i] + (other.position[i] - pose.position[i]) * t
            });
            pose.orientation_xyzw = DQuat::from_array(pose.orientation_xyzw)
                .normalize()
                .slerp(DQuat::from_array(other.orientation_xyzw).normalize(), t)
                .normalize()
                .to_array();
            let mix2 = |a: [f64; 2], b: [f64; 2]| std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t);
            let mix3 = |a: [f64; 3], b: [f64; 3]| std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t);
            calibration = Calibration::new(
                data.source_extent,
                mix2(calibration.focal(), lens.focal()),
                mix2(calibration.principal(), lens.principal()),
                mix3(calibration.radial(), lens.radial()),
                mix2(calibration.tangential(), lens.tangential()),
            )
            .map_err(invalid)?;
        }
        let source = self
            .conversion(data.meters_per_world_unit)?
            .pose(pose)
            .map_err(invalid)?;
        let authored = crate::camera::authored_sample(
            camera,
            frame,
            [project.width, project.height],
            Mat4::IDENTITY,
        )?;
        // A(frame) * inverse(A_reference) is the independent world-space manual
        // correction. The authored spatial parent is composed exactly once.
        let world = prefix.as_dmat4()
            * authored.world.as_dmat4()
            * rigid(self.correction_reference.world).inverse()
            * rigid(source);
        ensure(
            world.is_finite()
                && world.determinant().abs() > 1e-12
                && world.w_axis.truncate().abs().max_element() <= 1e7,
            "tracking camera world transform is singular or exceeds range",
        )?;
        let zoom = (self.correction_reference.vertical_fov_degrees.to_radians() * 0.5).tan()
            / (f64::from(camera.fov.sample(frame)?.clamp(10., 120.)).to_radians() * 0.5).tan();
        let plate = PlateTransform::new(
            data.source_extent,
            self.crop,
            self.rotation,
            Extent::new(project.width, project.height).map_err(invalid)?,
        )
        .map_err(invalid)?;
        let projection = calibrated_projection(calibration, plate, zoom, self.near, self.far)?;
        let view_projection = (projection.as_dmat4() * world.inverse()).as_mat4();
        let eye = world.w_axis.truncate().as_vec3();
        let target = world.transform_point3(-DVec3::Z).as_vec3();
        let world = world.as_mat4();
        ensure(
            world.is_finite()
                && view_projection.is_finite()
                && eye.is_finite()
                && target.is_finite(),
            "tracking camera projection exceeds finite range",
        )?;
        Ok(CameraSample {
            pose: CameraPose {
                eye,
                target,
                projection: ProjectionKind::Perspective,
                view_projection,
            },
            world,
            viewport: [0., 0., project.width as f32, project.height as f32],
            depth_of_field: authored.depth_of_field,
        })
    }
}
fn validate_clip(near: f64, far: f64) -> Result<()> {
    ensure(
        near.is_finite() && far.is_finite() && near >= 0.000001 && far > near && far <= 1e9,
        "tracking camera requires 0.000001 <= near < far <= 1000000000",
    )
}
/// Off-centre calibrated pinhole, followed by the exact affine plate transform.
/// Rotation can introduce off-diagonal projection coefficients. Depth is 0..1.
pub fn calibrated_projection(
    lens: Calibration,
    plate: PlateTransform,
    zoom: f64,
    near: f64,
    far: f64,
) -> Result<Mat4> {
    validate_clip(near, far)?;
    ensure(
        lens.source() == plate.source(),
        "camera calibration and plate source extent differ",
    )?;
    ensure(
        zoom.is_finite() && zoom > 0.,
        "camera correction focal multiplier invalid",
    )?;
    if lens.radial() != [0.; 3] || lens.tangential() != [0.; 2] {
        return Err(invalid(format!(
            "camera distortion requires explicit output stage radial={:?} tangential={:?}",
            lens.radial(),
            lens.tangential()
        )));
    }
    let origin = plate.apply([0., 0.]).map_err(invalid)?;
    let x = plate.apply([1., 0.]).map_err(invalid)?;
    let y = plate.apply([0., 1.]).map_err(invalid)?;
    let [ax, bx] = [x[0] - origin[0], x[1] - origin[1]];
    let [ay, by] = [y[0] - origin[0], y[1] - origin[1]];
    let principal = plate.apply(lens.principal()).map_err(invalid)?;
    let [w, h] = plate.output().pixels().map(f64::from);
    let [fx, fy] = lens.focal().map(|v| v * zoom);
    let rows = [
        [
            2. * ax * fx / w,
            -2. * ay * fy / w,
            1. - 2. * principal[0] / w,
            0.,
        ],
        [
            -2. * bx * fx / h,
            2. * by * fy / h,
            2. * principal[1] / h - 1.,
            0.,
        ],
        [0., 0., far / (near - far), far * near / (near - far)],
        [0., 0., -1., 0.],
    ];
    let matrix = DMat4::from_cols_array_2d(&std::array::from_fn(|c| {
        std::array::from_fn(|r| rows[r][c])
    }))
    .as_mat4();
    ensure(
        matrix.is_finite() && matrix.determinant().abs() > 1e-12,
        "calibrated camera projection is singular or exceeds range",
    )?;
    Ok(matrix)
}
