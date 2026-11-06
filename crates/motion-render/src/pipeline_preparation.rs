//! Owned preparation tasks. The host supplies its bounded worker; no thread is
//! created here and no task can install itself into a renderer.
use crate::{
    effect_gpu::native_pipeline::NativePipelineCompiler, effect_plan::PlanBuilder, RenderError,
    Scene,
};
use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering},
        Arc, Weak,
    },
    time::Instant,
};

pub const MAX_PIPELINE_PREPARATION_SCENES: usize = 3;
pub const MAX_PREPARED_PIPELINES: usize = 128;
pub const MAX_PREPARED_WGSL_BYTES: usize = 8 << 20;
const MAX_ALPHA_SNAPSHOT_BYTES: usize = 16 << 20;
const MAX_SCENE_NODES: usize = 256;
const MAX_SCENE_DEPTH: usize = 32;

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
pub enum PipelinePreparationError {
    #[error("pipeline preparation requires 1..={MAX_PIPELINE_PREPARATION_SCENES} sampled scenes")]
    SceneCount,
    #[error("pipeline preparation alpha snapshot requires {bytes} bytes; limit={MAX_ALPHA_SNAPSHOT_BYTES}")]
    SnapshotBudget { bytes: usize },
    #[error("pipeline preparation must execute away from its renderer owner")]
    OwnerThread,
    #[error("prepared pipelines belong to another renderer")]
    ForeignRenderer,
    #[error("pipeline preparation panicked: {reason}")]
    Panicked { reason: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelinePreparationContext {
    pub document_generation: u64,
    pub project_revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PipelinePreparationPhase {
    Queued,
    Planning,
    Compiling,
    Ready,
    Cancelled,
    Failed,
    Installed,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelinePreparationStatus {
    pub context: PipelinePreparationContext,
    pub request: u64,
    pub request_saturated: bool,
    pub phase: PipelinePreparationPhase,
    pub cancelled: bool,
    pub current_program: Option<u32>,
    pub compiled_pipelines: u64,
    pub reused_pipelines: u64,
    pub elapsed_ms: u64,
}

struct Progress {
    context: PipelinePreparationContext,
    request: u64,
    request_saturated: bool,
    created: Instant,
    phase: AtomicU8,
    cancelled: AtomicBool,
    program: AtomicU32,
    compiled: AtomicU64,
    reused: AtomicU64,
}

#[derive(Clone)]
pub struct PipelinePreparationTicket(Arc<Progress>);
impl PipelinePreparationTicket {
    pub fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::Release);
    }
    pub fn status(&self) -> PipelinePreparationStatus {
        let phase = match self.0.phase.load(Ordering::Acquire) {
            0 => PipelinePreparationPhase::Queued,
            1 => PipelinePreparationPhase::Planning,
            2 => PipelinePreparationPhase::Compiling,
            3 => PipelinePreparationPhase::Ready,
            4 => PipelinePreparationPhase::Cancelled,
            6 => PipelinePreparationPhase::Installed,
            _ => PipelinePreparationPhase::Failed,
        };
        let program = self.0.program.load(Ordering::Acquire);
        PipelinePreparationStatus {
            context: self.0.context,
            request: self.0.request,
            request_saturated: self.0.request_saturated,
            phase,
            cancelled: self.cancelled(),
            current_program: (program != u32::MAX).then_some(program),
            compiled_pipelines: self.0.compiled.load(Ordering::Acquire),
            reused_pipelines: self.0.reused.load(Ordering::Acquire),
            elapsed_ms: self
                .0
                .created
                .elapsed()
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
        }
    }
    pub fn same_request(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    fn cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }
    fn phase(&self, phase: u8) {
        self.0.phase.store(phase, Ordering::Release);
    }
}

#[derive(Default)]
pub(crate) struct PreparationState {
    current: Option<PipelinePreparationTicket>,
    // Renderer lifetime diagnostic ordinal, saturated, never used for identity.
    next_request: u64,
    pub(crate) counts: Arc<PreparationCounts>,
}
#[derive(Default)]
pub(crate) struct PreparationCounts {
    pub(crate) created: AtomicU64,
    pub(crate) nanos: AtomicU64,
    pub(crate) driver_cached: AtomicU64,
}
fn add(counter: &AtomicU64, value: u64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
        Some(n.saturating_add(value))
    });
}
impl PreparationState {
    pub(crate) fn cancel(&mut self) {
        if let Some(ticket) = self.current.take() {
            ticket.cancel();
        }
    }
    pub(crate) fn begin(
        &mut self,
        context: PipelinePreparationContext,
    ) -> PipelinePreparationTicket {
        self.cancel();
        self.next_request = self.next_request.saturating_add(1);
        let ticket = PipelinePreparationTicket(Arc::new(Progress {
            context,
            request: self.next_request,
            request_saturated: self.next_request == u64::MAX,
            created: Instant::now(),
            phase: AtomicU8::new(0),
            cancelled: AtomicBool::new(false),
            program: AtomicU32::new(u32::MAX),
            compiled: AtomicU64::new(0),
            reused: AtomicU64::new(0),
        }));
        self.current = Some(ticket.clone());
        ticket
    }
    pub(crate) fn accepts(&self, ticket: &PipelinePreparationTicket) -> bool {
        !ticket.cancelled()
            && self
                .current
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(&current.0, &ticket.0))
    }
}
impl Drop for PreparationState {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub struct PipelinePreparationJob {
    owner: Weak<()>,
    owner_thread: std::thread::ThreadId,
    ticket: PipelinePreparationTicket,
    compiler: NativePipelineCompiler,
    known: Vec<crate::effect_gpu::KnownPipeline>,
    registry: motion_effects::Registry,
    policy: crate::resource_policy::MemoryPolicy,
    precision: motion_effects::image_contract::ChannelPrecision,
    target_format: wgpu::TextureFormat,
    device_dimension: u32,
    fusion: bool,
    plane_coverage: bool,
    preview: bool,
    best_effort_lookahead: bool,
    assets: Vec<u64>,
    alpha_images: std::collections::HashMap<u64, crate::scene_generator::AlphaImage>,
    scenes: Vec<Scene>,
    width: u32,
    height: u32,
    driver_cache: bool,
    counts: Arc<PreparationCounts>,
    #[cfg(test)]
    before_plan: Option<Box<dyn FnOnce() + Send>>,
}

pub(crate) struct PreparedPipeline {
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) wgsl: Arc<str>,
    pub(crate) format: u32,
    pub(crate) sprite: bool,
    pub(crate) additive: bool,
}
pub struct PreparedEffectPipelines {
    pub(crate) entries: Vec<PreparedPipeline>,
    pub(crate) wgsl_bytes: usize,
    pub(crate) compile_nanos: u64,
    pub(crate) driver_cache: bool,
    pub(crate) target_format: wgpu::TextureFormat,
    owner: Weak<()>,
    ticket: PipelinePreparationTicket,
    limited: bool,
    installed: bool,
    lookahead_failures: Vec<PipelineLookaheadFailure>,
}
#[derive(Debug)]
pub struct PipelineLookaheadFailure {
    pub frame: f64,
    pub cause: RenderError,
}
impl PreparedEffectPipelines {
    pub fn pipeline_count(&self) -> usize {
        self.entries.len()
    }
    pub fn wgsl_bytes(&self) -> usize {
        self.wgsl_bytes
    }
    pub fn limited(&self) -> bool {
        self.limited
    }
    pub fn cancelled(&self) -> bool {
        self.ticket.cancelled()
    }
    pub fn native_compile_nanos(&self) -> u64 {
        self.compile_nanos
    }
    pub fn uses_driver_cache(&self) -> bool {
        self.driver_cache
    }
    pub fn take_lookahead_failures(&mut self) -> Vec<PipelineLookaheadFailure> {
        std::mem::take(&mut self.lookahead_failures)
    }
    pub(crate) fn mark_installed(&mut self) {
        self.installed = true;
        self.ticket.phase(6);
    }
    pub(crate) fn cancel_superseded(&self) {
        self.ticket.cancel();
        self.ticket.phase(4);
    }
    pub(crate) fn accepts(
        &self,
        owner: &Arc<()>,
        state: &PreparationState,
        context: PipelinePreparationContext,
    ) -> Result<bool, PipelinePreparationError> {
        if !self
            .owner
            .upgrade()
            .is_some_and(|id| Arc::ptr_eq(&id, owner))
        {
            return Err(PipelinePreparationError::ForeignRenderer);
        }
        Ok(state.accepts(&self.ticket) && self.ticket.0.context == context)
    }
}
impl Drop for PreparedEffectPipelines {
    fn drop(&mut self) {
        if !self.installed && self.ticket.0.phase.load(Ordering::Acquire) == 3 {
            self.cancel_superseded();
        }
    }
}
impl Drop for PipelinePreparationJob {
    fn drop(&mut self) {
        if self.ticket.0.phase.load(Ordering::Acquire) == 0 {
            self.ticket.cancel();
            self.ticket.phase(4);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipelinePreparationInstall {
    Installed { pipelines: usize, limited: bool },
    Superseded,
}

impl PipelinePreparationJob {
    pub(crate) fn new(
        renderer: &mut crate::Renderer,
        scenes: Vec<Scene>,
        width: u32,
        height: u32,
        context: PipelinePreparationContext,
    ) -> Result<Self, RenderError> {
        if scenes.is_empty() || scenes.len() > MAX_PIPELINE_PREPARATION_SCENES {
            return Err(PipelinePreparationError::SceneCount.into());
        }
        let alpha_bytes = renderer
            .effect_gpu
            .builder
            .alpha_images
            .values()
            .fold(0usize, |sum, image| sum.saturating_add(image.pixels.len()));
        if alpha_bytes > MAX_ALPHA_SNAPSHOT_BYTES {
            return Err(PipelinePreparationError::SnapshotBudget { bytes: alpha_bytes }.into());
        }
        let alpha_images = renderer
            .effect_gpu
            .builder
            .alpha_images
            .iter()
            .map(|(&id, image)| {
                (
                    id,
                    crate::scene_generator::AlphaImage {
                        width: image.width,
                        height: image.height,
                        pixels: image.pixels.clone(),
                    },
                )
            })
            .collect();
        let ticket = renderer.pipeline_preparation.begin(context);
        Ok(Self {
            owner: Arc::downgrade(&renderer.allocation_owner),
            owner_thread: std::thread::current().id(),
            ticket,
            compiler: renderer
                .effect_gpu
                .state
                .pipeline_compiler(&renderer.device),
            known: renderer.effect_gpu.state.preparation_snapshot(),
            registry: renderer.effect_gpu.builder.registry.clone(),
            policy: renderer.effect_gpu.builder.memory_policy(),
            precision: renderer.effect_gpu.builder.precision(),
            target_format: renderer.target_format,
            device_dimension: renderer.effect_gpu.builder.device_dimension,
            fusion: renderer.effect_gpu.builder.pointwise_fusion_enabled(),
            plane_coverage: renderer.effect_gpu.builder.plane_coverage_enabled(),
            preview: false,
            best_effort_lookahead: false,
            assets: renderer.asset_order.clone(),
            alpha_images,
            scenes,
            width,
            height,
            driver_cache: renderer.effect_gpu.state.driver_pipeline_cache.is_some(),
            counts: renderer.pipeline_preparation.counts.clone(),
            #[cfg(test)]
            before_plan: None,
        })
    }
    pub fn ticket(&self) -> PipelinePreparationTicket {
        self.ticket.clone()
    }
    /// Match guarded interactive preview rather than formal rendering. Both
    /// paths also prepare the strict preflight plan used before the first draw.
    pub fn for_preview(mut self) -> Self {
        self.preview = true;
        self
    }
    /// Keep the current scene authoritative. Future-scene failures are logged
    /// and retained in the result, so invalid future content cannot prevent a
    /// valid current frame from rendering. The default multi-scene API is strict.
    pub fn with_best_effort_lookahead(mut self) -> Self {
        self.best_effort_lookahead = true;
        self
    }
    /// Execute on a host-owned bounded worker. CPU plans and GPU scopes are local
    /// to this call. Cancellation cannot interrupt one blocking driver operation;
    /// it is checked before planning, between native creates, and before install.
    pub fn run(self) -> Result<PreparedEffectPipelines, RenderError> {
        self.run_with_scenes(|_| Ok(()))
    }
    /// Sample additional immutable inputs on the same bounded host worker.
    /// Planning state and Info start are published before this callback. It is
    /// subject to the same panic boundary and 1..=3 scene admission as planning.
    pub fn run_with_scenes(
        mut self,
        sample: impl FnOnce(&mut Vec<Scene>) -> Result<(), RenderError>,
    ) -> Result<PreparedEffectPipelines, RenderError> {
        if std::thread::current().id() == self.owner_thread {
            self.ticket.phase(5);
            return Err(PipelinePreparationError::OwnerThread.into());
        }
        let ticket = self.ticket.clone();
        ticket.phase(1);
        let context = ticket.0.context;
        // No observation/business mutex is held while calling the host logger.
        log::info!(target: "motion_render::pipeline_preparation",
            "pipeline preparation start request={} saturated={} generation={} revision={} phase=plan",
            ticket.0.request, ticket.0.request_saturated, context.document_generation, context.project_revision);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            #[cfg(test)]
            if let Some(before) = self.before_plan.take() {
                before();
            }
            if !self.ticket.cancelled() {
                sample(&mut self.scenes)?;
                if self.scenes.is_empty() || self.scenes.len() > MAX_PIPELINE_PREPARATION_SCENES {
                    return Err(PipelinePreparationError::SceneCount.into());
                }
            }
            self.run_inner()
        }))
        .unwrap_or_else(|payload| {
            let reason = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("non-text panic");
            Err(PipelinePreparationError::Panicked {
                reason: reason.chars().take(2048).collect(),
            }
            .into())
        });
        match &result {
            Ok(batch) => {
                ticket.phase(if batch.cancelled() { 4 } else { 3 });
                log::debug!(target: "motion_render::pipeline_preparation",
                    "pipeline preparation done request={} generation={} revision={} elapsed_ms={} pipelines={} limited={} cancelled={}",
                    ticket.0.request, context.document_generation, context.project_revision,
                    ticket.status().elapsed_ms, batch.pipeline_count(), batch.limited(), batch.cancelled());
            }
            Err(error) => {
                ticket.phase(5);
                let cause: String = error.to_string().chars().take(2048).collect();
                log::error!(target: "motion_render::pipeline_preparation",
                    "pipeline preparation failed request={} generation={} revision={} elapsed_ms={} cause={}",
                    ticket.0.request, context.document_generation, context.project_revision,
                    ticket.status().elapsed_ms, cause);
            }
        }
        result
    }
    fn run_inner(&mut self) -> Result<PreparedEffectPipelines, RenderError> {
        let mut batch = PreparedEffectPipelines {
            entries: Vec::new(),
            wgsl_bytes: 0,
            compile_nanos: 0,
            driver_cache: self.driver_cache,
            target_format: self.target_format,
            owner: self.owner.clone(),
            ticket: self.ticket.clone(),
            limited: false,
            installed: false,
            lookahead_failures: Vec::new(),
        };
        if self.owner.upgrade().is_none() {
            self.ticket.cancel();
        }
        if self.ticket.cancelled() {
            return Ok(batch);
        }
        let mut builder = PlanBuilder::new(self.registry.clone())?;
        builder.set_memory_policy(self.policy)?;
        builder.set_precision(self.precision);
        builder.set_pointwise_fusion(self.fusion);
        if self.plane_coverage {
            builder.enable_plane_coverage();
        }
        builder.device_dimension = self.device_dimension;
        // Move the one bounded alpha snapshot; never duplicate it again in run.
        builder.alpha_images = std::mem::take(&mut self.alpha_images);
        let mut nodes = 0;
        for (index, scene) in self.scenes.iter().enumerate() {
            let result = self.compile_scene(
                &mut builder,
                scene,
                self.width,
                self.height,
                0,
                &mut nodes,
                &mut batch,
            );
            if let Err(cause) = result {
                if index == 0 || !self.best_effort_lookahead {
                    return Err(cause);
                }
                let bounded: String = cause.to_string().chars().take(2048).collect();
                log::error!(target: "motion_render::pipeline_preparation",
                    "pipeline lookahead failed request={} generation={} revision={} frame={} elapsed_ms={} cause={}",
                    self.ticket.0.request, self.ticket.0.context.document_generation,
                    self.ticket.0.context.project_revision, scene.frame, self.ticket.status().elapsed_ms, bounded);
                batch.lookahead_failures.push(PipelineLookaheadFailure {
                    frame: scene.frame,
                    cause,
                });
            }
            if self.ticket.cancelled() || batch.limited {
                break;
            }
        }
        Ok(batch)
    }
    fn compile_scene(
        &self,
        builder: &mut PlanBuilder,
        scene: &Scene,
        width: u32,
        height: u32,
        depth: usize,
        nodes: &mut usize,
        batch: &mut PreparedEffectPipelines,
    ) -> Result<(), RenderError> {
        if self.ticket.cancelled() {
            return Ok(());
        }
        if depth > MAX_SCENE_DEPTH || *nodes == MAX_SCENE_NODES {
            batch.limited = true;
            return Ok(());
        }
        *nodes += 1;
        for child in &scene.nested {
            let (w, h) = crate::renderer::nested_size(scene, &child.scene, width, height);
            self.compile_scene(builder, &child.scene, w, h, depth + 1, nodes, batch)?;
            if self.ticket.cancelled() || batch.limited {
                return Ok(());
            }
        }
        self.ticket.phase(1);
        builder.build(scene, &self.assets, width, height, true)?;
        self.compile_plan(builder, batch)?;
        if self.ticket.cancelled() || batch.limited {
            return Ok(());
        }
        // The live renderer enables plane coverage when drawing. Its first
        // preflight can precede that transition and omit a required utility
        // pass; prepare both actual plans, never the entire effect library.
        builder.enable_plane_coverage();
        if self.preview {
            builder.build_preview_guarded(scene, &self.assets, width, height, true)?;
        } else {
            builder.build(scene, &self.assets, width, height, true)?;
        }
        self.compile_plan(builder, batch)
    }
    fn compile_plan(
        &self,
        builder: &PlanBuilder,
        batch: &mut PreparedEffectPipelines,
    ) -> Result<(), RenderError> {
        let mut seen = HashSet::new();
        for (index, pass) in builder.frame.passes.iter().enumerate() {
            if self.ticket.cancelled() {
                return Ok(());
            }
            let binding = builder
                .frame
                .scratch_bindings
                .get(index)
                .ok_or("missing prewarm scratch binding")?;
            let target = builder
                .frame
                .scratch_allocations
                .get(binding.output as usize)
                .ok_or("missing prewarm scratch descriptor")?
                .class
                .format;
            if !seen.insert((pass.program, target as u32)) {
                continue;
            }
            let program = builder
                .programs
                .get(pass.program as usize)
                .ok_or("missing prewarm program")?;
            if batch.entries.iter().any(|entry| {
                entry.format == target as u32
                    && entry.wgsl.as_ref() == program.shader.wgsl
                    && entry.sprite == program.shader.sprite
                    && entry.additive == program.shader.additive
            }) {
                continue;
            }
            if batch.entries.len() == MAX_PREPARED_PIPELINES
                || program.shader.wgsl.len() > MAX_PREPARED_WGSL_BYTES - batch.wgsl_bytes
            {
                batch.limited = true;
                return Ok(());
            }
            if let Some(known) = self
                .known
                .iter()
                .find(|entry| entry.matches(&program.shader, target as u32))
            {
                batch.wgsl_bytes += program.shader.wgsl.len();
                batch.entries.push(known.prepared());
                add(&self.ticket.0.reused, 1);
                continue;
            }
            self.ticket.0.program.store(pass.program, Ordering::Release);
            self.ticket.phase(2);
            let key: String = program.key.chars().take(128).collect();
            log::info!(target: "motion_render::pipeline_preparation",
                "pipeline preparation start request={} generation={} revision={} phase=gpu program={} key={} format={:?}",
                self.ticket.0.request, self.ticket.0.context.document_generation,
                self.ticket.0.context.project_revision, pass.program, key, target);
            let began = Instant::now();
            let pipeline = self.compiler.compile(program, target.native(), None)?;
            let nanos = began.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
            batch.compile_nanos = batch.compile_nanos.saturating_add(nanos);
            add(&self.counts.created, 1);
            add(&self.counts.nanos, nanos);
            if self.driver_cache {
                add(&self.counts.driver_cached, 1);
            }
            if self.owner.upgrade().is_none() {
                self.ticket.cancel();
            }
            batch.wgsl_bytes += program.shader.wgsl.len();
            batch.entries.push(PreparedPipeline {
                pipeline,
                wgsl: Arc::from(program.shader.wgsl.as_str()),
                format: target as u32,
                sprite: program.shader.sprite,
                additive: program.shader.additive,
            });
            add(&self.ticket.0.compiled, 1);
        }
        Ok(())
    }
}
