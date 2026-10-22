//! Retire failures keep their original once-per-budget policy and leave host locks.
use super::AllocationCategory;
use std::{
    cell::{Cell, RefCell},
    marker::PhantomData,
    rc::Rc,
    time::Instant,
};

thread_local! {
    static SESSION: Cell<Option<i64>> = const { Cell::new(None) };
    static PENDING: RefCell<Pending> = const { RefCell::new(Pending { first: None, omitted: 0 }) };
}

struct Pending {
    first: Option<Event>,
    omitted: usize,
}

/// Bind before taking a host's business/session guard. A release invariant fault
/// is emitted after that guard drops, including unwind. Nested scopes flush only
/// at the outer boundary. At most one fixed-size cause and an omission count are
/// retained per thread; healthy releases allocate nothing and perform no IO.
#[must_use = "bind before acquiring business guards to defer fault logging until unlock"]
pub struct AllocationLogScope {
    prior: Option<i64>,
    _thread: PhantomData<Rc<()>>,
}

impl AllocationLogScope {
    pub fn enter(session: i64) -> Self {
        Self {
            prior: SESSION.replace(Some(session)),
            _thread: PhantomData,
        }
    }
}

impl Drop for AllocationLogScope {
    fn drop(&mut self) {
        SESSION.set(self.prior);
        if self.prior.is_none() {
            let (first, omitted) = PENDING.with(|pending| {
                let mut pending = pending.borrow_mut();
                (pending.first.take(), std::mem::take(&mut pending.omitted))
            });
            // A host-owned logger may inspect the Session or observation state.
            // Release the observation borrow before invoking it.
            if let Some(event) = first {
                event.emit(omitted);
            }
        }
    }
}

pub(super) struct Event {
    id: u64,
    bytes: u64,
    category: AllocationCategory,
    poisoned: bool,
    inconsistent: bool,
    session: Option<i64>,
    began: Instant,
}

impl Event {
    pub(super) fn record(
        id: u64,
        bytes: u64,
        category: AllocationCategory,
        poisoned: bool,
        inconsistent: bool,
    ) {
        let event = Self {
            id,
            bytes,
            category,
            poisoned,
            inconsistent,
            session: SESSION.get(),
            began: Instant::now(),
        };
        if event.session.is_none() {
            event.emit(0);
            return;
        }
        PENDING.with(|pending| {
            let mut pending = pending.borrow_mut();
            if pending.first.is_none() {
                pending.first = Some(event);
            } else {
                pending.omitted = pending.omitted.saturating_add(1);
            }
        });
    }

    fn emit(self, omitted: usize) {
        log::error!(
            "render allocation release id={} bytes={} category={:?} poisoned={} inconsistent={} session={:?} phase=retire deferred_ms={} deferred_omitted={}",
            self.id,
            self.bytes,
            self.category,
            self.poisoned,
            self.inconsistent,
            self.session,
            self.began.elapsed().as_millis(),
            omitted
        );
    }
}

#[cfg(test)]
mod tests;
