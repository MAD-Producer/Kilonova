//! Preserve best-effort membership queries while exposing poisoned registries.
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};

/// One fixed site accumulates over the process lifetime. A healthy registry in
/// another session does not clear evidence from a registry that is still poisoned.
pub(crate) struct RegistryObservation(AtomicUsize);

impl RegistryObservation {
    pub(crate) const fn new() -> Self {
        Self(AtomicUsize::new(0))
    }

    pub(crate) fn contains<T>(
        &self,
        registry: &Mutex<HashMap<String, T>>,
        request: &str,
        site: &'static str,
    ) -> bool {
        let (contains, entries, poisoned) = {
            let (tasks, poisoned) = match registry.lock() {
                Ok(tasks) => (tasks, false),
                Err(error) => (error.into_inner(), true),
            };
            (tasks.contains_key(request), tasks.len(), poisoned)
        };
        // The recovered guard must be dropped before invoking the host logger.
        if poisoned {
            let previous = self
                .0
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                    Some(n.saturating_add(1))
                })
                .unwrap_or_else(|n| n);
            if previous == 0 || previous == 59 {
                let request: String = request.chars().take(128).collect();
                let count = previous.saturating_add(1);
                let context = format_args!(
                    "media registry recovery site={site} lifetime_count={count} request={request:?} entries={entries} contains={contains} reason=poisoned_registry recovery=into_inner"
                );
                if previous == 0 {
                    log::warn!("{context}");
                } else {
                    log::error!("{context}");
                }
            }
        }
        contains
    }
}

#[cfg(test)]
pub(crate) mod tests;
