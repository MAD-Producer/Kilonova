//! Preserve recovery behavior, then report after the ledger guard is released.
use super::State;
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        LockResult, MutexGuard,
    },
    time::Instant,
};

#[derive(Clone, Copy)]
pub(super) enum Site {
    Begin,
    Complete,
    Info,
}
impl Site {
    fn name(self) -> &'static str {
        match self {
            Self::Begin => "submission.begin",
            Self::Complete => "submission.complete",
            Self::Info => "submission.info",
        }
    }
    fn counter(self) -> &'static AtomicUsize {
        static BEGIN: AtomicUsize = AtomicUsize::new(0);
        static COMPLETE: AtomicUsize = AtomicUsize::new(0);
        static INFO: AtomicUsize = AtomicUsize::new(0);
        match self {
            Self::Begin => &BEGIN,
            Self::Complete => &COMPLETE,
            Self::Info => &INFO,
        }
    }
}
struct Evidence {
    count: usize,
    began: Instant,
    submitted: u64,
    completed: u64,
    pending_tracked: usize,
    pending_untracked: usize,
    known_bytes: u64,
    skipped: u64,
}

/// Declare before the business guard. No logging or allocation while locked;
/// reverse drop order also reports recovery on early return or unwind.
/// Counters are per fixed site for the process lifetime, never reset by a
/// healthy ledger, disabling tracking, or a caller's explicit clear_poison.
pub(super) struct Recovery {
    site: Site,
    pub submission: Option<u64>,
    evidence: Option<Evidence>,
}
impl Recovery {
    pub fn new(site: Site, submission: Option<u64>) -> Self {
        Self {
            site,
            submission,
            evidence: None,
        }
    }
    pub fn lock<'a>(&mut self, result: LockResult<MutexGuard<'a, State>>) -> MutexGuard<'a, State> {
        match result {
            Ok(state) => state,
            Err(error) => {
                let state = error.into_inner();
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
                        submitted: state.submitted,
                        completed: state.completed,
                        pending_tracked: state.pending.len(),
                        pending_untracked: state.pending_untracked,
                        known_bytes: state.known_bytes,
                        skipped: state.skipped,
                    });
                }
                state
            }
        }
    }
}
impl Drop for Recovery {
    fn drop(&mut self) {
        if let Some(e) = &self.evidence {
            let site = self.site.name();
            let submission = self.submission;
            let elapsed = e.began.elapsed().as_millis();
            let context = format_args!(
                "submission ledger poison recovery site={site} lifetime_count={} submission={submission:?} recovery_elapsed_ms={elapsed} submitted_before={} completed_before={} pending_tracked_before={} pending_untracked_before={} known_bytes_before={} skipped_before={} reason=poisoned_mutex recovery=into_inner",
                e.count, e.submitted, e.completed, e.pending_tracked, e.pending_untracked, e.known_bytes, e.skipped
            );
            if e.count == 1 {
                log::warn!("{context}");
            } else {
                log::error!("{context}");
            }
        }
    }
}
