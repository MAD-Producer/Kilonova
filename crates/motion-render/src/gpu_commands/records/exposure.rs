//! CPU caller scopes only. Captured identities travel with the command ticket;
//! a late callback never reads the current scope or a replacement profiler.
use super::*;
use std::{marker::PhantomData, rc::Rc, thread::ThreadId};

/// Request IDs are process-unique; poll IDs advance within their request. None
/// explicitly represents exhausted identity space, never zero or a reused ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExposureCommandContext {
    pub request: Option<u64>,
    pub poll: Option<u64>,
}

struct Entry {
    context: Option<ExposureCommandContext>,
    thread: ThreadId,
}
#[derive(Default)]
pub(super) struct ExposureScopes {
    next: u64,
    entries: BTreeMap<u64, Entry>,
    pub(super) suppressed: usize,
}
impl ExposureScopes {
    pub(super) fn current(&self) -> (Option<ExposureCommandContext>, bool) {
        if self.suppressed != 0 {
            return (None, false);
        }
        match self.entries.last_key_value() {
            Some((_, entry)) if entry.thread != std::thread::current().id() => (None, true),
            Some((_, entry)) => (entry.context, false),
            None => (None, false),
        }
    }
    pub(super) fn active(&self) -> Vec<ExposureCommandContext> {
        self.entries
            .values()
            .filter_map(|entry| entry.context)
            .collect()
    }
}

/// Bind before entering media preparation or encoding. This owned guard does
/// not borrow Renderer, so the original mutable render calls remain possible.
/// It is CPU-thread-bound and holds metadata only, never a native resource.
#[must_use = "bind across the exposure poll, including media preparation"]
pub struct ExposureCommandScope {
    records: Arc<CommandRecords>,
    id: Option<u64>,
    thread: PhantomData<Rc<()>>,
}
impl CommandRecords {
    pub(crate) fn exposure_scope(
        self: &Arc<Self>,
        context: ExposureCommandContext,
    ) -> ExposureCommandScope {
        let mut state = self.lock();
        let state = &mut *state;
        let valid =
            context.request.is_some_and(|id| id != 0) && context.poll.is_some_and(|id| id != 0);
        let id = state
            .exposure
            .next
            .checked_add(1)
            .filter(|_| state.exposure.entries.len() < MAX_GPU_COMMAND_CONTEXTS);
        if !valid || id.is_none() {
            state.counts.saturated |= increment(&mut state.counts.omitted_exposure_scopes, 1);
            self.fault.store(true, Ordering::Relaxed);
        }
        if let Some(id) = id {
            state.exposure.next = id;
            state.exposure.entries.insert(
                id,
                Entry {
                    context: valid.then_some(context),
                    thread: std::thread::current().id(),
                },
            );
        } else {
            // Suppress the enclosing identity while an unrecorded child runs.
            // Capacity/ID exhaustion must not falsely attribute work to it.
            let next = state.exposure.suppressed.checked_add(1);
            state.counts.saturated |= next.is_none();
            state.exposure.suppressed = next.unwrap_or(usize::MAX);
        }
        ExposureCommandScope {
            records: self.clone(),
            id,
            thread: PhantomData,
        }
    }
}
impl Drop for ExposureCommandScope {
    fn drop(&mut self) {
        let mut state = self.records.lock();
        if let Some(id) = self.id {
            if state.exposure.entries.remove(&id).is_none() {
                self.records.fault.store(true, Ordering::Relaxed);
            }
        } else if state.exposure.suppressed == 0 {
            self.records.fault.store(true, Ordering::Relaxed);
        } else {
            state.exposure.suppressed -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context(request: u64, poll: u64) -> ExposureCommandContext {
        ExposureCommandContext {
            request: Some(request),
            poll: Some(poll),
        }
    }
    fn record(records: &Arc<CommandRecords>) -> CommandRecord {
        let ticket = records.begin(CommandStage::ExposureMedia, vec![]).unwrap();
        let info = records
            .snapshot()
            .active
            .into_iter()
            .find(|r| r.id == ticket.id())
            .unwrap();
        drop(ticket);
        info
    }
    #[test]
    fn scopes_restore_on_unwind_without_late_callback_reassociation() {
        let records = CommandRecords::new().unwrap();
        let scope = records.exposure_scope(context(11, 1));
        assert_eq!(records.snapshot().active_exposure_scopes, [context(11, 1)]);
        let ticket = records.begin(CommandStage::ExposurePhase, vec![]).unwrap();
        ticket.encoded();
        ticket.submit_attempted();
        ticket.mapping();
        let unwind = std::panic::catch_unwind(|| {
            let _nested = records.exposure_scope(context(22, 1));
            assert_eq!(record(&records).exposure, Some(context(22, 1)));
            panic!("controlled exposure scope unwind");
        });
        assert!(unwind.is_err());
        assert_eq!(record(&records).exposure, Some(context(11, 1)));
        drop(scope);
        let _later = records.exposure_scope(context(33, 2));
        ticket.mapped(Some([10, 20]), 1.0, None);
        assert_eq!(
            records.snapshot().recent.last().unwrap().exposure,
            Some(context(11, 1))
        );
        assert_eq!(record(&records).exposure, Some(context(33, 2)));
        let foreign_thread = records.clone();
        assert!(std::thread::spawn(move || record(&foreign_thread).exposure)
            .join()
            .unwrap()
            .is_none());
        assert!(records.snapshot().observation_fault);
    }
    #[test]
    fn bounded_or_unknown_scopes_shadow_the_parent_without_changing_render_work() {
        let records = CommandRecords::new().unwrap();
        let mut scopes = Vec::new();
        for id in 1..=MAX_GPU_COMMAND_CONTEXTS as u64 {
            scopes.push(records.exposure_scope(context(id, 1)));
        }
        assert_eq!(
            record(&records).exposure,
            Some(context(MAX_GPU_COMMAND_CONTEXTS as u64, 1))
        );
        let omitted = records.exposure_scope(context(99, 1));
        assert!(record(&records).exposure.is_none());
        assert_eq!(
            records.snapshot().active_exposure_scopes.len(),
            MAX_GPU_COMMAND_CONTEXTS
        );
        assert_eq!(records.snapshot().suppressed_exposure_scopes, 1);
        drop(omitted);
        assert!(record(&records).exposure.is_some());
        // Out-of-order drops remove the exact scope, never a newer child.
        scopes.remove(0);
        assert_eq!(
            record(&records).exposure.unwrap().request,
            Some(MAX_GPU_COMMAND_CONTEXTS as u64)
        );
        let unknown = records.exposure_scope(ExposureCommandContext {
            request: None,
            poll: Some(2),
        });
        assert!(record(&records).exposure.is_none());
        drop(unknown);
        drop(scopes);
        records.lock().exposure.next = u64::MAX;
        let exhausted = records.exposure_scope(context(100, 1));
        assert!(record(&records).exposure.is_none());
        assert_eq!(records.snapshot().counts.omitted_exposure_scopes, 3);
        drop(exhausted);
        assert!(records.snapshot().active_exposure_scopes.is_empty());
        assert_eq!(records.snapshot().suppressed_exposure_scopes, 0);
    }
    #[test]
    fn exposure_identity_is_visible_while_media_preparation_is_blocked() {
        let records = CommandRecords::new().unwrap();
        let worker = records.clone();
        let (entered, observed) = std::sync::mpsc::sync_channel(1);
        let (release, blocked) = std::sync::mpsc::sync_channel(1);
        let thread = std::thread::spawn(move || {
            let _scope = worker.exposure_scope(context(41, 7));
            entered.send(()).unwrap();
            blocked.recv().unwrap();
        });
        observed
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let snapshot = records.snapshot();
        // Release even if an assertion below fails, keeping the test bounded.
        release.send(()).unwrap();
        thread.join().unwrap();
        assert_eq!(snapshot.active_exposure_scopes, [context(41, 7)]);
        assert!(snapshot.active.is_empty()); // No GPU command has been encoded.
        assert!(records.snapshot().active_exposure_scopes.is_empty());
        assert!(!snapshot.observation_fault);
    }
}
