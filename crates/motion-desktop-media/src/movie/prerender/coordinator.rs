//! Foreground-owned scheduling, actual destination admission and native install.
//! The Engine must invalidate this owner before edited/document/device/source
//! generations can be consumed. Session presentation/profile integration is separate.
use super::*;
use motion_host::prerender::{self as schedule, Scheduler, Ticket};
use motion_render::{CompositionFrameCache, NativeCompositionCandidate};
use worker::{Outcome, Profile, Worker};

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
    cache: Option<CompositionFrameCache>,
    cpu_bytes: u64,
    gpu_bytes: u64,
    epoch: u64,
    retiring: bool,
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
            cache: None,
            cpu_bytes,
            gpu_bytes,
            epoch: 0,
            retiring: false,
        })
    }
    pub fn profile(&self) -> Option<Profile> {
        self.profile
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
        if self.worker.is_none() {
            self.worker = Some(Worker::start(self.cpu_bytes)?);
        }
        let Some(id) = self
            .worker
            .as_mut()
            .ok_or("missing prerender owner")?
            .prepare_with_control(plan, control)?
        else {
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
        self.cache_profile = None;
        if let Some(cache) = &mut self.cache {
            cache.clear();
        }
        Ok(())
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
        Ok(())
    }
    pub fn update(&mut self, demand: Demand) -> Result<()> {
        let Some(profile) = self.profile else {
            return Ok(());
        };
        self.scheduler
            .update(schedule::Demand {
                context: context(profile),
                frame: demand.frame,
                total_frames: profile.frames,
                direction: demand.direction,
                playing: demand.playing,
                editing: demand.editing,
                foreground_pending: demand.foreground_pending,
                pressure: demand.pressure,
                now: demand.now,
            })
            .map_err(|e| e.to_string())?;
        if matches!(&self.active, Some(Active::Render { ticket, .. }) if ticket.canceled()) {
            if let Some(worker) = &self.worker {
                worker.cancel();
            }
        }
        Ok(())
    }
    /// Drain one matching completion. Run update/invalidation before this call.
    pub fn poll(&mut self, renderer: &Renderer) -> Result<bool> {
        let Some(worker) = &mut self.worker else {
            return Ok(false);
        };
        let Some(complete) = worker.poll()? else {
            return Ok(false);
        };
        if self.active.as_ref().map(Active::id) != Some(complete.id) {
            return Err("prerender coordinator completion ticket mismatch".into());
        }
        let active = self
            .active
            .take()
            .ok_or("missing prerender coordinator flight")?;
        match active {
            Active::Prepare { epoch, .. } => match complete.outcome {
                Outcome::Prepared(profile) if epoch == self.epoch => {
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
                let installed = match complete.outcome {
                    Outcome::Frame(frame) if epoch == self.epoch && !ticket.canceled() => {
                        let key = frame.key();
                        let context = key.context();
                        log::info!("start #{} prerender_install frame={} document={} device={}",
                            complete.id, key.frame(), context.document_generation, context.device_generation);
                        log::logger().flush();
                        if frame.key() != target.key() {
                            Err("prerender completion differs from admitted frame".into())
                        } else {
                            match &mut self.cache {
                                Some(cache) => cache
                                    .install_native(renderer, target, frame.image())
                                    .map_err(|e| e.to_string()),
                                None => Err("missing prerender cache".into()),
                            }
                        }
                    }
                    Outcome::Failed(cause) => Err(cause),
                    Outcome::Cancelled | Outcome::Frame(_) => Ok(false),
                    Outcome::Prepared(_) => Err("prerender render returned preparation".into()),
                };
                // A noninstalled result must not become a scheduler completion.
                if !matches!(installed, Ok(true)) {
                    self.scheduler.invalidate();
                    // Worker retires canceled/failed source and GPU generations.
                    self.profile = None;
                }
                self.scheduler.finish(&ticket);
                installed
            }
        }
    }
    /// Reserve the real foreground GPU destination before consuming a ticket.
    pub fn schedule(&mut self, renderer: &Renderer, control: ExportControl) -> Result<bool> {
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
        let Some(target) = cache
            .prepare_native(renderer, key)
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
        let sent = worker.render_with_control(next.frame(), control);
        match sent {
            Ok(Some(id)) => {
                self.active = Some(Active::Render {
                    id,
                    epoch: self.epoch,
                    ticket,
                    target,
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
