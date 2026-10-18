//! Fixed sites accumulate failures over the process lifetime. Healthy samples
//! and unrelated component serialization do not reset another track's evidence.
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) struct Failures(AtomicUsize);
impl Failures {
    pub(crate) const fn new() -> Self {
        Self(AtomicUsize::new(0))
    }

    pub(crate) fn record(&self, site: &'static str, context: impl FnOnce() -> String) {
        let previous = self
            .0
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                Some(n.saturating_add(1))
            })
            .unwrap_or_else(|n| n);
        // No keys, retained causes, or logging on the healthy path. These are
        // lifetime counts, not consecutive frames; emission is bounded per site.
        if previous == 0 {
            log::warn!(
                "model degradation site={site} lifetime_count=1 {}",
                context()
            );
        } else if previous == 59 {
            log::error!(
                "persistent model degradation site={site} lifetime_count=60 {}",
                context()
            );
        }
    }
}
