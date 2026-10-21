//! One independent PNG export per engine, including cancelling native work.
use super::*;
use motion_desktop_media::movie::{frame::FrameFreezePlan, ExportControl};

pub(super) struct Worker {
    active: Option<(JoinHandle<()>, ExportControl, Option<Flight>)>,
    flight: Arc<Mutex<Option<Flight>>>,
}
impl Worker {
    pub(super) fn new(flight: Arc<Mutex<Option<Flight>>>) -> Self {
        Self {
            active: None,
            flight,
        }
    }
    pub(super) fn status(&self) -> Value {
        static FAILURES: crate::fallback_observation::Counter =
            crate::fallback_observation::Counter::new();
        let flight = crate::fallback_observation::recovered(
            &self.flight,
            &FAILURES,
            "frame.flight.status",
            |flight| *flight,
        );
        match (&self.active, flight) {
            (Some((worker, control, _)), Some(flight)) => serde_json::json!({
                "request":flight.request, "operation":flight.operation,
                "elapsedMs":flight.started.elapsed().as_millis(), "progress":control.progress(),
                "workerFinished":worker.is_finished()}),
            _ => Value::Null,
        }
    }
    pub(super) fn start(
        &mut self,
        id: i64,
        output: Option<PathBuf>,
        control: ExportControl,
        reply: Sender<Result<Value, String>>,
    ) -> Result<(), String> {
        if let Some((worker, _, _)) = &self.active {
            if !worker.is_finished() {
                return Err("PNG export is still running or cancelling".into());
            }
        }
        if let Some((worker, _, flight)) = self.active.take() {
            let result = worker.join().map_err(|payload| {
                format!(
                    "PNG worker closure failed: request={:?} cause={}",
                    flight.map(|flight| flight.request),
                    motion_media::panic_reason(payload)
                )
            });
            clear_flight(&self.flight);
            result?;
        }
        let plan = motion_host::session::with_session(id, |session| {
            FrameFreezePlan::capture(session, output, &control).map_err(motion_host::Error::from)
        })?;
        let flight = CURRENT_COMMAND.get();
        let observation = self.flight.clone();
        let cancel = control.clone();
        let worker = std::thread::Builder::new()
            .name("motion-frame-export".into())
            .spawn(move || {
                CURRENT_COMMAND.set(flight);
                static FAILURES: crate::fallback_observation::Counter =
                    crate::fallback_observation::Counter::new();
                crate::fallback_observation::recovered(
                    &observation,
                    &FAILURES,
                    "frame.flight.start",
                    |state| *state = flight,
                );
                let began = Instant::now();
                if let Some(flight) = flight {
                    log::info!(
                        "async start #{} {} phase=freeze_render_publish",
                        flight.request,
                        flight.operation
                    );
                    crate::logging::flush();
                }
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| plan.export(&cancel)))
                        .unwrap_or_else(|payload| {
                            Err(format!(
                                "PNG export worker failed: {}",
                                motion_media::panic_reason(payload)
                            ))
                        });
                respond_frame(reply, result);
                if let Some(flight) = flight {
                    log::debug!(
                        "async done #{} {} {}ms",
                        flight.request,
                        flight.operation,
                        began.elapsed().as_millis()
                    );
                }
                crate::fallback_observation::recovered(
                    &observation,
                    &FAILURES,
                    "frame.flight.finish",
                    |state| *state = None,
                );
                CURRENT_COMMAND.set(None);
            })
            .map_err(|error| format!("PNG export worker spawn failed: {error}"))?;
        self.active = Some((worker, control, flight));
        Ok(())
    }
}
pub(super) fn respond_frame(reply: Sender<Result<Value, String>>, result: Result<Value, String>) {
    // Cancellation can happen before the serial worker captures source handles,
    // during freezing, or during rendering. All three are normal terminal paths.
    if result.as_ref().err().is_some_and(|error| {
        matches!(
            error.as_str(),
            "PNG export cancelled" | "movie export cancelled"
        )
    }) {
        if let Some(flight) = CURRENT_COMMAND.get() {
            log::info!(
                "cancelled #{} {} {}ms",
                flight.request,
                flight.operation,
                flight.started.elapsed().as_millis()
            );
        }
        let _ = reply.send(Err("PNG export cancelled".into()));
    } else {
        respond(reply, result.map_err(motion_host::Error::from));
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Some((worker, control, flight)) = self.active.take() {
            control.cancel();
            if !worker.is_finished() {
                log::info!(
                    "PNG export shutdown start request={:?} phase=join elapsed_ms={:?}",
                    flight.map(|flight| flight.request),
                    flight.map(|flight| flight.started.elapsed().as_millis())
                );
                crate::logging::flush();
            }
            let began = Instant::now();
            if let Err(payload) = worker.join() {
                log::error!(
                    "PNG export shutdown failed request={:?} phase=join elapsed_ms={:?} cause={}",
                    flight.map(|flight| flight.request),
                    flight.map(|flight| flight.started.elapsed().as_millis()),
                    motion_media::panic_reason(payload)
                );
            }
            clear_flight(&self.flight);
            log::debug!(
                "PNG export shutdown done request={:?} phase=join join_ms={}",
                flight.map(|flight| flight.request),
                began.elapsed().as_millis()
            );
        }
    }
}

fn clear_flight(flight: &Mutex<Option<Flight>>) {
    static FAILURES: crate::fallback_observation::Counter =
        crate::fallback_observation::Counter::new();
    crate::fallback_observation::recovered(flight, &FAILURES, "frame.flight.join", |value| {
        *value = None
    });
}

#[cfg(test)]
mod tests;
