//! Foreground-owned scheduling, actual destination admission and native install.
//! The Engine must invalidate this owner before edited/document/device/source
//! generations can be consumed. Session presentation/profile integration is separate.
use super::*;
use motion_host::prerender::{self as schedule, Scheduler, Ticket};
use motion_render::{CompositionFrameCache, NativeCompositionCandidate};
use worker::{Outcome, Profile, Worker, YieldControl};

enum Control {
    Cancel(ExportControl),
    Yield(YieldControl),
}

#[derive(Clone, Copy)]
pub struct Demand {
    pub frame: f64,
    pub direction: schedule::Direction,
    pub playing: bool,
    pub editing: bool,
    pub foreground_pending: bool,
    pub pressure: bool,
    pub now: Duration,
}
enum Active {
    Prepare {
        id: u64,
        epoch: u64,
    },
    Render {
        id: u64,
        epoch: u64,
        ticket: Ticket,
        target: NativeCompositionCandidate,
        yielding: Option<YieldControl>,
    },
}
impl Active {
    fn id(&self) -> u64 {
        match self {
            Self::Prepare { id, .. } | Self::Render { id, .. } => *id,
        }
    }
}

/// One foreground owner and one persistent background owner. Disabled initial
/// policy creates no thread, device, native destination or source snapshot.
pub struct Coordinator {
    scheduler: Scheduler,
    policy: schedule::Policy,
    worker: Option<Worker>,
    active: Option<Active>,
    profile: Option<Profile>,
    // Cache validity survives cooperative render cancellation, but lookup stays
    // closed until a new preparation acknowledges the complete frozen context.
    cache_profile: Option<Profile>,
    source: Option<source_watch::Observation>,
    cache: Option<CompositionFrameCache>,
    cpu_bytes: u64,
    gpu_bytes: u64,
    epoch: u64,
    retiring: bool,
    demand: Option<schedule::Demand>,
    disk: Option<disk::Config>,
    disk_status: disk::Stats,
}
impl Coordinator {
    pub fn new(policy: schedule::Policy, cpu_bytes: u64, gpu_bytes: u64) -> Result<Self> {
        Ok(Self {
            scheduler: Scheduler::new(policy).map_err(|e| e.to_string())?,
            policy,
            worker: None,
            active: None,
            profile: None,
            cache_profile: None,
            source: None,
            cache: None,
            cpu_bytes,
            gpu_bytes,
            epoch: 0,
            retiring: false,
            demand: None,
            disk: None,
            disk_status: disk::Stats::default(),
        })
    }
    pub fn profile(&self) -> Option<Profile> {
        self.profile.filter(|_| {
            self.source
                .as_ref()
                .is_some_and(source_watch::Observation::valid)
        })
    }
    pub fn busy(&self) -> bool {
        self.active.is_some()
    }
    pub fn status(&self) -> schedule::Status {
        self.scheduler.status()
    }
    pub fn observation(&self) -> Option<worker::Observation> {
        self.worker.as_ref().map(Worker::observation)
    }
    pub fn cache_info(&self) -> Option<motion_render::CompositionFrameCacheInfo> {
        self.cache.as_ref().map(CompositionFrameCache::info)
    }
    /// Reconfiguration invalidates current work; actual cache IO is deferred
    /// until the background worker prepares the next frozen closure.
    pub fn set_disk_cache(&mut self, config: Option<disk::Config>) -> Result<()> {
        if self.disk != config {
            self.invalidate()?;
            self.disk = config;
            self.disk_status.enabled = self.disk.is_some();
            self.disk_status.ready = false;
            self.disk_status.quota_bytes = self.disk.as_ref().map_or(0, disk::Config::quota_bytes);
        }
        Ok(())
    }
    pub fn disk_info(&self) -> &disk::Stats { &self.disk_status }
    /// Reduce newly scheduled work without throwing away already warm frames.
    pub fn set_window_limit(&mut self, limit: usize) -> Result<()> {
        self.scheduler
            .set_window_limit(limit)
            .map_err(|e| e.to_string())
    }
    pub fn set_policy(&mut self, policy: schedule::Policy) -> Result<()> {
        // Validate without disturbing existing ownership on an invalid policy.
        Scheduler::new(policy).map_err(|e| e.to_string())?;
        self.invalidate()?;
        self.scheduler
            .set_policy(policy)
            .map_err(|e| e.to_string())?;
        self.policy = policy;
        if !policy.enabled || policy.window == 0 {
            if let Some(worker) = &mut self.worker {
                worker.shutdown();
                self.retiring = true;
            }
        }
        Ok(())
    }
    /// Publish the caller's cancel control before this operation can enqueue.
    /// Busy/disabled/quota holds are normal, not failures or replacement threads.
    pub fn prepare(&mut self, plan: PrerenderPlan, control: ExportControl) -> Result<bool> {
        self.prepare_job(plan, Control::Cancel(control))
    }
    pub fn prepare_with_yield(
        &mut self,
        plan: PrerenderPlan,
        control: YieldControl,
    ) -> Result<bool> {
        self.prepare_job(plan, Control::Yield(control))
    }
    fn prepare_job(&mut self, mut plan: PrerenderPlan, control: Control) -> Result<bool> {
        if !self.policy.enabled || self.policy.window == 0 || self.active.is_some() {
            return Ok(false);
        }
        if self.retiring {
            if let Some(worker) = &self.worker {
                if !worker.stopped() || worker.snapshot()?.retained_cpu_bytes != 0 {
                    return Ok(false);
                }
            }
            self.worker = None;
            self.retiring = false;
        }
        self.suspend()?;
        self.source = None;
        if self.worker.is_none() {
            self.worker = Some(Worker::start(self.cpu_bytes)?);
        }
        let worker = self.worker.as_mut().ok_or("missing prerender owner")?;
        plan.disk = self.disk.clone();
        let sent = match control {
            Control::Cancel(control) => worker.prepare_with_control(plan, control),
            Control::Yield(control) => worker.prepare_with_yield(plan, control),
        };
        let Some(id) = sent? else {
            return Ok(false);
        };
        self.active = Some(Active::Prepare {
            id,
            epoch: self.epoch,
        });
        Ok(true)
    }
    /// Cancel cooperatively; keep the actual destination and job slot until ack.
    pub fn invalidate(&mut self) -> Result<()> {
        self.suspend()?;
        self.source = None;
        self.cache_profile = None;
        if let Some(cache) = &mut self.cache {
            cache.clear();
        }
        Ok(())
    }
    fn reconcile_source(&mut self) -> Result<bool> {
        let Some(source) = &self.source else {
            return Ok(false);
        };
        if source.valid() {
            return Ok(true);
        }
        let snapshot = source.snapshot()?;
        if snapshot.fault {
            return Err(format!(
                "prerender source watcher failed: {}",
                snapshot
                    .fault_cause
                    .as_deref()
                    .or(snapshot.cause.as_deref())
                    .unwrap_or("unknown native watcher fault")
            ));
        }
        self.invalidate()?;
        Ok(false)
    }
    fn suspend(&mut self) -> Result<()> {
        self.epoch = self
            .epoch
            .checked_add(1)
            .ok_or("prerender generation exhausted")?;
        self.scheduler.invalidate();
        if let Some(worker) = &self.worker {
            worker.cancel();
        }
        self.profile = None;
        self.demand = None;
        Ok(())
    }
    pub fn update(&mut self, demand: Demand) -> Result<()> {
        self.update_job(demand, false)
    }
    pub fn advance_playback(&mut self, demand: Demand) -> Result<()> {
        self.update_job(demand, true)
    }
    fn update_job(&mut self, demand: Demand, playback: bool) -> Result<()> {
        if !self.reconcile_source()? {
            return Ok(());
        }
        let Some(profile) = self.profile else {
            return Ok(());
        };
        let demand = schedule::Demand {
            context: context(profile),
            frame: demand.frame,
            total_frames: profile.frames,
            direction: demand.direction,
            playing: demand.playing,
            editing: demand.editing,
            foreground_pending: demand.foreground_pending,
            pressure: demand.pressure,
            now: demand.now,
        };
        if playback {
            self.scheduler.advance_playback(demand)
        } else {
            self.scheduler.update(demand)
        }
        .map_err(|e| e.to_string())?;
        self.demand = Some(demand);
        if let Some(Active::Render {
            ticket, yielding, ..
        }) = &self.active
        {
            if ticket.canceled() {
                if let Some(token) = yielding
                    .as_ref()
                    .filter(|_| !demand.editing && !demand.pressure)
                {
                    // Foreground queueing and a superseded frame do not change
                    // immutable content. Yield the job without retiring its
                    // reusable native owner. Pressure/editing still retire it.
                    token.preempt();
                } else if let Some(worker) = &self.worker {
                    worker.cancel();
                }
            }
        }
        Ok(())
    }
    /// The caller must reconcile live context and holds first. Drain only a
    /// completed yielded frame that is due now; a canceled future job must not
    /// close a still-valid cache before the foreground reads its current frame.
    pub fn poll_due(&mut self, renderer: &Renderer, frame: f64) -> Result<bool> {
        if matches!(&self.active, Some(Active::Render { target, yielding: Some(token), .. })
            if target.key().frame() == frame && token.completed())
        {
            self.poll(renderer)
        } else {
            Ok(false)
        }
    }
    /// Drain one matching completion. Run update/invalidation before this call.
    pub fn poll(&mut self, renderer: &Renderer) -> Result<bool> {
        // A source fault closes lookup/admission, but cannot prevent the real
        // native operation from acknowledging. Drain before returning the
        // fault so its original failure and retained resources are accounted for.
        let source_failure = self.reconcile_source().err();
        let Some(worker) = &mut self.worker else {
            return source_failure.map_or(Ok(false), Err);
        };
        let Some(complete) = worker.poll()? else {
            return source_failure.map_or(Ok(false), Err);
        };
        self.disk_status = complete.disk.clone();
        self.disk_status.ready &= self.disk.is_some() && match &self.active {
            Some(Active::Prepare { epoch, .. } | Active::Render { epoch, .. }) => *epoch == self.epoch,
            None => false,
        };
        self.disk_status.enabled = self.disk.is_some();
        self.disk_status.quota_bytes = self.disk.as_ref().map_or(0, disk::Config::quota_bytes);
        let source = worker.source_observation()?;
        if self.active.as_ref().map(Active::id) != Some(complete.id) {
            return Err("prerender coordinator completion ticket mismatch".into());
        }
        let active = self
            .active
            .take()
            .ok_or("missing prerender coordinator flight")?;
        let observed_failure = match source.as_ref().map(|s| s.snapshot()).transpose() {
            Ok(Some(snapshot)) if snapshot.fault => Some(format!(
                "prerender source watcher failed: {}",
                snapshot
                    .fault_cause
                    .as_deref()
                    .or(snapshot.cause.as_deref())
                    .unwrap_or("unknown native watcher fault")
            )),
            Err(cause) => Some(cause),
            _ => None,
        };
        if let Some(cause) = source_failure.or(observed_failure) {
            self.scheduler.invalidate();
            if let Active::Render { ticket, .. } = &active {
                self.scheduler.finish(ticket);
            }
            self.profile = None;
            self.cache_profile = None;
            if let Some(cache) = &mut self.cache {
                cache.clear();
            }
            // Retain the fault for the existing explicit recovery boundary,
            // including when preparation failed before publishing a profile.
            if source.is_some() {
                self.source = source;
            }
            if let Outcome::Failed(primary) = &complete.outcome {
                log::error!(
                    "prerender ticket={} phase=source_watch secondary_cause={cause}",
                    complete.id
                );
                return Err(primary.clone());
            }
            return Err(cause);
        }
        match active {
            Active::Prepare { epoch, .. } => match complete.outcome {
                Outcome::Prepared(profile)
                    if epoch == self.epoch
                        && source
                            .as_ref()
                            .is_some_and(source_watch::Observation::valid) =>
                {
                    // Reopening the same immutable source must not discard
                    // completed native frames. No old entry becomes readable
                    // before this full content/output/generation acknowledgment.
                    if self.cache_profile != Some(profile) || self.cache.is_none() {
                        self.cache = Some(
                            CompositionFrameCache::new(
                                profile.context,
                                self.gpu_bytes,
                                self.policy.window,
                            )
                            .map_err(|e| e.to_string())?,
                        );
                    }
                    self.cache_profile = Some(profile);
                    self.source = source;
                    self.profile = Some(profile);
                    Ok(false)
                }
                Outcome::Failed(cause) => Err(cause),
                Outcome::Cancelled | Outcome::Prepared(_) => Ok(false),
                Outcome::Frame(_) => Err("prerender preparation returned frame pixels".into()),
            },
            Active::Render {
                epoch,
                ticket,
                target,
                ..
            } => {
                let reusable_native =
                    complete.retained_native || matches!(&complete.outcome, Outcome::Frame(_));
                let installed = match complete.outcome {
                    Outcome::Frame(frame)
                        if epoch == self.epoch
                            && !ticket.canceled()
                            && self
                                .source
                                .as_ref()
                                .is_some_and(source_watch::Observation::valid) =>
                    {
                        let began = Instant::now();
                        let key = frame.key();
                        let context = key.context();
                        let result = if frame.key() != target.key() {
                            Err("prerender completion differs from admitted frame".into())
                        } else {
                            match &mut self.cache {
                                Some(cache) => cache
                                    .install_native_for_next_submission(renderer, target, frame.image())
                                    .map_err(|e| e.to_string()),
                                None => Err("missing prerender cache".into()),
                            }
                        };
                        // This may run inside Present. Keep healthy installs
                        // free of file IO; the Engine publishes its Present
                        // flight before entry, while the background worker keeps
                        // its own default-level start signal before real work.
                        if began.elapsed() >= Duration::from_millis(100) {
                            log::debug!(
                                "done #{} prerender_install frame={} document={} device={} elapsed_ms={}",
                                complete.id, key.frame(), context.document_generation,
                                context.device_generation, began.elapsed().as_millis()
                            );
                        }
                        result
                    }
                    Outcome::Failed(cause) => Err(cause),
                    Outcome::Cancelled | Outcome::Frame(_) => Ok(false),
                    Outcome::Prepared(_) => Err("prerender render returned preparation".into()),
                };
                // A noninstalled result must not become a scheduler completion.
                if !matches!(installed, Ok(true)) {
                    self.scheduler.invalidate();
                    // A cooperative foreground yield publishes no frame. It
                    // can keep the same immutable native generation, avoiding
                    // source freeze/device initialization on every frame.
                    // Strong cancellation or a changed epoch still closes it.
                    if !reusable_native || epoch != self.epoch || installed.is_err() {
                        self.profile = None;
                    }
                }
                self.scheduler.finish(&ticket);
                installed
            }
        }
    }
    /// Reserve the real foreground GPU destination before consuming a ticket.
    pub fn schedule(&mut self, renderer: &Renderer, control: ExportControl) -> Result<bool> {
        self.schedule_job(renderer, Control::Cancel(control))
    }
    pub fn schedule_with_yield(
        &mut self,
        renderer: &Renderer,
        control: YieldControl,
    ) -> Result<bool> {
        self.schedule_job(renderer, Control::Yield(control))
    }
    fn schedule_job(&mut self, renderer: &Renderer, control: Control) -> Result<bool> {
        if !self.reconcile_source()? {
            return Ok(false);
        }
        if self.active.is_some() {
            return Ok(false);
        }
        let (Some(profile), Some(worker), Some(cache)) =
            (self.profile, &mut self.worker, &mut self.cache)
        else {
            return Ok(false);
        };
        let Some(next) = self.scheduler.next_key() else {
            return Ok(false);
        };
        let key = profile.frame_key(next.frame())?;
        // Preserve the due frame and every nearer future frame. Admission for
        // a distant lookahead must not turn a warm upcoming interval cold.
        // This list is bounded by the scheduler window, never project length.
        let mut protected = Vec::new();
        if matches!(&control, Control::Yield(_)) {
            if let Some(demand) = self.demand {
                let sign = if demand.direction == schedule::Direction::Forward {
                    1.
                } else {
                    -1.
                };
                for step in 0..=self.policy.window {
                    let frame = demand.frame + sign * step as f64;
                    if frame == next.frame() {
                        break;
                    }
                    if frame >= 0. && frame < f64::from(profile.frames) {
                        protected.push(profile.frame_key(frame)?);
                    }
                }
            }
        }
        let Some(target) = cache
            .prepare_native_preserving(renderer, key, &protected)
            .map_err(|e| e.to_string())?
        else {
            self.scheduler.take_next(false).map_err(|e| e.to_string())?;
            return Ok(false);
        };
        let ticket = self
            .scheduler
            .take_next(true)
            .map_err(|e| e.to_string())?
            .ok_or("admitted prerender destination lost scheduler ticket")?;
        let yielding = match &control {
            Control::Yield(token) => Some(token.clone()),
            Control::Cancel(_) => None,
        };
        let sent = match control {
            Control::Cancel(control) => worker.render_with_control(next.frame(), control),
            Control::Yield(control) => worker.render_with_yield(next.frame(), control),
        };
        match sent {
            Ok(Some(id)) => {
                self.active = Some(Active::Render {
                    id,
                    epoch: self.epoch,
                    ticket,
                    target,
                    yielding,
                });
                Ok(true)
            }
            result => {
                self.scheduler.invalidate();
                self.scheduler.finish(&ticket);
                result.map(|_| false)
            }
        }
    }
    /// Exact native cache lookup; this is not a submitted/presented-hit counter.
    pub fn get(
        &mut self,
        renderer: &Renderer,
        frame: f64,
    ) -> Result<Option<Arc<motion_render::CachedCompositionFrame>>> {
        if !self.reconcile_source()? {
            return Ok(None);
        }
        let (Some(profile), Some(cache)) = (self.profile, &mut self.cache) else {
            return Ok(None);
        };
        cache
            .get(renderer, profile.frame_key(frame)?)
            .map_err(|e| e.to_string())
    }
}
fn context(profile: Profile) -> schedule::Context {
    let c = profile.context;
    schedule::Context {
        document_generation: c.document_generation,
        device_generation: c.device_generation,
        content: c.content,
        output: c.output,
    }
}

#[cfg(test)]
mod tests;
