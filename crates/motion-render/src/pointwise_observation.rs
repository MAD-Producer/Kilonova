//! Cold compiler observations. Host scopes defer sink calls until business
//! guards are released; the engine's pre-handler start/in-flight state remains
//! the hang signal while such a scope is open.
use std::{
    cell::{Cell, RefCell},
    marker::PhantomData,
    rc::Rc,
    time::Instant,
};

const MAX_EVENTS: usize = 64;
const MAX_CAUSE_CHARS: usize = 2048;
thread_local! {
    static SESSION: Cell<Option<i64>> = const { Cell::new(None) };
    static PENDING: RefCell<Pending> = const { RefCell::new(Pending {
        events: Vec::new(), first_overflow_error: None, omitted: 0,
    }) };
}
struct Pending {
    events: Vec<Event>,
    first_overflow_error: Option<Event>,
    omitted: u64,
}

/// Enter before taking a host Session guard. Nested scopes flush only after the
/// outer guard drops, including error/unwind. Deferred `phase=start` records are
/// explicitly marked: they cannot replace a worker's pre-handler start signal.
#[must_use = "bind before acquiring business guards"]
pub struct PointwiseLogScope {
    prior: Option<i64>,
    _thread: PhantomData<Rc<()>>,
}
impl PointwiseLogScope {
    pub fn enter(session: i64) -> Self {
        Self {
            prior: SESSION.replace(Some(session)),
            _thread: PhantomData,
        }
    }
}
impl Drop for PointwiseLogScope {
    fn drop(&mut self) {
        SESSION.set(self.prior);
        if self.prior.is_some() {
            return;
        }
        let (events, overflow, omitted) = PENDING.with(|pending| {
            let mut p = pending.borrow_mut();
            (
                std::mem::take(&mut p.events),
                p.first_overflow_error.take(),
                std::mem::take(&mut p.omitted),
            )
        });
        // No TLS borrow or business guard spans a host-owned logger call.
        for event in events.into_iter().chain(overflow) {
            event.emit();
        }
        if omitted != 0 {
            log::warn!(target: "motion_render::pointwise_compile",
                "pointwise compiler observations omitted={omitted} capacity={MAX_EVENTS} phase=scope_flush");
        }
    }
}

#[derive(Clone)]
struct Context {
    operation: &'static str,
    structure: String,
    object: Option<u64>,
    effects: Vec<u64>,
    program: Option<u32>,
    format: Option<u32>,
    session: Option<i64>,
}
struct Event {
    context: Context,
    level: log::Level,
    phase: &'static str,
    elapsed_ns: u64,
    cause: String,
    source_bytes: usize,
    observed: Instant,
    deferred: bool,
}
impl Event {
    fn record(mut self) {
        self.deferred = SESSION.get().is_some();
        if !self.deferred {
            self.emit();
            return;
        }
        PENDING.with(|pending| {
            let mut p = pending.borrow_mut();
            if p.events.len() < MAX_EVENTS {
                p.events.push(self);
            } else if self.level == log::Level::Error && p.first_overflow_error.is_none() {
                // A burst of informational starts must not discard the cause.
                p.first_overflow_error = Some(self);
            } else {
                p.omitted = p.omitted.saturating_add(1);
            }
        });
    }
    fn emit(self) {
        let c = self.context;
        log::log!(target: "motion_render::pointwise_compile", self.level,
            "operation={} phase={} structure={} object={:?} effects={:?} program={:?} format={:?} session={:?} elapsed_ns={} source_bytes={} deferred={} deferred_ms={} cause={}",
            c.operation, self.phase, c.structure, c.object, c.effects, c.program, c.format, c.session,
            self.elapsed_ns, self.source_bytes, self.deferred, self.observed.elapsed().as_millis(), self.cause);
    }
}

pub(crate) struct CompilePhase {
    context: Context,
    began: Instant,
}
impl CompilePhase {
    pub(crate) fn start(
        operation: &'static str,
        structure: &str,
        object: Option<u64>,
        effects: impl IntoIterator<Item = u64>,
        program: Option<u32>,
        format: Option<u32>,
    ) -> Self {
        let phase = Self {
            context: Context {
                operation,
                structure: structure.chars().take(128).collect(),
                object,
                effects: effects.into_iter().take(16).collect(),
                program,
                format,
                session: SESSION.get(),
            },
            began: Instant::now(),
        };
        phase.event(log::Level::Info, "start", "", 0);
        phase
    }
    pub(crate) fn done(self, source_bytes: usize) {
        self.event(log::Level::Debug, "done", "", source_bytes);
    }
    pub(crate) fn failed(self, cause: &str) {
        self.event(log::Level::Error, "failure", cause, 0);
    }
    fn event(&self, level: log::Level, phase: &'static str, cause: &str, source_bytes: usize) {
        if !log::log_enabled!(target: "motion_render::pointwise_compile", level) {
            return;
        }
        Event {
            context: self.context.clone(),
            level,
            phase,
            elapsed_ns: self.began.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
            cause: cause.chars().take(MAX_CAUSE_CHARS).collect(),
            source_bytes,
            observed: Instant::now(),
            deferred: false,
        }
        .record();
    }
}

#[cfg(test)]
mod tests;
