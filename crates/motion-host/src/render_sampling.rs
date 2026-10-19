//! Host media preparation for a persistent GPU exposure task.
use crate::{preview_video::PreviewVideoCache, video_frames::VideoFrames};
use motion_core::Project;
use motion_render::{image_resources::Resolution, RenderError, Renderer, Scene};
use std::path::Path;

#[derive(Clone, Copy)]
pub struct ExposureMediaPolicy {
    pub image_resolution: Resolution,
    pub asynchronous_images: bool,
    pub video_edge: Option<u32>,
}
impl ExposureMediaPolicy {
    pub const FULL: Self = Self {
        image_resolution: Resolution::Full,
        asynchronous_images: false,
        video_edge: None,
    };
}

pub fn needs_exposure(project: &Project) -> crate::Result<bool> {
    if project
        .layers
        .iter()
        .any(|layer| layer.sampling.motion_blur.is_some())
    {
        return Ok(true);
    }
    if !project
        .compositions
        .iter()
        .any(|c| c.layers.iter().any(|l| l.sampling.motion_blur.is_some()))
    {
        return Ok(false);
    }
    let reachable = project
        .reachable_compositions()
        .map_err(|error| error.to_string())?;
    Ok(project.compositions.iter().any(|composition| {
        reachable.contains(&composition.id)
            && composition
                .layers
                .iter()
                .any(|layer| layer.sampling.motion_blur.is_some())
    }))
}

pub struct ExposureMedia<'a> {
    pub project: &'a Project,
    pub root: &'a Path,
    pub videos: &'a mut VideoFrames,
    pub proxies: &'a mut PreviewVideoCache,
    pub policy: ExposureMediaPolicy,
}
impl ExposureMedia<'_> {
    pub fn prepare(&mut self, renderer: &mut Renderer, scene: &Scene) -> Result<bool, RenderError> {
        renderer.retain_video_instances(scene);
        self.proxies.retain(scene);
        if !renderer.prepare_scene_assets(
            scene,
            self.policy.image_resolution,
            self.policy.asynchronous_images,
        )? {
            return Ok(false);
        }
        // No speculative next-frame requests: the task owns the order of
        // receiver clocks. Repeated pending polls keep this exact generation.
        let Some(frames) = self
            .videos
            .prepare_scene_exact(self.project, self.root, scene)
            .map_err(|error| {
                RenderError::Invalid(format!(
                    "exposure composition={} sample_frame={} phase=media_prepare: {error}",
                    scene.composition_id, scene.frame
                ))
            })?
        else {
            return Ok(false);
        };
        let mut encoder = renderer
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("exposure media conversions"),
            });
        let mut uploads = Vec::new();
        let layers = scene.video_layers();
        for (object, frame) in frames {
            let source = layers.iter().find(|layer| layer.id == object)
                .and_then(|layer| layer.video.as_ref()).ok_or_else(|| RenderError::Invalid(
                    format!("exposure object={object} phase=media_upload: sampled video instance missing")))?;
            // Local materialization owns another encoder, so all conversions
            // must be submitted before it can consume the required PTS pair.
            if let Some(upload) = self.proxies.upload_prepared(
                renderer,
                object,
                source.asset,
                &frame,
                self.policy.video_edge,
                true,
                &mut encoder,
            ).map_err(|error| {
                let message = format!("exposure object={object} source={} time_us={} sample_frame={} phase=media_upload: {error}", source.asset, source.source_time_us, scene.frame);
                match error.resource_rejection().copied() {
                    Some(rejection) => RenderError::Resource { rejection, message },
                    None => RenderError::Invalid(message),
                }
            })? {
                uploads.push(upload);
            }
        }
        if !uploads.is_empty() {
            renderer.submit_tracked(encoder.finish(), |_| {});
            for upload in uploads {
                upload.mark_submitted();
            }
        }
        renderer.check_health()?;
        Ok(true)
    }
}
