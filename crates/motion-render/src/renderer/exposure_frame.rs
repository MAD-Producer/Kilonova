//! Persistent GPU exposure progress. Media readiness is supplied by the host;
//! pending polls never restart evaluation, drop completed receivers, or advance
//! the exposure sum. Dropping the task cancels its owned GPU result.
use super::nested_exposure::NestedPreparation;
use super::*;
use crate::exposure_scene::{ExposureDocument, ExposureScope};
use crate::exposure_scene::{ExposureSceneStep, ExposureScenes};
use motion_model::FrameEvaluator;
use std::{sync::Arc, time::Duration};

const MAX_WORK_UNITS_PER_POLL: usize = motion_model::MAX_LAYERS;
const RETAINED_CAUSE_CHARS: usize = 2048;

/// One top-level poll budget, shared by every recursive child. Traversing a
/// pending parent does not spend the first unit; even a tiny positive slice
/// must eventually reach the deepest actionable operation.
pub(super) struct PollBudget {
    started: Instant,
    slice: Duration,
    remaining: usize,
}
impl PollBudget {
    pub fn available(&self) -> bool {
        self.remaining > 0
            && (self.remaining == MAX_WORK_UNITS_PER_POLL || self.started.elapsed() < self.slice)
    }
    pub fn take(&mut self) -> bool {
        if !self.available() {
            return false;
        }
        self.remaining -= 1;
        true
    }
}

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
    /// Completed local/nested work in this frame; stable while media waits.
    pub work_units: u64,
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
    nested: Option<NestedPreparation<E>>,
    factory: (ExposureDocument, Arc<E>, ExposureScope),
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
        let factory = scenes.child_factory();
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
            work_units: 0,
        };
        Ok(Self {
            scenes,
            target,
            rasters: vec![],
            stats: RenderStats::default(),
            progress,
            failure: None,
            nested: None,
            factory,
        })
    }
    pub fn progress(&self) -> ExposureProgress {
        self.progress
    }
    pub fn target(&self) -> &ExposureTarget {
        &self.target
    }
    /// Include every task-owned output while a nested receiver is pending.
    /// Metadata only; imported source images are owned by the renderer instead.
    pub fn texture_resource_ledger(&self) -> crate::TextureResourceLedger {
        let mut ledger = self.target.texture_resource_ledger();
        for raster in &self.rasters {
            ledger.merge(raster.texture_resource_ledger());
        }
        if let Some(nested) = &self.nested {
            ledger.merge(nested.texture_resource_ledger());
        }
        ledger
    }

    /// The callback must prepare assets and exact video samples for this scene,
    /// submit all GPU conversions, and return false while media is pending.
    /// Each scene is one local composition scope with stable document-instance
    /// IDs. Nested outputs are prepared separately before its callback runs.
    /// A shared time/unit slice bounds receiver and resolve batches; it cannot
    /// preempt a synchronous evaluator, driver or callback that never returns.
    pub fn poll(
        &mut self,
        renderer: &mut Renderer,
        time_slice: Duration,
        mut prepare: impl FnMut(&mut Renderer, &Scene) -> Result<bool>,
    ) -> Result<ExposurePoll> {
        self.poll_with(renderer, time_slice, &mut prepare)
    }
    pub(super) fn poll_with(
        &mut self,
        renderer: &mut Renderer,
        time_slice: Duration,
        prepare: &mut dyn FnMut(&mut Renderer, &Scene) -> Result<bool>,
    ) -> Result<ExposurePoll> {
        let mut budget = PollBudget {
            started: Instant::now(),
            slice: time_slice,
            remaining: MAX_WORK_UNITS_PER_POLL,
        };
        self.poll_budget(renderer, &mut budget, prepare)
    }
    pub(super) fn poll_budget(
        &mut self,
        renderer: &mut Renderer,
        budget: &mut PollBudget,
        prepare: &mut dyn FnMut(&mut Renderer, &Scene) -> Result<bool>,
    ) -> Result<ExposurePoll> {
        if let Some(failure) = &self.failure {
            return Err(failure.error());
        }
        renderer.check_exposure(&self.target, "task_poll")?;
        if budget.slice.is_zero() {
            return Err(RenderError::Invalid(
                "exposure phase=task_poll: zero time slice".into(),
            ));
        }
        if self.progress.stage == ExposureStage::Complete {
            return Ok(ExposurePoll::Ready(self.stats));
        }
        let started = Instant::now();
        let result = self.poll_inner(renderer, budget, prepare);
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
            self.nested = None;
            error.with_diagnostic(context)
        })
    }
    fn poll_inner(
        &mut self,
        renderer: &mut Renderer,
        budget: &mut PollBudget,
        prepare: &mut dyn FnMut(&mut Renderer, &Scene) -> Result<bool>,
    ) -> Result<ExposurePoll> {
        for _ in 0..MAX_WORK_UNITS_PER_POLL {
            if !budget.available() {
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
                            if !prepare_context(
                                renderer,
                                &mut self.nested,
                                &self.factory,
                                scene,
                                self.target.size(),
                                preview,
                                budget,
                                &mut self.progress,
                                prepare,
                            )? {
                                return Ok(ExposurePoll::Pending(self.progress));
                            }
                            let context =
                                self.nested.as_ref().map_or(scene, |nested| &nested.context);
                            if let Some((raster, stats)) = renderer.materialize_layer(
                                context,
                                receiver.object,
                                self.target.size()[0],
                                self.target.size()[1],
                                preview,
                            )? {
                                self.rasters.push(raster);
                                self.add_stats(stats);
                            }
                        }
                    } else if !budget.take() {
                        return Ok(ExposurePoll::Pending(self.progress));
                    }
                    if let Some(nested) = self.nested.take() {
                        self.add_stats(nested.stats);
                    }
                    self.progress.work_units = self.progress.work_units.saturating_add(1);
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
                    if center.layers.iter().any(|layer| layer.adjustment)
                        && !prepare_context(
                            renderer,
                            &mut self.nested,
                            &self.factory,
                            center,
                            self.target.size(),
                            preview,
                            budget,
                            &mut self.progress,
                            prepare,
                        )?
                    {
                        return Ok(ExposurePoll::Pending(self.progress));
                    }
                    if !center.layers.iter().any(|layer| layer.adjustment) && !budget.take() {
                        return Ok(ExposurePoll::Pending(self.progress));
                    }
                    let center = self
                        .nested
                        .as_ref()
                        .map_or(center, |nested| &nested.context);
                    let refs: Vec<_> = self.rasters.iter().collect();
                    let stats = renderer.accumulate_exposure_phase(
                        &mut self.target,
                        center,
                        &refs,
                        preview,
                    )?;
                    self.add_stats(stats);
                    if let Some(nested) = self.nested.take() {
                        self.add_stats(nested.stats);
                    }
                    self.rasters.clear();
                    self.progress.work_units = self.progress.work_units.saturating_add(1);
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
        add_stats(&mut self.stats, stats);
    }
}

fn prepare_context<E: FrameEvaluator>(
    renderer: &mut Renderer,
    nested: &mut Option<NestedPreparation<E>>,
    factory: &(ExposureDocument, Arc<E>, ExposureScope),
    scene: &Scene,
    size: [u32; 2],
    preview: bool,
    budget: &mut PollBudget,
    progress: &mut ExposureProgress,
    prepare: &mut dyn FnMut(&mut Renderer, &Scene) -> Result<bool>,
) -> Result<bool> {
    if scene.nested.is_empty() {
        if !budget.take() {
            return Ok(false);
        }
        return prepare(renderer, scene);
    }
    let nested = nested.get_or_insert_with(|| NestedPreparation::new(scene, size));
    if !nested.poll(renderer, factory, preview, budget, progress, prepare)? {
        return Ok(false);
    }
    if !budget.take() {
        return Ok(false);
    }
    if !prepare(renderer, &nested.context)? {
        return Ok(false);
    }
    nested.install(renderer)?;
    Ok(true)
}

pub(super) fn add_stats(total: &mut RenderStats, stats: RenderStats) {
    // Work counters saturate within this one frame task; resource/particle
    // fields are peaks across contexts, not duplicated logical scene counts.
    total.draw_calls = total.draw_calls.saturating_add(stats.draw_calls);
    total.parameter_upload_bytes = total
        .parameter_upload_bytes
        .saturating_add(stats.parameter_upload_bytes);
    total.parameter_resource_upload_bytes = total
        .parameter_resource_upload_bytes
        .saturating_add(stats.parameter_resource_upload_bytes);
    total.instance_upload_bytes = total
        .instance_upload_bytes
        .saturating_add(stats.instance_upload_bytes);
    total.texture_bytes = total.texture_bytes.max(stats.texture_bytes);
    total.particles_alive = total.particles_alive.max(stats.particles_alive);
    total.particles_visible = total.particles_visible.max(stats.particles_visible);
    total.particles_culled = total.particles_culled.max(stats.particles_culled);
}
