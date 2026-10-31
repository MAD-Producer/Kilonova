//! Camera choice for one sampled view. No project edits, global observer
//! mutation, camera substitution, or duplicate source resolution.
use super::*;

#[derive(Clone, Copy, Debug)]
pub enum ViewCamera<'a> {
    /// The composition's time-dependent active camera, including its implicit
    /// default when no authored camera is active.
    Composition,
    /// A specific authored camera in this composition. Zero selects the
    /// authored legacy camera only when it actually exists. Explicit cameras
    /// can be inspected outside their active interval or while hidden/locked.
    Camera(u64),
    Observer(&'a Observer),
}
impl<'a> From<Option<&'a Observer>> for ViewCamera<'a> {
    fn from(observer: Option<&'a Observer>) -> Self {
        observer.map_or(Self::Composition, Self::Observer)
    }
}

impl Scene {
    pub(super) fn select_view(
        &mut self,
        project: &Project,
        frame: f64,
        view: ViewCamera<'_>,
    ) -> Result<()> {
        let fail = |object: u64, cause: &str| {
            motion_model::Error::Invalid(format!(
                "viewer_camera object={object} composition={} frame={frame} phase=resolve: {cause}",
                project.composition_id
            ))
        };
        let index = match view {
            ViewCamera::Composition => project
                .active_camera_layer(frame)
                .and_then(|layer| project.layers.iter().position(|l| l.id == layer.id))
                .unwrap_or(project.layers.len()),
            ViewCamera::Camera(0) if project.camera.created => project.layers.len(),
            ViewCamera::Camera(0) => return Err(fail(0, "authored camera does not exist")),
            ViewCamera::Camera(id) => {
                let index = project
                    .layers
                    .iter()
                    .position(|l| l.id == id)
                    .ok_or_else(|| fail(id, "camera layer does not exist"))?;
                if project.layers[index].camera_data().is_none() {
                    return Err(fail(id, "object is not a camera"));
                }
                index
            }
            ViewCamera::Observer(observer) => {
                self.camera = observer.pose(project.width, project.height)?;
                self.camera_viewport = [0., 0., project.width as f32, project.height as f32];
                self.camera_sample = None;
                return Ok(());
            }
        };
        let object = project.layers.get(index).map_or(0, |layer| layer.id);
        let sample = self
            .node_cameras
            .get(index)
            .copied()
            .flatten()
            .ok_or_else(|| fail(object, "camera has no resolved sample"))?;
        self.camera = sample.pose;
        self.camera_viewport = sample.viewport;
        self.camera_sample = Some(sample);
        let [x, y, w, h] = sample.viewport;
        let [width, height] = [project.width as f32, project.height as f32];
        if sample.viewport != [0., 0., width, height] {
            let fit = Mat4::from_translation(Vec3::new(
                (2. * x + w) / width - 1.,
                1. - (2. * y + h) / height,
                0.,
            )) * Mat4::from_scale(Vec3::new(w / width, h / height, 1.));
            self.camera.view_projection = fit * self.camera.view_projection;
        }
        Ok(())
    }
}
