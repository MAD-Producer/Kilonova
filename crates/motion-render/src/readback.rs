//! Bounded observations of application readback work, not bus traffic or VRAM.
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub(crate) struct Counters<const N: usize> {
    values: [AtomicU64; N],
    saturated: AtomicBool,
}
impl<const N: usize> Default for Counters<N> {
    fn default() -> Self {
        Self {
            values: std::array::from_fn(|_| AtomicU64::new(0)),
            saturated: AtomicBool::new(false),
        }
    }
}
impl<const N: usize> Counters<N> {
    pub(crate) fn add(&self, index: usize, amount: u64) {
        let before = self.values[index]
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                Some(value.saturating_add(amount))
            })
            .unwrap_or_else(|value| value);
        if before.checked_add(amount).is_none() {
            self.saturated.store(true, Ordering::Relaxed);
        }
    }
    pub(crate) fn get(&self, index: usize) -> u64 {
        self.values[index].load(Ordering::Relaxed)
    }
    pub(crate) fn saturated(&self) -> bool {
        self.saturated.load(Ordering::Relaxed)
    }
    fn subtract_one(&self, index: usize) {
        let _ = self.values[index].fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            Some(value.saturating_sub(1))
        });
    }
}

/// Cumulative over one Renderer lifetime; resets only with a new Renderer.
/// Counters are individually atomic, not an atomic cross-counter snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageReadbackInfo {
    pub requested_calls: u64,
    pub submitted_calls: u64,
    pub submitted_payload_bytes: u64,
    /// Addressed buffer span, including row gaps but excluding trailing padding.
    pub submitted_buffer_span_bytes: u64,
    pub mapped_buffer_bytes: u64,
    pub returned_payload_bytes: u64,
    pub completed_calls: u64,
    pub failed_calls: u64,
    pub in_flight_calls: u64,
    pub counters_saturated: bool,
}

#[derive(Default)]
pub(crate) struct ImageReadbackCounters(Counters<9>);
impl ImageReadbackCounters {
    pub(crate) fn info(&self) -> ImageReadbackInfo {
        ImageReadbackInfo {
            requested_calls: self.0.get(0),
            submitted_calls: self.0.get(1),
            submitted_payload_bytes: self.0.get(2),
            submitted_buffer_span_bytes: self.0.get(3),
            mapped_buffer_bytes: self.0.get(4),
            returned_payload_bytes: self.0.get(5),
            completed_calls: self.0.get(6),
            failed_calls: self.0.get(7),
            in_flight_calls: self.0.get(8),
            counters_saturated: self.0.saturated(),
        }
    }
    pub(crate) fn begin(&self) -> ImageReadbackAttempt<'_> {
        self.0.add(0, 1);
        self.0.add(8, 1);
        ImageReadbackAttempt {
            counters: self,
            complete: false,
        }
    }
}

pub(crate) struct ImageReadbackAttempt<'a> {
    counters: &'a ImageReadbackCounters,
    complete: bool,
}
impl ImageReadbackAttempt<'_> {
    pub(crate) fn submitted(&self, payload: u64, span: u64) {
        self.counters.0.add(1, 1);
        self.counters.0.add(2, payload);
        self.counters.0.add(3, span);
    }
    pub(crate) fn mapped(&self, bytes: u64) {
        self.counters.0.add(4, bytes);
    }
    pub(crate) fn completed(mut self, returned: u64) {
        self.counters.0.add(5, returned);
        self.counters.0.add(6, 1);
        self.complete = true;
    }
}
impl Drop for ImageReadbackAttempt<'_> {
    fn drop(&mut self) {
        if !self.complete {
            self.counters.0.add(7, 1);
        }
        self.counters.0.subtract_one(8);
    }
}

/// Cumulative over one GpuTimer lifetime, including unassociated callbacks.
/// Encoding can be discarded: encoded bytes do not imply submission/completion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimestampReadbackInfo {
    pub encoded_resolve_calls: u64,
    pub encoded_resolve_bytes: u64,
    pub encoded_copy_bytes: u64,
    pub map_requests: u64,
    pub map_successes: u64,
    pub map_failures: u64,
    pub mapped_buffer_bytes: u64,
    pub valid_samples: u64,
    pub invalid_samples: u64,
    pub counters_saturated: bool,
}

#[derive(Default)]
pub(crate) struct TimestampReadbackCounters(Counters<9>);
impl TimestampReadbackCounters {
    pub(crate) fn info(&self) -> TimestampReadbackInfo {
        TimestampReadbackInfo {
            encoded_resolve_calls: self.0.get(0),
            encoded_resolve_bytes: self.0.get(1),
            encoded_copy_bytes: self.0.get(2),
            map_requests: self.0.get(3),
            map_successes: self.0.get(4),
            map_failures: self.0.get(5),
            mapped_buffer_bytes: self.0.get(6),
            valid_samples: self.0.get(7),
            invalid_samples: self.0.get(8),
            counters_saturated: self.0.saturated(),
        }
    }
    pub(crate) fn encoded(&self, bytes: u64) {
        self.0.add(0, 1);
        self.0.add(1, bytes);
        self.0.add(2, bytes);
    }
    pub(crate) fn map_requested(&self) {
        self.0.add(3, 1);
    }
    pub(crate) fn map_result(&self, success: bool) {
        self.0.add(if success { 4 } else { 5 }, 1);
    }
    pub(crate) fn mapped(&self, bytes: u64) {
        self.0.add(6, bytes);
    }
    pub(crate) fn sample(&self, valid: bool) {
        self.0.add(if valid { 7 } else { 8 }, 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_attempt_distinguishes_nonreturning_submitted_failed_and_completed_work() {
        let counters = ImageReadbackCounters::default();
        let attempt = counters.begin();
        assert_eq!(counters.info().in_flight_calls, 1);
        assert_eq!(counters.info().failed_calls, 0);
        attempt.submitted(24, 268);
        assert_eq!(counters.info().completed_calls, 0);
        drop(attempt);
        let attempt = counters.begin();
        attempt.submitted(24, 268);
        attempt.mapped(512);
        attempt.completed(24);
        assert_eq!(
            counters.info(),
            ImageReadbackInfo {
                requested_calls: 2,
                submitted_calls: 2,
                submitted_payload_bytes: 48,
                submitted_buffer_span_bytes: 536,
                mapped_buffer_bytes: 512,
                returned_payload_bytes: 24,
                completed_calls: 1,
                failed_calls: 1,
                in_flight_calls: 0,
                counters_saturated: false,
            }
        );
        assert_eq!(
            ImageReadbackCounters::default().info(),
            ImageReadbackInfo::default()
        );
    }

    #[test]
    fn observations_saturate_and_signal_loss_of_exact_counter_coverage() {
        let counters = Counters::<1>::default();
        counters.add(0, u64::MAX);
        assert!(!counters.saturated());
        counters.add(0, 1);
        assert_eq!(counters.get(0), u64::MAX);
        assert!(counters.saturated());
    }
}
