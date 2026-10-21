//! A bounded, owned snapshot worker; shutdown cancels and joins its source IO.
use super::*;
use motion_desktop_media::movie::ExportControl;

#[cfg(test)]
mod tests;

pub(super) struct Worker {
    active: Option<(
        JoinHandle<()>,
        ExportControl,
        Option<Flight>,
        Arc<Mutex<bool>>,
    )>,
    flight: Arc<Mutex<Option<Flight>>>,
}

impl Worker {
    pub(super) fn new(flight: Arc<Mutex<Option<Flight>>>) -> Self {
        Self {
            active: None,
            flight,
        }
    }

    // Check before capturing source handles, not merely before spawning.
    pub(super) fn available(&mut self) -> Result<(), String> {
        if self
            .active
            .as_ref()
            .is_some_and(|(worker, _, _, _)| !worker.is_finished())
        {
            return Err("movie freeze is still running or cancelling".into());
        }
        if let Some((worker, _, flight, _)) = self.active.take() {
            let result = worker.join().map_err(|payload| {
                format!(
                    "movie freeze worker closure failed: request={:?} cause={}",
                    flight.map(|flight| flight.request),
                    motion_media::panic_reason(payload)
                )
            });
            set_flight(&self.flight, None);
            result?;
        }
        Ok(())
    }

    pub(super) fn status(&self) -> Value {
        static FAILURES: crate::fallback_observation::Counter =
            crate::fallback_observation::Counter::new();
        let flight = crate::fallback_observation::recovered(
            &self.flight,
            &FAILURES,
            "movie.freeze.status",
            |flight| *flight,
        );
        match (&self.active, flight) {
            (Some((worker, control, _, _)), Some(flight)) => serde_json::json!({
                "request":flight.request, "operation":flight.operation,
                "elapsedMs":flight.started.elapsed().as_millis(), "progress":control.progress(),
                "workerFinished":worker.is_finished()}),
            _ => Value::Null,
        }
    }

    pub(super) fn start<T: Send + 'static>(
        &mut self,
        reply: Sender<Result<T, String>>,
        control: ExportControl,
        freeze: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> Result<(), String> {
        self.available()?;
        let flight = CURRENT_COMMAND.get();
        set_flight(&self.flight, flight);
        let observation = self.flight.clone();
        let completed = Arc::new(Mutex::new(false));
        let completion = completed.clone();
        let worker = match std::thread::Builder::new()
            .name("motion-freeze-movie".into())
            .spawn(move || {
                CURRENT_COMMAND.set(flight);
                let began = Instant::now();
                if let Some(flight) = flight {
                    log::info!(
                        "async start #{} {} phase=freeze_sources",
                        flight.request,
                        flight.operation
                    );
                    crate::logging::flush();
                }
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(freeze))
                    .unwrap_or_else(|payload| {
                        Err(format!(
                            "movie freeze worker failed: {}",
                            motion_media::panic_reason(payload)
                        ))
                    });
                // Handoff and shutdown cancellation share a short gate. Once
                // the result is ready, closing this engine must not cancel a
                // caller's subsequent independent export using the same token.
                completion_gate(&completion, |complete| *complete = true);
                respond_movie(reply, result.map_err(motion_host::Error::from));
                if let Some(flight) = flight {
                    log::debug!(
                        "async done #{} {} {}ms",
                        flight.request,
                        flight.operation,
                        began.elapsed().as_millis()
                    );
                }
                set_flight(&observation, None);
                CURRENT_COMMAND.set(None);
            }) {
            Ok(worker) => worker,
            Err(error) => {
                set_flight(&self.flight, None);
                return Err(format!("movie freeze worker spawn failed: {error}"));
            }
        };
        self.active = Some((worker, control, flight, completed));
        Ok(())
    }
}

fn completion_gate(completed: &Mutex<bool>, operation: impl FnOnce(&mut bool)) {
    static FAILURES: crate::fallback_observation::Counter =
        crate::fallback_observation::Counter::new();
    crate::fallback_observation::recovered(
        completed,
        &FAILURES,
        "movie.freeze.completion",
        operation,
    );
}

fn set_flight(observation: &Mutex<Option<Flight>>, flight: Option<Flight>) {
    static FAILURES: crate::fallback_observation::Counter =
        crate::fallback_observation::Counter::new();
    crate::fallback_observation::recovered(
        observation,
        &FAILURES,
        "movie.freeze.flight",
        |value| *value = flight,
    );
}

pub(super) fn respond_movie<T>(reply: Sender<Result<T, String>>, result: motion_host::Result<T>) {
    if result
        .as_ref()
        .err()
        .is_some_and(|error| error.to_string() == "movie export cancelled")
    {
        if let Some(flight) = CURRENT_COMMAND.get() {
            log::info!(
                "cancelled #{} {} {}ms",
                flight.request,
                flight.operation,
                flight.started.elapsed().as_millis()
            );
        }
        // Cancellation is a normal terminal result; preserve the wire reply.
        let _ = reply.send(result.map_err(String::from));
    } else {
        respond(reply, result);
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if let Some((worker, control, flight, completed)) = self.active.take() {
            completion_gate(&completed, |complete| {
                if !*complete {
                    control.cancel();
                }
            });
            if !worker.is_finished() {
                log::info!(
                    "movie freeze shutdown start request={:?} phase=join elapsed_ms={:?}",
                    flight.map(|flight| flight.request),
                    flight.map(|flight| flight.started.elapsed().as_millis())
                );
                crate::logging::flush();
            }
            if let Err(payload) = worker.join() {
                log::error!(
                    "movie freeze shutdown failed request={:?} phase=join cause={}",
                    flight.map(|flight| flight.request),
                    motion_media::panic_reason(payload)
                );
            }
            set_flight(&self.flight, None);
        }
    }
}
