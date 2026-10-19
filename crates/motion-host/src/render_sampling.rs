//! Host media preparation for a persistent GPU exposure task.
use crate::{preview_video::PreviewVideoCache, video_frames::VideoFrames};
use motion_core::Project;
use motion_render::{image_resources::Resolution, RenderError, Renderer, Scene};
use std::path::Path;

/// Renderer contracts, separate from a platform's available video decoders.
pub fn capabilities() -> serde_json::Value {
    use motion_core::render_sampling::{MAX_SHUTTER_SAMPLES, PREVIEW_SHUTTER_SAMPLES};
    serde_json::json!({
        "version":1,"project_format":21,"command":"set_layer_sampling","animated":false,
        "wgpu_preview":true,
        "frame_blend":{"sources":["video"],"modes":["hold","linear"],
            "clock":"actual_source_pts","space":"linear_premultiplied"},
        "motion_blur":{"sources":["solid","image","text","vector","video","composition"],
            "shutter":"centered_midpoints","frames_min_exclusive":0,"frames_max":2,
            "samples_min":2,"samples_max":MAX_SHUTTER_SAMPLES,
            "preview_samples_max":PREVIEW_SHUTTER_SAMPLES,"frozen_samples":"authored",
            "outside_interval":"transparent_without_weight_renormalization","nested":"independent_composition_clocks"},
        "continuous_rasterization":{"sources":["vector"],"density":"projected_source_plane",
            "text":false,"collapse_composition_transforms":false,"over_budget":"error"},
        "legacy_frame_plans":{"motion_blur":false,"on_unsupported":"error_before_output"},
        "legacy_capture":{"motion_blur":false}
    })
}

/// A single-time packed plan cannot encode a multi-time exposure. Reject it
/// before changing session time or touching caller-owned output buffers.
pub fn require_single_time(project: &Project, operation: &str) -> crate::Result<()> {
    if needs_exposure(project)? {
        return Err(format!("operation={operation} composition={}: motion blur requires the exposure renderer; single-time plans cannot represent it", project.composition_id));
    }
    Ok(())
}

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
            .prepare_scene_sampling_exact(self.project, self.root, scene)
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
