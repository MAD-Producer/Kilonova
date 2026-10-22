use std::{
    cell::{Cell, RefCell},
    marker::PhantomData,
    rc::Rc,
    time::Duration,
};

thread_local! {
    static SESSION: Cell<Option<i64>> = const { Cell::new(None) };
    static PENDING: RefCell<Pending> = const { RefCell::new(Pending {
        events: [None, None], omitted: 0,
    }) };
}

struct Pending {
    // Preserve the first Debug and Error cause, with fixed retention even if a
    // host queries many independent renderers before releasing its business lock.
    events: [Option<Event>; 2],
    omitted: usize,
}

/// Bind before acquiring a host's session/business guard. Records then emit
/// after that guard drops, including unwind. Nested scopes flush at the outer
/// boundary; the scope cannot be moved to another thread.
#[must_use = "bind before acquiring the business guard to defer logging until unlock"]
pub struct MemoryPressureLogScope {
    prior: Option<i64>,
    _thread: PhantomData<Rc<()>>,
}

impl MemoryPressureLogScope {
    pub fn enter(session: i64) -> Self {
        Self {
            prior: SESSION.replace(Some(session)),
            _thread: PhantomData,
        }
    }
}

impl Drop for MemoryPressureLogScope {
    fn drop(&mut self) {
        SESSION.set(self.prior);
        if self.prior.is_none() {
            let (events, omitted) = PENDING.with(|pending| {
                let mut pending = pending.borrow_mut();
                (
                    std::mem::take(&mut pending.events),
                    std::mem::take(&mut pending.omitted),
                )
            });
            // Release the RefCell borrow before invoking a host-owned logger.
            for event in events.into_iter().flatten() {
                event.emit(omitted);
            }
        }
    }
}

pub(super) struct Event {
    backend: String,
    cause: String,
    count: u8,
    elapsed: Duration,
    session: Option<i64>,
}

impl Event {
    pub(super) fn record(backend: &str, cause: &str, count: u8, elapsed: Duration) {
        let event = Self {
            backend: backend.chars().take(32).collect(),
            cause: cause.chars().take(super::MAX_CAUSE_CHARS).collect(),
            count,
            elapsed,
            session: SESSION.get(),
        };
        if event.session.is_none() {
            event.emit(0);
            return;
        }
        PENDING.with(|pending| {
            let mut pending = pending.borrow_mut();
            let slot = usize::from(count == 60);
            if pending.events[slot].is_none() {
                pending.events[slot] = Some(event);
            } else {
                pending.omitted = pending.omitted.saturating_add(1);
            }
        });
    }

    fn emit(self, omitted: usize) {
        let level = if self.count == 60 {
            log::Level::Error
        } else {
            log::Level::Debug
        };
        log::log!(target: "motion_render::memory_pressure", level,
            "memory pressure query backend={} phase=native elapsed_ms={} session={:?} failures={} omitted={} cause={}",
            self.backend, self.elapsed.as_millis(), self.session, self.count, omitted, self.cause);
    }
}
