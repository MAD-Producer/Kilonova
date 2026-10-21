//! Fixed-site, bounded evidence. Counters belong to a process site or owner;
//! callers explicitly reset consecutive episodes. No document-keyed registry.
use std::{
    fmt::{self, Write},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};
#[derive(Default)]
pub(crate) struct Counter(AtomicUsize);
impl Counter {
    pub const fn new() -> Self {
        Self(AtomicUsize::new(0))
    }
    pub fn success(&self) {
        self.0.store(0, Ordering::Relaxed);
    }
    pub fn failure(&self, site: &'static str, context: fmt::Arguments<'_>) {
        let previous = self
            .0
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                Some(n.saturating_add(1))
            })
            .unwrap_or_else(|n| n);
        if previous != 0 && previous != 59 {
            return;
        }
        let context = bounded(context);
        if previous == 0 {
            log::warn!("desktop degradation site={site} count=1 {context}");
        } else {
            log::error!("persistent desktop degradation site={site} count=60 {context}");
        }
    }
}
// Bound formatting as it is written, rather than allocating an arbitrary cause.
fn bounded(args: fmt::Arguments<'_>) -> String {
    bounded_limit(args, 768)
}
fn bounded_limit(args: fmt::Arguments<'_>, limit: usize) -> String {
    struct Buffer {
        text: String,
        remaining: usize,
    }
    impl Write for Buffer {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            for ch in s.chars().take(self.remaining) {
                self.text.push(if ch.is_control() { ' ' } else { ch });
                self.remaining -= 1;
            }
            Ok(())
        }
    }
    let mut buffer = Buffer {
        text: String::new(),
        remaining: limit,
    };
    let _ = buffer.write_fmt(args);
    buffer.text
}
/// Limit diagnostic output at the writer; callers keep the complete error for
/// replies and source inspection. Display precision is not a length guarantee.
pub(crate) fn error_cause(value: impl fmt::Display) -> String {
    bounded_limit(format_args!("{value}"), 2_048)
}
/// Bound each path/cause independently so a long path cannot hide the cause.
pub(crate) fn short(value: impl fmt::Display) -> impl fmt::Display {
    struct Short<T>(T);
    impl<T: fmt::Display> fmt::Display for Short<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(&bounded_limit(format_args!("{}", self.0), 256))
        }
    }
    Short(value)
}
/// Preserve the original into_inner policy and emit only after releasing the
/// business guard, including when the closure unwinds. Counts are lifetime
/// recoveries at this fixed site: healthy access does not conceal poisoned use.
pub(crate) fn recovered<T, R>(
    mutex: &Mutex<T>,
    counter: &Counter,
    site: &'static str,
    f: impl FnOnce(&mut T) -> R,
) -> R {
    struct Evidence<'a> {
        counter: &'a Counter,
        site: &'static str,
        poisoned: bool,
    }
    impl Drop for Evidence<'_> {
        fn drop(&mut self) {
            if self.poisoned {
                self.counter.failure(
                    self.site,
                    format_args!("reason=mutex_poisoned policy=into_inner"),
                );
            }
        }
    }
    let mut evidence = Evidence {
        counter,
        site,
        poisoned: false,
    };
    let mut guard = mutex.lock().unwrap_or_else(|error| {
        evidence.poisoned = true;
        error.into_inner()
    });
    f(&mut guard)
}
#[cfg(test)]
mod tests;
