//! Preview-only adapter advice. Every log is deferred beyond the Session guard.
use super::*;
use motion_render::preview_memory::{
    PreviewPressureAction, PreviewPressureInfo, PreviewPressureUpdate,
};
use std::{
    cell::{Cell, RefCell},
    marker::PhantomData,
    rc::Rc,
    time::Duration,
};

thread_local! {
    static SESSION: Cell<Option<i64>> = const { Cell::new(None) };
    static PENDING: RefCell<Pending> = const { RefCell::new(Pending { events: [None, None, None], omitted: 0 }) };
}
struct Pending {
    events: [Option<Event>; 3],
    omitted: usize,
}
pub(super) struct Scope(Option<i64>, PhantomData<Rc<()>>);
impl Scope {
    pub(super) fn enter(id: i64) -> Self {
        Self(SESSION.replace(Some(id)), PhantomData)
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
            for event in events.into_iter().flatten() {
                event.emit(omitted);
            }
        }
    }
}
struct Event {
    session: Option<i64>,
    frame: f64,
    info: PreviewPressureInfo,
    level: log::Level,
    phase: &'static str,
    began: Instant,
}
impl Event {
    fn record(
        frame: f64,
        info: PreviewPressureInfo,
        level: log::Level,
        phase: &'static str,
        began: Instant,
    ) {
        let event = Self {
            session: SESSION.get(),
            frame,
            info,
            level,
            phase,
            began,
        };
        if event.session.is_none() {
            event.emit(0);
            return;
        }
        PENDING.with(|pending| {
            let mut pending = pending.borrow_mut();
            let slot = match level {
                log::Level::Error => 2,
                log::Level::Warn => 1,
                _ => 0,
            };
            if pending.events[slot].is_none() {
                pending.events[slot] = Some(event);
            } else {
                pending.omitted = pending.omitted.saturating_add(1);
            }
        });
    }
    fn emit(self, omitted: usize) {
        log::log!(self.level, "preview adapter pressure site=host.preview_pressure session={:?} operation=preview frame={} phase={} elapsed_ms={} count={} requested_floor={} source={:?} pool={:?} deferred_omitted={} scope=adapter_advisory_preview_only",
            self.session,self.frame,self.phase,self.began.elapsed().as_millis(),
            self.info.consecutive_high_samples,self.info.level,self.info.source,self.info.worst_pool,omitted);
    }
}

impl Session {
    pub(super) fn observe_preview_pressure(&mut self) -> Result<bool> {
        self.observe_preview_pressure_with(self.preview_memory_clock.elapsed(), |renderer| {
            renderer.memory_pressure()
        })
    }
    fn observe_preview_pressure_with(
        &mut self,
        now: Duration,
        query: impl FnOnce(&mut Renderer) -> motion_render::MemoryPressureSnapshot,
    ) -> Result<bool> {
        let began = Instant::now();
        let Some(graphics) = &mut self.graphics else {
            return Ok(false);
        };
        let previous = self
            .preview_memory
            .quality(self.preview.mode == PreviewMode::High);
        let update = self
            .preview_memory
            .observe_pressure(now, || query(&mut graphics.renderer));
        if !update.sampled {
            return Ok(false);
        }
        let info = self.preview_memory.pressure_info(now);
        record(self.frame, info, update, began);
        if update.action == PreviewPressureAction::EvictIdle {
            graphics.renderer.evict_idle_preview_cache();
            self.preview_video.retain(&self.scene);
        }
        let changed = previous
            != self
                .preview_memory
                .quality(self.preview.mode == PreviewMode::High);
        if changed {
            // The last completed front remains valid. Pending exposure tasks own
            // their leases until cancellation/queue retirement, as before.
            self.view_revision += 1;
            self.sampling.cancel(&mut self.video_frames)?;
            graphics.scratch.discard_candidate();
            self.preview_video.clear();
        }
        Ok(changed)
    }
}
fn record(frame: f64, info: PreviewPressureInfo, update: PreviewPressureUpdate, began: Instant) {
    if update.first_high {
        Event::record(frame, info, log::Level::Debug, "evict_idle", began);
    }
    if update.persistent_high {
        Event::record(frame, info, log::Level::Error, "sustained", began);
    }
    if let PreviewPressureAction::Quality { previous, level } = update.action {
        Event::record(
            frame,
            info,
            log::Level::Warn,
            if level > previous {
                "lower_quality"
            } else {
                "recover_quality"
            },
            began,
        );
    }
}

#[cfg(test)]
mod tests;
