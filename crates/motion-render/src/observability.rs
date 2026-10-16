//! Process-wide, bounded logging for failures that preserve an Ok return value.
use std::{
    fmt,
    sync::atomic::{AtomicUsize, Ordering},
};

pub struct FailureCounter(AtomicUsize);
impl FailureCounter {
    pub const fn new() -> Self {
        Self(AtomicUsize::new(0))
    }
    pub fn success(&self) {
        self.0.store(0, Ordering::Relaxed);
    }
    /// Returns the consecutive count, saturating to avoid a second escalation.
    pub fn failure(&self, site: &str, context: fmt::Arguments<'_>) -> usize {
        let previous = self
            .0
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                Some(value.saturating_add(1))
            })
            .unwrap_or_else(|value| value);
        let count = previous.saturating_add(1);
        if count == 1 {
            log::debug!("degradation site={site} count={count} {context}");
        }
        if count == 60 {
            log::error!("persistent degradation site={site} count={count} {context}");
        }
        count
    }
}
impl Default for FailureCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    struct Capture(Mutex<Vec<(log::Level, String)>>);
    static CAPTURE: Capture = Capture(Mutex::new(Vec::new()));
    impl log::Log for Capture {
        fn enabled(&self, _: &log::Metadata<'_>) -> bool {
            true
        }
        fn log(&self, record: &log::Record<'_>) {
            let message = record.args().to_string();
            if message.contains("site=test_counter") {
                self.0.lock().unwrap().push((record.level(), message));
            }
        }
        fn flush(&self) {}
    }
    #[test]
    fn consecutive_failures_log_once_at_sixty_and_reset_after_recovery() {
        log::set_logger(&CAPTURE).unwrap();
        log::set_max_level(log::LevelFilter::Debug);
        let counter = FailureCounter::new();
        for count in 1..=120 {
            assert_eq!(
                counter.failure("test_counter", format_args!("source_id=7")),
                count
            );
        }
        counter.success();
        for count in 1..=60 {
            assert_eq!(
                counter.failure("test_counter", format_args!("source_id=8")),
                count
            );
        }
        let events = CAPTURE.0.lock().unwrap();
        assert_eq!(events.len(), 4);
        for (episode, id) in [(0, 7), (1, 8)] {
            let (level, message) = &events[episode * 2];
            assert_eq!(*level, log::Level::Debug);
            assert!(message.contains("count=1 "));
            assert!(message.contains(&format!("source_id={id}")));
            let (level, message) = &events[episode * 2 + 1];
            assert_eq!(*level, log::Level::Error);
            assert!(message.contains("count=60 "));
            assert!(message.contains(&format!("source_id={id}")));
        }
    }
}
