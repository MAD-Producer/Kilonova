//! Cold registry-load recovery evidence, emitted outside the session registry.
use std::{
    cell::{Cell, RefCell},
    path::Path,
    time::Instant,
};

thread_local! {
    static SESSION: Cell<Option<i64>> = const { Cell::new(None) };
    static PENDING: RefCell<Pending> = const { RefCell::new(Pending {
        events: [None, None, None, None], omitted: 0,
    }) };
}
struct Pending {
    events: [Option<Event>; 4],
    omitted: usize,
}

/// Declare before the session guard so Err and unwind release it before logging.
pub(super) struct Scope(Option<i64>);
impl Scope {
    pub(super) fn enter(session: i64) -> Self {
        Self(SESSION.replace(Some(session)))
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        SESSION.set(self.0);
        if self.0.is_none() {
            let (events, omitted) = PENDING.with(|pending| {
                let mut pending = pending.borrow_mut();
                (
                    std::mem::take(&mut pending.events),
                    std::mem::take(&mut pending.omitted),
                )
            });
            let last = events.iter().rposition(Option::is_some);
            for (index, event) in events.into_iter().enumerate() {
                if let Some(event) = event {
                    event.emit(if Some(index) == last { omitted } else { 0 });
                }
            }
        }
    }
}
struct Event {
    session: Option<i64>,
    path: String,
    diagnostics: Vec<String>,
    count: usize,
    began: Instant,
}
impl Event {
    fn emit(self, omitted_loads: usize) {
        log::error!(
            "plugin registry recovered with diagnostics site=host.registry.load operation=registry_load session={:?} path={:?} elapsed_ms={} diagnostics={} omitted_diagnostics={} omitted_loads={} causes={:?}",
            self.session, self.path, self.began.elapsed().as_millis(), self.count,
            self.count.saturating_sub(self.diagnostics.len()), omitted_loads, self.diagnostics,
        );
    }
}

pub(super) fn load(path: &Path) -> motion_effects::Result<motion_effects::Registry> {
    let began = Instant::now();
    let registry = motion_effects::Registry::load(path)?;
    if !registry.diagnostics.is_empty() {
        let event = Event {
            session: SESSION.get(),
            path: path.to_string_lossy().chars().take(512).collect(),
            diagnostics: registry
                .diagnostics
                .iter()
                .take(4)
                .map(|cause| cause.chars().take(512).collect())
                .collect(),
            count: registry.diagnostics.len(),
            began,
        };
        if event.session.is_some() {
            PENDING.with(|pending| {
                let mut pending = pending.borrow_mut();
                if let Some(slot) = pending.events.iter_mut().find(|slot| slot.is_none()) {
                    *slot = Some(event);
                } else {
                    pending.omitted = pending.omitted.saturating_add(1);
                }
            });
        } else {
            event.emit(0);
        }
    }
    Ok(registry)
}

#[cfg(test)]
mod tests;
