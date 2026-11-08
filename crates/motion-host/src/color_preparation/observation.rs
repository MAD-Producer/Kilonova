//! Session callbacks may prepare cold resources under the owner lock. Their
//! enclosing serial command publishes its start/in-flight state before entering
//! the callback. Phase records here are deferred until the lock is released;
//! independent preparation keeps its immediate start signal.
use std::{cell::RefCell, fmt};

const MAX_RECORDS: usize = 64;
const MAX_ERRORS: usize = 4;
const MAX_TEXT: usize = 2048;
thread_local! {
    static PENDING: RefCell<Option<Pending>> = const { RefCell::new(None) };
}
struct Pending {
    session: i64,
    records: Vec<Event>,
    error_count: usize,
    record_count: usize,
    omitted: usize,
    omitted_errors: usize,
}
struct Event {
    level: log::Level,
    target: &'static str,
    text: String,
    session: i64,
}

/// Declare before the Session guard. Neither TLS nor Session is borrowed when
/// the logger is called, including on Err and unwinding. No healthy Present
/// records are introduced by entering or leaving this scope.
pub(crate) struct Scope {
    previous: Option<i64>,
}
impl Scope {
    pub(crate) fn enter(session: i64) -> Self {
        let previous = PENDING.with(|pending| {
            let mut pending = pending.borrow_mut();
            if let Some(pending) = pending.as_mut() {
                Some(std::mem::replace(&mut pending.session, session))
            } else {
                *pending = Some(Pending {
                    session,
                    records: Vec::new(),
                    error_count: 0,
                    record_count: 0,
                    omitted: 0,
                    omitted_errors: 0,
                });
                None
            }
        });
        Self { previous }
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        let pending = PENDING.with(|slot| {
            let mut slot = slot.borrow_mut();
            if let Some(previous) = self.previous {
                if let Some(pending) = slot.as_mut() {
                    pending.session = previous;
                }
                None
            } else {
                slot.take()
            }
        });
        if let Some(pending) = pending {
            for event in pending.records {
                log::log!(target: event.target, event.level,
                    "{} session={} deferred=true", event.text, event.session);
            }
            if pending.omitted != 0 {
                let level = if pending.omitted_errors != 0 {
                    log::Level::Error
                } else {
                    log::Level::Debug
                };
                log::log!(level, "color preparation phase=observation_capacity session={} omitted_records={} omitted_errors={} limit={} error_limit={}",
                    pending.session, pending.omitted, pending.omitted_errors, MAX_RECORDS, MAX_ERRORS);
            }
        }
    }
}

pub(super) fn record(level: log::Level, target: &'static str, args: fmt::Arguments<'_>) {
    let deferred = PENDING.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(pending) = slot.as_mut() else {
            return false;
        };
        let count = if level == log::Level::Error {
            &mut pending.error_count
        } else {
            &mut pending.record_count
        };
        let limit = if level == log::Level::Error {
            MAX_ERRORS
        } else {
            MAX_RECORDS
        };
        if *count < limit {
            // Bounded formatting, rather than constructing an unbounded payload
            // and truncating it afterward. Error cause chains are bounded again
            // at their existing source formatter.
            let mut text = Bounded(String::new());
            let _ = fmt::write(&mut text, args);
            *count += 1;
            pending.records.push(Event {
                level,
                target,
                text: text.0,
                session: pending.session,
            });
        } else {
            pending.omitted = pending.omitted.saturating_add(1);
            if level == log::Level::Error {
                pending.omitted_errors = pending.omitted_errors.saturating_add(1);
            }
        }
        true
    });
    if !deferred {
        log::log!(target: target, level, "{args}");
    }
}

struct Bounded(String);
impl fmt::Write for Bounded {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let remaining = MAX_TEXT.saturating_sub(self.0.len());
        let mut end = value.len().min(remaining);
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        self.0.push_str(&value[..end]);
        if end == value.len() {
            Ok(())
        } else {
            Err(fmt::Error)
        }
    }
}
