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
    pub(super) fn select_view(&mut self, view: ViewCamera<'_>) -> Result<()> {
        let fail = |object: u64, cause: &str| {
            motion_model::Error::Invalid(format!(
                "viewer_camera object={object} composition={} frame={} phase=resolve: {cause}",
                self.composition_id, self.frame
            ))
        };
        let sample = match view {
            ViewCamera::Composition => self
                .composition_camera
                .ok_or_else(|| fail(0, "composition camera has no resolved sample"))?,
            ViewCamera::Camera(0) if self.legacy_camera_created => self
                .node_cameras
                .last()
                .copied()
                .flatten()
                .ok_or_else(|| fail(0, "camera has no resolved sample"))?,
            ViewCamera::Camera(0) => return Err(fail(0, "authored camera does not exist")),
            ViewCamera::Camera(id) => {
                let resolved = self
                    .source_objects
                    .iter()
                    .find(|(_, source)| *source == id)
                    .map_or(id, |(alias, _)| *alias);
                let index = self
                    .node_ids
                    .iter()
                    .position(|node| *node == resolved)
                    .ok_or_else(|| fail(id, "camera layer does not exist"))?;
                self.node_cameras
                    .get(index)
                    .copied()
                    .flatten()
                    .ok_or_else(|| fail(id, "object is not a camera"))?
            }
            ViewCamera::Observer(observer) => {
                self.camera = observer.pose(self.width, self.height)?;
                self.camera_viewport = [0., 0., self.width as f32, self.height as f32];
                self.camera_sample = None;
                return Ok(());
            }
        };
        self.camera = sample.pose;
        self.camera_viewport = sample.viewport;
        self.camera_sample = Some(sample);
        let [x, y, w, h] = sample.viewport;
        let [width, height] = [self.width as f32, self.height as f32];
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

    /// Reuse one successfully sampled frame with a different root camera.
    /// No expression evaluation, source resolution, or child resampling occurs.
    /// Evaluated projects and child scenes are shared; view-dependent root
    /// projection and ordering are independent. Sampling this scene again uses
    /// copy-on-write for children retained by another view.
    pub fn fork_view(&self, view: ViewCamera<'_>) -> Result<Self> {
        if self.composition_camera.is_none() {
            return Err(motion_model::Error::Invalid(format!(
                "viewer_frame composition={} frame={} phase=fork: no successfully sampled frame",
                self.composition_id, self.frame
            )));
        }
        let mut context = self.local_context();
        context.composition_camera = self.composition_camera;
        context.legacy_camera_created = self.legacy_camera_created;
        context.node_cameras = self.node_cameras.clone();
        context.evaluated_project = self.evaluated_project.clone();
        context.nested = self.nested.clone();
        context.select_view(view)?;
        if !context.camera.view_projection.is_finite()
            || !context.camera.eye.is_finite()
            || !context.camera.target.is_finite()
            || context.camera.eye == context.camera.target
        {
            return Err(motion_model::Error::Invalid(format!(
                "viewer_frame composition={} frame={} phase=fork: invalid view camera pose",
                self.composition_id, self.frame
            )));
        }
        let forward = (context.camera.target - context.camera.eye).normalize();
        for layer in &mut context.layers {
            let world = context.node_world.get(layer.order).ok_or_else(||
                motion_model::Error::Invalid(format!(
                    "viewer_frame composition={} frame={} object={} phase=fork: draw order has no world transform",
                    context.composition_id, context.frame, layer.id)))?;
            layer.depth = (world.w_axis.truncate() - context.camera.eye).dot(forward);
            if layer.three_d {
                layer.view_projection = context.camera.view_projection;
            }
        }
        context.sort_spatial_layers();
        Ok(context)
    }

    /// The viewer has one, two, or four cells. An invalid binding fails the
    /// complete batch rather than publishing a partially updated set of views.
    pub fn fork_views(&self, views: &[ViewCamera<'_>]) -> Result<Vec<Self>> {
        ensure(
            matches!(views.len(), 1 | 2 | 4),
            "viewer_frame phase=fork: expected 1, 2, or 4 views",
        )?;
        views.iter().map(|view| self.fork_view(*view)).collect()
    }

    pub(super) fn sort_spatial_layers(&mut self) {
        // Visible flat layers and already-rasterized overlays delimit spatial groups.
        let mut start = 0;
        while start < self.layers.len() {
            if !self.layers[start].three_d || self.layers[start].screen_overlay {
                start += 1;
                continue;
            }
            let mut end = start + 1;
            while end < self.layers.len()
                && self.layers[end].three_d
                && !self.layers[end].screen_overlay
            {
                end += 1;
            }
            self.layers[start..end]
                .sort_unstable_by(|a, b| b.depth.total_cmp(&a.depth).then(a.order.cmp(&b.order)));
            start = end;
        }
    }
}
