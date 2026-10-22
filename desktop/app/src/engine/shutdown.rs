//! Publish cleanup independently of the command that made the worker stop.
use super::*;

pub(super) struct Cleanup {
    state: Arc<Mutex<Option<Flight>>>,
    diagnostics: Arc<crate::diagnostics::Diagnostics>,
    flight: Flight,
    context: crate::diagnostics::Context,
}

impl Cleanup {
    pub(super) fn begin(
        state: Arc<Mutex<Option<Flight>>>,
        diagnostics: Arc<crate::diagnostics::Diagnostics>,
        session: i64,
    ) -> Self {
        let flight = Flight {
            request: diagnostics.next_request(),
            operation: "shutdown_cleanup",
            started: Instant::now(),
        };
        static FAILURES: crate::fallback_observation::Counter =
            crate::fallback_observation::Counter::new();
        crate::fallback_observation::recovered(
            &state,
            &FAILURES,
            "engine.flight.cleanup_start",
            |current| *current = Some(flight),
        );
        log::info!(
            "start #{} shutdown_cleanup session={session}",
            flight.request
        );
        crate::logging::flush();
        let context = diagnostics.context();
        Self {
            state,
            diagnostics,
            flight,
            context,
        }
    }
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        static FAILURES: crate::fallback_observation::Counter =
            crate::fallback_observation::Counter::new();
        crate::fallback_observation::recovered(
            &self.state,
            &FAILURES,
            "engine.flight.cleanup_done",
            |current| *current = None,
        );
        let elapsed = self.flight.started.elapsed();
        self.diagnostics.record(
            self.flight.request,
            "worker",
            self.flight.operation,
            elapsed,
            None,
            Some(elapsed),
            self.context.clone(),
        );
        if std::thread::panicking() {
            log::error!("cleanup #{} shutdown_cleanup {}ms: worker unwinding; original cause recorded by panic hook", self.flight.request, elapsed.as_millis());
        } else {
            log::debug!(
                "done #{} shutdown_cleanup {}ms",
                self.flight.request,
                elapsed.as_millis()
            );
        }
    }
}

#[cfg(test)]
mod tests;
