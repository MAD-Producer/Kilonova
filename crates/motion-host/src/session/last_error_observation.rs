use std::time::Instant;

/// Counts calls exiting with an error latch, not frames or new failed samples.
/// Each Session owns one counter. An exit with no last_error resets the episode.
#[derive(Default)]
pub(super) struct Observation {
    count: usize,
}
pub(super) struct Event {
    count: usize,
    frame: f64,
    composition: String,
    reason: String,
    began: Instant,
}
impl Observation {
    pub(super) fn observe(
        &mut self,
        error: Option<&str>,
        frame: f64,
        composition: &str,
        began: Instant,
    ) -> Option<Event> {
        let Some(error) = error else {
            self.count = 0;
            return None;
        };
        self.count = self.count.saturating_add(1);
        if self.count != 1 && self.count != 60 {
            return None;
        }
        Some(Event {
            count: self.count,
            frame,
            composition: composition.chars().take(128).collect(),
            reason: error.chars().take(512).collect(),
            began,
        })
    }
}
impl Event {
    /// Call only after releasing the session registry, including caller Errs.
    pub(super) fn emit(self, session: i64) {
        let elapsed = self.began.elapsed().as_millis();
        let context = format_args!("session retained render error site=host.session.last_error session={session} calls_with_latched_error={} frame={} composition={:?} scope_elapsed_ms={} reason={}", self.count, self.frame, self.composition, elapsed, self.reason);
        if self.count == 1 {
            log::warn!("{context}");
        } else {
            log::error!("{context}");
        }
    }
}
