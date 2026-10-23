//! Opt-in command-encoder intervals. These include marker-pass overhead and
//! possible queue/backend scheduling, not exclusive GPU busy time. Queue writes
//! executed before an encoder are outside these intervals. Never add them to
//! overlapping root-pass timings. Hosts must explicitly attach this observer;
//! constructing a profiler does not instrument other encoders or raw submits.
mod records;
use crate::{
    allocation_buffer::AllocationBuffer, readback::TimestampReadbackCounters, AllocationContext,
    AllocationEncoder, RenderError, Renderer, TimestampReadbackInfo,
};
pub use records::{
    CommandContext, CommandCounts, CommandPhase, CommandRecord, CommandSnapshot, CommandStage,
    MAX_ACTIVE_GPU_COMMANDS, MAX_GPU_COMMAND_CONTEXTS, MAX_RECENT_GPU_COMMANDS,
};
use records::{CommandRecords, CommandTicket};
use serde::Serialize;
use std::{
    ops::{Deref, DerefMut},
    sync::{
        atomic::{AtomicU8, Ordering},
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
        if !(1..=MAX_ACTIVE_GPU_COMMANDS).contains(&capacity) {
            return Err(RenderError::Invalid(
                "GPU command slots must be 1..128".into(),
            ));
        }
        let device = &renderer.device;
        let period_ns = f64::from(renderer.queue.get_timestamp_period());
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY)
            || device.limits().max_compute_workgroups_per_dimension == 0
            || !period_ns.is_finite()
            || period_ns <= 0.0
        {
            return Ok(None);
        }
        let records = CommandRecords::new()
            .ok_or_else(|| RenderError::Invalid("GPU command epoch identity exhausted".into()))?;
        let context = renderer.allocation_context();
        let slots = (0..capacity)
            .map(|_| {
                // Complete fallible reservations before creating/publishing queries.
                let resolve = AllocationBuffer::new(
                    device,
                    context.budget(),
                    &wgpu::BufferDescriptor {
                        label: Some("Motion Studio command timestamp resolve"),
                        size: wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT,
                        usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                        mapped_at_creation: false,
                    },
                )?;
                let mapped = AllocationBuffer::new(
                    device,
                    context.budget(),
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
                }))
            })
            .collect::<Result<Vec<_>, RenderError>>()?;
        Ok(Some(Self {
            context,
            pool: Arc::new(Pool {
                slots,
                records,
                readback: Arc::default(),
                period_ns,
                queue: renderer.queue.clone(),
            }),
        }))
    }
    pub fn observer(&self) -> GpuCommandObserver {
        GpuCommandObserver {
            records: self.pool.records.clone(),
            readback: self.pool.readback.clone(),
            states: self
                .pool
                .slots
                .iter()
                .map(|slot| slot.state.clone())
                .collect(),
        }
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
        let mut encoder = renderer.allocation_command_encoder(descriptor)?;
        encoder.observe_commands(span);
        Ok(encoder)
    }
}
impl Pool {
    fn begin(&self, stage: CommandStage, contexts: Vec<CommandContext>) -> Option<Span> {
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
        let Some(ticket) = self.records.begin(stage, contexts) else {
            slot.state.store(FREE, Ordering::Release);
            return None;
        };
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
                    ticket.mapped(words, period_ns, cause);
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
impl ObservedCommandBuffer {
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
