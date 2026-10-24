//! Explicit recording, separate from the small watchdog ring. No native handles,
//! file IO or logging are retained or invoked, including in late GPU callbacks.
use super::*;

pub const MAX_GPU_TRACE_COMMANDS: usize = 8192;
pub const MAX_GPU_TRACE_METADATA_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandTraceSnapshot {
    pub epoch: u64,
    pub window: u64,
    pub observation_fault: bool,
    /// Exclusive lower bound in this epoch's command-ID domain.
    pub started_after_command: u64,
    /// Inclusive upper bound; None means the creation window remains open.
    pub stopped_at_command: Option<u64>,
    pub excluded_active_at_start: usize,
    pub started_commands: u64,
    /// Epoch-wide counters, including observations omitted before an ID exists.
    /// They are not exclusive per-window GPU costs or counts.
    pub epoch_counts_at_start: CommandCounts,
    pub epoch_counts_at_snapshot: CommandCounts,
    /// Includes reserved record slots and retained string/context byte lengths;
    /// excludes allocator overhead, live state and caller-owned snapshot copies.
    pub retained_metadata_bytes: usize,
    pub max_metadata_bytes: usize,
    pub max_commands: usize,
    pub truncated: bool,
    pub omitted_records: u64,
    pub first_omitted_command: Option<u64>,
    pub last_omitted_command: Option<u64>,
    pub counter_saturated: bool,
    /// Completion order, preserving the first bounded prefix on overflow.
    pub records: Vec<CommandRecord>,
    /// Included commands still pending; stop never fabricates their completion.
    pub active: Vec<CommandRecord>,
}

pub(super) struct CommandTrace {
    window: u64,
    after: u64,
    stop: Option<u64>,
    excluded: usize,
    counts_at_start: CommandCounts,
    bytes: usize,
    max_bytes: usize,
    max_commands: usize,
    omitted: u64,
    first_omitted: Option<u64>,
    last_omitted: Option<u64>,
    saturated: bool,
    records: Vec<CommandRecord>,
}
impl CommandTrace {
    fn new(window: u64, state: &State) -> Result<Self, crate::RenderError> {
        let mut records = Vec::new();
        records
            .try_reserve_exact(MAX_GPU_TRACE_COMMANDS)
            .map_err(|cause| {
                crate::RenderError::Invalid(format!(
                    "GPU command trace metadata allocation failed: {cause}"
                ))
            })?;
        let bytes = records.capacity() * std::mem::size_of::<CommandRecord>();
        if bytes > MAX_GPU_TRACE_METADATA_BYTES {
            return Err(crate::RenderError::Invalid(
                "GPU command trace metadata exceeds its limit".into(),
            ));
        }
        Ok(Self {
            window,
            after: state.next,
            stop: None,
            excluded: state.active.len(),
            counts_at_start: state.counts.clone(),
            bytes,
            max_bytes: MAX_GPU_TRACE_METADATA_BYTES,
            max_commands: MAX_GPU_TRACE_COMMANDS,
            omitted: 0,
            first_omitted: None,
            last_omitted: None,
            saturated: false,
            records,
        })
    }
    fn includes(&self, id: u64) -> bool {
        id > self.after && self.stop.is_none_or(|stop| id <= stop)
    }
    pub(super) fn retain(&mut self, record: &CommandRecord) {
        if !self.includes(record.id) {
            return;
        }
        // Clone allocates exactly the retained string/context lengths. The
        // record slots themselves were admitted once when opening the trace.
        let extra = record.contexts.len() * std::mem::size_of::<CommandContext>()
            + record.cause.as_ref().map_or(0, String::len)
            + record
                .input
                .as_ref()
                .and_then(|input| input.composition.as_ref())
                .map_or(0, String::len);
        let next = self.bytes.checked_add(extra);
        if self.omitted != 0
            || self.records.len() == self.max_commands
            || next.is_none_or(|bytes| bytes > self.max_bytes)
        {
            self.saturated |= increment(&mut self.omitted, 1);
            self.first_omitted.get_or_insert(record.id);
            self.last_omitted = Some(record.id);
            return;
        }
        if let Some(next) = next {
            self.bytes = next;
            self.records.push(record.clone());
        }
    }
}

impl CommandRecords {
    pub(crate) fn start_trace(&self) -> Result<u64, crate::RenderError> {
        let mut state = self.lock();
        let window = state.trace_sequence.checked_add(1).ok_or_else(|| {
            crate::RenderError::Invalid("GPU command trace identity exhausted".into())
        })?;
        // Failure preserves the previous trace and never changes native work.
        let trace = CommandTrace::new(window, &state)?;
        state.trace_sequence = window;
        state.trace = Some(trace);
        Ok(window)
    }
    pub(crate) fn stop_trace(&self) -> Option<u64> {
        let mut state = self.lock();
        let end = state.next;
        let trace = state.trace.as_mut()?;
        // Idempotent: later unrelated work cannot extend a stopped window.
        trace.stop.get_or_insert(end);
        Some(trace.window)
    }
    pub(crate) fn trace_snapshot(&self) -> Option<CommandTraceSnapshot> {
        let state = self.lock();
        let trace = state.trace.as_ref()?;
        Some(CommandTraceSnapshot {
            epoch: self.epoch,
            window: trace.window,
            observation_fault: self.fault.load(Ordering::Relaxed),
            started_after_command: trace.after,
            stopped_at_command: trace.stop,
            excluded_active_at_start: trace.excluded,
            started_commands: trace.stop.unwrap_or(state.next) - trace.after,
            epoch_counts_at_start: trace.counts_at_start.clone(),
            epoch_counts_at_snapshot: state.counts.clone(),
            retained_metadata_bytes: trace.bytes,
            max_metadata_bytes: trace.max_bytes,
            max_commands: trace.max_commands,
            truncated: trace.omitted != 0,
            omitted_records: trace.omitted,
            first_omitted_command: trace.first_omitted,
            last_omitted_command: trace.last_omitted,
            counter_saturated: trace.saturated,
            records: trace.records.clone(),
            active: state
                .active
                .values()
                .filter(|active| trace.includes(active.info.id))
                .map(|active| {
                    let mut record = active.info.clone();
                    record.elapsed_us = micros(active.began);
                    record
                })
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn discard(records: &Arc<CommandRecords>) -> u64 {
        let ticket = records.begin(CommandStage::ReadbackCopy, vec![]).unwrap();
        ticket.id()
    }
    #[test]
    fn full_trace_retains_early_records_after_recent_eviction_and_preserves_exact_windows() {
        let records = CommandRecords::new().unwrap();
        assert!(records.trace_snapshot().is_none());
        let old = records.begin(CommandStage::AuthoringInput, vec![]).unwrap();
        let old_id = old.id();
        let window = records.start_trace().unwrap();
        drop(old);
        for _ in 0..300 {
            discard(&records);
        }
        let late = records.begin(CommandStage::ExposurePhase, vec![]).unwrap();
        assert_eq!(records.stop_trace(), Some(window));
        let stopped = records.trace_snapshot().unwrap();
        assert_eq!(stopped.records.len(), 300);
        assert_eq!(stopped.active.len(), 1);
        assert_eq!(stopped.started_commands, 301);
        assert_eq!(stopped.excluded_active_at_start, 1);
        assert_eq!(stopped.started_after_command, old_id);
        assert!(stopped.records.iter().all(|record| record.id > old_id));
        let unrelated = discard(&records);
        assert_eq!(records.stop_trace(), Some(window));
        assert_eq!(
            records.trace_snapshot().unwrap().stopped_at_command,
            stopped.stopped_at_command
        );
        drop(late);
        let trace = records.trace_snapshot().unwrap();
        assert_eq!(trace.records.len(), 301);
        assert!(trace.active.is_empty());
        assert!(trace.records.iter().all(|record| record.id != unrelated));
        assert_eq!(records.snapshot().recent.len(), MAX_RECENT_GPU_COMMANDS);
        assert!(!trace.truncated);
        assert!(trace
            .records
            .iter()
            .all(|record| record.gpu_elapsed_us.is_none()));
        let json = serde_json::to_value(&trace).unwrap();
        assert!(json["records"][0].get("elapsedUs").is_some());
        assert!(json["records"][0].get("cpuElapsedUs").is_none());
        // A new trace excludes the old in-flight ticket even if it finishes later.
        let old = records.begin(CommandStage::ExposurePhase, vec![]).unwrap();
        let newer = records.start_trace().unwrap();
        assert_ne!(window, newer);
        drop(old);
        discard(&records);
        assert_eq!(records.trace_snapshot().unwrap().records.len(), 1);
    }
    #[test]
    fn trace_limits_keep_a_completion_prefix_and_make_every_omission_visible() {
        let records = CommandRecords::new().unwrap();
        records.start_trace().unwrap();
        for _ in 0..MAX_GPU_TRACE_COMMANDS + 3 {
            discard(&records);
        }
        let trace = records.trace_snapshot().unwrap();
        assert!(trace.truncated);
        assert_eq!(trace.records.len(), MAX_GPU_TRACE_COMMANDS);
        assert_eq!(trace.omitted_records, 3);
        assert_eq!(
            trace.first_omitted_command,
            Some(MAX_GPU_TRACE_COMMANDS as u64 + 1)
        );
        assert_eq!(
            trace.last_omitted_command,
            Some(MAX_GPU_TRACE_COMMANDS as u64 + 3)
        );
        assert_eq!(trace.started_commands, MAX_GPU_TRACE_COMMANDS as u64 + 3);
        records.start_trace().unwrap();
        let contexts = vec![
            CommandContext {
                allocation_frame: 1,
                request: Some(99),
                operation: FrameOperation::Present,
                source_frame: Some(2.0),
                exposure_request: None,
                exposure_poll: None,
            };
            MAX_GPU_COMMAND_CONTEXTS
        ];
        let mut attempts = 0;
        while records.lock().trace.as_ref().unwrap().omitted == 0 {
            let ticket = records
                .begin(CommandStage::Composition, contexts.clone())
                .unwrap();
            drop(ticket);
            attempts += 1;
        }
        let before = records.trace_snapshot().unwrap().records.len();
        assert!(before > 0 && before < MAX_GPU_TRACE_COMMANDS);
        discard(&records); // Smaller later records must not conceal the lost prefix.
        let trace = records.trace_snapshot().unwrap();
        assert_eq!(trace.records.len(), before);
        assert_eq!(trace.omitted_records, 2);
        assert!(trace.retained_metadata_bytes <= trace.max_metadata_bytes);
        assert_eq!(trace.max_metadata_bytes, MAX_GPU_TRACE_METADATA_BYTES);
        assert_eq!(
            records.snapshot().counts.discarded,
            (MAX_GPU_TRACE_COMMANDS + 4 + attempts) as u64
        );
        assert!(!records.snapshot().observation_fault);
    }
    #[test]
    fn trace_identity_exhaustion_preserves_prior_evidence_and_native_state() {
        let records = CommandRecords::new().unwrap();
        let window = records.start_trace().unwrap();
        discard(&records);
        records.lock().trace_sequence = u64::MAX;
        assert!(records.start_trace().is_err());
        assert_eq!(records.trace_snapshot().unwrap().window, window);
        let ticket = records.begin(CommandStage::Composition, vec![]).unwrap();
        assert_eq!(records.snapshot().active.len(), 1);
        drop(ticket);
        assert_eq!(records.trace_snapshot().unwrap().records.len(), 2);
        records.lock().trace.as_mut().unwrap().omitted = u64::MAX;
        discard(&records);
        let trace = records.trace_snapshot().unwrap();
        assert_eq!(trace.omitted_records, u64::MAX);
        assert!(trace.counter_saturated);
        assert_eq!(trace.records.len(), 2);
        assert_ne!(CommandRecords::new().unwrap().snapshot().epoch, trace.epoch);
    }
}
