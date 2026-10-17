use super::*;

/// Observe the whole thread, including calls outside the frame panic boundary.
/// This guard does not restart the worker or change its business error result.
pub(super) struct WorkerObservation {
    object: u64,
    source: u64,
    shared: Arc<(Mutex<State>, Condvar)>,
    reason: Option<&'static str>,
    began: Instant,
}
impl WorkerObservation {
    pub(super) fn new(object: u64, source: u64, shared: Arc<(Mutex<State>, Condvar)>) -> Self {
        Self {
            object,
            source,
            shared,
            reason: None,
            began: Instant::now(),
        }
    }
    pub(super) fn stopped(&mut self, reason: &'static str) {
        self.reason = Some(reason);
    }
}
impl Drop for WorkerObservation {
    fn drop(&mut self) {
        // Inner loop guards have already unwound before this outer guard.
        // Recover only observation data; never resume a poisoned operation.
        let flight = {
            let mut state = self
                .shared
                .0
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            state.worker_stopped = true;
            state.in_flight.take()
        };
        let object = self.object;
        let source = self.source;
        if let Some(reason) = self.reason {
            log::info!(
                "video decoder worker stopped object={object} source={source} reason={reason} {}ms",
                self.began.elapsed().as_millis()
            );
            log::logger().flush();
        } else {
            let reason = if std::thread::panicking() {
                "unwind"
            } else {
                "unexpected_return"
            };
            log::error!("video decoder worker exited unexpectedly object={object} source={source} reason={reason} sequence={:?} pts_us={:?} phase={} decode_elapsed_ms={} worker_elapsed_ms={}",
                flight.map(|active| active.sequence), flight.map(|active| active.time),
                flight.map_or("idle", |active| active.phase),
                flight.map_or(0, |active| active.began.elapsed().as_millis()), self.began.elapsed().as_millis());
        }
    }
}
