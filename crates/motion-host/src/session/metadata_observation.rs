use std::{
    cell::{Cell, RefCell},
    fmt::{self, Write},
    io,
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Instant, SystemTime},
};

// Optional metadata can fail on one filesystem while another succeeds. Keep
// fixed process-lifetime evidence instead of retaining a map of source paths.
static FAILURES: AtomicUsize = AtomicUsize::new(0);
thread_local! {
    static IN_SESSION: Cell<bool> = const { Cell::new(false) };
    static PENDING: RefCell<[Option<Event>; 2]> = const { RefCell::new([None, None]) };
}

/// Declare before the session registry guard. Unwind also releases that guard
/// before this scope emits; nested scopes leave their evidence to the outer one.
pub(super) struct Scope(bool);
impl Scope {
    pub(super) fn enter() -> Self {
        Self(IN_SESSION.replace(true))
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        IN_SESSION.set(self.0);
        if !self.0 {
            let pending = PENDING.with(|events| std::mem::take(&mut *events.borrow_mut()));
            for event in pending.into_iter().flatten() {
                event.emit();
            }
        }
    }
}
struct Event {
    source: u64,
    path: String,
    kind: io::ErrorKind,
    cause: String,
    count: usize,
    began: Instant,
}
impl Event {
    fn emit(self) {
        let elapsed = self.began.elapsed().as_millis();
        let context = format_args!("preview image mtime unavailable site=host.preview_input.modified lifetime_count={} source_id={} path={:?} kind={:?} deferred_elapsed_ms={elapsed} fallback=length_only reason={}",self.count,self.source,self.path,self.kind,self.cause);
        if self.count == 1 {
            log::warn!("{context}");
        } else {
            log::error!("{context}");
        }
    }
}
pub(crate) fn modified(
    result: io::Result<SystemTime>,
    source: u64,
    path: &Path,
) -> Option<SystemTime> {
    let error = match result {
        Ok(time) => return Some(time),
        Err(error) => error,
    };
    let previous = FAILURES
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
            Some(n.saturating_add(1))
        })
        .unwrap_or_else(|n| n);
    if previous == 0 || previous == 59 {
        let event = Event {
            source,
            path: bounded(format_args!("{}", path.display())),
            kind: error.kind(),
            cause: bounded(format_args!("{error}")),
            count: previous.saturating_add(1),
            began: Instant::now(),
        };
        if IN_SESSION.get() {
            PENDING.with(|events| events.borrow_mut()[usize::from(previous != 0)] = Some(event));
        } else {
            event.emit();
        }
    }
    None
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
