//! Persistent GPU exposure progress. Media readiness is supplied by the host;
//! pending polls never restart evaluation, drop completed receivers, or advance
//! the exposure sum. Dropping the task cancels its owned GPU result.
use super::*;
use crate::exposure_scene::{ExposureSceneStep, ExposureScenes};
use motion_model::FrameEvaluator;
use std::time::Duration;

const MAX_WORK_UNITS_PER_POLL: usize = motion_model::MAX_LAYERS;
const RETAINED_CAUSE_CHARS: usize = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExposureStage {
    Receiver,
    Center,
    Complete,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExposureProgress {
    pub stage: ExposureStage,
    pub phase: usize,
    pub phases: usize,
    pub receiver: Option<u64>,
    pub sample_frame: Option<f64>,
}
#[derive(Clone, Copy, Debug)]
pub enum ExposurePoll {
    /// Either media is pending or the finite work/time slice has elapsed.
    /// Neither case is a fault; only Ready exposes a complete frame.
    Pending(ExposureProgress),
    Ready(RenderStats),
}

struct Failure {
    message: String,
    rejection: Option<ResourceRejection>,
}
impl Failure {
    fn error(&self) -> RenderError {
        match self.rejection {
            Some(rejection) => RenderError::Resource {
                rejection,
                message: self.message.clone(),
            },
            None => RenderError::Invalid(self.message.clone()),
        }
    }
}

pub struct ExposureFrame<E> {
    scenes: ExposureScenes<E>,
    target: ExposureTarget,
    rasters: Vec<LayerRaster>,
    stats: RenderStats,
    progress: ExposureProgress,
    failure: Option<Failure>,
}
impl<E: FrameEvaluator> ExposureFrame<E> {
    pub fn new(
        renderer: &mut Renderer,
        mut scenes: ExposureScenes<E>,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        if !scenes.at_start() {
            return Err(RenderError::Invalid(
                "exposure phase=task_init: scene schedule already advanced or failed".into(),
            ));
        }
        check_nested_project_sampling(scenes.project())?;
        scenes
            .center_scene()
            .map_err(|error| RenderError::Invalid(error.to_string()))?;
        let weights: Vec<_> = scenes.phase_weights().collect();
        let target = renderer.exposure_target(width, height, &weights)?;
        let progress = ExposureProgress {
            stage: ExposureStage::Receiver,
            phase: 0,
            phases: scenes.phase_count(),
            receiver: None,
            sample_frame: None,
        };
        Ok(Self {
            scenes,
            target,
            rasters: vec![],
            stats: RenderStats::default(),
            progress,
            failure: None,
        })
    }
    pub fn progress(&self) -> ExposureProgress {
        self.progress
    }
    pub fn target(&self) -> &ExposureTarget {
        &self.target
    }

    /// The callback must prepare assets and exact video samples for this scene,
    /// submit all GPU conversions, and return false while media is pending.
    /// A supplied time slice bounds work between GPU operations; it cannot
    /// preempt a synchronous evaluator, driver or callback that never returns.
    pub fn poll(
        &mut self,
        renderer: &mut Renderer,
        time_slice: Duration,
        mut prepare: impl FnMut(&mut Renderer, &Scene) -> Result<bool>,
    ) -> Result<ExposurePoll> {
        if let Some(failure) = &self.failure {
            return Err(failure.error());
        }
        renderer.check_exposure(&self.target, "task_poll")?;
        if time_slice.is_zero() {
            return Err(RenderError::Invalid(
                "exposure phase=task_poll: zero time slice".into(),
            ));
        }
        if self.progress.stage == ExposureStage::Complete {
            return Ok(ExposurePoll::Ready(self.stats));
        }
        let started = Instant::now();
        let result = self.poll_inner(renderer, time_slice, &mut prepare);
        self.stats.cpu_prepare_us = self
            .stats
            .cpu_prepare_us
            .saturating_add(started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64);
        result.map(|state| match state {
            ExposurePoll::Ready(_) => ExposurePoll::Ready(self.stats),
            pending => pending,
        }).map_err(|error| {
            let context = format!("exposure composition={} phase=task_{:?} sample={}/{} object={:?} sample_frame={:?}: {error}",
                self.scenes.project().composition_id, self.progress.stage, self.progress.phase,
                self.progress.phases, self.progress.receiver, self.progress.sample_frame);
            // Retain a bounded cause until this task is dropped. The first
            // returned error preserves the complete original cause/refusal.
            self.failure = Some(Failure { message: context.chars().take(RETAINED_CAUSE_CHARS).collect(),
                rejection: error.resource_rejection().copied() });
            self.progress.stage = ExposureStage::Failed;
            self.rasters.clear();
            error.with_diagnostic(context)
        })
    }
    fn poll_inner(
        &mut self,
        renderer: &mut Renderer,
        time_slice: Duration,
        prepare: &mut impl FnMut(&mut Renderer, &Scene) -> Result<bool>,
    ) -> Result<ExposurePoll> {
        let started = Instant::now();
        for unit in 0..MAX_WORK_UNITS_PER_POLL {
            if unit > 0 && started.elapsed() >= time_slice {
                return Ok(ExposurePoll::Pending(self.progress));
            }
            let receiver = self.scenes.receiver();
            self.progress.phase = self.scenes.phase();
            self.progress.receiver = receiver.map(|r| r.0);
            self.progress.sample_frame = receiver.map(|r| r.1);
            self.progress.stage = if receiver.is_some() {
                ExposureStage::Receiver
            } else {
                ExposureStage::Center
            };
            let preview = self.scenes.preview();
            match self
                .scenes
                .current()
                .map_err(|error| RenderError::Invalid(error.to_string()))?
            {
                ExposureSceneStep::Receiver(receiver) => {
                    if receiver.layer().is_some_and(|layer| !layer.adjustment) {
                        if let Some(scene) = receiver.scene {
                            check_nested_sampling(scene)?;
                            if !prepare(renderer, scene)? {
                                return Ok(ExposurePoll::Pending(self.progress));
                            }
                            if let Some((raster, stats)) = renderer.materialize_layer(
                                scene,
                                receiver.object,
                                self.target.size()[0],
                                self.target.size()[1],
                                preview,
                            )? {
                                self.rasters.push(raster);
                                self.add_stats(stats);
                            }
                        }
                    }
                    self.scenes
                        .finish_receiver()
                        .map_err(|error| RenderError::Invalid(error.to_string()))?;
                }
                ExposureSceneStep::PhaseReady { index, .. } => {
                    if index != self.target.phase_index() {
                        return Err(RenderError::Invalid(
                            "exposure phase=task_accumulate: progress mismatch".into(),
                        ));
                    }
                    let center = self
                        .scenes
                        .center_scene()
                        .map_err(|error| RenderError::Invalid(error.to_string()))?;
                    check_nested_sampling(center)?;
                    if center.layers.iter().any(|layer| layer.adjustment)
                        && !prepare(renderer, center)?
                    {
                        return Ok(ExposurePoll::Pending(self.progress));
                    }
                    let refs: Vec<_> = self.rasters.iter().collect();
                    let stats = renderer.accumulate_exposure_phase(
                        &mut self.target,
                        center,
                        &refs,
                        preview,
                    )?;
                    self.add_stats(stats);
                    self.rasters.clear();
                    self.scenes
                        .finish_phase()
                        .map_err(|error| RenderError::Invalid(error.to_string()))?;
                }
                ExposureSceneStep::Complete => {
                    if !self.target.complete() {
                        return Err(RenderError::Invalid(
                            "exposure phase=task_complete: incomplete sum".into(),
                        ));
                    }
                    self.progress.stage = ExposureStage::Complete;
                    return Ok(ExposurePoll::Ready(self.stats));
                }
            }
        }
        Ok(ExposurePoll::Pending(self.progress))
    }
    fn add_stats(&mut self, stats: RenderStats) {
        // Work counters saturate within this one frame task; resource/particle
        // fields are peaks across contexts, not duplicated logical scene counts.
        self.stats.draw_calls = self.stats.draw_calls.saturating_add(stats.draw_calls);
        self.stats.parameter_upload_bytes = self
            .stats
            .parameter_upload_bytes
            .saturating_add(stats.parameter_upload_bytes);
        self.stats.parameter_resource_upload_bytes = self
            .stats
            .parameter_resource_upload_bytes
            .saturating_add(stats.parameter_resource_upload_bytes);
        self.stats.instance_upload_bytes = self
            .stats
            .instance_upload_bytes
            .saturating_add(stats.instance_upload_bytes);
        self.stats.texture_bytes = self.stats.texture_bytes.max(stats.texture_bytes);
        self.stats.particles_alive = self.stats.particles_alive.max(stats.particles_alive);
        self.stats.particles_visible = self.stats.particles_visible.max(stats.particles_visible);
        self.stats.particles_culled = self.stats.particles_culled.max(stats.particles_culled);
    }
}

// Ordinary nested scenes are already evaluated at the receiving clock. A
// child's independent shutter requires recursive exposure composition, which
// remains pending; never silently render such a child as an ordinary image.
fn check_nested_sampling(scene: &Scene) -> Result<()> {
    for nested in &scene.nested {
        if let Some(layer) = nested
            .scene
            .layers
            .iter()
            .find(|layer| layer.sampling.motion_blur.is_some())
        {
            return Err(RenderError::Invalid(format!("exposure composition={} object={} phase=nested_sampling: independent nested exposure is not connected", nested.composition, layer.id)));
        }
        check_nested_sampling(&nested.scene)?;
    }
    Ok(())
}

fn check_nested_project_sampling(project: &motion_model::Project) -> Result<()> {
    let reachable = project
        .reachable_compositions()
        .map_err(|error| RenderError::Invalid(error.to_string()))?;
    for composition in &project.compositions {
        if composition.id != project.composition_id && reachable.contains(&composition.id) {
            if let Some(layer) = composition
                .layers
                .iter()
                .find(|layer| layer.sampling.motion_blur.is_some())
            {
                return Err(RenderError::Invalid(format!("exposure composition={} object={} phase=nested_sampling: independent nested exposure is not connected", composition.id, layer.id)));
            }
        }
    }
    Ok(())
}
