//! Upcoming clips borrow only otherwise idle decoder slots. A speculative
//! failure never replaces the current frame, and is retained for its eventual
//! mandatory request instead of reopening/retrying the failed source each tick.
use super::*;

pub(super) struct Failure {
    pub asset: VideoAsset,
    pub error: MediaError,
}
impl VideoFrames {
    pub(super) fn upcoming<'a>(
        &mut self,
        project: &'a Project,
        scene: &Scene,
        required: &[u64],
    ) -> Vec<motion_render::UpcomingVideo<'a>> {
        let capacity = MAX_DECODER_WORKERS.saturating_sub(required.len());
        if capacity == 0 {
            self.warmup_planning.success();
            return Vec::new();
        }
        let visible: Vec<_> = scene.video_layers().iter().map(|layer| layer.id).collect();
        match self.warmup.plan(
            project,
            &scene.composition_id,
            scene.frame,
            LOOKAHEAD,
            &visible,
            capacity,
        ) {
            Ok(plan) => {
                self.warmup_planning.success();
                plan
            }
            Err(error) => {
                let composition: String = scene.composition_id.chars().take(64).collect();
                let reason: String = error.to_string().chars().take(2048).collect();
                self.warmup_planning.failure(
                    "video_warmup_plan",
                    format_args!(
                        "composition={composition} frame={} phase=plan reason={reason}",
                        scene.frame
                    ),
                );
                Vec::new()
            }
        }
    }
    pub(super) fn warm_up(&mut self, root: &Path, upcoming: &[motion_render::UpcomingVideo<'_>]) {
        for candidate in upcoming {
            let asset = candidate.asset;
            if let Some(failure) = self.warmup_errors.get(&candidate.object) {
                if failure.asset == *asset {
                    continue;
                }
                self.warmup_errors.remove(&candidate.object);
            }
            if !self.streams.contains_key(&candidate.object)
                && self.streams.len() + self.retiring.len() >= MAX_DECODER_WORKERS
            {
                continue;
            }
            let started = Instant::now();
            let opening = self
                .streams
                .get(&candidate.object)
                .is_none_or(|stream| stream.asset != *asset);
            if opening {
                log::info!(
                    "video warmup start object={} source={} pts_us={} phase=prepare",
                    candidate.object,
                    asset.id,
                    candidate.source_time_us
                );
            }
            let outcome = (|| {
                let generation =
                    self.generation(asset, candidate.object, candidate.source_time_us)?;
                // Decode only the first required picture/pair. Existing active
                // instance lookahead takes over when this layer is sampled.
                self.request_source(
                    root,
                    asset,
                    candidate.object,
                    candidate.source_time_us,
                    generation,
                    false,
                    candidate.blend,
                    None,
                )
            })();
            if opening {
                log::debug!(
                    "video warmup dispatch object={} source={} {}ms phase=request accepted={}",
                    candidate.object,
                    asset.id,
                    started.elapsed().as_millis(),
                    outcome.is_ok()
                );
            }
            if let Err(error) = outcome {
                let error = error.context(format!(
                    "video warmup object={} source={} pts_us={} phase=prepare",
                    candidate.object, asset.id, candidate.source_time_us
                ));
                let reason: String = error.to_string().chars().take(2048).collect();
                log::error!(
                    "video warmup failed object={} source={} pts_us={} {}ms reason={reason}",
                    candidate.object,
                    asset.id,
                    candidate.source_time_us,
                    started.elapsed().as_millis()
                );
                self.warmup_errors.insert(
                    candidate.object,
                    Failure {
                        asset: asset.clone(),
                        error,
                    },
                );
            }
        }
    }
}
