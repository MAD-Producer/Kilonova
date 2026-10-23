//! Admission for explicitly enrolled logical GPU allocations, not driver VRAM.
use crate::{RenderError, ResourceBudgetScope};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    ops::{Deref, DerefMut},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, MutexGuard, Weak,
    },
};

const MAX_ALLOCATIONS: usize = 65_536;
const MAX_ENCODERS: usize = 64;
mod categories;
mod observation;
pub use observation::AllocationLogScope;
pub use categories::{AllocationCategory, AllocationCategoryFrameInfo, AllocationCategoryInfo};
use categories::{CATEGORIES, CATEGORY_COUNT};
pub const DEFAULT_RENDER_ALLOCATION_BUDGET: u64 = 512 * 1024 * 1024;
pub const MAX_RENDER_ALLOCATION_BUDGET: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Default)]
struct State {
    limit: u64,
    used: u64,
    peak: u64,
    categories: [u64; CATEGORY_COUNT],
    category_peaks: [u64; CATEGORY_COUNT],
    next: u64,
    release_fault_seen: bool,
    allocations: BTreeMap<u64, Weak<Lease>>,
    encoders: Vec<Weak<EncoderReservations>>,
    frames: frames::Frames,
}

/// Clone this before constructing renderers to share one admission limit.
/// Only allocations enrolled through this budget are included. In particular,
/// this is not inferred from the optional diagnostics/submission ledger.
#[derive(Clone)]
pub struct AllocationBudget(Arc<Mutex<State>>);

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationBudgetInfo {
    pub budget_bytes: u64,
    pub admitted_bytes: u64,
    pub peak_admitted_bytes: u64,
    pub live_allocations: usize,
    pub encoder_scopes: usize,
    pub categories: Vec<AllocationCategoryInfo>,
}

struct Lease {
    owner: Weak<Mutex<State>>,
    id: u64,
    bytes: u64,
    category: AllocationCategory,
    // Gauge of actual resource/view/binding handles, bounded by Arc's own
    // reference count. Command and upload retentions do not renew ownership.
    owners: AtomicUsize,
}

pub(crate) struct AllocationLease(Arc<Lease>);
impl AllocationLease {
    pub(crate) fn id(&self) -> u64 {
        self.0.id
    }
    pub(crate) fn retention(&self) -> AllocationRetention {
        AllocationRetention {
            _lease: self.0.clone(),
        }
    }
}
impl Clone for AllocationLease {
    fn clone(&self) -> Self {
        let lease = self.0.clone();
        lease.owners.fetch_add(1, Ordering::Relaxed);
        Self(lease)
    }
}
impl Drop for AllocationLease {
    fn drop(&mut self) {
        self.0.owners.fetch_sub(1, Ordering::Release);
    }
}

/// Keeps capacity alive for existing GPU work without making the resource a
/// candidate for every future encoder's creation-time snapshot.
#[derive(Clone)]
pub(crate) struct AllocationRetention {
    _lease: Arc<Lease>,
}

impl Drop for Lease {
    fn drop(&mut self) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let (poisoned, inconsistent, report) = {
            let (mut state, poisoned) = match owner.lock() {
                Ok(state) => (state, false),
                Err(error) => (error.into_inner(), true),
            };
            let registered = state.allocations.remove(&self.id).is_some();
            let remaining = state.used.checked_sub(self.bytes);
            let category_remaining =
                state.categories[self.category as usize].checked_sub(self.bytes);
            let inconsistent = !registered || remaining.is_none() || category_remaining.is_none();
            if !inconsistent {
                state.used = remaining.unwrap_or(state.used);
                state.categories[self.category as usize] =
                    category_remaining.unwrap_or(state.categories[self.category as usize]);
            }
            let report = (poisoned || inconsistent) && !state.release_fault_seen;
            state.release_fault_seen |= poisoned || inconsistent;
            (poisoned, inconsistent, report)
        };
        // The host scope also defers logging beyond outer business guards.
        if report {
            observation::Event::record(
                self.id,
                self.bytes,
                self.category,
                poisoned,
                inconsistent,
            );
        }
    }
}

#[derive(Default)]
struct EncoderReservations(Mutex<BTreeMap<u64, AllocationRetention>>);

fn invalid(message: &str) -> RenderError {
    RenderError::Invalid(message.into())
}

impl Default for AllocationBudget {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(State {
            limit: DEFAULT_RENDER_ALLOCATION_BUDGET,
            ..State::default()
        })))
    }
}

impl AllocationBudget {
    pub fn new(bytes: u64) -> Result<Self, RenderError> {
        let budget = Self::default();
        budget.set_limit(bytes)?;
        Ok(budget)
    }
    fn lock(&self) -> Result<MutexGuard<'_, State>, RenderError> {
        let state = self
            .0
            .lock()
            .map_err(|_| invalid("render allocation admission state is poisoned"))?;
        if state.release_fault_seen {
            return Err(invalid(
                "render allocation admission stopped after a release invariant failure",
            ));
        }
        Ok(state)
    }
    /// Existing allocations keep their leases when the limit is lowered. New
    /// admission and explicit checks then refuse until enough capacity retires.
    pub fn set_limit(&self, bytes: u64) -> Result<(), RenderError> {
        if !(1..=MAX_RENDER_ALLOCATION_BUDGET).contains(&bytes) {
            return Err(invalid("render allocation budget must be 1 byte..4 GiB"));
        }
        self.lock()?.limit = bytes;
        Ok(())
    }
    pub fn info(&self) -> Result<AllocationBudgetInfo, RenderError> {
        let state = self.lock()?;
        Ok(AllocationBudgetInfo {
            budget_bytes: state.limit,
            admitted_bytes: state.used,
            peak_admitted_bytes: state.peak,
            live_allocations: state.allocations.len(),
            encoder_scopes: state
                .encoders
                .iter()
                .filter(|scope| scope.strong_count() > 0)
                .count(),
            categories: CATEGORIES
                .into_iter()
                .map(|category| AllocationCategoryInfo {
                    category,
                    admitted_bytes: state.categories[category as usize],
                    peak_admitted_bytes: state.category_peaks[category as usize],
                })
                .collect(),
        })
    }
    pub fn check(&self) -> Result<(), RenderError> {
        let state = self.lock()?;
        check_capacity(state.used, state.limit)
    }
    #[cfg(test)]
    pub(crate) fn reserve(&self, bytes: u64) -> Result<AllocationLease, RenderError> {
        self.reserve_in(bytes, AllocationCategory::OtherBuffers)
    }
    pub(crate) fn reserve_in(
        &self,
        bytes: u64,
        category: AllocationCategory,
    ) -> Result<AllocationLease, RenderError> {
        if bytes == 0 {
            return Err(invalid("empty render allocation"));
        }
        let (lease, encoders) = {
            let mut state = self.lock()?;
            let required = state
                .used
                .checked_add(bytes)
                .ok_or_else(|| invalid("render allocation capacity overflow"))?;
            check_capacity(required, state.limit)?;
            let category_required = state.categories[category as usize]
                .checked_add(bytes)
                .ok_or_else(|| invalid("render allocation category capacity overflow"))?;
            if state.allocations.len() >= MAX_ALLOCATIONS {
                return Err(invalid("render allocation metadata limit exceeded"));
            }
            let id = state
                .next
                .checked_add(1)
                .ok_or_else(|| invalid("render allocation identity exhausted"))?;
            let lease = AllocationLease(Arc::new(Lease {
                owner: Arc::downgrade(&self.0),
                id,
                bytes,
                category,
                owners: AtomicUsize::new(1),
            }));
            state.next = id;
            state.used = required;
            state.peak = state.peak.max(required);
            state.categories[category as usize] = category_required;
            state.category_peaks[category as usize] =
                state.category_peaks[category as usize].max(category_required);
            state.frames.observe(required, category, category_required);
            state.allocations.insert(id, Arc::downgrade(&lease.0));
            state.encoders.retain(|scope| scope.strong_count() > 0);
            let encoders: Vec<_> = state.encoders.iter().filter_map(Weak::upgrade).collect();
            (lease, encoders)
        };
        // An open encoder conservatively retains every allocation admitted in
        // its shared budget. This also covers allocation/replacement mid-encode.
        for scope in encoders {
            scope
                .0
                .lock()
                .map_err(|_| invalid("render encoder reservations are poisoned"))?
                .insert(lease.0.id, lease.retention());
        }
        Ok(lease)
    }
    /// Admission identity, independent of current limits or measured bytes.
    /// Hosts use it to reject renderers constructed outside their Session.
    pub fn same_budget(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub(crate) fn encoder(
        &self,
        device: &wgpu::Device,
        owner: Arc<()>,
        descriptor: &wgpu::CommandEncoderDescriptor<'_>,
    ) -> Result<AllocationEncoder, RenderError> {
        let (scope, leases) = {
            let mut state = self.lock()?;
            state.encoders.retain(|scope| scope.strong_count() > 0);
            if state.encoders.len() >= MAX_ENCODERS {
                return Err(invalid("too many pending render allocation encoder scopes"));
            }
            let leases = state
                .allocations
                .values()
                .filter_map(Weak::upgrade)
                .collect::<Vec<_>>();
            let scope = Arc::new(EncoderReservations::default());
            state.encoders.push(Arc::downgrade(&scope));
            (scope, leases)
        };
        // A concurrent last owner/retention may disappear after Weak::upgrade.
        // Drop omitted leases outside admission state: Lease::drop takes it.
        for lease in leases {
            if lease.owners.load(Ordering::Acquire) > 0 {
                scope
                    .0
                    .lock()
                    .map_err(|_| invalid("render encoder reservations are poisoned"))?
                    .insert(lease.id, AllocationRetention { _lease: lease });
            }
        }
        Ok(AllocationEncoder {
            encoder: crate::gpu_commands::ObservedEncoder::new(device.create_command_encoder(descriptor)),
            scope,
            budget: self.clone(),
            owner,
        })
    }
}

fn check_capacity(required: u64, budget: u64) -> Result<(), RenderError> {
    if required > budget {
        return Err(RenderError::budget(
            ResourceBudgetScope::RenderAllocations,
            required,
            budget,
            format!("admitted render allocations exceed configured combined budget required_bytes={required} budget_bytes={budget}"),
        ));
    }
    Ok(())
}

/// Owns admission leases while commands are being encoded. Dropping without
/// submission drops the GPU encoder before its leases. Keep the finished
/// command buffer intact until passing it to Renderer::submit_allocated.
pub struct AllocationEncoder {
    encoder: crate::gpu_commands::ObservedEncoder,
    scope: Arc<EncoderReservations>,
    budget: AllocationBudget,
    owner: Arc<()>,
}
impl Deref for AllocationEncoder {
    type Target = wgpu::CommandEncoder;
    fn deref(&self) -> &Self::Target {
        &self.encoder
    }
}
impl DerefMut for AllocationEncoder {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.encoder
    }
}
impl AllocationEncoder {
    pub(crate) fn observe_commands(&mut self, span: Option<crate::gpu_commands::Span>) {
        self.encoder.observe(span);
    }
    pub fn finish(self) -> AllocatedCommandBuffer {
        AllocatedCommandBuffer {
            buffer: self.encoder.finish(),
            scope: self.scope,
            budget: self.budget,
            owner: self.owner,
        }
    }
}
pub struct AllocatedCommandBuffer {
    buffer: crate::gpu_commands::ObservedCommandBuffer,
    scope: Arc<EncoderReservations>,
    budget: AllocationBudget,
    owner: Arc<()>,
}
impl AllocatedCommandBuffer {
    pub(crate) fn submit(
        self,
        budget: &AllocationBudget,
        owner: &Arc<()>,
        submit: impl FnOnce(wgpu::CommandBuffer) -> wgpu::SubmissionIndex,
        queue: &wgpu::Queue,
        uploads: &crate::allocation_uploads::AllocationUploads,
    ) -> Result<wgpu::SubmissionIndex, RenderError> {
        if !budget.same_budget(&self.budget) {
            return Err(invalid(
                "render command buffer belongs to another allocation budget",
            ));
        }
        if !Arc::ptr_eq(owner, &self.owner) {
            return Err(invalid(
                "render command buffer belongs to another render queue",
            ));
        }
        let completion = budget.frame_submission();
        let index = self.buffer.submit(submit);
        let pending_uploads = uploads.take();
        queue.on_submitted_work_done(move || {
            // One callback establishes cleanup-before-observation ordering;
            // independent queue callbacks need not provide that relationship.
            drop(pending_uploads);
            drop(self.scope);
            completion.complete();
        });
        Ok(index)
    }
}

mod frames;
pub use frames::{
    AllocationFrame, AllocationFrameInfo, AllocationFrameOutcome, AllocationFramePeaks,
    FrameOperation, MAX_ACTIVE_ALLOCATION_FRAMES, MAX_RECENT_ALLOCATION_FRAMES,
};

/// A cloned offscreen view retains the same single allocation reservation.
#[derive(Clone)]
pub struct AdmittedTextureView {
    pub(crate) view: wgpu::TextureView,
    pub(crate) lease: AllocationLease,
}
impl Deref for AdmittedTextureView {
    type Target = wgpu::TextureView;
    fn deref(&self) -> &Self::Target {
        &self.view
    }
}

#[cfg(test)]
mod tests;
