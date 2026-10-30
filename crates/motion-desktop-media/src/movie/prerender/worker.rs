//! One native rendering owner, one command and one bounded completion mailbox.
use super::*;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Profile {
    pub context: CompositionFrameContext,
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    pub precision: motion_effects::image_contract::ChannelPrecision,
}
impl Profile {
    pub fn frame_key(self, frame: f64) -> Result<CompositionFrameKey> {
        if !frame.is_finite() || frame < 0. || frame >= f64::from(self.frames) {
            return Err("prerender worker frame outside the frozen composition".into());
        }
        CompositionFrameKey::new(
            self.context,
            frame,
            self.width,
            self.height,
            motion_render::image_formats::native_format(
                motion_render::image_formats::color_storage(self.precision),
            ),
        )
        .map_err(|e| e.to_string())
    }
    fn bytes(self) -> Result<u64> {
        motion_render::image_formats::color_storage(self.precision)
            .capacity(self.width, self.height)
            .ok_or("prerender completion capacity overflow".into())
    }
}

struct CpuBudget {
    limit: u64,
    live: AtomicU64,
}
struct CpuPermit {
    budget: Arc<CpuBudget>,
    bytes: u64,
}
impl CpuBudget {
    fn reserve(self: &Arc<Self>, bytes: u64) -> Option<CpuPermit> {
        self.live
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |live| {
                live.checked_add(bytes).filter(|n| *n <= self.limit)
            })
            .ok()?;
        Some(CpuPermit {
            budget: self.clone(),
            bytes,
        })
    }
}
impl Drop for CpuPermit {
    fn drop(&mut self) {
        self.budget.live.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

/// The permit follows the actual CPU owner, including a caller-held completion.
/// There is no into-image escape that could release the permit before its bytes.
pub struct WorkerFrame {
    frame: CompletedFrame,
    _permit: CpuPermit,
}
impl WorkerFrame {
    pub fn key(&self) -> CompositionFrameKey {
        self.frame.key()
    }
    pub fn image(&self) -> &CapturedImage {
        self.frame.image()
    }
}
pub enum Outcome {
    Prepared(Profile),
    Frame(WorkerFrame),
    Cancelled,
    Failed(String),
}
pub struct Completion {
    pub id: u64,
    pub outcome: Outcome,
}
#[derive(Clone, Debug)]
pub struct Flight {
    pub id: u64,
    pub operation: &'static str,
    pub phase: &'static str,
    pub frame: Option<f64>,
    pub elapsed: Duration,
}
pub struct Snapshot {
    pub in_flight: Option<Flight>,
    pub completion_waiting: bool,
    pub stopped: bool,
    pub retained_cpu_bytes: u64,
    pub cpu_limit_bytes: u64,
    pub last_failure: Option<String>,
}
/// Read-only and Send + Sync: a watchdog need not enqueue a command on either
/// the Engine owner or the potentially blocked native rendering thread.
#[derive(Clone)]
pub struct Observation {
    state: Arc<Mutex<State>>,
    cpu: Arc<CpuBudget>,
}
impl Observation {
    pub fn snapshot(&self) -> Result<Snapshot> {
        let state = self
            .state
            .lock()
            .map_err(|_| "prerender observation state poisoned")?;
        Ok(Snapshot {
            in_flight: state.flight.as_ref().map(|f| Flight {
                id: f.id,
                operation: f.operation,
                phase: f.phase,
                frame: f.frame,
                elapsed: f.began.elapsed(),
            }),
            completion_waiting: state.waiting,
            stopped: state.stopped,
            retained_cpu_bytes: self.cpu.live.load(Ordering::Acquire),
            cpu_limit_bytes: self.cpu.limit,
            last_failure: state.last_failure.clone(),
        })
    }
}
struct ActiveObservation {
    id: u64,
    operation: &'static str,
    phase: &'static str,
    frame: Option<f64>,
    began: Instant,
}
#[derive(Default)]
struct State {
    flight: Option<ActiveObservation>,
    waiting: bool,
    stopped: bool,
    last_failure: Option<String>,
}
enum Work {
    Prepare(PrerenderPlan),
    Render { frame: f64, permit: CpuPermit },
    Discard,
}
struct Job {
    id: u64,
    control: ExportControl,
    work: Work,
}

/// The caller must not create a replacement while this owner is still running.
/// Disabled initial policy should own no Worker. Cancellation is cooperative;
/// a driver call that never returns stays visible and is never timed out/restarted.
pub struct Worker {
    sender: Option<SyncSender<Job>>,
    receiver: Receiver<Completion>,
    handle: Option<JoinHandle<()>>,
    state: Arc<Mutex<State>>,
    cpu: Arc<CpuBudget>,
    active: Option<(u64, ExportControl)>,
    profile: Option<Profile>,
    next_id: u64,
    _owner: PhantomData<Rc<()>>,
}
impl Worker {
    pub fn start(cpu_limit_bytes: u64) -> Result<Self> {
        Self::start_with_phase_hook(cpu_limit_bytes, |_, _| {})
    }
    fn start_with_phase_hook(
        cpu_limit_bytes: u64,
        phase_hook: impl Fn(u64, &'static str) + Send + 'static,
    ) -> Result<Self> {
        if cpu_limit_bytes == 0 {
            return Err("prerender CPU quota is disabled".into());
        }
        let (sender, jobs) = mpsc::sync_channel(1);
        let (completed, receiver) = mpsc::sync_channel(1);
        let state = Arc::new(Mutex::new(State::default()));
        let observation = state.clone();
        let handle = thread::Builder::new()
            .name("motion-prerender".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(jobs, &completed, &observation, &phase_hook)
                }));
                if result.is_err() {
                    // The process panic hook already captured the original stack.
                    // Never reuse the renderer or launch a replacement here.
                    let flight = observation
                        .lock()
                        .ok()
                        .and_then(|s| s.flight.as_ref().map(|f| f.id));
                    log::error!("prerender worker panic ticket={flight:?} phase=worker_exit");
                    if let Some(id) = flight {
                        let _ = completed.try_send(Completion {
                            id,
                            outcome: Outcome::Failed(
                                "prerender worker panicked; native owner retired".into(),
                            ),
                        });
                    }
                }
                if let Ok(mut state) = observation.lock() {
                    state.flight = None;
                    state.stopped = true;
                    if result.is_err() {
                        state.last_failure = Some("prerender worker panicked".into());
                    }
                }
            })
            .map_err(|e| format!("start prerender worker: {e}"))?;
        Ok(Self {
            sender: Some(sender),
            receiver,
            handle: Some(handle),
            state,
            cpu: Arc::new(CpuBudget {
                limit: cpu_limit_bytes,
                live: AtomicU64::new(0),
            }),
            active: None,
            profile: None,
            next_id: 0,
            _owner: PhantomData,
        })
    }
    pub fn prepare(&mut self, plan: PrerenderPlan) -> Result<Option<u64>> {
        self.prepare_with_control(plan, ExportControl::default())
    }
    pub fn prepare_with_control(
        &mut self,
        mut plan: PrerenderPlan,
        control: ExportControl,
    ) -> Result<Option<u64>> {
        plan.max_frame_bytes = plan.max_frame_bytes.min(self.cpu.limit);
        let accepted = self.send(Work::Prepare(plan), control)?;
        if accepted.is_some() {
            self.profile = None;
        }
        Ok(accepted)
    }
    pub fn render(&mut self, frame: f64) -> Result<Option<u64>> {
        self.render_with_control(frame, ExportControl::default())
    }
    /// Publish this control in the Engine interruption state before calling.
    pub fn render_with_control(
        &mut self,
        frame: f64,
        control: ExportControl,
    ) -> Result<Option<u64>> {
        if self.active.is_some() {
            return Ok(None);
        }
        let profile = self
            .profile
            .ok_or("prerender worker has no acknowledged preparation")?;
        profile.frame_key(frame)?;
        let Some(permit) = self.cpu.reserve(profile.bytes()?) else {
            return Ok(None);
        };
        self.send(Work::Render { frame, permit }, control)
    }
    pub fn discard(&mut self) -> Result<Option<u64>> {
        let accepted = self.send(Work::Discard, ExportControl::default())?;
        if accepted.is_some() {
            self.profile = None;
        }
        Ok(accepted)
    }
    fn send(&mut self, work: Work, control: ExportControl) -> Result<Option<u64>> {
        if self.active.is_some() {
            return Ok(None);
        }
        if self.handle.as_ref().is_none_or(JoinHandle::is_finished) {
            return Err("prerender worker is stopped; no automatic restart".into());
        }
        let id = self
            .next_id
            .checked_add(1)
            .ok_or("prerender worker ticket exhausted")?;
        control.check()?;
        let sender = self.sender.as_ref().ok_or("prerender worker is closing")?;
        match sender.try_send(Job {
            id,
            control: control.clone(),
            work,
        }) {
            Ok(()) => {
                self.active = Some((id, control));
                self.next_id = id;
                Ok(Some(id))
            }
            Err(TrySendError::Full(_)) => {
                Err("prerender worker mailbox occupied without an active owner".into())
            }
            Err(TrySendError::Disconnected(_)) => Err("prerender worker channel closed".into()),
        }
    }
    pub fn cancel(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|(_, control)| control.cancel())
    }
    pub fn poll(&mut self) -> Result<Option<Completion>> {
        let mut complete = match self.receiver.try_recv() {
            Ok(complete) => complete,
            Err(TryRecvError::Empty) => return Ok(None),
            Err(TryRecvError::Disconnected) if self.active.is_none() => return Ok(None),
            Err(TryRecvError::Disconnected) => {
                return Err("prerender worker exited without acknowledgment".into())
            }
        };
        let Some((id, control)) = self.active.take() else {
            return Err("prerender worker completion has no active owner".into());
        };
        if id != complete.id {
            return Err("prerender worker completion ticket mismatch".into());
        }
        // Cancellation after worker publication still cannot expose a usable frame.
        if let Err(cause) = control.check() {
            if !matches!(complete.outcome, Outcome::Failed(_)) {
                complete.outcome = if cause == "movie export cancelled" {
                    Outcome::Cancelled
                } else {
                    Outcome::Failed(
                        format!("prerender completion control: {cause}")
                            .chars()
                            .take(2048)
                            .collect(),
                    )
                };
            } else if cause != "movie export cancelled" {
                log::error!(
                    "prerender worker ticket={id} phase=completion_control secondary_cause={cause}"
                );
            }
        }
        self.profile = match &complete.outcome {
            Outcome::Prepared(profile) => Some(*profile),
            Outcome::Frame(_) => self.profile,
            Outcome::Cancelled | Outcome::Failed(_) => None,
        };
        self.state
            .lock()
            .map_err(|_| "prerender observation state poisoned")?
            .waiting = false;
        Ok(Some(complete))
    }
    pub fn snapshot(&self) -> Result<Snapshot> {
        self.observation().snapshot()
    }
    pub fn observation(&self) -> Observation {
        Observation {
            state: self.state.clone(),
            cpu: self.cpu.clone(),
        }
    }
    /// Does not wait for GPU/decoder work. A running thread keeps its sole owner
    /// and source closure until it acknowledges cancellation and drops them.
    pub fn shutdown(&mut self) {
        self.cancel();
        self.sender = None;
        if self.handle.as_ref().is_some_and(JoinHandle::is_finished) {
            if let Some(handle) = self.handle.take() {
                if handle.join().is_err() {
                    log::error!("prerender worker join failed phase=shutdown");
                }
            }
        }
    }
    pub fn stopped(&self) -> bool {
        self.handle.as_ref().is_none_or(JoinHandle::is_finished)
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run(
    jobs: Receiver<Job>,
    completed: &SyncSender<Completion>,
    state: &Arc<Mutex<State>>,
    phase_hook: &impl Fn(u64, &'static str),
) {
    let mut input: Option<PreparedPrerender> = None;
    let mut renderer: Option<NativePrerender> = None;
    let mut last_id = 0;
    while let Ok(Job { id, control, work }) = jobs.recv() {
        last_id = id;
        let (operation, frame) = match &work {
            Work::Prepare(_) => ("prepare", None),
            Work::Render { frame, .. } => ("render", Some(*frame)),
            Work::Discard => ("discard", None),
        };
        let began = Instant::now();
        let started = match state.lock() {
            Ok(mut state) => {
                state.flight = Some(ActiveObservation {
                    id,
                    operation,
                    phase: "start",
                    frame,
                    began,
                });
                true
            }
            Err(_) => false,
        };
        if !started {
            log::error!("prerender worker ticket={id} operation={operation} phase=start: observation state poisoned");
            return;
        }
        log::info!("start #{id} prerender_worker operation={operation} frame={frame:?}");
        log::logger().flush();
        phase_hook(id, "start");
        let mut observation_failed = false;
        let mut progress = |phase: Phase| {
            if let Ok(mut state) = state.lock() {
                if let Some(flight) = &mut state.flight {
                    flight.phase = phase.name();
                }
            } else {
                observation_failed = true;
                control.cancel();
            }
            phase_hook(id, phase.name());
        };
        let result: Result<Outcome> = (|| {
            control.check()?;
            match work {
                Work::Prepare(plan) => {
                    retiring(state, phase_hook, id);
                    renderer = None;
                    input = None;
                    let prepared = plan.prepare(&control, &mut progress)?;
                    let project = &prepared.frozen.project;
                    let profile = Profile {
                        context: prepared.context(),
                        width: project.width,
                        height: project.height,
                        frames: project.frames,
                        precision: project.working_precision,
                    };
                    input = Some(prepared);
                    Ok(Outcome::Prepared(profile))
                }
                Work::Render { frame, permit } => {
                    if renderer.is_none() {
                        progress(Phase::Initialize);
                        renderer = Some(NativePrerender::open(
                            input
                                .take()
                                .ok_or("prerender worker prepared input missing")?,
                            &control,
                        )?);
                    }
                    let frame = renderer
                        .as_mut()
                        .ok_or("prerender native owner missing")?
                        .render(frame, &control, &mut progress)?;
                    if frame.image().pixels.capacity() as u64 > permit.bytes {
                        return Err(
                            "prerender native completion capacity exceeds CPU admission".into()
                        );
                    }
                    Ok(Outcome::Frame(WorkerFrame {
                        frame,
                        _permit: permit,
                    }))
                }
                Work::Discard => {
                    retiring(state, phase_hook, id);
                    renderer = None;
                    input = None;
                    Ok(Outcome::Cancelled)
                }
            }
        })();
        let result = match result {
            Err(cause) if observation_failed && cause == "movie export cancelled" => {
                Err("prerender observation state poisoned".into())
            }
            other => other,
        };
        let outcome = match result {
            Ok(value) => match control.check() {
                Ok(()) => value,
                Err(cause) if cause == "movie export cancelled" => Outcome::Cancelled,
                Err(cause) => Outcome::Failed(cause.chars().take(2048).collect()),
            },
            Err(cause) if cause == "movie export cancelled" => Outcome::Cancelled,
            Err(cause) => Outcome::Failed(cause.chars().take(2048).collect()),
        };
        if let Outcome::Failed(cause) = &outcome {
            log::error!("prerender worker ticket={id} operation={operation} frame={frame:?} elapsed_ms={} cause={cause}", began.elapsed().as_millis());
        }
        if matches!(outcome, Outcome::Cancelled | Outcome::Failed(_)) {
            // Failed/canceled generations release their source and native owner
            // only after real operation acknowledgment; no interactive join.
            retiring(state, phase_hook, id);
            renderer = None;
            input = None;
        }
        let failure = match &outcome {
            Outcome::Failed(cause) => Some(cause.clone()),
            _ => None,
        };
        // State publication precedes completion: the owner can never drain a
        // reply and then have the worker restore a stale waiting=true flag.
        if let Ok(mut state) = state.lock() {
            state.flight = None;
            state.waiting = true;
            if failure.is_some() {
                state.last_failure = failure;
            }
        } else {
            return;
        }
        if let Err(error) = completed.try_send(Completion { id, outcome }) {
            if matches!(error, TrySendError::Full(_)) {
                log::error!("prerender worker ticket={id} operation={operation} phase=completion: bounded mailbox already occupied");
            }
            return;
        }
        log::debug!(
            "done #{id} prerender_worker operation={operation} elapsed_ms={}",
            began.elapsed().as_millis()
        );
    }
    if input.is_some() || renderer.is_some() {
        if let Ok(mut state) = state.lock() {
            state.flight = Some(ActiveObservation {
                id: last_id,
                operation: "shutdown",
                phase: "retire-native",
                frame: None,
                began: Instant::now(),
            });
        }
        log::info!("start prerender_worker operation=shutdown last_ticket={last_id}");
        phase_hook(last_id, "retire-native");
        drop(renderer);
        drop(input);
    }
}

fn retiring(state: &Arc<Mutex<State>>, phase_hook: &impl Fn(u64, &'static str), id: u64) {
    if let Ok(mut state) = state.lock() {
        if let Some(flight) = &mut state.flight {
            flight.phase = "retire-native";
        }
    }
    phase_hook(id, "retire-native");
}

#[cfg(test)]
mod tests;
