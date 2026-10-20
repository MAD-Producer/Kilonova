use std::{
    error::Error as _,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

static NEXT_RENDERER: AtomicU64 = AtomicU64::new(1);
const CAUSE_BYTES: usize = 2048;
const CONTEXT_BYTES: usize = 512;

// Keep both the operation/label prefix and the root diagnosis, which native
// validation errors often append after a caller-supplied label or shader text.
fn bounded(text: &str, limit: usize) -> String {
    const MARKER: &str = " ...[truncated]... ";
    if text.len() <= limit {
        return text.to_owned();
    }
    if limit < MARKER.len() {
        let mut end = limit;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        return text[..end].to_owned();
    }
    let remaining = limit.saturating_sub(MARKER.len());
    let mut head = remaining / 2;
    while !text.is_char_boundary(head) {
        head -= 1;
    }
    let mut tail = text.len() - (remaining - remaining / 2);
    while !text.is_char_boundary(tail) {
        tail += 1;
    }
    format!("{}{MARKER}{}", &text[..head], &text[tail..])
}

struct AdapterContext {
    name: String,
    driver: String,
    backend: wgpu::Backend,
    vendor: u32,
    device: u32,
}

mod recovery;
use recovery::{Recovery, Site};

pub(super) fn read(failure: &Mutex<Option<String>>) -> Option<String> {
    let mut recovery = Recovery::new(Site::Read, None);
    let value = recovery.lock(failure.lock());
    value.clone()
}

/// Callback evidence must not depend on another frame or check_health() call.
pub(super) struct Observation {
    failure: Arc<Mutex<Option<String>>>,
    adapter: AdapterContext,
    renderer: u64,
    began: Instant,
    uncaptured: [AtomicU64; 3],
    lost: AtomicU64,
}
impl Observation {
    pub(super) fn new(failure: Arc<Mutex<Option<String>>>, adapter: wgpu::AdapterInfo) -> Self {
        Self {
            failure,
            adapter: AdapterContext {
                name: bounded(&adapter.name, CONTEXT_BYTES),
                driver: bounded(&adapter.driver, CONTEXT_BYTES),
                backend: adapter.backend,
                vendor: adapter.vendor,
                device: adapter.device,
            },
            renderer: NEXT_RENDERER.fetch_add(1, Ordering::Relaxed),
            began: Instant::now(),
            uncaptured: std::array::from_fn(|_| AtomicU64::new(0)),
            lost: AtomicU64::new(0),
        }
    }
    pub(super) fn uncaptured(&self, error: wgpu::Error) {
        let (kind, index) = match &error {
            wgpu::Error::OutOfMemory { .. } => ("out_of_memory", 0),
            wgpu::Error::Validation { .. } => ("validation", 1),
            wgpu::Error::Internal { .. } => ("internal", 2),
        };
        let (retained_first_cause, count) = {
            let mut recovery = Recovery::new(Site::Uncaptured, Some(self.renderer));
            let mut failure = recovery.lock(self.failure.lock());
            let retained = failure.is_some();
            // Preserve the first-error policy; only oversized diagnostic text
            // changes, and retains both its beginning and original root cause.
            failure.get_or_insert_with(|| {
                format!(
                    "GPU error: {}",
                    bounded(&error.to_string(), CAUSE_BYTES - "GPU error: ".len())
                )
            });
            (retained, increment(&self.uncaptured[index]))
        };
        // Follow-up invalid-command errors can arrive on every submission.
        // Record the first of each kind, so validation cannot hide later OOM
        // or internal failure, then one bounded storm warning per kind.
        if count == 1 && log::log_enabled!(log::Level::Error) {
            let reason = bounded(&error.to_string(), CAUSE_BYTES);
            let source = bounded(&format!("{:?}", error.source()), CONTEXT_BYTES);
            log::error!("GPU uncaptured error renderer={} count={count} kind={kind} retained_first_cause={retained_first_cause} elapsed_ms={} adapter={:?} backend={:?} vendor={} device={} driver={:?} reason=GPU error: {reason} source={source}",
                self.renderer, self.began.elapsed().as_millis(), self.adapter.name, self.adapter.backend,
                self.adapter.vendor, self.adapter.device, self.adapter.driver);
        } else if count == 60 && log::log_enabled!(log::Level::Warn) {
            let reason = bounded(&error.to_string(), CAUSE_BYTES);
            let source = bounded(&format!("{:?}", error.source()), CONTEXT_BYTES);
            log::warn!("GPU follow-up errors suppressed renderer={} count={count} last_kind={kind} retained_first_cause={retained_first_cause} elapsed_ms={} reason=GPU error: {reason} source={source}",
                self.renderer, self.began.elapsed().as_millis());
        }
    }
    pub(super) fn device_lost(&self, reason: wgpu::DeviceLostReason, message: String) {
        // Device loss has always replaced the stored cause. Observability must
        // not turn it into a different recovery or health-check policy.
        let count = {
            let mut recovery = Recovery::new(Site::DeviceLost, Some(self.renderer));
            let mut failure = recovery.lock(self.failure.lock());
            let prefix = format!("GPU device lost: {reason:?}: ");
            *failure = Some(format!(
                "{prefix}{}",
                bounded(&message, CAUSE_BYTES - prefix.len())
            ));
            if reason == wgpu::DeviceLostReason::Destroyed {
                0
            } else {
                increment(&self.lost)
            }
        };
        let message = bounded(&message, CAUSE_BYTES);
        if reason == wgpu::DeviceLostReason::Destroyed {
            log::debug!(
                "GPU device destroyed renderer={} elapsed_ms={} reason={message}",
                self.renderer,
                self.began.elapsed().as_millis()
            );
        } else if count == 1 {
            log::error!("GPU device lost renderer={} count={count} cause={reason:?} elapsed_ms={} adapter={:?} backend={:?} vendor={} device={} driver={:?} reason={message}",
                self.renderer, self.began.elapsed().as_millis(), self.adapter.name, self.adapter.backend,
                self.adapter.vendor, self.adapter.device, self.adapter.driver);
        } else if count == 60 {
            log::warn!("GPU device loss callbacks suppressed renderer={} count={count} cause={reason:?} elapsed_ms={} reason={message}", self.renderer, self.began.elapsed().as_millis());
        }
    }
}

fn increment(counter: &AtomicU64) -> u64 {
    counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
            Some(count.saturating_add(1))
        })
        .unwrap_or_else(|count| count)
        .saturating_add(1)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod recovery_tests;

#[cfg(test)]
mod limits_tests;
