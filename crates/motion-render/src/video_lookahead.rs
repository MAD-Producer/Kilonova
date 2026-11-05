//! Authored source-time lookahead. No expression evaluation, raster work or IO.
use crate::scene::instance_ids::InstanceIds;
use motion_model::{
    composition::CompositionView, layer_time::SourceTimeSample, Content, Error, Project, Result,
    VideoAsset,
};
use std::collections::HashSet;

/// A future instance uses the same alias as ordinary scene sampling, including
/// reference paths that have not become visible yet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UpcomingVideo<'a> {
    pub object: u64,
    pub asset: &'a VideoAsset,
    pub source_time_us: u64,
    pub blend: bool,
    pub output_steps: usize,
}

/// Reuses the renderer's bounded topology registry rather than inventing IDs or
/// sampling complete future scenes. Retained topology is replaced atomically.
#[derive(Default)]
pub struct VideoLookahead {
    instances: InstanceIds,
}
impl VideoLookahead {
    /// At most four future output instants and four candidates. Current required
    /// instances are excluded before the bound is applied, so they cannot crowd
    /// out a newly entering layer. Nearest output instant wins.
    pub fn plan<'a>(
        &mut self,
        project: &'a Project,
        composition: &str,
        frame: f64,
        steps: usize,
        required: &[u64],
        capacity: usize,
    ) -> Result<Vec<UpcomingVideo<'a>>> {
        if !frame.is_finite() || steps > 4 || capacity > 4 {
            return Err(Error::Invalid(
                "invalid video warmup lookahead bounds".into(),
            ));
        }
        let root = project.composition_view(composition)?;
        if frame < 0. || frame >= f64::from(root.frames) {
            return Err(Error::Invalid(
                "video warmup frame outside composition".into(),
            ));
        }
        if capacity == 0 || steps == 0 {
            return Ok(Vec::new());
        }
        if root.layers.len() > motion_model::MAX_LAYERS {
            return Err(Error::Invalid("video warmup layer limit exceeded".into()));
        }
        if root
            .layers
            .iter()
            .any(|l| matches!(l.content, Content::Composition { .. }))
        {
            self.instances.prepare_view(project, composition)?;
        }
        let required: HashSet<_> = required.iter().copied().collect();
        let mut result = Vec::with_capacity(capacity);
        let mut path = Vec::new();
        for output_steps in 1..=steps {
            let cursor = SourceTimeSample::new(frame + output_steps as f64, 1.)?;
            self.visit(
                project,
                root,
                cursor,
                &mut path,
                &required,
                capacity,
                output_steps,
                &mut result,
            )?;
            if result.len() == capacity {
                break;
            }
        }
        Ok(result)
    }
    fn visit<'a>(
        &self,
        project: &'a Project,
        body: CompositionView<'a>,
        cursor: SourceTimeSample,
        path: &mut Vec<u64>,
        required: &HashSet<u64>,
        capacity: usize,
        output_steps: usize,
        result: &mut Vec<UpcomingVideo<'a>>,
    ) -> Result<()> {
        if !cursor.in_interval(0., f64::from(body.frames)) {
            return Ok(());
        }
        let solo = body.layers.iter().any(|l| l.solo);
        let mut references = motion_model::compositing::matte_sources(body.layers);
        references.extend(
            body.layers
                .iter()
                .flat_map(|l| l.effects.iter())
                .filter(|e| e.enabled)
                .filter_map(|e| {
                    e.image_input
                        .and_then(motion_model::EffectImageInput::layer)
                }),
        );
        for layer in body.layers {
            if result.len() == capacity {
                break;
            }
            if !matches!(
                layer.content,
                Content::Video { .. } | Content::Composition { .. }
            ) {
                continue;
            }
            let referenced = references.contains(&layer.id);
            if !(layer.visible && (!solo || layer.solo)) && !referenced {
                continue;
            }
            let clip = layer.interval(body.frames);
            if !cursor.in_interval(clip.in_frame, clip.out_frame) {
                continue;
            }
            if !referenced
                && layer
                    .transform
                    .opacity
                    .sample(layer.checked_local_frame(cursor.frame)?)?
                    <= 0.
            {
                continue;
            }
            match &layer.content {
                Content::Video { video } => {
                    let object = if path.is_empty() {
                        layer.id
                    } else {
                        self.instances.object(path, body.id, layer.id)?
                    };
                    if required.contains(&object) || result.iter().any(|c| c.object == object) {
                        continue;
                    }
                    let asset = project
                        .video_assets
                        .iter()
                        .find(|a| a.id == video.asset)
                        .ok_or_else(|| Error::LayerTimeObject {
                            object: layer.id,
                            phase: "video_warmup_asset",
                            source: Box::new(Error::Missing(video.asset)),
                        })?;
                    let source = layer.video_picture_time_at(cursor, body.fps, asset)?;
                    if source < asset.video_start_us as i64 || source >= asset.video_end_us as i64 {
                        continue;
                    }
                    result.push(UpcomingVideo {
                        object,
                        asset,
                        source_time_us: source as u64,
                        blend: layer.sampling.frame_blend
                            == motion_model::render_sampling::FrameBlend::Linear,
                        output_steps,
                    });
                }
                Content::Composition { clip } => {
                    let child = project.composition_view(&clip.composition)?;
                    let source = layer
                        .source_time_sample_from(cursor)?
                        .through_composition(clip, body.fps, child.fps)?;
                    path.push(layer.id);
                    let outcome = self.visit(
                        project,
                        child,
                        source,
                        path,
                        &required,
                        capacity,
                        output_steps,
                        result,
                    );
                    path.pop();
                    outcome?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
