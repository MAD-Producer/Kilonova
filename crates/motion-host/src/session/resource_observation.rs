use std::{
    fmt::{self, Write},
    io,
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

static FAILED_REPORTS: AtomicUsize = AtomicUsize::new(0);

pub(super) struct UnresolvedRoot {
    session: i64,
    path: String,
    kind: io::ErrorKind,
    cause: String,
}
impl UnresolvedRoot {
    pub(super) fn new(session: i64, path: &Path, error: &io::Error) -> Self {
        Self {
            session,
            path: bounded(format_args!("{}", path.display())),
            kind: error.kind(),
            cause: bounded(format_args!("{error}")),
        }
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
    let mut result = Buffer(String::new(), 256);
    let _ = result.write_fmt(args);
    result.0
}

/// Call after dropping the registry. A report resolving every root resets the
/// process-wide streak; resolving one healthy or out-of-scope root does not.
pub(super) fn report(failed_roots: usize, first: Option<UnresolvedRoot>, began: Instant) {
    let Some(first) = first else {
        FAILED_REPORTS.store(0, Ordering::Relaxed);
        return;
    };
    let previous = FAILED_REPORTS
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
            Some(n.saturating_add(1))
        })
        .unwrap_or_else(|n| n);
    if previous != 0 && previous != 59 {
        return;
    }
    let count = previous.saturating_add(1);
    let elapsed = began.elapsed().as_millis();
    let context = format_args!("resource report unresolved roots site=host.resource_info.root count={count} unresolved_roots={failed_roots} first_session={} first_root={:?} kind={:?} scope_elapsed_ms={elapsed} reason={} requested_scope_membership=unknown", first.session, first.path, first.kind, first.cause);
    if count == 1 {
        log::warn!("{context}");
    } else {
        log::error!("{context}");
    }
}
