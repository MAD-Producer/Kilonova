//! Optional, bounded desktop depth jobs. The inference engine lives behind a
//! backend; callers must provide the *current* source identity when taking output.
//!
//! One running job and one replaceable pending input are retained. Cancellation
//! is cooperative: backends must check it during work (subprocess backends can
//! terminate their own child). This worker never owns a render session or GPU.

use motion_model::depth::{DepthGeometry, DepthModel, DepthResource, DepthSource};
pub(crate) mod image;
pub mod onnx;
use std::{
    any::Any,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Resource(#[from] motion_model::Error),
    #[error("depth input rejected: {0}")]
    Invalid(&'static str),
    #[error("depth inference canceled")]
    Canceled,
    #[error("depth worker state is unavailable")]
    State,
    #[error("depth worker stopped")]
    Stopped,
    #[error("depth backend panicked: {0}")]
    Panic(String),
    #[error("depth backend failed: {0}")]
    Backend(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Image(#[from] ::image::ImageError),
    #[error("{primary}; depth temporary cleanup also failed: {cleanup}")]
    Cleanup {
        #[source]
        primary: Box<Error>,
        cleanup: std::io::Error,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Key {
    pub source: DepthSource,
    pub geometry: DepthGeometry,
    pub model: DepthModel,
}

impl Key {
    pub fn same_sample(&self, other: &Self) -> bool {
        self.source.same_sample(&other.source)
            && self.geometry == other.geometry
            && self.model == other.model
    }

    fn validate(&self) -> Result<()> {
        self.source.pts.validate()?;
        self.geometry.validate()?;
        self.model.validate()?;
        Ok(())
    }
}

/// Opaque RGB8 in sRGB encoding, after the declared crop/rotation/mirroring.
/// Alpha compositing and conversion from a renderer's working space are the
/// caller's explicit responsibility. No implicit unpremultiplication is made.
#[derive(Clone, Debug)]
pub struct Input {
    key: Key,
    size: [u32; 2],
    rgb: Arc<[u8]>,
}

impl Input {
    pub fn new(key: Key, size: [u32; 2], rgb: Arc<[u8]>) -> Result<Self> {
        key.validate()?;
        let [_, _, w, h] = key.geometry.crop;
        let expected = if key.geometry.quarter_turns % 2 == 0 {
            [w, h]
        } else {
            [h, w]
        };
        if size != expected || size.contains(&0) || size.iter().any(|n| *n > 4096) {
            return Err(Error::Invalid(
                "dimensions must match the oriented crop, up to 4096",
            ));
        }
        if rgb.len() as u64 != u64::from(size[0]) * u64::from(size[1]) * 3 {
            return Err(Error::Invalid("RGB byte count differs from dimensions"));
        }
        Ok(Self { key, size, rgb })
    }
    pub fn key(&self) -> &Key {
        &self.key
    }
    pub fn size(&self) -> [u32; 2] {
        self.size
    }
    pub fn rgb(&self) -> &[u8] {
        &self.rgb
    }
}

#[derive(Clone, Debug, Default)]
pub struct Costs {
    /// Full backend wall time, including child startup, transport and cleanup.
    pub total: Duration,
    /// Measured parent-side staging/serialization and output ownership checks.
    /// This is separate from the child's tensor copies and filesystem phases.
    pub host_transport: Duration,
    pub load: Duration,
    pub preprocess: Duration,
    pub input_copy: Duration,
    pub inference: Duration,
    pub output_copy: Duration,
    pub postprocess: Duration,
    pub input_bytes: u64,
    pub output_bytes: u64,
}

#[derive(Debug)]
pub struct Estimate {
    pub resource: DepthResource,
    /// Actual provider returned by the runtime, never just the requested one.
    pub provider: String,
    pub costs: Costs,
}

impl Estimate {
    fn validate(&self, key: &Key) -> Result<()> {
        let d = self.resource.descriptor();
        if !key.source.same_sample(&d.source) || key.geometry != d.geometry || key.model != d.model
        {
            return Err(Error::Invalid(
                "backend result identity differs from request",
            ));
        }
        if self.provider.is_empty()
            || self.provider.len() > 128
            || self.provider.chars().any(char::is_control)
        {
            return Err(Error::Invalid(
                "missing or unbounded actual execution provider",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn is_canceled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub fn check(&self) -> Result<()> {
        if self.is_canceled() {
            Err(Error::Canceled)
        } else {
            Ok(())
        }
    }
    fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
}

/// Return promptly after cancellation. Errors retain their original causes; no
/// implicit retries, requested-provider reports or synthetic depth are allowed.
pub trait Backend: Send + 'static {
    fn infer(&mut self, input: &Input, cancellation: &Cancellation) -> Result<Estimate>;
}

#[derive(Clone, Copy, Debug, Default)]
/// Saturating lifetime counts of this Jobs instance; never reset by a seek.
pub struct Counts {
    pub submitted: u64,
    pub superseded_pending: u64,
    pub completed: u64,
    pub canceled: u64,
    pub failed: u64,
    pub stale: u64,
}

#[derive(Clone, Debug)]
pub struct InFlight {
    pub request: u64,
    pub source: DepthSource,
    pub elapsed: Duration,
}

/// Read-only session observation. Counts belong to this worker's lifetime;
/// invalidating a document/sample does not reset them.
#[derive(Clone, Debug)]
pub struct SessionStatus {
    pub request: Option<u64>,
    pub installed: bool,
    pub counts: Counts,
    pub in_flight: Option<InFlight>,
    pub queue_depth: usize,
}

struct Job {
    request: u64,
    input: Work,
    cancellation: Cancellation,
}

enum Work {
    Prepared(Input),
    Image(image::ImageInput),
}
impl Work {
    fn key(&self) -> &Key {
        match self {
            Self::Prepared(input) => input.key(),
            Self::Image(input) => &input.key,
        }
    }
    fn prepare(self, cancellation: &Cancellation) -> Result<Input> {
        match self {
            Self::Prepared(input) => Ok(input),
            Self::Image(input) => input.decode(cancellation),
        }
    }
}
struct Flight {
    request: u64,
    source: DepthSource,
    started: Instant,
    cancellation: Cancellation,
}
struct Completion {
    request: u64,
    key: Key,
    result: Result<Estimate>,
}
#[derive(Default)]
struct State {
    latest: u64,
    pending: Option<Job>,
    flight: Option<Flight>,
    ready: Option<Completion>,
    stopped: bool,
    counts: Counts,
}
type Shared = Arc<(Mutex<State>, Condvar)>;

pub struct Jobs {
    shared: Shared,
    worker: Option<JoinHandle<()>>,
}

impl Jobs {
    pub fn start(backend: impl Backend) -> Result<Self> {
        let shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let worker_shared = shared.clone();
        let worker = thread::Builder::new()
            .name("motion-depth".into())
            .spawn(move || run(backend, worker_shared))?;
        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }

    /// Replace the one pending input and cancel the old in-flight sample. The
    /// monotonically increasing ticket prevents an older completion overwriting it.
    pub fn request(&mut self, input: Input) -> Result<u64> {
        self.submit(Work::Prepared(input))
    }

    pub(crate) fn request_image(&mut self, input: image::ImageInput) -> Result<u64> {
        self.submit(Work::Image(input))
    }

    fn submit(&mut self, input: Work) -> Result<u64> {
        input.key().validate()?;
        let mut state = self.shared.0.lock().map_err(|_| Error::State)?;
        if state.stopped {
            return Err(Error::Stopped);
        }
        let request = state
            .latest
            .checked_add(1)
            .ok_or(Error::Invalid("request counter exhausted"))?;
        if let Some(flight) = &state.flight {
            flight.cancellation.cancel();
        }
        if let Some(pending) = state.pending.take() {
            pending.cancellation.cancel();
            state.counts.superseded_pending = state.counts.superseded_pending.saturating_add(1);
        }
        state.latest = request;
        state.ready = None;
        state.pending = Some(Job {
            request,
            input,
            cancellation: Cancellation::default(),
        });
        state.counts.submitted = state.counts.submitted.saturating_add(1);
        drop(state);
        self.shared.1.notify_one();
        Ok(request)
    }

    /// Call on seek/source/model changes even when no replacement is requested.
    pub fn cancel(&mut self) -> Result<()> {
        let mut state = self.shared.0.lock().map_err(|_| Error::State)?;
        if let Some(flight) = &state.flight {
            flight.cancellation.cancel();
        }
        if let Some(pending) = state.pending.take() {
            pending.cancellation.cancel();
        }
        state.ready = None;
        Ok(())
    }

    /// This is the installation boundary: independently supply the live key.
    /// A source/revision/PTS/crop/model mismatch is a normal discarded old result.
    pub fn take_ready(&mut self, current: &Key) -> Result<Option<Estimate>> {
        current.validate()?;
        let mut state = self.shared.0.lock().map_err(|_| Error::State)?;
        let Some(done) = state.ready.take() else {
            return Ok(None);
        };
        if done.request != state.latest || !done.key.same_sample(current) {
            state.counts.stale = state.counts.stale.saturating_add(1);
            return Ok(None);
        }
        drop(state);
        done.result.map(Some)
    }

    pub fn counts(&self) -> Result<Counts> {
        Ok(self.shared.0.lock().map_err(|_| Error::State)?.counts)
    }
    pub fn in_flight(&self) -> Result<Option<InFlight>> {
        Ok(self
            .shared
            .0
            .lock()
            .map_err(|_| Error::State)?
            .flight
            .as_ref()
            .map(|f| InFlight {
                request: f.request,
                source: f.source.clone(),
                elapsed: f.started.elapsed(),
            }))
    }
    pub fn queue_depth(&self) -> Result<usize> {
        Ok(usize::from(
            self.shared
                .0
                .lock()
                .map_err(|_| Error::State)?
                .pending
                .is_some(),
        ))
    }
}

impl Drop for Jobs {
    fn drop(&mut self) {
        let unavailable = match self.shared.0.lock() {
            Ok(mut state) => {
                state.stopped = true;
                if let Some(flight) = &state.flight {
                    flight.cancellation.cancel();
                }
                state.pending = None;
                false
            }
            Err(poison) => {
                drop(poison);
                true
            }
        };
        if unavailable {
            log::error!("operation=depth_shutdown phase=state cause=poisoned_state");
        }
        self.shared.1.notify_one();
        if let Some(worker) = self.worker.take() {
            if let Err(payload) = worker.join() {
                log::error!(
                    "operation=depth_shutdown phase=join cause={}",
                    panic_cause(payload.as_ref())
                );
            }
        }
    }
}

fn panic_cause(payload: &(dyn Any + Send)) -> String {
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("non-string panic");
    message.chars().take(512).collect()
}

fn run(mut backend: impl Backend, shared: Shared) {
    loop {
        let job = {
            let Ok(mut state) = lock_state(&shared) else {
                log::error!("operation=depth_worker phase=receive cause=poisoned_state");
                return;
            };
            while !state.stopped && state.pending.is_none() {
                state = match shared.1.wait(state) {
                    Ok(state) => state,
                    Err(poison) => {
                        drop(poison);
                        log::error!("operation=depth_worker phase=wait cause=poisoned_state");
                        return;
                    }
                };
            }
            if state.stopped {
                return;
            }
            let Some(job) = state.pending.take() else {
                continue;
            };
            state.flight = Some(Flight {
                request: job.request,
                source: job.input.key().source.clone(),
                started: Instant::now(),
                cancellation: job.cancellation.clone(),
            });
            job
        };
        let began = Instant::now();
        let key = job.input.key().clone();
        let source = &key.source;
        log::info!(
            "start #{} depth_infer asset={} revision={} pts={}/{} model={} model_revision={}",
            job.request,
            source.asset,
            source.revision,
            source.pts.ticks,
            source.pts.timescale,
            key.model.id,
            key.model.revision
        );
        let result = catch_unwind(AssertUnwindSafe(|| {
            job.cancellation.check()?;
            let input = job.input.prepare(&job.cancellation)?;
            job.cancellation.check()?;
            let estimate = backend.infer(&input, &job.cancellation)?;
            job.cancellation.check()?;
            estimate.validate(&key)?;
            Ok(estimate)
        }))
        .unwrap_or_else(|p| Err(Error::Panic(panic_cause(p.as_ref()))))
        .map_err(|error| match error {
            Error::Backend(cause) => Error::Backend(cause.chars().take(2048).collect()),
            other => other,
        });
        if let Err(error) = &result {
            if !matches!(error, Error::Canceled) {
                log::error!("request={} operation=depth_infer asset={} revision={} phase=prepare_or_backend elapsed_ms={} cause={}",
                    job.request, source.asset, source.revision, began.elapsed().as_millis(),
                    error.to_string().chars().take(768).collect::<String>());
            }
        }
        log::debug!(
            "done #{} depth_infer {}ms",
            job.request,
            began.elapsed().as_millis()
        );
        let halted = matches!(result, Err(Error::Panic(_)));
        let Ok(mut state) = lock_state(&shared) else {
            log::error!(
                "request={} operation=depth_infer phase=publish cause=poisoned_state",
                job.request
            );
            return;
        };
        state.flight = None;
        match &result {
            Ok(_) => state.counts.completed = state.counts.completed.saturating_add(1),
            Err(Error::Canceled) => state.counts.canceled = state.counts.canceled.saturating_add(1),
            Err(_) => state.counts.failed = state.counts.failed.saturating_add(1),
        }
        // Both checks matter: cancel() need not allocate another ticket.
        if state.latest == job.request && !job.cancellation.is_canceled() && !state.stopped {
            state.ready = Some(Completion {
                request: job.request,
                key,
                result,
            });
        }
        if halted {
            state.stopped = true;
            if let Some(pending) = state.pending.take() {
                pending.cancellation.cancel();
            }
            return;
        }
    }
}

fn lock_state(shared: &Shared) -> Result<std::sync::MutexGuard<'_, State>> {
    shared.0.lock().map_err(|poison| {
        drop(poison);
        Error::State
    })
}
