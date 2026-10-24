//! Opt-in command-encoder intervals. These include marker-pass overhead and
//! possible queue/backend scheduling, not exclusive GPU busy time. Queue writes
//! executed before an encoder are outside these intervals. Never add them to
//! overlapping root-pass timings. Hosts must explicitly attach this observer;
//! constructing a profiler does not instrument other encoders or raw submits.
#[cfg(all(debug_assertions, feature = "gpu-tests"))]
mod diagnostic;
mod logging;
mod records;
use crate::{
    allocation_buffer::AllocationBuffer, readback::TimestampReadbackCounters, AllocationContext,
    AllocationEncoder, RenderError, Renderer, TimestampReadbackInfo,
};
pub use logging::GpuCommandLogScope;
pub use records::{
    CommandContext, CommandCounts, CommandIdentity, CommandPhase, CommandRecord, CommandSnapshot,
    CommandStage, CommandTraceSnapshot, ExposureCommandContext, ExposureCommandScope,
    InputCommandContext, MAX_ACTIVE_GPU_COMMANDS, MAX_GPU_COMMAND_CONTEXTS, MAX_GPU_TRACE_COMMANDS,
    MAX_GPU_TRACE_METADATA_BYTES, MAX_RECENT_GPU_COMMANDS,
};
use records::{CommandRecords, CommandTicket};
use serde::Serialize;
use std::{
    ops::{Deref, DerefMut},
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc,
    },
};

const FREE: u8 = 0;
const ENCODING: u8 = 1;
const UNCONFIRMED: u8 = 2;
const MAPPING: u8 = 3;
const WORD_BYTES: u64 = 16;

struct Slot {
    queries: wgpu::QuerySet,
    resolve: AllocationBuffer,
    mapped: AllocationBuffer,
    state: Arc<AtomicU8>,
    cancelled: AtomicBool,
}
struct Pool {
    slots: Vec<Arc<Slot>>,
    records: Arc<CommandRecords>,
    readback: Arc<TimestampReadbackCounters>,
    period_ns: f64,
    queue: wgpu::Queue,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuCommandSnapshot {
    pub commands: CommandSnapshot,
    pub readback: TimestampReadbackInfo,
    pub capacity: usize,
    /// A submission attempt that unwound is not safe to recycle. This capacity
    /// stays quarantined until this entire pool is retired.
    pub quarantined_slots: usize,
}

/// A read-only handle usable by a watchdog while the owner is inside encoding.
/// Counts are cumulative for this profiler epoch and never reset by success.
/// The metadata and readback atomics are not one atomic cross-counter snapshot.
#[derive(Clone)]
pub struct GpuCommandObserver {
    records: Arc<CommandRecords>,
    readback: Arc<TimestampReadbackCounters>,
    states: Vec<Arc<AtomicU8>>,
}
impl GpuCommandObserver {
    /// Explicitly start a bounded recording window. Replaces only the previous
    /// metadata trace, never the profiler, native pool or command identities.
    /// Commands already encoding/mapping are excluded, including late callbacks.
    pub fn start_trace(&self) -> Result<u64, RenderError> {
        self.records.start_trace()
    }
    /// Close the creation window without waiting for GPU work. Existing commands
    /// retain their original window until their actual callbacks arrive.
    pub fn stop_trace(&self) -> Option<u64> {
        self.records.stop_trace()
    }
    /// Separate from routine snapshots so polling does not clone the full trace.
    /// Completion-order records retain original words; null/omissions are not zero.
    pub fn trace_snapshot(&self) -> Option<CommandTraceSnapshot> {
        self.records.trace_snapshot()
    }
    pub fn snapshot(&self) -> GpuCommandSnapshot {
        GpuCommandSnapshot {
            commands: self.records.snapshot(),
            readback: self.readback.info(),
            capacity: self.states.len(),
            quarantined_slots: self
                .states
                .iter()
                .filter(|state| state.load(Ordering::Acquire) == UNCONFIRMED)
                .count(),
        }
    }
}

/// Explicit profiling allocation, independent of the ordinary root GpuTimer.
/// Default rendering allocates no command query pool and inserts no markers.
/// A new profiler gets a new epoch; late callbacks retain the original epoch.
pub struct GpuCommandProfiler {
    context: AllocationContext,
    pool: Arc<Pool>,
}
impl GpuCommandProfiler {
    /// Unsupported standard timestamp queries/compute markers return None.
    /// Admission failure preserves the actual typed budget error. Slots are
    /// bounded; busy observations are omitted without blocking/retrying work.
    pub fn new(renderer: &Renderer, capacity: usize) -> Result<Option<Self>, RenderError> {
        let pool = Pool::new(
            &renderer.device,
            &renderer.queue,
            renderer.allocation_budget(),
            capacity,
        )?;
        Ok(pool.map(|pool| Self {
            context: renderer.allocation_context(),
            pool,
        }))
    }
    pub fn observer(&self) -> GpuCommandObserver {
        self.pool.observer()
    }
    pub(crate) fn into_attached(self) -> AttachedCommandProfiler {
        AttachedCommandProfiler(self.pool)
    }

    /// Publishes Encoding before native encoder creation. Ownership checks and
    /// ordinary allocation-retirement semantics remain those of the renderer.
    pub fn encoder(
        &self,
        renderer: &Renderer,
        descriptor: &wgpu::CommandEncoderDescriptor<'_>,
        stage: CommandStage,
        contexts: Vec<CommandContext>,
    ) -> Result<AllocationEncoder, RenderError> {
        if !self.context.same_owner(&renderer.allocation_context()) {
            return Err(RenderError::Invalid(
                "GPU command profiler belongs to another renderer".into(),
            ));
        }
        let span = self.pool.begin(stage, contexts);
        let mut encoder = renderer.unobserved_command_encoder(descriptor)?;
        encoder.observe_commands(span);
        Ok(encoder)
    }
}

// The external profiler retains AllocationContext's owning-thread contract.
// Renderer attachment moves only the pool, preserving Renderer/Session's Send
// layout and their existing explicit owner-thread checks. No native owner is
// merged with another renderer or PreviewInput merely to attach observations.
pub(crate) struct AttachedCommandProfiler(Arc<Pool>);
impl AttachedCommandProfiler {
    pub(crate) fn for_input(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        budget: &crate::AllocationBudget,
        capacity: usize,
    ) -> Result<Option<Self>, RenderError> {
        Ok(Pool::new(device, queue, budget, capacity)?.map(Self))
    }
    pub(crate) fn exposure_scope(&self, context: ExposureCommandContext) -> ExposureCommandScope {
        self.0.records.exposure_scope(context)
    }
    pub(crate) fn observer(&self) -> GpuCommandObserver {
        self.0.observer()
    }
    pub(crate) fn encoder(
        &self,
        renderer: &Renderer,
        descriptor: &wgpu::CommandEncoderDescriptor<'_>,
        stage: CommandStage,
    ) -> Result<AllocationEncoder, RenderError> {
        let span = self
            .0
            .begin(stage, renderer.allocation_budget().gpu_command_contexts());
        let mut encoder = renderer.unobserved_command_encoder(descriptor)?;
        encoder.observe_commands(span);
        Ok(encoder)
    }
    pub(crate) fn allocated_encoder(
        &self,
        device: &wgpu::Device,
        budget: &crate::AllocationBudget,
        owner: Arc<()>,
        descriptor: &wgpu::CommandEncoderDescriptor<'_>,
        stage: CommandStage,
        input: Option<InputCommandContext>,
    ) -> Result<AllocationEncoder, RenderError> {
        let span = self
            .0
            .begin_with_input(stage, budget.gpu_command_contexts(), input);
        let mut encoder = budget.encoder(device, owner, descriptor)?;
        encoder.observe_commands(span);
        Ok(encoder)
    }
    pub(crate) fn raw_encoder(
        &self,
        renderer: &Renderer,
        descriptor: &wgpu::CommandEncoderDescriptor<'_>,
        stage: CommandStage,
    ) -> ObservedEncoder {
        let span = self
            .0
            .begin(stage, renderer.allocation_budget().gpu_command_contexts());
        let mut encoder = ObservedEncoder::new(renderer.device.create_command_encoder(descriptor));
        encoder.observe(span);
        encoder
    }
}
impl Drop for AttachedCommandProfiler {
    fn drop(&mut self) {
        // Ending the owner epoch does not cancel GPU commands or unmap buffers.
        // A late failed callback after explicit retirement is normal cancellation;
        // exact bytes, original cause and old identity remain in its metadata.
        for slot in &self.0.slots {
            if slot.state.load(Ordering::Acquire) == MAPPING {
                slot.cancelled.store(true, Ordering::Release);
            }
        }
    }
}
impl Pool {
    // One pool belongs to exactly one device/queue owner. Shared admission is
    // not permission to reuse query sets or mapping buffers across devices.
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        budget: &crate::AllocationBudget,
        capacity: usize,
    ) -> Result<Option<Arc<Self>>, RenderError> {
        if !(1..=MAX_ACTIVE_GPU_COMMANDS).contains(&capacity) {
            return Err(RenderError::Invalid(
                "GPU command slots must be 1..128".into(),
            ));
        }
        let period_ns = f64::from(queue.get_timestamp_period());
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY)
            || device.limits().max_compute_workgroups_per_dimension == 0
            || !period_ns.is_finite()
            || period_ns <= 0.0
        {
            return Ok(None);
        }
        let records = CommandRecords::new()
            .ok_or_else(|| RenderError::Invalid("GPU command epoch identity exhausted".into()))?;
        let slots = (0..capacity)
            .map(|_| {
                // Complete fallible reservations before creating/publishing queries.
                let resolve = AllocationBuffer::new(
                    device,
                    budget,
                    &wgpu::BufferDescriptor {
                        label: Some("Motion Studio command timestamp resolve"),
                        size: wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT,
                        usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                        mapped_at_creation: false,
                    },
                )?;
                let mapped = AllocationBuffer::new(
                    device,
                    budget,
                    &wgpu::BufferDescriptor {
                        label: Some("Motion Studio command timestamp words"),
                        size: WORD_BYTES,
                        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                        mapped_at_creation: false,
                    },
                )?;
                Ok(Arc::new(Slot {
                    queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                        label: Some("Motion Studio command interval"),
                        ty: wgpu::QueryType::Timestamp,
                        count: 2,
                    }),
                    resolve,
                    mapped,
                    state: Arc::new(AtomicU8::new(FREE)),
                    cancelled: AtomicBool::new(false),
                }))
            })
            .collect::<Result<Vec<_>, RenderError>>()?;
        Ok(Some(Arc::new(Self {
            slots,
            records,
            readback: Arc::default(),
            period_ns,
            queue: queue.clone(),
        })))
    }
    fn observer(&self) -> GpuCommandObserver {
        GpuCommandObserver {
            records: self.records.clone(),
            readback: self.readback.clone(),
            states: self.slots.iter().map(|slot| slot.state.clone()).collect(),
        }
    }

    fn begin(&self, stage: CommandStage, contexts: Vec<CommandContext>) -> Option<Span> {
        self.begin_with_input(stage, contexts, None)
    }
    fn begin_with_input(
        &self,
        stage: CommandStage,
        contexts: Vec<CommandContext>,
        input: Option<InputCommandContext>,
    ) -> Option<Span> {
        let Some(slot) = self
            .slots
            .iter()
            .find(|slot| {
                slot.state
                    .compare_exchange(FREE, ENCODING, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            })
            .cloned()
        else {
            self.records.omit_slot();
            return None;
        };
        let Some(ticket) = self.records.begin_with_input(stage, contexts, input) else {
            slot.state.store(FREE, Ordering::Release);
            return None;
        };
        slot.cancelled.store(false, Ordering::Release);
        Some(Span {
            slot,
            ticket: Some(ticket),
            readback: self.readback.clone(),
            period_ns: self.period_ns,
            queue: self.queue.clone(),
        })
    }
}
fn boundary(encoder: &mut wgpu::CommandEncoder, queries: &wgpu::QuerySet, first: bool) {
    drop(encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some("Motion Studio command timestamp boundary"),
        timestamp_writes: Some(wgpu::ComputePassTimestampWrites {
            query_set: queries,
            beginning_of_pass_write_index: first.then_some(0),
            end_of_pass_write_index: (!first).then_some(1),
        }),
    }));
}
fn read_words(bytes: &[u8]) -> Option<[u64; 2]> {
    (bytes.len() == WORD_BYTES as usize).then_some(())?;
    Some([
        u64::from_le_bytes(bytes[..8].try_into().ok()?),
        u64::from_le_bytes(bytes[8..].try_into().ok()?),
    ])
}

struct MappingAttempt(Arc<Slot>, bool);
impl Drop for MappingAttempt {
    fn drop(&mut self) {
        if !self.1 {
            let _ = self.0.state.compare_exchange(
                MAPPING,
                UNCONFIRMED,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
    }
}

/// Must be dropped after its associated raw encoder/command buffer. A failed
/// submit attempt is quarantined; pre-submit discards recycle their slots.
pub(crate) struct Span {
    slot: Arc<Slot>,
    ticket: Option<CommandTicket>,
    readback: Arc<TimestampReadbackCounters>,
    period_ns: f64,
    queue: wgpu::Queue,
}
impl Span {
    fn resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        boundary(encoder, &self.slot.queries, false);
        encoder.resolve_query_set(&self.slot.queries, 0..2, &self.slot.resolve, 0);
        encoder.copy_buffer_to_buffer(&self.slot.resolve, 0, &self.slot.mapped, 0, WORD_BYTES);
        self.readback.encoded(WORD_BYTES);
    }
    fn submit_attempted(&self) {
        self.slot.state.store(UNCONFIRMED, Ordering::Release);
        if let Some(ticket) = &self.ticket {
            ticket.submit_attempted();
        }
    }
    fn map(mut self) {
        self.slot.state.store(MAPPING, Ordering::Release);
        self.readback.map_requested();
        let ticket = self.ticket.take();
        if let Some(ticket) = &ticket {
            ticket.mapping();
        }
        // The callback owns exact native handles and leases, without retaining
        // Pool (which owns Slot) in a cycle. No IO or logging in this callback.
        let slot = self.slot.clone();
        let readback = self.readback.clone();
        let period_ns = self.period_ns;
        let mut registration = MappingAttempt(slot.clone(), false);
        self.slot
            .mapped
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let mapped_successfully = result.is_ok();
                readback.map_result(mapped_successfully);
                let (words, cause) = match result {
                    Ok(()) => match slot.mapped.slice(..).get_mapped_range() {
                        Ok(bytes) => {
                            readback.mapped(bytes.len() as u64);
                            (read_words(&bytes), None)
                        }
                        Err(error) => (None, Some(error.to_string())),
                    },
                    Err(error) => (None, Some(error.to_string())),
                };
                // A cancelled/failed request is already unmapped. Calling
                // unmap again creates a new validation fault and masks the
                // original callback cause. Only successful mappings need it.
                if mapped_successfully {
                    slot.mapped.unmap();
                }
                let valid = cause.is_none() && words.is_some_and(|words| words[1] > words[0]);
                readback.sample(valid);
                if let Some(ticket) = ticket {
                    if !mapped_successfully && slot.cancelled.load(Ordering::Acquire) {
                        ticket.cancelled(cause);
                    } else {
                        ticket.mapped(words, period_ns, cause);
                    }
                }
                slot.state.store(FREE, Ordering::Release);
            });
        registration.1 = true;
    }
}
impl Drop for Span {
    fn drop(&mut self) {
        // Mapping transfers the ticket to its callback. Unconfirmed attempts
        // retain the slot until the pool is destroyed; no speculative reuse.
        if self.slot.state.load(Ordering::Acquire) == ENCODING {
            drop(self.ticket.take());
            self.slot.state.store(FREE, Ordering::Release);
        } else if self.slot.state.load(Ordering::Acquire) == UNCONFIRMED {
            // Preserve timing-buffer admission until an actual queue fence,
            // including when the submission closure unwinds after queue.submit.
            // This callback owns no Pool, so pool retirement cannot form a cycle.
            let slot = self.slot.clone();
            self.queue.on_submitted_work_done(move || drop(slot));
        }
    }
}

pub(crate) struct ObservedEncoder {
    // Declaration order is intentional: native commands drop before tickets.
    raw: wgpu::CommandEncoder,
    span: Option<Span>,
}
impl ObservedEncoder {
    pub(crate) fn new(raw: wgpu::CommandEncoder) -> Self {
        Self { raw, span: None }
    }
    pub(crate) fn command_identity(&self) -> Option<CommandIdentity> {
        self.span
            .as_ref()?
            .ticket
            .as_ref()
            .map(CommandTicket::identity)
    }
    pub(crate) fn observe(&mut self, span: Option<Span>) {
        self.span = span;
        if let Some(span) = &self.span {
            boundary(&mut self.raw, &span.slot.queries, true);
        }
    }
    pub(crate) fn finish(mut self) -> ObservedCommandBuffer {
        if let Some(span) = &self.span {
            span.resolve(&mut self.raw);
        }
        let raw = self.raw.finish();
        if let Some(ticket) = self.span.as_ref().and_then(|span| span.ticket.as_ref()) {
            ticket.encoded();
        }
        ObservedCommandBuffer {
            raw,
            span: self.span,
        }
    }
}
impl Deref for ObservedEncoder {
    type Target = wgpu::CommandEncoder;
    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}
impl DerefMut for ObservedEncoder {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.raw
    }
}
pub(crate) struct ObservedCommandBuffer {
    raw: wgpu::CommandBuffer,
    span: Option<Span>,
}

/// Observation wrapper for an existing raw-submit path. It deliberately does
/// not add the allocation-encoder reservation scope: upload/resource retirement
/// remains the original Renderer::submit_tracked contract.
pub struct TrackedCommandEncoder {
    encoder: ObservedEncoder,
    owner: Arc<()>,
}
impl TrackedCommandEncoder {
    pub(crate) fn new(encoder: ObservedEncoder, owner: Arc<()>) -> Self {
        Self { encoder, owner }
    }
    pub fn finish(self) -> TrackedCommandBuffer {
        TrackedCommandBuffer {
            buffer: self.encoder.finish(),
            owner: self.owner,
        }
    }
}
impl Deref for TrackedCommandEncoder {
    type Target = wgpu::CommandEncoder;
    fn deref(&self) -> &Self::Target {
        &self.encoder
    }
}
impl DerefMut for TrackedCommandEncoder {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.encoder
    }
}
pub struct TrackedCommandBuffer {
    buffer: ObservedCommandBuffer,
    owner: Arc<()>,
}
impl TrackedCommandBuffer {
    pub(crate) fn submit(
        self,
        owner: &Arc<()>,
        submit: impl FnOnce(wgpu::CommandBuffer) -> wgpu::SubmissionIndex,
    ) -> Result<wgpu::SubmissionIndex, RenderError> {
        if !Arc::ptr_eq(owner, &self.owner) {
            return Err(RenderError::Invalid(
                "observed raw command buffer belongs to another render queue".into(),
            ));
        }
        Ok(self.buffer.submit(submit))
    }
}
impl ObservedCommandBuffer {
    pub(crate) fn command_identity(&self) -> Option<CommandIdentity> {
        self.span
            .as_ref()?
            .ticket
            .as_ref()
            .map(CommandTicket::identity)
    }
    /// Called only after the existing owner/budget validation. The closure is
    /// the original native submission and retains its exact error/unwind path.
    pub(crate) fn submit(
        self,
        submit: impl FnOnce(wgpu::CommandBuffer) -> wgpu::SubmissionIndex,
    ) -> wgpu::SubmissionIndex {
        if let Some(span) = &self.span {
            span.submit_attempted();
        }
        let index = submit(self.raw);
        if let Some(span) = self.span {
            span.map();
        }
        index
    }
}

#[cfg(test)]
mod tests;
