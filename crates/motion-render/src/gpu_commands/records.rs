//! Bounded identities and lifecycle observations. Native words are supplied only
//! by the paired GPU buffer callback; CPU metadata is not GPU execution proof.
use crate::FrameOperation;
use serde::Serialize;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, MutexGuard,
    },
    time::Instant,
};
mod exposure;
pub use exposure::{ExposureCommandContext, ExposureCommandScope};

pub const MAX_ACTIVE_GPU_COMMANDS: usize = 128;
pub const MAX_RECENT_GPU_COMMANDS: usize = 256;
pub const MAX_GPU_COMMAND_CONTEXTS: usize = 64;
const MAX_PENDING_LOG_EVENTS: usize = 16;
static NEXT_EPOCH: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandStage {
    Unclassified,
    Composition,
    MediaPreparation,
    VideoBlend,
    CompositionAndPresentation,
    NestedComposition,
    ExposureMedia,
    ExposurePhase,
    ExposureResolve,
    RetainedLayer,
    PhaseAdjustment,
    ReadbackCopy,
    AuthoringInput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandPhase {
    Encoding,
    Encoded,
    SubmitAttempted,
    Mapping,
    Complete,
    Discarded,
    Unconfirmed,
    MapFailed,
    Cancelled,
    Invalid,
}

/// IDs identify actual caller scopes. None is unknown, never an invented ID or
/// a join by floating-point source frame. Exposure identity must survive polls.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandContext {
    pub allocation_frame: u64,
    pub request: Option<u64>,
    pub operation: FrameOperation,
    pub source_frame: Option<f64>,
    pub exposure_request: Option<u64>,
    pub exposure_poll: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRecord {
    pub epoch: u64,
    pub id: u64,
    pub stage: CommandStage,
    /// Captured independently of allowance frames, which may be absent.
    pub exposure: Option<ExposureCommandContext>,
    pub contexts: Vec<CommandContext>,
    pub omitted_contexts: usize,
    pub phase: CommandPhase,
    pub cpu_elapsed_us: u64,
    pub gpu_elapsed_us: Option<f64>,
    pub timestamp_words: Option<[u64; 2]>,
    pub cause: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandCounts {
    pub started: u64,
    pub omitted_busy: u64,
    pub omitted_slots: u64,
    pub omitted_invalid_context: u64,
    pub omitted_identity_exhausted: u64,
    pub omitted_contexts: u64,
    pub omitted_exposure_scopes: u64,
    pub completed: u64,
    pub discarded: u64,
    pub unconfirmed: u64,
    pub map_failed: u64,
    /// Subset of failed native callbacks explicitly cancelled by the owner.
    pub cancelled: u64,
    pub invalid: u64,
    pub evicted_recent: u64,
    pub omitted_log_events: u64,
    pub saturated: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSnapshot {
    pub epoch: u64,
    pub counts: CommandCounts,
    pub observation_fault: bool,
    /// Completion-order readback streak within this epoch. Valid samples and
    /// explicit callback cancellation reset it. Work never mapped is unknown,
    /// so busy omissions or pre-submit discards cannot falsely prove recovery.
    pub consecutive_readback_failures: u64,
    pub active_exposure_scopes: Vec<ExposureCommandContext>,
    pub suppressed_exposure_scopes: usize,
    pub active: Vec<CommandRecord>,
    /// Completion order, not source-frame or callback creation order.
    pub recent: Vec<CommandRecord>,
}

struct Active {
    info: CommandRecord,
    began: Instant,
}
#[derive(Default)]
struct State {
    next: u64,
    counts: CommandCounts,
    active: BTreeMap<u64, Active>,
    recent: VecDeque<CommandRecord>,
    consecutive_failures: u64,
    pending_logs: VecDeque<CommandLogEvent>,
    reported_log_omissions: u64,
    reported_fault: bool,
    exposure: exposure::ExposureScopes,
}
pub(super) struct CommandLogEvent {
    pub level: log::Level,
    pub occurrence: u64,
    pub record: CommandRecord,
}
pub(super) struct CommandLogBatch {
    pub epoch: u64,
    pub events: Vec<CommandLogEvent>,
    pub omitted: u64,
    pub invariant_fault: bool,
}
pub(crate) struct CommandRecords {
    epoch: u64,
    fault: AtomicBool,
    state: Mutex<State>,
}

fn increment(value: &mut u64, amount: u64) -> bool {
    let checked = value.checked_add(amount);
    *value = checked.unwrap_or(u64::MAX);
    checked.is_none()
}
fn micros(start: Instant) -> u64 {
    start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
}

impl CommandRecords {
    pub(super) fn epoch(&self) -> u64 {
        self.epoch
    }
    #[cfg(test)]
    pub(super) fn lock_available(&self) -> bool {
        self.state.try_lock().is_ok()
    }
    pub(crate) fn new() -> Option<Arc<Self>> {
        let epoch = NEXT_EPOCH
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .ok()?;
        Some(Arc::new(Self {
            epoch,
            fault: AtomicBool::new(false),
            state: Mutex::default(),
        }))
    }
    fn lock(&self) -> MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(state) => state,
            Err(error) => {
                self.fault.store(true, Ordering::Relaxed);
                error.into_inner()
            }
        }
    }
    pub(crate) fn omit_slot(&self) {
        let mut state = self.lock();
        let counts = &mut state.counts;
        counts.saturated |= increment(&mut counts.omitted_slots, 1);
    }
    pub(super) fn take_logs(&self) -> CommandLogBatch {
        let mut state = self.lock();
        let omitted = state
            .counts
            .omitted_log_events
            .saturating_sub(state.reported_log_omissions);
        state.reported_log_omissions = state.counts.omitted_log_events;
        let invariant_fault = self.fault.load(Ordering::Relaxed) && !state.reported_fault;
        state.reported_fault |= invariant_fault;
        CommandLogBatch {
            epoch: self.epoch,
            events: state.pending_logs.drain(..).collect(),
            omitted,
            invariant_fault,
        }
    }
    /// Reserve before entering an encoder operation. An omitted observation
    /// must never refuse/retry the actual render operation. No logging or native
    /// API is invoked under this lock (callbacks may run under Session locks).
    pub(crate) fn begin(
        self: &Arc<Self>,
        stage: CommandStage,
        mut contexts: Vec<CommandContext>,
    ) -> Option<CommandTicket> {
        let mut state = self.lock();
        let state = &mut *state;
        if state.active.len() == MAX_ACTIVE_GPU_COMMANDS {
            state.counts.saturated |= increment(&mut state.counts.omitted_busy, 1);
            return None;
        }
        let Some(id) = state.next.checked_add(1) else {
            state.counts.saturated |= increment(&mut state.counts.omitted_identity_exhausted, 1);
            return None;
        };
        // Reject nonsensical metadata without changing caller rendering. Do not
        // turn an invalid source frame into a plausible zero frame.
        if contexts
            .iter()
            .any(|context| context.source_frame.is_some_and(|f| !f.is_finite()))
        {
            self.fault.store(true, Ordering::Relaxed);
            state.counts.saturated |= increment(&mut state.counts.omitted_invalid_context, 1);
            return None;
        }
        let omitted = contexts.len().saturating_sub(MAX_GPU_COMMAND_CONTEXTS);
        contexts.truncate(MAX_GPU_COMMAND_CONTEXTS);
        let (exposure, wrong_thread) = state.exposure.current();
        if wrong_thread {
            self.fault.store(true, Ordering::Relaxed);
        }
        if let Some(exposure) = exposure {
            for context in &mut contexts {
                context.exposure_request = exposure.request;
                context.exposure_poll = exposure.poll;
            }
        }
        state.counts.saturated |= increment(&mut state.counts.omitted_contexts, omitted as u64);
        state.counts.saturated |= increment(&mut state.counts.started, 1);
        state.next = id;
        state.active.insert(
            id,
            Active {
                began: Instant::now(),
                info: CommandRecord {
                    epoch: self.epoch,
                    id,
                    stage,
                    exposure,
                    contexts,
                    omitted_contexts: omitted,
                    phase: CommandPhase::Encoding,
                    cpu_elapsed_us: 0,
                    gpu_elapsed_us: None,
                    timestamp_words: None,
                    cause: None,
                },
            },
        );
        Some(CommandTicket {
            records: self.clone(),
            id,
            armed: true,
        })
    }
    pub(crate) fn snapshot(&self) -> CommandSnapshot {
        let state = self.lock();
        CommandSnapshot {
            epoch: self.epoch,
            counts: state.counts.clone(),
            observation_fault: self.fault.load(Ordering::Relaxed),
            consecutive_readback_failures: state.consecutive_failures,
            active_exposure_scopes: state.exposure.active(),
            suppressed_exposure_scopes: state.exposure.suppressed,
            active: state
                .active
                .values()
                .map(|active| {
                    let mut record = active.info.clone();
                    record.cpu_elapsed_us = micros(active.began);
                    record
                })
                .collect(),
            recent: state.recent.iter().cloned().collect(),
        }
    }
    fn transition(&self, id: u64, from: CommandPhase, to: CommandPhase) -> bool {
        let mut state = self.lock();
        let Some(active) = state.active.get_mut(&id) else {
            self.fault.store(true, Ordering::Relaxed);
            return false;
        };
        if active.info.phase != from {
            self.fault.store(true, Ordering::Relaxed);
            return false;
        }
        active.info.phase = to;
        true
    }
    fn retire(
        &self,
        id: u64,
        outcome: Option<(CommandPhase, Option<[u64; 2]>, Option<f64>, Option<String>)>,
    ) {
        let mut state = self.lock();
        let state = &mut *state;
        let Some(mut active) = state.active.remove(&id) else {
            self.fault.store(true, Ordering::Relaxed);
            return;
        };
        let (phase, words, elapsed, cause) = outcome.unwrap_or_else(|| {
            let discarded = matches!(
                active.info.phase,
                CommandPhase::Encoding | CommandPhase::Encoded
            );
            (
                if discarded {
                    CommandPhase::Discarded
                } else {
                    CommandPhase::Unconfirmed
                },
                None,
                None,
                None,
            )
        });
        let counter = match phase {
            CommandPhase::Complete => &mut state.counts.completed,
            CommandPhase::Discarded => &mut state.counts.discarded,
            CommandPhase::Unconfirmed => &mut state.counts.unconfirmed,
            CommandPhase::MapFailed => &mut state.counts.map_failed,
            CommandPhase::Cancelled => {
                state.counts.saturated |= increment(&mut state.counts.cancelled, 1);
                &mut state.counts.map_failed
            }
            CommandPhase::Invalid => &mut state.counts.invalid,
            _ => {
                self.fault.store(true, Ordering::Relaxed);
                &mut state.counts.unconfirmed
            }
        };
        state.counts.saturated |= increment(counter, 1);
        active.info.phase = phase;
        active.info.cpu_elapsed_us = micros(active.began);
        active.info.timestamp_words = words;
        active.info.gpu_elapsed_us = elapsed;
        active.info.cause = cause.map(|cause| cause.chars().take(256).collect());
        // Publish only bounded events here. GPU callbacks can execute under an
        // outer business guard, so native callbacks and state locks perform no IO.
        if matches!(phase, CommandPhase::MapFailed | CommandPhase::Invalid) {
            state.counts.saturated |= increment(&mut state.consecutive_failures, 1);
            let occurrence = state.consecutive_failures;
            if occurrence == 1 || occurrence == 60 {
                if state.pending_logs.len() == MAX_PENDING_LOG_EVENTS {
                    // Keep escalation evidence when a consumer falls behind.
                    // Any discarded event remains explicit in snapshot counts.
                    let replace = (occurrence == 60)
                        .then(|| {
                            state
                                .pending_logs
                                .iter()
                                .position(|event| event.level == log::Level::Debug)
                        })
                        .flatten();
                    if let Some(index) = replace {
                        state.pending_logs.remove(index);
                    }
                    state.counts.saturated |= increment(&mut state.counts.omitted_log_events, 1);
                }
                if state.pending_logs.len() < MAX_PENDING_LOG_EVENTS {
                    state.pending_logs.push_back(CommandLogEvent {
                        level: if occurrence == 1 {
                            log::Level::Debug
                        } else {
                            log::Level::Error
                        },
                        occurrence,
                        record: active.info.clone(),
                    });
                }
            }
        } else if matches!(phase, CommandPhase::Complete | CommandPhase::Cancelled) {
            state.consecutive_failures = 0;
        }
        if state.recent.len() == MAX_RECENT_GPU_COMMANDS {
            state.recent.pop_front();
            state.counts.saturated |= increment(&mut state.counts.evicted_recent, 1);
        }
        state.recent.push_back(active.info);
    }
}

/// Move this ticket from the raw encoder to its finished command buffer, then
/// to the mapping callback. Dropped pre-submit work is normal discard; dropped
/// post-attempt work is unconfirmed and must not release/reuse a query slot.
pub(crate) struct CommandTicket {
    records: Arc<CommandRecords>,
    id: u64,
    armed: bool,
}
impl CommandTicket {
    #[cfg(test)]
    pub(crate) fn id(&self) -> u64 {
        self.id
    }
    pub(crate) fn encoded(&self) -> bool {
        self.records
            .transition(self.id, CommandPhase::Encoding, CommandPhase::Encoded)
    }
    pub(crate) fn submit_attempted(&self) -> bool {
        self.records.transition(
            self.id,
            CommandPhase::Encoded,
            CommandPhase::SubmitAttempted,
        )
    }
    pub(crate) fn mapping(&self) -> bool {
        self.records.transition(
            self.id,
            CommandPhase::SubmitAttempted,
            CommandPhase::Mapping,
        )
    }
    pub(crate) fn mapped(mut self, words: Option<[u64; 2]>, period_ns: f64, error: Option<String>) {
        self.finish_mapping(words, period_ns, error, false);
    }
    pub(crate) fn cancelled(mut self, error: Option<String>) {
        self.finish_mapping(None, 0.0, error, true);
    }
    fn finish_mapping(
        &mut self,
        words: Option<[u64; 2]>,
        period_ns: f64,
        error: Option<String>,
        cancelled: bool,
    ) {
        let valid_phase = self
            .records
            .lock()
            .active
            .get(&self.id)
            .is_some_and(|active| active.info.phase == CommandPhase::Mapping);
        let elapsed = words.filter(|w| w[1] > w[0]).and_then(|words| {
            let value = (words[1] - words[0]) as f64 * period_ns / 1000.0;
            (period_ns > 0.0 && value.is_finite() && value > 0.0).then_some(value)
        });
        let phase = if !valid_phase {
            self.records.fault.store(true, Ordering::Relaxed);
            CommandPhase::Unconfirmed
        } else if cancelled {
            CommandPhase::Cancelled
        } else if error.is_some() {
            CommandPhase::MapFailed
        } else if elapsed.is_some() {
            CommandPhase::Complete
        } else {
            CommandPhase::Invalid
        };
        self.records.retire(
            self.id,
            Some((
                phase,
                words,
                if phase == CommandPhase::Complete {
                    elapsed
                } else {
                    None
                },
                error,
            )),
        );
        self.armed = false;
    }
}
impl Drop for CommandTicket {
    fn drop(&mut self) {
        if self.armed {
            self.records.retire(self.id, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context(request: u64) -> Vec<CommandContext> {
        vec![CommandContext {
            allocation_frame: request,
            request: Some(request),
            operation: FrameOperation::Present,
            source_frame: Some(1.25),
            exposure_request: Some(7),
            exposure_poll: Some(request),
        }]
    }
    #[test]
    fn blocked_and_superseded_commands_keep_distinct_live_contexts_and_epochs() {
        let records = CommandRecords::new().unwrap();
        let (entered, wait) = std::sync::mpsc::sync_channel(1);
        let (release, blocked) = std::sync::mpsc::sync_channel(1);
        let worker = records.clone();
        let thread = std::thread::spawn(move || {
            let ticket = worker
                .begin(CommandStage::ExposurePhase, context(31))
                .unwrap();
            entered.send(ticket.id()).unwrap();
            blocked.recv().unwrap();
            drop(ticket);
        });
        let old = wait
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let newer = records
            .begin(CommandStage::ExposurePhase, context(32))
            .unwrap();
        assert_ne!(old, newer.id());
        let snapshot = records.snapshot();
        assert_eq!(snapshot.active.len(), 2);
        assert_eq!(snapshot.active[0].contexts[0].request, Some(31));
        assert_eq!(snapshot.active[1].contexts[0].request, Some(32));
        assert!(snapshot
            .active
            .iter()
            .all(|entry| entry.gpu_elapsed_us.is_none()));
        release.send(()).unwrap();
        thread.join().unwrap();
        drop(newer);
        let ended = records.snapshot();
        assert!(ended.active.is_empty());
        assert_eq!(ended.counts.discarded, 2);
        assert_eq!(ended.counts.completed, 0);
        assert_ne!(ended.epoch, CommandRecords::new().unwrap().snapshot().epoch);
    }
    #[test]
    fn bounded_pending_and_recent_records_recover_without_fake_gpu_success() {
        let records = CommandRecords::new().unwrap();
        let pending: Vec<_> = (0..MAX_ACTIVE_GPU_COMMANDS)
            .map(|_| records.begin(CommandStage::Unclassified, vec![]).unwrap())
            .collect();
        assert!(records.begin(CommandStage::Unclassified, vec![]).is_none());
        assert_eq!(records.snapshot().counts.omitted_busy, 1);
        drop(pending);
        for _ in 0..MAX_RECENT_GPU_COMMANDS + 1 {
            let ticket = records
                .begin(CommandStage::ReadbackCopy, context(41))
                .unwrap();
            assert!(ticket.encoded());
            assert!(ticket.submit_attempted());
            drop(ticket);
        }
        let snapshot = records.snapshot();
        assert_eq!(snapshot.recent.len(), MAX_RECENT_GPU_COMMANDS);
        assert_eq!(
            snapshot.counts.unconfirmed,
            (MAX_RECENT_GPU_COMMANDS + 1) as u64
        );
        assert!(snapshot.counts.evicted_recent > 0);
        assert_eq!(snapshot.counts.completed, 0);
        assert!(snapshot
            .recent
            .iter()
            .all(|entry| entry.gpu_elapsed_us.is_none()));
        assert!(!snapshot.observation_fault);
    }

    #[test]
    fn invalid_context_and_counter_or_identity_exhaustion_remain_explicit() {
        let records = CommandRecords::new().unwrap();
        let mut invalid = context(1);
        invalid[0].source_frame = Some(f64::NAN);
        assert!(records.begin(CommandStage::Unclassified, invalid).is_none());
        let snapshot = records.snapshot();
        assert_eq!(snapshot.counts.omitted_invalid_context, 1);
        assert!(snapshot.observation_fault);
        assert!(snapshot.active.is_empty());

        let records = CommandRecords::new().unwrap();
        records.lock().counts.started = u64::MAX;
        let contexts = vec![context(9)[0].clone(); MAX_GPU_COMMAND_CONTEXTS + 2];
        let ticket = records
            .begin(CommandStage::ExposurePhase, contexts)
            .unwrap();
        let snapshot = records.snapshot();
        assert!(snapshot.counts.saturated);
        assert_eq!(snapshot.counts.started, u64::MAX);
        assert_eq!(snapshot.active[0].contexts.len(), MAX_GPU_COMMAND_CONTEXTS);
        assert_eq!(snapshot.active[0].omitted_contexts, 2);
        assert_eq!(snapshot.counts.omitted_contexts, 2);
        drop(ticket);
        records.lock().next = u64::MAX;
        assert!(records.begin(CommandStage::Unclassified, vec![]).is_none());
        assert_eq!(records.snapshot().counts.omitted_identity_exhausted, 1);
        assert!(records.snapshot().active.is_empty());
    }

    // These are metadata/state-machine tests. Synthetic words below do not
    // constitute native timestamp acceptance; separate native tests own that.
    fn outcome(records: &Arc<CommandRecords>, valid: bool) {
        let ticket = records
            .begin(CommandStage::ReadbackCopy, context(71))
            .unwrap();
        assert!(ticket.encoded());
        assert!(ticket.submit_attempted());
        assert!(ticket.mapping());
        ticket.mapped(Some(if valid { [11, 22] } else { [0, 0] }), 1.0, None);
    }
    #[test]
    fn deferred_readback_episodes_escalate_once_and_unknown_work_cannot_fake_recovery() {
        let records = CommandRecords::new().unwrap();
        for _ in 0..120 {
            outcome(&records, false);
        }
        let events = records.take_logs().events;
        assert_eq!(events.len(), 2);
        assert_eq!(
            (events[0].level, events[0].occurrence),
            (log::Level::Debug, 1)
        );
        assert_eq!(
            (events[1].level, events[1].occurrence),
            (log::Level::Error, 60)
        );
        assert_eq!(events[1].record.contexts[0].request, Some(71));
        assert!(records.take_logs().events.is_empty());
        outcome(&records, true);
        assert_eq!(records.snapshot().consecutive_readback_failures, 0);
        for _ in 0..60 {
            outcome(&records, false);
        }
        assert_eq!(records.take_logs().events.len(), 2);
        records.omit_slot();
        assert_eq!(records.snapshot().consecutive_readback_failures, 60);
        drop(records.begin(CommandStage::Unclassified, vec![]).unwrap());
        assert_eq!(records.snapshot().consecutive_readback_failures, 60);
        let ticket = records
            .begin(CommandStage::ReadbackCopy, context(72))
            .unwrap();
        ticket.encoded();
        ticket.submit_attempted();
        ticket.mapping();
        ticket.cancelled(Some("actual owner cancellation".into()));
        assert_eq!(records.snapshot().consecutive_readback_failures, 0);
        for _ in 0..59 {
            outcome(&records, false);
        }
        assert_eq!(records.take_logs().events.len(), 1);
        drop(records.begin(CommandStage::Unclassified, vec![]).unwrap());
        assert_eq!(records.snapshot().consecutive_readback_failures, 59);
        outcome(&records, false);
        let events = records.take_logs().events;
        assert_eq!(events.len(), 1);
        assert_eq!(
            (events[0].level, events[0].occurrence),
            (log::Level::Error, 60)
        );
        outcome(&records, true);
        assert_eq!(records.snapshot().consecutive_readback_failures, 0);
        assert_eq!(records.snapshot().counts.cancelled, 1);
        assert!(!records
            .take_logs()
            .events
            .iter()
            .any(|event| event.record.contexts[0].request == Some(72)));
    }
    #[test]
    fn deferred_events_and_original_causes_are_bounded_and_omissions_remain_visible() {
        let records = CommandRecords::new().unwrap();
        for _ in 0..100 {
            let ticket = records
                .begin(CommandStage::ReadbackCopy, context(81))
                .unwrap();
            ticket.encoded();
            ticket.submit_attempted();
            ticket.mapping();
            ticket.mapped(None, 1.0, Some("x".repeat(1024)));
            outcome(&records, true);
        }
        let batch = records.take_logs();
        assert_eq!(batch.events.len(), MAX_PENDING_LOG_EVENTS);
        assert_eq!(batch.omitted, 100 - MAX_PENDING_LOG_EVENTS as u64);
        assert_eq!(records.snapshot().counts.omitted_log_events, batch.omitted);
        assert_eq!(records.take_logs().omitted, 0);
        assert!(batch.events.iter().all(|event| event
            .record
            .cause
            .as_ref()
            .unwrap()
            .chars()
            .count()
            == 256));
    }

    #[test]
    fn mixed_failure_streak_saturation_is_visible_and_recovery_does_not_erase_it() {
        let records = CommandRecords::new().unwrap();
        records.lock().consecutive_failures = u64::MAX;
        outcome(&records, false);
        let snapshot = records.snapshot();
        assert_eq!(snapshot.consecutive_readback_failures, u64::MAX);
        assert_eq!(snapshot.counts.invalid, 1);
        assert!(snapshot.counts.saturated);
        assert!(records.take_logs().events.is_empty());
        outcome(&records, true);
        assert_eq!(records.snapshot().consecutive_readback_failures, 0);
        assert!(records.snapshot().counts.saturated);
    }
}
