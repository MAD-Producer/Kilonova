//! Fixed call-site keys and process-lifetime counts bound panel fallback evidence.
//! Healthy rows do not reset a neighbouring malformed row's counter: otherwise
//! mixed lists could warn every frame and never reach the escalation threshold.
use std::sync::atomic::{AtomicUsize, Ordering};

pub(super) struct Counter(AtomicUsize);
impl Counter {
    pub const fn new() -> Self {
        Self(AtomicUsize::new(0))
    }
    pub fn failure(&self, site: &str, context: std::fmt::Arguments<'_>) {
        let previous = self
            .0
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                Some(v.saturating_add(1))
            })
            .unwrap_or_else(|v| v);
        if previous != 0 && previous != 59 {
            return;
        }
        let context = context.to_string().chars().take(4096).collect::<String>();
        match previous {
            0 => log::warn!("panel degradation site={site} count=1 {context}"),
            59 => log::error!("persistent panel degradation site={site} count=60 {context}"),
            _ => {}
        }
    }
}
