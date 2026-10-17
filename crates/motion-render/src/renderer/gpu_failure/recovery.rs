use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        LockResult, MutexGuard,
    },
    time::Instant,
};

#[derive(Clone, Copy)]
pub(super) enum Site {
    Uncaptured,
    DeviceLost,
    Read,
}
impl Site {
    fn name(self) -> &'static str {
        match self {
            Self::Uncaptured => "gpu_failure.uncaptured",
            Self::DeviceLost => "gpu_failure.device_lost",
            Self::Read => "gpu_failure.read",
        }
    }
    fn counter(self) -> &'static AtomicUsize {
        static UNCAPTURED: AtomicUsize = AtomicUsize::new(0);
        static DEVICE_LOST: AtomicUsize = AtomicUsize::new(0);
        static READ: AtomicUsize = AtomicUsize::new(0);
        match self {
            Self::Uncaptured => &UNCAPTURED,
            Self::DeviceLost => &DEVICE_LOST,
            Self::Read => &READ,
        }
    }
}
struct Evidence {
    count: usize,
    began: Instant,
    cause_present: bool,
    cause_bytes: usize,
}

/// Declare before the latch guard, including scopes that may unwind. Each fixed
/// site counts poison recoveries for the process lifetime; healthy reads and a
/// caller clearing poison cannot erase evidence. No original cause is retained.
pub(super) struct Recovery {
    site: Site,
    renderer: Option<u64>,
    evidence: Option<Evidence>,
}
impl Recovery {
    pub(super) fn new(site: Site, renderer: Option<u64>) -> Self {
        Self {
            site,
            renderer,
            evidence: None,
        }
    }
    pub(super) fn lock<'a>(
        &mut self,
        result: LockResult<MutexGuard<'a, Option<String>>>,
    ) -> MutexGuard<'a, Option<String>> {
        match result {
            Ok(value) => value,
            Err(error) => {
                let value = error.into_inner();
                let previous = self
                    .site
                    .counter()
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                        Some(n.saturating_add(1))
                    })
                    .unwrap_or_else(|n| n);
                if previous == 0 || previous == 59 {
                    self.evidence = Some(Evidence {
                        count: previous.saturating_add(1),
                        began: Instant::now(),
                        cause_present: value.is_some(),
                        cause_bytes: value.as_ref().map_or(0, String::len),
                    });
                }
                value
            }
        }
    }
}
impl Drop for Recovery {
    fn drop(&mut self) {
        if let Some(e) = &self.evidence {
            let site = self.site.name();
            let renderer = self.renderer;
            let elapsed = e.began.elapsed().as_millis();
            let context = format_args!("GPU failure latch poison recovery site={site} lifetime_count={} renderer={renderer:?} recovery_elapsed_ms={elapsed} cause_present_before={} cause_bytes_before={} reason=poisoned_mutex recovery=into_inner", e.count, e.cause_present, e.cause_bytes);
            if e.count == 1 {
                log::warn!("{context}");
            } else {
                log::error!("{context}");
            }
        }
    }
}
