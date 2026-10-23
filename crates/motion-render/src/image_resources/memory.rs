//! One task's actual vector ownership. No pixels, pointers, IO or process-wide
//! registry are retained by an observation. Uninstrumented codec storage is unknown.
use super::{DecodeTask, Pixels};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    ops::{Deref, DerefMut},
    sync::{Arc, Mutex, TryLockError},
    time::Instant,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecodeMemorySnapshot {
    pub known_payload_bytes: u64,
    pub known_capacity_bytes: u64,
    pub details: Value,
}

#[derive(Clone, Copy)]
pub(super) enum Kind {
    Output,
    Accumulator,
    DecodedImage,
}
impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Output => "output",
            Self::Accumulator => "accumulator",
            Self::DecodedImage => "decoded_image",
        }
    }
}
const KINDS: [Kind; 3] = [Kind::Output, Kind::Accumulator, Kind::DecodedImage];
#[derive(Clone, Copy)]
struct Slot {
    active: bool,
    allocating: bool,
    payload: Option<u64>,
    capacity: Option<u64>,
}
const EMPTY: Slot = Slot {
    active: false,
    allocating: false,
    payload: Some(0),
    capacity: Some(0),
};
struct State {
    phase: &'static str,
    slots: [Slot; 3],
}
pub(super) struct Allocations {
    state: Mutex<State>,
    #[cfg(test)]
    control: Option<tests::Control>,
}
impl Default for Allocations {
    fn default() -> Self {
        Self {
            state: Mutex::new(State {
                phase: "worker_pending",
                slots: [EMPTY; 3],
            }),
            #[cfg(test)]
            control: None,
        }
    }
}
impl Allocations {
    // Writers only copy fixed metadata. Readers try_lock and copy these fields
    // before constructing JSON; neither side logs or calls user code under it.
    fn update(&self, f: impl FnOnce(&mut State)) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        f(&mut state);
    }
    pub(super) fn phase(&self, phase: &'static str) {
        self.update(|s| s.phase = phase);
    }
    pub(super) fn checkpoint(&self, phase: &'static str) {
        #[cfg(test)]
        if let Some(control) = &self.control {
            control.checkpoint(phase);
        }
        #[cfg(not(test))]
        let _ = phase;
    }
}

pub(super) struct TaskMemory {
    pub(super) allocations: Arc<Allocations>,
    began: Instant,
}
impl Default for TaskMemory {
    fn default() -> Self {
        Self {
            allocations: Arc::new(Allocations::default()),
            began: Instant::now(),
        }
    }
}

pub(super) trait OwnedBytes {
    fn owned_bytes(&self) -> (Option<u64>, Option<u64>);
}
impl<T> OwnedBytes for Vec<T> {
    fn owned_bytes(&self) -> (Option<u64>, Option<u64>) {
        let element = std::mem::size_of::<T>() as u64;
        (
            Some(self.len() as u64 * element),
            Some(self.capacity() as u64 * element),
        )
    }
}
impl OwnedBytes for Pixels {
    fn owned_bytes(&self) -> (Option<u64>, Option<u64>) {
        self.rgba.owned_bytes()
    }
}
impl OwnedBytes for image::DynamicImage {
    fn owned_bytes(&self) -> (Option<u64>, Option<u64>) {
        use image::DynamicImage::*;
        match self {
            ImageLuma8(p) => p.as_raw().owned_bytes(),
            ImageLumaA8(p) => p.as_raw().owned_bytes(),
            ImageRgb8(p) => p.as_raw().owned_bytes(),
            ImageRgba8(p) => p.as_raw().owned_bytes(),
            ImageLuma16(p) => p.as_raw().owned_bytes(),
            ImageLumaA16(p) => p.as_raw().owned_bytes(),
            ImageRgb16(p) => p.as_raw().owned_bytes(),
            ImageRgba16(p) => p.as_raw().owned_bytes(),
            ImageRgb32F(p) => p.as_raw().owned_bytes(),
            ImageRgba32F(p) => p.as_raw().owned_bytes(),
            _ => (Some(self.as_bytes().len() as u64), None),
        }
    }
}

struct Lease {
    allocations: Option<Arc<Allocations>>,
    kind: Kind,
}
impl Lease {
    fn begin(allocations: Option<&Arc<Allocations>>, kind: Kind) -> Self {
        if let Some(a) = allocations {
            a.update(|s| {
                s.slots[kind as usize] = Slot {
                    active: true,
                    allocating: true,
                    payload: None,
                    capacity: None,
                }
            });
        }
        Self {
            allocations: allocations.cloned(),
            kind,
        }
    }
    fn publish(&self, bytes: (Option<u64>, Option<u64>)) {
        if let Some(a) = &self.allocations {
            a.update(|s| {
                s.slots[self.kind as usize] = Slot {
                    active: true,
                    allocating: false,
                    payload: bytes.0,
                    capacity: bytes.1,
                }
            });
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(a) = &self.allocations {
            a.update(|s| s.slots[self.kind as usize] = EMPTY);
        }
    }
}

// Declaration order matters: real storage drops BEFORE its observation lease.
// Moving a result to the completion channel preserves this same lease.
pub(super) struct Tracked<T> {
    value: T,
    lease: Lease,
}
impl<T> Deref for Tracked<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}
impl<T> DerefMut for Tracked<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}
impl<T> Tracked<T> {
    pub(super) fn map<U>(self, f: impl FnOnce(T) -> U) -> Tracked<U> {
        let Self { value, lease } = self;
        Tracked {
            value: f(value),
            lease,
        }
    }
    pub(super) fn into_inner(self) -> T {
        let Self { value, lease } = self;
        // poll has exclusive access to its owner: the next query observes the
        // caller/cache domain. Escaped results are explicitly outside this task.
        drop(lease);
        value
    }
}
pub(super) fn allocate<T: OwnedBytes, E>(
    allocations: Option<&Arc<Allocations>>,
    kind: Kind,
    make: impl FnOnce() -> Result<T, E>,
) -> Result<Tracked<T>, E> {
    // Publish unknown before allocation, not a guessed zero/request-size cost.
    // The allocator, decoder, IO and test checkpoints run without this lock.
    let lease = Lease::begin(allocations, kind);
    let value = make()?;
    lease.publish(value.owned_bytes());
    if let Some(a) = allocations {
        a.checkpoint(kind.name());
    }
    Ok(Tracked { value, lease })
}

impl DecodeTask {
    /// Explicit, non-consuming CPU query. Codec internals and retired tasks are
    /// unknown; this does not poll the result channel or wait for the worker.
    pub fn memory_info(&self) -> DecodeMemorySnapshot {
        let Some((source, resolution, budget, speculative, cancel, _, memory)) = &self.pending
        else {
            return DecodeMemorySnapshot {
                known_payload_bytes: 0,
                known_capacity_bytes: 0,
                details: json!({"version":1,"scope":"one_image_decode_task_owned_vectors",
                    "pending":false,"phase":"idle","source":null,"buffers":[],
                    "allInstrumentedBuffersObserved":true,"completeCoverage":false,"unknownBytes":null}),
            };
        };
        let copied = match memory.allocations.state.try_lock() {
            Ok(s) => Ok((s.phase, s.slots)),
            Err(TryLockError::WouldBlock) => Err("busy"),
            Err(TryLockError::Poisoned(_)) => Err("poisoned_observation"),
        };
        let (phase, slots, unavailable) = match copied {
            Ok((phase, slots)) => (phase, slots, None),
            Err(reason) => ("unknown", [EMPTY; 3], Some(reason)),
        };
        let buffers: Vec<_> = KINDS
            .into_iter()
            .zip(slots)
            .filter(|(_, s)| s.active)
            .map(|(kind, s)| {
                json!({"kind":kind.name(),"allocating":s.allocating,
                "payloadBytes":s.payload,"capacityBytes":s.capacity})
            })
            .collect();
        DecodeMemorySnapshot {
            known_payload_bytes: slots.iter().filter_map(|s| s.payload).sum(),
            known_capacity_bytes: slots.iter().filter_map(|s| s.capacity).sum(),
            details: json!({"version":1,"scope":"one_image_decode_task_owned_vectors",
                "pending":true,"source":source.id,"resolution":format!("{resolution:?}"),
                "decodedBudgetBytes":budget,"speculative":speculative,
                "elapsedMs":memory.began.elapsed().as_millis(),
                "cancelRequested":cancel.load(std::sync::atomic::Ordering::Relaxed),
                "phase":phase,"buffers":buffers,"unavailable":unavailable,
                "allInstrumentedBuffersObserved":unavailable.is_none() && slots.iter().all(|s| !s.allocating && s.capacity.is_some()),
                "completeCoverage":false,"unknownBytes":null,"codecInternalBytes":null,
                "excludes":["codec_internal_storage","reader_and_proxy_metadata","retiring_workers",
                    "escaped_pixels","allocator_and_container_overhead"]}),
        }
    }
}

#[cfg(test)]
mod tests;
