//! Predict only the bounded future output instants, using authored clocks.
//! No scene resampling, expressions, file IO or decoder work occurs here.
use super::*;
use motion_core::layer_time::{SourceTimeSample, TimeTraversal};

pub(super) type Times = [Option<u64>; LOOKAHEAD];
pub(super) type Plans = HashMap<u64, Times>;

pub(super) fn plans(
    project: &Project,
    scene: &Scene,
    layers: &[&motion_render::DrawLayer],
) -> Result<Plans> {
    let mut result = HashMap::new();
    if layers.is_empty() {
        return Ok(result);
    }
    let cursors = std::array::from_fn(|i| {
        Some(SourceTimeSample {
            frame: scene.frame + (i + 1) as f64,
            rate: 1.,
            traversal: TimeTraversal::Later,
        })
    });
    visit(project, scene, layers, cursors, false, &mut result)?;
    Ok(result)
}

fn active(
    cursor: Option<SourceTimeSample>,
    layer: &motion_core::Layer,
    frames: u32,
) -> Option<SourceTimeSample> {
    let clip = layer.interval(frames);
    cursor.filter(|c| {
        c.in_interval(0., f64::from(frames)) && c.in_interval(clip.in_frame, clip.out_frame)
    })
}

fn visit(
    project: &Project,
    scene: &Scene,
    wanted: &[&motion_render::DrawLayer],
    cursors: [Option<SourceTimeSample>; LOOKAHEAD],
    mapped: bool,
    result: &mut Plans,
) -> Result<()> {
    let body = project.composition_view(&scene.composition_id)?;
    for drawn in &scene.layers {
        if !wanted.iter().any(|layer| layer.id == drawn.id) {
            continue;
        }
        let object = scene.source_object(drawn.id);
        let layer = body.layers.iter().find(|l| l.id == object).ok_or_else(|| {
            MediaError::from(motion_core::Error::Missing(object)).context(format!(
                "video prefetch plan composition={} object={object} source={} phase=resolve",
                body.id,
                drawn.video.as_ref().map_or(0, |v| v.asset)
            ))
        })?;
        if !mapped && layer.time_mapping.is_none() {
            continue;
        }
        let Some(video) = &drawn.video else {
            continue;
        };
        let asset = project
            .video_assets
            .iter()
            .find(|a| a.id == video.asset)
            .ok_or_else(|| MediaError::from("video prefetch asset missing"))?;
        let mut times = [None; LOOKAHEAD];
        for (i, cursor) in cursors.iter().enumerate() {
            if let Some(cursor) = active(*cursor, layer, body.frames) {
                let time = layer.video_picture_time_at(cursor, body.fps, asset)
                    .map_err(MediaError::from).map_err(|e| e.context(format!(
                        "video prefetch plan composition={} object={object} source={} frame={} phase=source_time",
                        body.id, asset.id, cursor.frame)))?;
                if time >= asset.video_start_us as i64 && time < asset.video_end_us as i64 {
                    times[i] = Some(time as u64);
                }
            }
        }
        result.insert(drawn.id, times);
    }
    for child in &scene.nested {
        let object = scene.source_object(child.layer);
        let layer = body
            .layers
            .iter()
            .find(|l| l.id == object)
            .ok_or(motion_core::Error::Missing(object))?;
        let Content::Composition { clip } = &layer.content else {
            return Err("video prefetch composition reference changed".into());
        };
        let child_body = project.composition_view(&child.composition)?;
        let mut next = [None; LOOKAHEAD];
        for (i, cursor) in cursors.iter().enumerate() {
            if let Some(cursor) = active(*cursor, layer, body.frames) {
                next[i] = Some(layer.source_time_sample_from(cursor)
                    .and_then(|source| source.through_composition(clip, body.fps, child_body.fps))
                    .map_err(MediaError::from).map_err(|e| e.context(format!(
                        "video prefetch plan composition={} object={object} frame={} phase=child_time", body.id, cursor.frame)))?);
            }
        }
        visit(
            project,
            &child.scene,
            wanted,
            next,
            mapped || layer.time_mapping.is_some(),
            result,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
