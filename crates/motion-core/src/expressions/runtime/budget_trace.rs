//! Failure-only context for the existing deadlines; does not adjust either clock.
use super::*;
use std::sync::atomic::AtomicU8;

#[derive(Clone, Copy)]
pub(super) enum Phase {
    Context = 0,
    Bindings = 1,
    Execution = 2,
}
struct Failure {
    phase: u8,
    cause: &'static str,
    elapsed_us: u128,
}
pub(super) struct BudgetTrace {
    began: Instant,
    frame_remaining_us: u128,
    phase: AtomicU8,
    failure: OnceLock<Failure>,
}
impl BudgetTrace {
    pub(super) fn new(frame_deadline: Instant) -> Self {
        let began = Instant::now();
        Self {
            began,
            frame_remaining_us: frame_deadline.saturating_duration_since(began).as_micros(),
            phase: AtomicU8::new(Phase::Context as u8),
            failure: OnceLock::new(),
        }
    }
    pub(super) fn phase(&self, phase: Phase) {
        self.phase.store(phase as u8, Ordering::Relaxed);
    }
    pub(super) fn record(&self, now: Instant, cancelled: bool, frame_expired: bool) {
        let _ = self.failure.set(Failure {
            phase: self.phase.load(Ordering::Relaxed),
            cause: if cancelled { "cancelled" } else if frame_expired { "frame_deadline" } else { "execution_deadline" },
            elapsed_us: now.saturating_duration_since(self.began).as_micros(),
        });
    }
    pub(super) fn error(&self) -> Error {
        let details = self.failure.get().map(|failure| {
            let phase = match failure.phase {
                0 => "context_setup",
                1 => "bindings_and_module",
                _ => "execution",
            };
            format!("phase={phase} cause={} elapsed_us={} frame_remaining_at_entry_us={}",
                failure.cause, failure.elapsed_us, self.frame_remaining_us)
        }).unwrap_or_else(|| "phase=admission cause=unavailable".into());
        Error::Invalid(format!("expression cancelled or execution budget exceeded ({details})"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interruption_keeps_first_phase_cause_and_elapsed_without_payload() {
        for (phase, name, cancelled, frame_expired, cause) in [
            (Phase::Context, "context_setup", false, true, "frame_deadline"),
            (Phase::Bindings, "bindings_and_module", true, true, "cancelled"),
            (Phase::Execution, "execution", false, false, "execution_deadline"),
        ] {
            let mut trace = BudgetTrace::new(Instant::now() + Duration::from_millis(100));
            let began = Instant::now();
            trace.began = began;
            trace.frame_remaining_us = 100_000;
            trace.phase(phase);
            trace.record(began + Duration::from_millis(21), cancelled, frame_expired);
            trace.phase(Phase::Execution);
            trace.record(began + Duration::from_millis(300), true, true);
            assert_eq!(trace.error().to_string(), format!(
                "expression cancelled or execution budget exceeded (phase={name} cause={cause} elapsed_us=21000 frame_remaining_at_entry_us=100000)"
            ));
        }
    }
}
