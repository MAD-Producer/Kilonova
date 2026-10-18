//! Queue bounded evidence while locked; emit only after business guards drop.
use crate::task_logging::TaskLog;
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        LockResult,
    },
    time::Instant,
};

pub(crate) struct RecoverySite {
    name: &'static str,
    failures: AtomicUsize,
}
impl RecoverySite {
    pub(crate) const fn new(name: &'static str) -> Self {
        Self {
            name,
            failures: AtomicUsize::new(0),
        }
    }
}

struct Event {
    site: &'static str,
    count: usize,
    request: String,
    operation: &'static str,
}

/// Declare before every business guard in this scope. Reverse drop order then
/// releases those guards before invoking the embedding host's logger, including
/// early returns and unwind. Each current scope uses at most two fixed sites;
/// each site can emit only its first and sixtieth lifetime recovery.
pub(crate) struct RecoveryScope {
    events: [Option<Event>; 4],
    overflow: usize,
    began: Instant,
    spawn_error: Option<(TaskLog, String)>,
}
impl RecoveryScope {
    pub(crate) fn new() -> Self {
        Self {
            events: Default::default(),
            overflow: 0,
            began: Instant::now(),
            spawn_error: None,
        }
    }

    pub(crate) fn recover<T>(
        &mut self,
        result: LockResult<T>,
        site: &'static RecoverySite,
        request: &str,
        operation: &'static str,
    ) -> T {
        match result {
            Ok(value) => value,
            Err(error) => {
                let previous = site
                    .failures
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                        Some(n.saturating_add(1))
                    })
                    .unwrap_or_else(|n| n);
                if previous == 0 || previous == 59 {
                    if let Some(slot) = self.events.iter_mut().find(|slot| slot.is_none()) {
                        *slot = Some(Event {
                            site: site.name,
                            count: previous.saturating_add(1),
                            request: request.chars().take(128).collect(),
                            operation,
                        });
                    } else {
                        // Remain bounded if a future caller adds too many sites.
                        self.overflow = self.overflow.saturating_add(1);
                    }
                }
                error.into_inner()
            }
        }
    }

    /// The caller can still hold the registry while publishing failed status.
    /// Keep its primary spawn error until that registry has also been released.
    pub(crate) fn defer_spawn_error(&mut self, log: TaskLog, reason: String) {
        self.spawn_error = Some((log, reason.chars().take(2048).collect()));
    }
}
impl Drop for RecoveryScope {
    fn drop(&mut self) {
        for event in self.events.iter().flatten() {
            let Event {
                site,
                count,
                request,
                operation,
            } = event;
            let elapsed = self.began.elapsed().as_millis();
            let context = format_args!(
                "media poison recovery site={site} lifetime_count={count} request={request:?} operation={operation} scope_elapsed_ms={elapsed} reason=poisoned_mutex recovery=into_inner"
            );
            if *count == 1 {
                log::warn!("{context}");
            } else {
                log::error!("{context}");
            }
        }
        if self.overflow > 0 {
            log::error!(
                "media poison observation buffer exhausted dropped={}",
                self.overflow
            );
        }
        if let Some((log, reason)) = &self.spawn_error {
            log.finish("spawn", Some(reason));
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
