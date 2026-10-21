//! Reservations for enrolled upload destinations, including unsubmitted writes.
use crate::{
    allocation_budget::{AllocationLease, AllocationRetention},
    RenderError,
};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

/// Upload entry points require exclusive access to their owner. Record before the
/// queue write and retire after a subsequent submission, never on cache eviction.
/// The map deduplicates destinations; queue-owned staging is outside this scope.
pub(crate) struct AllocationUploads {
    queue: wgpu::Queue,
    pending: Mutex<BTreeMap<u64, AllocationRetention>>,
    reported_poison: AtomicBool,
}
impl AllocationUploads {
    pub(crate) fn new(queue: wgpu::Queue) -> Self {
        Self {
            queue,
            pending: Mutex::default(),
            reported_poison: AtomicBool::new(false),
        }
    }
    pub(crate) fn track(&self, lease: Option<AllocationLease>) -> Result<(), RenderError> {
        let lease = lease.ok_or_else(|| {
            RenderError::Invalid("upload destination has no allocation reservation".into())
        })?;
        self.track_many(std::iter::once(lease))
    }
    pub(crate) fn track_many(
        &self,
        leases: impl IntoIterator<Item = AllocationLease>,
    ) -> Result<(), RenderError> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| RenderError::Invalid("render upload reservations are poisoned".into()))?;
        for lease in leases {
            pending.insert(lease.id(), lease.retention());
        }
        Ok(())
    }
    fn take(&self) -> BTreeMap<u64, AllocationRetention> {
        let (pending, poisoned) = {
            let (mut state, poisoned) = match self.pending.lock() {
                Ok(state) => (state, false),
                Err(error) => (error.into_inner(), true),
            };
            (std::mem::take(&mut *state), poisoned)
        };
        if poisoned && !self.reported_poison.swap(true, Ordering::Relaxed) {
            // Cleanup preserves all known destinations. Future writes still
            // refuse the poisoned mutex. Never log while holding its guard.
            log::error!(
                "render upload reservations poisoned phase=retire retained_destinations={}",
                pending.len()
            );
        }
        pending
    }
    pub(crate) fn submitted(&self) {
        let pending = self.take();
        if !pending.is_empty() {
            self.queue.on_submitted_work_done(move || drop(pending));
        }
    }
}
impl Drop for AllocationUploads {
    fn drop(&mut self) {
        let pending = self.take();
        if !pending.is_empty() {
            // A caller may retain raw device/queue clones beyond the renderer.
            // Submit unsubmitted writes before handing their leases to the
            // completion callback. No blocking wait or upload retry is added.
            self.queue.submit([]);
            self.queue.on_submitted_work_done(move || drop(pending));
        }
    }
}

#[cfg(test)]
mod tests;
