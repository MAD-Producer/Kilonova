//! Experimental foreground owner. Disabled creates no worker or polling loop.
//! Current profile is full-resolution High, authored camera, single-time output.
use motion_desktop_media::movie::{
    prerender::{
        coordinator::{Coordinator, Demand},
        PrerenderPlan,
    },
    ExportControl,
};
use motion_host::{
    prerender::{Direction, Policy},
    session::Session,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
mod signals;
#[cfg(all(test, target_os = "windows"))]
mod tests;
pub(super) use signals::Signals;

const CPU_BYTES: u64 = 64 << 20;
const GPU_BYTES: u64 = 128 << 20;
pub(super) const POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(PartialEq)]
struct Signature {
    document: u64,
    revision: u64,
    surface: u64,
    view: u64,
    composition: String,
}
impl Signature {
    fn new(s: &Session) -> Self {
        Self {
            document: s.document_generation,
            revision: s.engine.revision(),
            surface: s.surface_epoch,
            view: s.view_revision,
            composition: s.engine.project().composition_id.clone(),
        }
    }
}

pub(super) struct Runtime {
    owner: Option<Coordinator>,
    signals: Arc<Signals>,
    enabled: bool,
    signature: Option<Signature>,
    lease: Option<u64>,
    fault: Option<String>,
    hold: &'static str,
    began: Instant,
    last_poll: Option<Instant>,
    previous_frame: Option<f64>,
    direction: Direction,
    idle_delay: Duration,
}
impl Runtime {
    pub fn new(signals: Arc<Signals>) -> Self {
        Self {
            owner: None,
            signals,
            enabled: false,
            signature: None,
            lease: None,
            fault: None,
            hold: "disabled",
            began: Instant::now(),
            last_poll: None,
            previous_frame: None,
            direction: Direction::Forward,
            idle_delay: Policy::default().idle_delay,
        }
    }
    pub fn polling(&self) -> bool {
        self.enabled || self.lease.is_some()
    }
    pub fn session_failure(&mut self, error: String) {
        self.fail("session", error);
    }
    /// Explicit policy change is the recovery boundary. No implicit retry of a
    /// failing source and no replacement of a spontaneously stopped worker.
    pub fn configure(&mut self, policy: Policy) -> Result<(), String> {
        motion_host::prerender::Scheduler::new(policy).map_err(|e| e.to_string())?;
        self.signals.interrupt()?;
        match &mut self.owner {
            Some(owner) => owner.set_policy(policy)?,
            None if policy.enabled && policy.window != 0 => {
                self.owner = Some(Coordinator::new(policy, CPU_BYTES, GPU_BYTES)?);
            }
            None => {}
        }
        self.enabled = policy.enabled && policy.window != 0;
        self.idle_delay = policy.idle_delay;
        self.signature = None;
        self.fault = None;
        self.hold = if self.enabled {
            "no_surface"
        } else {
            "disabled"
        };
        Ok(())
    }
    fn reconcile(&mut self, s: &Session) -> Result<(), String> {
        let signature = Signature::new(s);
        if self.signature.as_ref() != Some(&signature) {
            if let Some(owner) = &mut self.owner {
                owner.invalidate()?;
            }
            self.signature = Some(signature);
        }
        Ok(())
    }
    fn eligible(s: &Session) -> Result<bool, String> {
        Ok(s.graphics.is_some()
            && s.preview.mode == motion_render::PreviewMode::High
            && !s.observing
            && !motion_host::render_sampling::needs_exposure(s.engine.project())
                .map_err(|e| e.to_string())?
            && !s.engine.gesture_active())
    }
    fn fail(&mut self, phase: &str, error: String) {
        if self.fault.is_none() {
            log::error!(
                "prerender runtime phase={phase} cause={}",
                crate::fallback_observation::short(&error)
            );
            self.fault = Some(error.chars().take(2048).collect());
            self.hold = "fault";
            // Cancellation retains all actual owners until acknowledgment.
            if let Err(secondary) = self.signals.interrupt() {
                log::error!(
                    "prerender runtime phase=cancel_after_fault secondary_cause={secondary}"
                );
            }
        }
    }
    pub fn tick(
        &mut self,
        s: &Session,
        foreground_depth: &std::sync::atomic::AtomicUsize,
        request: u64,
    ) {
        if !self.enabled && self.lease.is_none() {
            return;
        }
        if self
            .last_poll
            .is_some_and(|last| last.elapsed() < POLL_INTERVAL)
        {
            return;
        }
        self.last_poll = Some(Instant::now());
        let result = self.signals.generation().and_then(|generation| {
            let pending = foreground_depth.load(std::sync::atomic::Ordering::Acquire) != 0;
            self.tick_inner(s, pending, request, generation)
        });
        if let Err(error) = result {
            if error != "movie export cancelled" {
                self.fail("tick", error);
            }
        }
    }
    fn tick_inner(
        &mut self,
        s: &Session,
        foreground_pending: bool,
        request: u64,
        generation: u64,
    ) -> Result<(), String> {
        self.reconcile(s)?;
        let Some(g) = &s.graphics else {
            self.hold = "no_surface";
            return Ok(());
        };
        let eligible = Self::eligible(s)?;
        let pressure = s.preview_memory.blocked()
            || s.preview_memory.quality(true).level != 0
            || !s.preview_memory.allows_image_prefetch();
        if self.previous_frame.is_some_and(|frame| s.frame < frame) {
            self.direction = Direction::Backward;
        } else if self.previous_frame.is_some_and(|frame| s.frame > frame) {
            self.direction = Direction::Forward;
        }
        self.previous_frame = Some(s.frame);
        let demand = Demand {
            frame: s.frame,
            direction: self.direction,
            playing: false,
            editing: s.engine.gesture_active(),
            foreground_pending,
            pressure,
            now: self.began.elapsed(),
        };
        let Some(owner) = &mut self.owner else {
            return Ok(());
        };
        // Cancel/invalidate before draining a completion; an obsolete result
        // must never be installed just because acknowledgment arrived first.
        if !eligible {
            owner.invalidate()?;
        } else {
            owner.update(demand)?;
        }
        owner.poll(&g.renderer)?;
        if !owner.busy() {
            if let Some(ticket) = self.lease.take() {
                self.signals.finish(ticket)?;
            }
        }
        if !self.enabled {
            self.hold = "disabled";
            return Ok(());
        }
        if self.fault.is_some() {
            self.hold = "fault";
            return Ok(());
        }
        if !eligible {
            self.hold = "unsupported_profile";
            return Ok(());
        }
        owner.update(demand)?;
        if foreground_pending || pressure {
            self.hold = if pressure { "pressure" } else { "foreground" };
            return Ok(());
        }
        if owner.busy() {
            self.hold = "busy";
            return Ok(());
        }
        if self.signals.quiet_for()? < self.idle_delay {
            self.hold = "idle_delay";
            return Ok(());
        }
        let control = ExportControl::default();
        let plan = if owner.profile().is_none() {
            log::info!("start #{request} prerender_runtime phase=capture");
            crate::logging::flush();
            Some(PrerenderPlan::capture(
                s,
                s.document_generation,
                s.surface_epoch,
                request,
                CPU_BYTES,
                &control,
            )?)
        } else {
            None
        };
        let Some(ticket) = self.signals.publish(generation, control.clone())? else {
            self.hold = "foreground";
            return Ok(());
        };
        let sent = match plan {
            Some(plan) => owner.prepare(plan, control),
            None => owner.schedule(&g.renderer, control),
        };
        match sent {
            Ok(true) => {
                self.lease = Some(ticket);
                if let Some(observation) = owner.observation() {
                    self.signals.observe(observation)?;
                }
                self.hold = "busy";
            }
            result => {
                self.signals.finish(ticket)?;
                self.hold = "idle";
                result?;
            }
        }
        Ok(())
    }
    pub fn cached(
        &mut self,
        s: &Session,
        frame: f64,
    ) -> Option<Arc<motion_render::CachedCompositionFrame>> {
        if !self.enabled || self.fault.is_some() {
            return None;
        }
        let result = (|| {
            self.reconcile(s)?;
            if !Self::eligible(s)?
                || s.preview_memory.blocked()
                || s.preview_memory.quality(true).level != 0
            {
                return Ok(None);
            }
            let Some(owner) = &mut self.owner else {
                return Ok(None);
            };
            owner.get(
                &s.graphics
                    .as_ref()
                    .ok_or("prerender surface missing")?
                    .renderer,
                frame,
            )
        })();
        match result {
            Ok(cached) => cached,
            Err(error) => {
                self.fail("lookup", error);
                None
            }
        }
    }
    pub fn status(&self) -> serde_json::Value {
        let scheduler = self.owner.as_ref().map(Coordinator::status);
        serde_json::json!({"enabled":self.enabled,"experimental":true,"hold":self.hold,
            "profile":"high_authored_single_time_idle","fault":self.fault,
            "cache":self.owner.as_ref().and_then(Coordinator::cache_info),
            "pending":scheduler.map(|s|s.pending),
            "started":scheduler.map_or(0,|s|s.counts.started),"completed":scheduler.map_or(0,|s|s.counts.completed),
            "canceled":scheduler.map_or(0,|s|s.counts.canceled),"stale":scheduler.map_or(0,|s|s.counts.stale),
            "cpuQuotaBytes":CPU_BYTES,"gpuQuotaBytes":GPU_BYTES,
            "sourceChanges":"managed_revision_or_explicit_refresh","diskCache":false})
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        if let Err(error) = self.signals.interrupt() {
            log::error!("prerender runtime shutdown: {error}");
        }
    }
}
