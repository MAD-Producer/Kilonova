//! Admission for explicitly enrolled logical GPU allocations, not driver VRAM.
use crate::{RenderError, ResourceBudgetScope};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    ops::{Deref, DerefMut},
    sync::{Arc, Mutex, MutexGuard, Weak},
};

const MAX_ALLOCATIONS: usize = 65_536;
const MAX_ENCODERS: usize = 64;
pub const DEFAULT_RENDER_ALLOCATION_BUDGET: u64 = 512 * 1024 * 1024;
pub const MAX_RENDER_ALLOCATION_BUDGET: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Default)]
struct State {
    limit: u64,
    used: u64,
    peak: u64,
    next: u64,
    release_fault_seen: bool,
    allocations: BTreeMap<u64, Weak<Lease>>,
    encoders: Vec<Weak<EncoderReservations>>,
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
}

struct Lease {
    owner: Weak<Mutex<State>>,
    id: u64,
    bytes: u64,
}

#[derive(Clone)]
pub(crate) struct AllocationLease(Arc<Lease>);
impl AllocationLease {
    pub(crate) fn id(&self) -> u64 { self.0.id }
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
            let inconsistent = !registered || remaining.is_none();
            if !inconsistent {
                state.used = remaining.unwrap_or(state.used);
            }
            let report = (poisoned || inconsistent) && !state.release_fault_seen;
            state.release_fault_seen |= poisoned || inconsistent;
            (poisoned, inconsistent, report)
        };
        // Never invoke the logger while holding admission state.
        if report {
            log::error!(
                "render allocation release id={} bytes={} poisoned={} inconsistent={}",
                self.id,
                self.bytes,
                poisoned,
                inconsistent
            );
        }
    }
}

#[derive(Default)]
struct EncoderReservations(Mutex<BTreeMap<u64, AllocationLease>>);

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
        })
    }
    pub fn check(&self) -> Result<(), RenderError> {
        let state = self.lock()?;
        check_capacity(state.used, state.limit)
    }
    pub(crate) fn reserve(&self, bytes: u64) -> Result<AllocationLease, RenderError> {
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
            }));
            state.next = id;
            state.used = required;
            state.peak = state.peak.max(required);
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
                .insert(lease.0.id, lease.clone());
        }
        Ok(lease)
    }
    pub(crate) fn same_budget(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub(crate) fn encoder(
        &self,
        device: &wgpu::Device,
        owner: Arc<()>,
        descriptor: &wgpu::CommandEncoderDescriptor<'_>,
    ) -> Result<AllocationEncoder, RenderError> {
        let scope = {
            let mut state = self.lock()?;
            state.encoders.retain(|scope| scope.strong_count() > 0);
            if state.encoders.len() >= MAX_ENCODERS {
                return Err(invalid("too many pending render allocation encoder scopes"));
            }
            let leases = state
                .allocations
                .iter()
                .filter_map(|(&id, lease)| {
                    lease.upgrade().map(|lease| (id, AllocationLease(lease)))
                })
                .collect();
            let scope = Arc::new(EncoderReservations(Mutex::new(leases)));
            state.encoders.push(Arc::downgrade(&scope));
            scope
        };
        Ok(AllocationEncoder {
            encoder: device.create_command_encoder(descriptor),
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
    encoder: wgpu::CommandEncoder,
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
    buffer: wgpu::CommandBuffer,
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
        let index = submit(self.buffer);
        queue.on_submitted_work_done(move || drop(self.scope));
        Ok(index)
    }
}

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
