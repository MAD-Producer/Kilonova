use std::{
    fmt::{self, Write},
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};

static FAILURES: AtomicUsize = AtomicUsize::new(0);
pub(super) fn reset() {
    FAILURES.store(0, Ordering::Relaxed);
}
pub(super) fn failure(path: &Path, phase: &str, reason: fmt::Arguments<'_>) {
    let previous = FAILURES
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
            Some(n.saturating_add(1))
        })
        .unwrap_or_else(|n| n);
    if previous != 0 && previous != 59 {
        return;
    }
    let path = bounded(format_args!("{}", path.display()));
    let reason = bounded(reason);
    let count = previous.saturating_add(1);
    let context=format_args!("UI workspace load degraded site=workspace.framework.load count={count} phase={phase} path={path:?} fallback=default_layout reason={reason}");
    if count == 1 {
        log::warn!("{context}");
    } else {
        log::error!("{context}");
    }
}
fn bounded(args: fmt::Arguments<'_>) -> String {
    struct Buffer(String, usize);
    impl Write for Buffer {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            for ch in text.chars().take(self.1) {
                self.0.push(ch);
                self.1 -= 1;
            }
            Ok(())
        }
    }
    let mut output = Buffer(String::new(), 256);
    let _ = output.write_fmt(args);
    output.0
}
