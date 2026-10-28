//! Saved, explicitly selected source camera. Source parsing belongs to the host.
use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportedCamera {
    pub asset: u64,
    pub scene: u32,
    pub node: u32,
    pub pixels_per_metre: f32,
    pub pivot: [f32; 3],
    /// Independent clock in camera-local frames, not the model layer's clock.
    pub animation: Option<crate::ModelAnimation>,
}
impl ImportedCamera {
    pub fn validate(&self) -> Result<()> {
        crate::ModelClip {
            asset: self.asset,
            scene: self.scene,
            pixels_per_metre: self.pixels_per_metre,
            pivot: self.pivot,
            samples: 1,
            animation: self.animation.clone(),
        }
        .validate()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CameraSample {
    pub pose: CameraPose,
    pub world: Mat4,
    /// Composition pixels; retained separately from projection to preserve aspect.
    pub viewport: [f32; 4],
}

pub trait CameraSampler {
    /// Validate the exact managed metadata before sampling an owned source.
    /// Editing a source reference must never silently reuse another revision.
    fn validate_source(&self, asset: &crate::ModelAsset) -> Result<()>;
    fn sample(
        &self,
        camera: &Camera,
        frame: f64,
        fps: u32,
        size: [u32; 2],
        prefix: Mat4,
    ) -> Result<CameraSample>;
}

pub struct AuthoredCameraSampler;
impl CameraSampler for AuthoredCameraSampler {
    fn validate_source(&self, _asset: &crate::ModelAsset) -> Result<()> {
        Err(crate::Error::Invalid("imported camera requires a source resolver".into()))
    }
    fn sample(
        &self,
        camera: &Camera,
        frame: f64,
        _fps: u32,
        [width, height]: [u32; 2],
        prefix: Mat4,
    ) -> Result<CameraSample> {
        let pose = camera.pose_parented(frame, width, height, prefix)?;
        let projection = Mat4::perspective_rh(
            camera.fov.sample(frame)?.clamp(10.0, 120.0).to_radians(),
            width as f32 / height as f32,
            0.5,
            100_000.0,
        );
        Ok(CameraSample {
            pose,
            world: (projection.inverse() * pose.view_projection).inverse(),
            viewport: [0., 0., width as f32, height as f32],
        })
    }
}
