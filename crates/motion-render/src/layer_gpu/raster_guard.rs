//! A failed/abandoned authored encoder must not leave cache entries marked ready.
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub(crate) struct RasterGuard {
    invalidated: Arc<AtomicBool>,
    committed: bool,
}
impl RasterGuard {
    pub(super) fn new(invalidated: Arc<AtomicBool>) -> Self {
        Self {
            invalidated,
            committed: false,
        }
    }
    pub(crate) fn commit(mut self) {
        self.committed = true;
    }
}
impl Drop for RasterGuard {
    fn drop(&mut self) {
        if !self.committed {
            self.invalidated.store(true, Ordering::Release);
        }
    }
}
