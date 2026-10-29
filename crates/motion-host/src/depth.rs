//! Optional, bounded desktop depth jobs. The inference engine lives behind a
//! backend; callers must provide the *current* source identity when taking output.
//!
//! One running job and one replaceable pending input are retained. Cancellation
//! is cooperative: backends must check it during work (subprocess backends can
//! terminate their own child). This worker never owns a render session or GPU.

use motion_model::depth::{
    DepthGeometry, DepthModel, DepthResource, DepthSource, PreparedDepthInput,
};
use sha2::{Digest, Sha256};
pub(crate) mod image;
pub mod onnx;
pub(crate) mod storage;
pub(crate) mod video;
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
    #[error("depth result request={request} asset={asset} phase=retain: capacity exceeded; bytes={required_bytes}/{byte_limit} samples={required_samples}/{sample_limit}")]
    RetentionCapacity {
        request: u64,
        asset: u64,
        required_bytes: usize,
        byte_limit: usize,
        required_samples: usize,
        sample_limit: usize,
    },
    #[error("depth storage request={request} object={object} phase=spill: capacity exceeded; bytes={required_bytes}/{byte_limit} sample_limit={sample_limit}")]
    StorageCapacity {
        request: u64,
        object: u64,
        required_bytes: u64,
        byte_limit: u64,
        sample_limit: usize,
    },
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
    #[error(transparent)]
    Media(#[from] motion_media::Error),
    #[error(transparent)]
    Render(#[from] motion_render::RenderError),
    #[error("{primary}; depth temporary cleanup also failed: {cleanup}")]
    Cleanup {
        #[source]
        primary: Box<Error>,
        cleanup: std::io::Error,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
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
    image: Option<motion_render::DepthImage>,
    video_end_us: Option<u64>,
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
        Ok(Self {
            key,
            size,
            rgb,
            image: None,
            video_end_us: None,
        })
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
    pub(crate) fn into_image(self) -> Option<motion_render::DepthImage> {
        self.image
    }
    pub(crate) fn video_end_us(&self) -> Option<u64> {
        self.video_end_us
    }
    pub(crate) fn fingerprint(&self, cancellation: &Cancellation) -> Result<PreparedDepthInput> {
        let mut hash = Sha256::new();
        hash.update(b"MotionStudio/depth/rgb8-srgb-opaque/v1\0");
        for size in self.size {
            hash.update(size.to_le_bytes());
        }
        for chunk in self.rgb.chunks(64 << 10) {
            cancellation.check()?;
            hash.update(chunk);
        }
        cancellation.check()?;
        let input = PreparedDepthInput {
            rgb_size: self.size,
            rgb_sha256: hash.finalize().into(),
        };
        input.validate(&self.key.geometry)?;
        Ok(input)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Costs {
    /// Worker-side managed source IO, decode and RGB preparation, before inference.
    pub source_prepare: Duration,
    pub source_rgb_bytes: u64,
    /// Full worker operation wall time, including managed-source preparation.
    /// Direct backend calls report their own wall time without source staging.
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

/// Only the worker constructs this pair, after all crop, orientation and color
/// conversion. The backend cannot supply the authoritative input fingerprint.
#[derive(Debug)]
pub struct PreparedEstimate {
    estimate: Estimate,
    input: PreparedDepthInput,
    image: Option<motion_render::DepthImage>,
    video_end_us: Option<u64>,
}
impl PreparedEstimate {
    pub fn estimate(&self) -> &Estimate {
        &self.estimate
    }
    pub fn input(&self) -> PreparedDepthInput {
        self.input
    }
    pub(crate) fn into_owned_parts(
        self,
    ) -> (
        Estimate,
        PreparedDepthInput,
        Option<motion_render::DepthImage>,
        Option<u64>,
    ) {
        (self.estimate, self.input, self.image, self.video_end_us)
    }
    pub fn into_parts(self) -> (Estimate, PreparedDepthInput) {
        (self.estimate, self.input)
    }
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
    /// Completed jobs restored from verified scalar spill data, without inference.
    pub restored: u64,
}

#[derive(Clone, Debug)]
pub struct InFlight {
    pub request: u64,
    pub asset: u64,
    pub revision: u64,
    /// Unknown until video decoding has established the exact source sample.
    pub source: Option<DepthSource>,
    pub target_time_us: Option<u64>,
    pub phase: &'static str,
    pub elapsed: Duration,
}

/// A requested video timeline coordinate, distinct from the decoded frame PTS.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VideoKey {
    pub object: u64,
    pub asset: u64,
    pub revision: u64,
    pub target_us: u64,
    pub geometry: DepthGeometry,
    pub model: DepthModel,
}
impl VideoKey {
    fn validate(&self) -> Result<()> {
        self.geometry.validate()?;
        self.model.validate()?;
        if self.target_us > 3_600_000_000 {
            return Err(Error::Invalid(
                "video depth target exceeds source duration limit",
            ));
        }
        Ok(())
    }
}

#[derive(Clone)]
pub(crate) enum Identity {
    Sample(Key),
    Video(VideoKey),
}
impl Identity {
    pub(crate) fn same_request(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Sample(a), Self::Sample(b)) => a.same_sample(b),
            (Self::Video(a), Self::Video(b)) => a == b,
            _ => false,
        }
    }
    fn validate(&self) -> Result<()> {
        match self {
            Self::Sample(key) => key.validate(),
            Self::Video(key) => key.validate(),
        }
    }
    fn source(&self) -> Option<DepthSource> {
        match self {
            Self::Sample(key) => Some(key.source.clone()),
            Self::Video(_) => None,
        }
    }
    fn asset_revision(&self) -> (u64, u64) {
        match self {
            Self::Sample(key) => (key.source.asset, key.source.revision),
            Self::Video(key) => (key.asset, key.revision),
        }
    }
    fn target_time_us(&self) -> Option<u64> {
        match self {
            Self::Sample(_) => None,
            Self::Video(key) => Some(key.target_us),
        }
    }
    fn log_start(&self, request: u64) {
        match self {
            Self::Sample(key) => log::info!("start #{} depth_infer asset={} revision={} pts={}/{} model={} model_revision={}",
                request,key.source.asset,key.source.revision,key.source.pts.ticks,key.source.pts.timescale,key.model.id,key.model.revision),
            Self::Video(key) => log::info!("start #{} depth_infer asset={} revision={} target_us={} phase=video_decode object={} model={} model_revision={}",
                request,key.asset,key.revision,key.target_us,key.object,key.model.id,key.model.revision),
        }
    }
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
    /// Results admitted for the current document/revision. Owned static image
    /// samples survive clock changes; video and pending tickets stay frame-bound.
    pub retained_samples: usize,
    /// Scalar/validity/confidence planes, preserving the existing observation.
    pub retained_bytes: usize,
    /// Owned original RGBA allocation capacity. Combined with retained_bytes,
    /// this is bounded by the session result-admission allowance.
    pub retained_image_bytes: usize,
    /// Lifetime host result-admission failures; separate from backend failures.
    pub retention_rejections: u64,
    /// Document-scoped scalar spill data, independent of the live RGBA cache.
    pub stored_samples: usize,
    pub stored_bytes: u64,
    /// Saturating capacity failures, reset when this document store is disposed.
    pub storage_rejections: u64,
}

struct Job {
    request: u64,
    input: Work,
    cancellation: Cancellation,
}

enum Work {
    Prepared(Input),
    Image(image::ImageInput),
    Video(video::VideoInput, Option<Arc<storage::Store>>),
}
impl Work {
    fn identity(&self) -> Identity {
        match self {
            Self::Prepared(input) => Identity::Sample(input.key().clone()),
            Self::Image(input) => Identity::Sample(input.key.clone()),
            Self::Video(input, _) => Identity::Video(input.key.clone()),
        }
    }
    fn prepare(self, cancellation: &Cancellation) -> Result<Input> {
        match self {
            Self::Prepared(input) => Ok(input),
            Self::Image(input) => input.decode(cancellation),
            Self::Video(input, _) => input.decode(cancellation),
        }
    }
    fn store(&self) -> Option<Arc<storage::Store>> {
        match self { Self::Video(_, store) => store.clone(), _ => None }
    }
    fn store_root(&self) -> Option<std::path::PathBuf> {
        match self { Self::Video(input, Some(_)) => Some(input.root.clone()), _ => None }
    }
}
struct Flight {
    request: u64,
    asset: u64,
    revision: u64,
    source: Option<DepthSource>,
    target_time_us: Option<u64>,
    phase: &'static str,
    started: Instant,
    cancellation: Cancellation,
}
struct Completion {
    request: u64,
    identity: Identity,
    result: Result<PreparedEstimate>,
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

    pub(crate) fn request_video(&mut self, input: video::VideoInput,
        store: Option<Arc<storage::Store>>) -> Result<u64> {
        self.submit(Work::Video(input, store))
    }

    fn submit(&mut self, input: Work) -> Result<u64> {
        input.identity().validate()?;
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
        self.take_prepared_ready(current)
            .map(|ready| ready.map(|ready| ready.estimate))
    }

    /// Preserve the worker's actual prepared input identity for saved calibration.
    /// The independent live-key comparison is identical to `take_ready`.
    pub fn take_prepared_ready(&mut self, current: &Key) -> Result<Option<PreparedEstimate>> {
        self.take_identity(&Identity::Sample(current.clone()))
    }

    pub(crate) fn take_video_prepared_ready(
        &mut self,
        current: &VideoKey,
    ) -> Result<Option<PreparedEstimate>> {
        self.take_identity(&Identity::Video(current.clone()))
    }

    fn take_identity(&mut self, current: &Identity) -> Result<Option<PreparedEstimate>> {
        current.validate()?;
        let mut state = self.shared.0.lock().map_err(|_| Error::State)?;
        let Some(done) = state.ready.take() else {
            return Ok(None);
        };
        if done.request != state.latest || !done.identity.same_request(current) {
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
                asset: f.asset,
                revision: f.revision,
                source: f.source.clone(),
                target_time_us: f.target_time_us,
                phase: f.phase,
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
            let identity = job.input.identity();
            let (asset, revision) = identity.asset_revision();
            state.flight = Some(Flight {
                request: job.request,
                asset,
                revision,
                source: identity.source(),
                target_time_us: identity.target_time_us(),
                phase: "source_prepare",
                started: Instant::now(),
                cancellation: job.cancellation.clone(),
            });
            job
        };
        let began = Instant::now();
        let identity = job.input.identity();
        let (asset, revision) = identity.asset_revision();
        identity.log_start(job.request);
        let mut stage = "source_prepare";
        let mut source = identity.source();
        let store = job.input.store();
        let store_root = job.input.store_root();
        let mut restored = false;
        let result = catch_unwind(AssertUnwindSafe(|| {
            job.cancellation.check()?;
            let prepare_began = Instant::now();
            let input = job.input.prepare(&job.cancellation)?;
            let prepared_input = input.fingerprint(&job.cancellation)?;
            let source_prepare = prepare_began.elapsed();
            job.cancellation.check()?;
            source = Some(input.key().source.clone());
            stage = "backend";
            {
                let mut state = lock_state(&shared)?;
                let flight = state.flight.as_mut().ok_or(Error::State)?;
                flight.source = Some(input.key().source.clone());
                flight.phase = "backend";
            }
            if identity.target_time_us().is_some() {
                log::info!("request={} operation=depth_infer asset={} revision={} phase=source_ready pts={}/{} source_prepare_ms={}",
                    job.request,asset,revision,input.key().source.pts.ticks,input.key().source.pts.timescale,source_prepare.as_millis());
            }
            let cached = match (&store, &identity) {
                (Some(store), Identity::Video(key)) => store.lookup(key)?,
                _ => None,
            };
            let mut estimate = if let Some(sample) = cached {
                stage = "archive_read";
                set_phase(&shared, stage)?;
                log::info!("start #{} depth_restore object={} asset={} pts={:?} phase=archive_read source_request={}",
                    job.request, match &identity { Identity::Video(key) => key.object, _ => 0 },
                    asset, input.key().source.pts, sample.request);
                log::logger().flush();
                let mut estimate = sample.restore(&input, prepared_input, &job.cancellation)?;
                estimate.costs = Costs::default();
                restored = true;
                estimate
            } else { backend.infer(&input, &job.cancellation)? };
            job.cancellation.check()?;
            estimate.validate(input.key())?;
            estimate.costs.source_prepare = source_prepare;
            estimate.costs.source_rgb_bytes = input.rgb().len() as u64;
            estimate.costs.total = began.elapsed();
            if !restored {
                if let (Some(store), Identity::Video(key)) = (&store, &identity) {
                    stage = "archive_write";
                    set_phase(&shared, stage)?;
                    log::info!("start #{} depth_spill object={} asset={} pts={:?} phase=archive_write",
                        job.request, key.object, asset, input.key().source.pts);
                    log::logger().flush();
                    // The root is captured with the job; no Session or GPU
                    // state is accessed on this worker.
                    store.append(store_root.as_deref().ok_or(Error::State)?, job.request,
                        key.object, &input, prepared_input, &estimate, &job.cancellation)?;
                }
            }
            estimate.costs.total = began.elapsed();
            Ok(PreparedEstimate { estimate, input: prepared_input, image: input.image, video_end_us: input.video_end_us })
        }))
        .unwrap_or_else(|p| Err(Error::Panic(panic_cause(p.as_ref()))))
        .map_err(|error| match error {
            Error::Backend(cause) => Error::Backend(cause.chars().take(2048).collect()),
            other => other,
        });
        if let Err(error) = &result {
            if !matches!(error, Error::Canceled) {
                log::error!("request={} operation=depth_infer asset={} revision={} phase=prepare_or_backend stage={} target_us={:?} pts={:?} elapsed_ms={} cause={}",
                    job.request, asset, revision, stage, identity.target_time_us(), source.as_ref().map(|s| &s.pts), began.elapsed().as_millis(),
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
            Ok(_) => {
                state.counts.completed = state.counts.completed.saturating_add(1);
                if restored { state.counts.restored = state.counts.restored.saturating_add(1); }
            },
            Err(Error::Canceled) => state.counts.canceled = state.counts.canceled.saturating_add(1),
            Err(_) => state.counts.failed = state.counts.failed.saturating_add(1),
        }
        // Both checks matter: cancel() need not allocate another ticket.
        if state.latest == job.request && !job.cancellation.is_canceled() && !state.stopped {
            state.ready = Some(Completion {
                request: job.request,
                identity,
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

fn set_phase(shared: &Shared, phase: &'static str) -> Result<()> {
    lock_state(shared)?.flight.as_mut().ok_or(Error::State)?.phase = phase;
    Ok(())
}
