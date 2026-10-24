//! Drain only after releasing host/business guards. Callbacks enqueue bounded
//! events; they never enter user log sinks or do file/console IO themselves.
use super::{CommandPhase, GpuCommandObserver};

impl GpuCommandObserver {
    /// Explicit consumers must call this beyond every business guard, including
    /// during unwind. The host's LogScope supplies that ordering automatically.
    pub fn flush_logs(&self, session: Option<i64>) {
        let batch = self.records.take_logs();
        // take_logs releases the observation lock before any host-owned sink.
        for event in batch.events {
            let record = event.record;
            let cause =
                record
                    .cause
                    .as_deref()
                    .unwrap_or(if record.phase == CommandPhase::Invalid {
                        "invalid native timestamp words"
                    } else {
                        "native mapping failed without a retained cause"
                    });
            log::log!(event.level,
                "render GPU command readback session={session:?} epoch={} command={} stage={:?} phase={:?} exposure={:?} input={:?} contexts={:?} elapsed_us={} count={} words={:?} cause={cause}",
                record.epoch, record.id, record.stage, record.phase, record.exposure, record.input, record.contexts,
                record.cpu_elapsed_us, event.occurrence, record.timestamp_words);
        }
        if batch.invariant_fault {
            log::error!("render GPU command observation invariant session={session:?} epoch={} phase=metadata exact_coverage=false", batch.epoch);
        }
        if batch.omitted != 0 {
            log::warn!("render GPU command log events omitted session={session:?} epoch={} omitted={} phase=deferred_log exact_log_coverage=false", batch.epoch, batch.omitted);
        }
    }
}

/// Bind before taking the Session/business mutex, then watch the old and new
/// observers around an operation. Observers retain bounded metadata, no native
/// buffers. Drop flushes beyond those guards on success, error and unwind.
#[must_use = "bind before acquiring business guards"]
pub struct GpuCommandLogScope {
    session: Option<i64>,
    observers: [Option<GpuCommandObserver>; 4],
    omitted: usize,
}
impl GpuCommandLogScope {
    pub fn new(session: Option<i64>) -> Self {
        Self {
            session,
            observers: std::array::from_fn(|_| None),
            omitted: 0,
        }
    }
    pub fn watch(&mut self, observer: Option<GpuCommandObserver>) {
        let Some(observer) = observer else {
            return;
        };
        if self
            .observers
            .iter()
            .flatten()
            .any(|old| old.records.epoch() == observer.records.epoch())
        {
            return;
        }
        if let Some(slot) = self.observers.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(observer);
        } else {
            self.omitted = self.omitted.saturating_add(1);
        }
    }
    pub fn flush(&mut self) {
        for slot in &mut self.observers {
            if let Some(observer) = slot.take() {
                observer.flush_logs(self.session);
            }
        }
        let omitted = std::mem::take(&mut self.omitted);
        if omitted != 0 {
            log::warn!("render GPU command observer scopes omitted session={:?} omitted={} phase=deferred_log", self.session, omitted);
        }
    }
}
impl Drop for GpuCommandLogScope {
    fn drop(&mut self) {
        self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::super::{records::CommandRecords, CommandStage};
    use super::*;
    use std::sync::{Arc, Mutex, TryLockError};
    struct Capture {
        business: Arc<Mutex<()>>,
        observer: GpuCommandObserver,
        events: Mutex<Vec<(log::Level, String)>>,
    }
    impl log::Log for Capture {
        fn enabled(&self, _: &log::Metadata<'_>) -> bool {
            true
        }
        fn log(&self, record: &log::Record<'_>) {
            let text = record.args().to_string();
            if text.starts_with("render GPU command ") {
                assert!(
                    !matches!(self.business.try_lock(), Err(TryLockError::WouldBlock)),
                    "business guard held in sink"
                );
                assert!(
                    self.observer.records.lock_available(),
                    "observation guard held in sink"
                );
                self.observer.snapshot(); // A user-owned sink may inspect live state.
                self.events.lock().unwrap().push((record.level(), text));
            }
        }
        fn flush(&self) {}
    }
    #[test]
    fn scoped_logs_filter_default_debug_and_flush_after_guards_even_during_unwind() {
        const TEST: &str = "gpu_commands::logging::tests::scoped_logs_filter_default_debug_and_flush_after_guards_even_during_unwind";
        const CHILD: &str = "MOTION_GPU_COMMAND_LOG_SCOPE_TEST";
        let Ok(mode) = std::env::var(CHILD) else {
            for mode in ["info", "debug", "unwind"] {
                let output = std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
                    .env(CHILD, mode)
                    .env_remove("MOTION_LOG")
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{mode}\n{}\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(!String::from_utf8_lossy(&output.stdout)
                    .contains("render GPU command readback"));
            }
            return;
        };
        let records = CommandRecords::new().unwrap();
        let observer = GpuCommandObserver {
            records: records.clone(),
            readback: Arc::default(),
            states: vec![],
        };
        let business = Arc::new(Mutex::new(()));
        let capture = Box::leak(Box::new(Capture {
            business: business.clone(),
            observer: observer.clone(),
            events: Mutex::default(),
        }));
        log::set_logger(capture).unwrap();
        log::set_max_level(if mode == "debug" {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        });
        let operation = || {
            let mut logs = GpuCommandLogScope::new(Some(91));
            logs.watch(Some(observer.clone()));
            logs.watch(Some(observer.clone()));
            let _guard = business.lock().unwrap();
            for _ in 0..120 {
                let ticket = records.begin(CommandStage::ExposureMedia, vec![]).unwrap();
                ticket.encoded();
                ticket.submit_attempted();
                ticket.mapping();
                ticket.mapped(None, 1.0, Some("original map failure".into()));
            }
            assert!(
                capture.events.lock().unwrap().is_empty(),
                "callbacks must only defer events"
            );
            if mode == "unwind" {
                panic!("controlled owner unwind");
            }
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation));
        assert_eq!(result.is_err(), mode == "unwind");
        let events = capture.events.lock().unwrap();
        assert_eq!(events.len(), if mode == "debug" { 2 } else { 1 });
        let (level, text) = events.last().unwrap();
        assert_eq!(*level, log::Level::Error);
        for context in [
            "session=Some(91)",
            "count=60",
            "ExposureMedia",
            "cause=original map failure",
        ] {
            assert!(text.contains(context));
        }
        drop(events);
        observer.flush_logs(Some(91));
        assert_eq!(
            capture.events.lock().unwrap().len(),
            if mode == "debug" { 2 } else { 1 }
        );
    }
}
