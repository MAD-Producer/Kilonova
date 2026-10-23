use crate::progress::checked_progress;
use crate::{cache_path, contained_dir, DecodeAudio, Result};
use motion_model::{AudioAsset, AudioClip, Command, Content, EditSink, Layer, LayerTimeline};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

static NONCE: AtomicU64 = AtomicU64::new(1);
static WORKERS: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub source_bytes: u64,
    pub cache_bytes: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            source_bytes: motion_model::storage::MAX_MEDIA_ASSET,
            cache_bytes: 48_000 * 2 * 4 * 3600,
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportOptions {
    pub request_id: String,
    #[serde(default)]
    pub at_frame: u32,
    #[serde(default)]
    pub track: Option<u32>,
    #[serde(default = "default_name")]
    pub name: String,
}
fn default_name() -> String {
    "音频".into()
}

#[derive(Clone, Debug, Serialize)]
pub struct TaskStatus {
    pub request_id: String,
    pub operation: String,
    pub state: String,
    pub phase: String,
    /// Phase-local fraction; None means the total work is not known.
    pub progress: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<AudioAsset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edit_result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
impl TaskStatus {
    fn terminal(&self) -> bool {
        matches!(self.state.as_str(), "succeeded" | "failed" | "cancelled")
    }
}
struct Stage {
    path: PathBuf,
}
impl Stage {
    fn new(root: &Path) -> Result<Self> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(crate::Error::from)?
            .as_nanos();
        let n = NONCE.fetch_add(1, Ordering::Relaxed);
        let path = root.join(format!(".media-import-{stamp}-{n}"));
        fs::create_dir(&path).map_err(crate::Error::from)?;
        Ok(Self { path })
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        crate::publication::cleanup_stage(&self.path);
    }
}
struct Prepared {
    stage: Stage,
    asset: AudioAsset,
    options: ImportOptions,
}
struct Task {
    status: TaskStatus,
    prepared: Option<Prepared>,
    cancel: Arc<AtomicBool>,
}
pub struct AudioJobs {
    root: PathBuf,
    limits: Limits,
    decode: DecodeAudio,
    tasks: Mutex<HashMap<String, Arc<Mutex<Task>>>>,
    workers: crate::workers::Workers,
}
struct Worker;
impl Drop for Worker {
    fn drop(&mut self) {
        WORKERS.fetch_sub(1, Ordering::AcqRel);
    }
}
fn finish_import(
    task: &Mutex<Task>,
    result: std::thread::Result<Result<Prepared>>,
    probe: bool,
    worker_log: &crate::task_logging::TaskLog,
) {
    enum Completion {
        Probe(AudioAsset),
        Import(Prepared),
    }
    // Probe results retain only metadata. Delete their staging tree before
    // acquiring the task mutex or publishing terminal success.
    let result = result.map(|result| {
        result.map(|prepared| {
            if probe {
                let metadata = prepared.asset.clone();
                drop(prepared);
                Completion::Probe(metadata)
            } else {
                Completion::Import(prepared)
            }
        })
    });
    static RECOVERY: crate::poison_observation::RecoverySite =
        crate::poison_observation::RecoverySite::new("audio.import.result_lock");
    let mut recoveries = crate::poison_observation::RecoveryScope::new();
    let mut t = recoveries.recover(
        task.lock(),
        &RECOVERY,
        worker_log.request_id(),
        if probe { "probe_audio" } else { "import_audio" },
    );
    if t.cancel.load(Ordering::Acquire) {
        drop(t);
        let error = crate::task_logging::cancelled_error(&result, &["audio task cancelled"]);
        worker_log.finish("cancelled", error.as_deref());
        return;
    }
    match result {
        Ok(Ok(Completion::Probe(metadata))) => {
            t.status.metadata = Some(metadata);
            t.status.progress = Some(1.0);
            t.status.state = "succeeded".into();
            t.status.phase = "complete".into();
        }
        Ok(Ok(Completion::Import(prepared))) => {
            t.status.metadata = Some(prepared.asset.clone());
            t.status.progress = Some(1.0);
            t.prepared = Some(prepared);
            t.status.state = "ready".into();
            t.status.phase = "awaiting_commit".into();
        }
        Ok(Err(error)) => {
            t.status.state = "failed".into();
            t.status.phase = "failed".into();
            t.status.error = Some(error.to_string());
        }
        Err(payload) => {
            t.status.state = "failed".into();
            t.status.phase = "failed".into();
            t.status.error = Some(format!(
                "audio worker failed: {}",
                crate::panic_reason(payload)
            ));
        }
    }
    let phase = t.status.phase.clone();
    let error = t.status.error.clone();
    drop(t);
    worker_log.finish(&phase, error.as_deref());
}

fn finish_cache(
    task: &Mutex<Task>,
    root: &Path,
    asset: AudioAsset,
    result: std::thread::Result<Result<Stage>>,
    worker_log: &crate::task_logging::TaskLog,
) {
    let mut publication = crate::publication::PublicationCleanup::default();
    static RECOVERY: crate::poison_observation::RecoverySite =
        crate::poison_observation::RecoverySite::new("audio.cache.result_lock");
    let mut recoveries = crate::poison_observation::RecoveryScope::new();
    let mut t = recoveries.recover(
        task.lock(),
        &RECOVERY,
        worker_log.request_id(),
        "prepare_audio",
    );
    if t.cancel.load(Ordering::Acquire) {
        drop(t);
        let error = crate::task_logging::cancelled_error(&result, &["audio task cancelled"]);
        worker_log.finish("cancelled", error.as_deref());
        return;
    }
    // The caught result owns Stage before the task guard. Borrow it so neither
    // ordinary completion nor unwinding destroys the stage while locked.
    let publication_result = match &result {
        Ok(Ok(stage)) => (|| {
            let dest = cache_path(root, &asset)?;
            contained_dir(root, dest.parent().unwrap())?;
            publication.publish_cache_pair(
                (
                    &stage.path.join("decoded.wave"),
                    &dest.with_extension("wave"),
                ),
                (&stage.path.join("decoded.pcm"), &dest),
            )
        })(),
        Ok(Err(error)) => Err(error.clone()),
        Err(payload) => Err(crate::Error::Panic {
            operation: "audio cache worker",
            reason: crate::panic_reason_ref(payload.as_ref()),
        }),
    };
    match publication_result {
        Ok(()) => {
            t.status.state = "succeeded".into();
            t.status.phase = "complete".into();
            t.status.progress = Some(1.0);
            t.status.metadata = Some(asset);
        }
        Err(e) => {
            t.status.state = "failed".into();
            t.status.phase = "failed".into();
            t.status.error = Some(e.to_string());
        }
    }
    let phase = t.status.phase.clone();
    let error = t.status.error.clone();
    drop(t);
    worker_log.finish(&phase, error.as_deref());
}

// Preserve the spawn failure even when terminal-state publication finds poison.
fn mark_spawn_failed(
    task: &Mutex<Task>,
    task_log: crate::task_logging::TaskLog,
    error: std::io::Error,
) -> crate::Error {
    use crate::poison_observation::{RecoveryScope, RecoverySite};
    static SPAWN: RecoverySite = RecoverySite::new("audio.start.spawn_failure_task");
    let mut recoveries = RecoveryScope::new();
    let reason = error.to_string();
    let mut task = recoveries.recover(
        task.lock(),
        &SPAWN,
        task_log.request_id(),
        "spawn_audio_worker",
    );
    task.status.state = "failed".into();
    task.status.phase = "failed".into();
    task.status.error = Some(reason.clone());
    drop(task);
    task_log.finish("spawn", Some(&reason));
    error.into()
}

impl AudioJobs {
    pub fn new(root: PathBuf, limits: Limits) -> Result<Self> {
        Self::with_decoder(root, limits, Arc::new(crate::decode_audio))
    }
    /// Platform adapters use the same transactions, cancellation and cache rebuild path.
    pub fn with_decoder(root: PathBuf, limits: Limits, decode: DecodeAudio) -> Result<Self> {
        if limits.source_bytes == 0
            || limits.source_bytes > motion_model::storage::MAX_MEDIA_ASSET
            || limits.cache_bytes == 0
        {
            return Err("invalid audio resource limits".into());
        }
        Ok(Self {
            root: root.canonicalize().map_err(crate::Error::from)?,
            limits,
            decode,
            tasks: Mutex::new(HashMap::new()),
            workers: Default::default(),
        })
    }
    fn task(&self, id: &str) -> Result<Arc<Mutex<Task>>> {
        self.tasks
            .lock()
            .map_err(|_| "audio task registry poisoned")?
            .get(id)
            .cloned()
            .ok_or_else(|| "audio task does not exist".into())
    }
    pub fn contains(&self, id: &str) -> bool {
        static OBSERVATION: crate::registry_observation::RegistryObservation =
            crate::registry_observation::RegistryObservation::new();
        OBSERVATION.contains(&self.tasks, id, "audio.registry.contains")
    }
    fn reserve(&self, id: &str, operation: &str) -> Result<(Arc<Mutex<Task>>, Worker, TaskStatus)> {
        if id.is_empty() || id.len() > 128 {
            return Err("invalid media request id".into());
        }
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| "audio task registry poisoned")?;
        if tasks.contains_key(id) {
            return Err("media request id already exists".into());
        }
        if tasks.len() >= 64 {
            return Err("release completed media tasks before creating more".into());
        }
        let active = tasks.values().try_fold(0, |active, task| -> Result<usize> {
            let task = task.lock().map_err(|_| "audio task poisoned")?;
            Ok(active + usize::from(!task.status.terminal()))
        })?;
        if active >= 2 {
            return Err("at most two audio tasks may run per session".into());
        }
        if WORKERS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 4).then_some(n + 1)
            })
            .is_err()
        {
            return Err("audio worker budget exhausted".into());
        }
        // Return the queued snapshot before publishing the task mutex.
        let initial = TaskStatus {
            request_id: id.into(),
            operation: operation.into(),
            state: "running".into(),
            phase: "queued".into(),
            progress: None,
            metadata: None,
            edit_result: None,
            error: None,
        };
        let task = Arc::new(Mutex::new(Task {
            status: initial.clone(),
            prepared: None,
            cancel: Arc::new(AtomicBool::new(false)),
        }));
        tasks.insert(id.into(), task.clone());
        Ok((task, Worker, initial))
    }
    pub fn status(&self, id: &str) -> Result<TaskStatus> {
        Ok(self
            .task(id)?
            .lock()
            .map_err(|_| "audio task poisoned")?
            .status
            .clone())
    }
    pub fn release(&self, id: &str) -> Result<()> {
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| "audio task registry poisoned")?;
        if !tasks
            .get(id)
            .ok_or("audio task does not exist")?
            .lock()
            .map_err(|_| "audio task poisoned")?
            .status
            .terminal()
        {
            return Err("cancel or finish the media task before release".into());
        }
        tasks.remove(id);
        Ok(())
    }
    pub fn cancel(&self, id: &str) -> Result<TaskStatus> {
        let task = self.task(id)?;
        let mut prepared = None;
        let mut t = task.lock().map_err(|_| "audio task poisoned")?;
        if !t.status.terminal() {
            t.cancel.store(true, Ordering::Release);
            prepared = t.prepared.take();
            t.status.state = "cancelled".into();
            t.status.phase = "cancelled".into();
        }
        let status = t.status.clone();
        drop(t);
        drop(prepared);
        Ok(status)
    }
    pub fn start(
        &self,
        reader: Box<dyn Read + Send>,
        expected_bytes: Option<u64>,
        options: ImportOptions,
        probe: bool,
    ) -> Result<TaskStatus> {
        if options.name.len() > 1024 || expected_bytes.is_some_and(|n| n > self.limits.source_bytes)
        {
            return Err("audio source/name exceeds import limit".into());
        }
        self.start_with_source(move || Ok((reader, expected_bytes)), options, probe)
    }
    /// The provider open itself can block. Run it on the media worker too,
    /// keeping the editor command queue free while a remote URI is opened.
    pub fn start_with_source(
        &self,
        open: impl FnOnce() -> Result<(Box<dyn Read + Send>, Option<u64>)> + Send + 'static,
        options: ImportOptions,
        probe: bool,
    ) -> Result<TaskStatus> {
        if options.name.len() > 1024 {
            return Err("audio name exceeds import limit".into());
        }
        self.workers.reap()?;
        let (task, worker, initial) = self.reserve(
            &options.request_id,
            if probe { "probe_audio" } else { "import_audio" },
        )?;
        let root = self.root.clone();
        let limits = self.limits;
        let decode = self.decode.clone();
        let task_log = crate::task_logging::TaskLog::new(&initial.request_id, &initial.operation);
        let worker_log = task_log.clone();
        let spawn_task = task.clone();
        let spawn = self.workers.spawn("motion-audio-import", task_log.clone(), move || {
                let _worker = worker;
                worker_log.start();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                    || -> Result<Prepared> {
                        update(&spawn_task, "opening", None)?;
                        let (mut reader, expected_bytes) = open()?;
                        if expected_bytes.is_some_and(|n| n > limits.source_bytes) {
                            return Err("audio source exceeds import size limit".into());
                        }
                        if fs2::available_space(&root).map_err(crate::Error::from)?
                            < expected_bytes.unwrap_or(0).saturating_add(1024 * 1024)
                        {
                            return Err("insufficient free space for audio source".into());
                        }
                        let stage = Stage::new(&root)?;
                        let source = stage.path.join("source");
                        let mut out = File::create(&source).map_err(crate::Error::from)?;
                        let mut buffer = vec![0; motion_model::storage::TRANSFER_BUFFER_BYTES];
                        let mut bytes = 0u64;
                        loop {
                            update(
                                &spawn_task,
                                "copying",
                                expected_bytes.filter(|n| *n > 0).map(|n| bytes as f64 / n as f64),
                            )?;
                            let n = reader.read(&mut buffer).map_err(crate::Error::from)?;
                            if n == 0 {
                                break;
                            }
                            bytes += n as u64;
                            if bytes > limits.source_bytes {
                                return Err("audio source exceeds import size limit".into());
                            }
                            if bytes % (1024 * 1024) < n as u64
                                && fs2::available_space(&root).map_err(crate::Error::from)?
                                    < 1024 * 1024
                            {
                                return Err("insufficient free space for audio import".into());
                            }
                            out.write_all(&buffer[..n]).map_err(crate::Error::from)?;
                        }
                        if bytes == 0 || expected_bytes.is_some_and(|n| n != bytes) {
                            return Err("audio source length mismatch".into());
                        }
                        out.sync_all().map_err(crate::Error::from)?;
                        drop(out);
                        drop(reader);
                        let pcm = stage.path.join("decoded.pcm");
                        let mut last_space = Instant::now();
                        let asset = decode(
                            &source,
                            &pcm,
                            options.track,
                            limits.cache_bytes,
                            &mut |progress| {
                                update(&spawn_task, "decoding", progress)?;
                                if last_space.elapsed() >= Duration::from_millis(100) {
                                    last_space = Instant::now();
                                    if fs2::available_space(&root).map_err(crate::Error::from)?
                                        < 1024 * 1024
                                    {
                                        return Err(
                                            "insufficient free space for decoded audio".into()
                                        );
                                    }
                                }
                                Ok(())
                            },
                        )?;
                        if fs::metadata(&pcm).map_err(crate::Error::from)?.len() > limits.cache_bytes
                        {
                            return Err("decoded audio cache exceeds limit".into());
                        }
                        crate::mixer::build_waveform(&pcm, &asset, || {
                            update(&spawn_task, "waveform", None)
                        })?;
                        Ok(Prepared {
                            stage,
                            asset,
                            options,
                        })
                    },
                ));
                finish_import(&spawn_task, result, probe, &worker_log);
            });
        if let Err(error) = spawn {
            return Err(mark_spawn_failed(&task, task_log, error));
        }
        Ok(initial)
    }
    /// Called on the editor's owning worker. Cancel and commit serialize on the
    /// task lock, so a completed cancellation cannot subsequently publish a layer.
    pub fn commit(&self, id: &str, engine: &mut impl EditSink) -> Result<TaskStatus> {
        let request: String = id.chars().take(128).collect();
        let observation = crate::task_logging::TaskLog::new(&request, "commit_audio");
        observation.start();
        let result = self.commit_inner(id, engine);
        match &result {
            Ok((status, attempted)) => observation.finish(
                &status.phase,
                if *attempted { status.error.as_deref() } else { None },
            ),
            Err(error) => observation.finish("commit", Some(&error.to_string())),
        }
        result.map(|(status, _)| status)
    }

    fn commit_inner(&self, id: &str, engine: &mut impl EditSink) -> Result<(TaskStatus, bool)> {
        let task = self.task(id)?;
        // Declare owned resources before the guard so unwind releases it first.
        let mut prepared;
        let mut publication = crate::publication::PublicationCleanup::default();
        let mut t = task.lock().map_err(|_| "audio task poisoned")?;
        if t.status.terminal() {
            return Ok((t.status.clone(), false));
        }
        if t.status.state != "ready" {
            return Err("audio import is not ready".into());
        }
        prepared = t.prepared.take().ok_or("prepared audio missing")?;
        let result = (|| -> Result<Value> {
            let p = engine.project();
            if prepared.options.at_frame >= p.frames {
                return Err("audio insertion outside composition".into());
            }
            let asset_id = p
                .assets
                .iter()
                .map(|a| a.id)
                .chain(p.audio_assets.iter().map(|a| a.id))
                .chain(p.video_assets.iter().map(|a| a.id))
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("asset ID space exhausted")?;
            let object = p
                .layers
                .iter()
                .map(|l| l.id)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("object ID space exhausted")?;
            let source_frames = (prepared.asset.sample_frames * u64::from(p.fps))
                .div_ceil(u64::from(prepared.asset.sample_rate));
            let in_frame = prepared.options.at_frame;
            let out_frame = (u64::from(in_frame) + source_frames).min(u64::from(p.frames)) as u32;
            let mut layer =
                Layer::solid(object, &prepared.options.name, [1.0; 2], [0.0; 3], [1.0; 4]);
            layer.size = [0.0; 2];
            layer.content = Content::Audio {
                audio: AudioClip::new(asset_id),
            };
            layer.timeline = Some(LayerTimeline {
                in_frame: in_frame as i32,
                out_frame: out_frame as i32,
                offset_frame: in_frame as i32,
            });
            let stem = prepared
                .stage
                .path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .trim_start_matches('.');
            let ext = crate::source_extension(&prepared.stage.path.join("source"))?;
            prepared.asset.id = asset_id;
            prepared.asset.path = format!("assets/audio-{stem}.{ext}");
            contained_dir(&self.root, &self.root.join("assets"))?;
            let source = self.root.join(&prepared.asset.path);
            let cache = cache_path(&self.root, &prepared.asset)?;
            contained_dir(&self.root, cache.parent().unwrap())?;
            fs::rename(prepared.stage.path.join("source"), &source).map_err(crate::Error::from)?;
            if let Err(error) = publication.publish_cache_pair(
                (&prepared.stage.path.join("decoded.pcm"), &cache),
                (&prepared.stage.path.join("decoded.wave"), &cache.with_extension("wave")),
            ) {
                return Err(publication.rollback_files(error, &[&source]));
            }
            let commands = vec![
                Command::RegisterAudioAsset {
                    asset: prepared.asset.clone(),
                },
                Command::Add { layer },
            ];
            if let Err(error) = engine.apply_batch_saved(commands, &self.root) {
                let error = publication.rollback_published_cache(error);
                return Err(publication.rollback_files(error, &[&source]));
            }
            Ok(
                json!({"op":"import_media", "kind":"audio", "asset":asset_id,"object":object,"in_frame":in_frame,"out_frame":out_frame,
                "source_offset_us":0,"has_audio":true,"truncated_to_composition": u64::from(out_frame-in_frame) < source_frames}),
            )
        })();
        match result {
            Ok(edit) => {
                t.status.state = "succeeded".into();
                t.status.phase = "complete".into();
                t.status.edit_result = Some(edit);
                t.status.metadata = Some(prepared.asset);
            }
            Err(error) => {
                t.status.state = "failed".into();
                t.status.phase = "failed".into();
                t.status.error = Some(error.to_string());
            }
        }
        Ok((t.status.clone(), true))
    }
    pub fn prepare_cache(&self, request_id: &str, asset: AudioAsset) -> Result<TaskStatus> {
        asset.validate().map_err(crate::Error::from)?;
        self.workers.reap()?;
        let (task, worker, initial) = self.reserve(request_id, "prepare_audio")?;
        let root = self.root.clone();
        let limit = self.limits.cache_bytes;
        let decode = self.decode.clone();
        let task_log = crate::task_logging::TaskLog::new(&initial.request_id, &initial.operation);
        let worker_log = task_log.clone();
        let spawn_task = task.clone();
        let spawn = self.workers.spawn("motion-audio-cache", task_log.clone(), move || {
                let _worker = worker;
                worker_log.start();
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<Stage> {
                        let stage = Stage::new(&root)?;
                        let pcm = stage.path.join("decoded.pcm");
                        let source = root
                            .join(&asset.path)
                            .canonicalize()
                            .map_err(crate::Error::from)?;
                        if !source.starts_with(&root) {
                            return Err("audio source outside project".into());
                        }
                        let mut last_space = Instant::now();
                        let decoded =
                            decode(&source, &pcm, Some(asset.track), limit, &mut |progress| {
                                update(&spawn_task, "decoding", progress)?;
                                if last_space.elapsed() >= Duration::from_millis(100) {
                                    last_space = Instant::now();
                                    if fs2::available_space(&root).map_err(crate::Error::from)?
                                        < 1024 * 1024
                                    {
                                        return Err("audio cache storage budget exceeded".into());
                                    }
                                }
                                Ok(())
                            })?;
                        if (
                            decoded.sample_rate,
                            decoded.channels,
                            decoded.sample_frames,
                            decoded.bytes,
                            &decoded.mime,
                        ) != (
                            asset.sample_rate,
                            asset.channels,
                            asset.sample_frames,
                            asset.bytes,
                            &asset.mime,
                        ) {
                            return Err("audio source no longer matches project metadata".into());
                        }
                        if pcm.metadata().map_err(crate::Error::from)?.len() > limit {
                            return Err("audio cache budget exceeded".into());
                        }
                        crate::mixer::build_waveform(&pcm, &asset, || {
                            update(&spawn_task, "waveform", None)
                        })?;
                        Ok(stage)
                    }));
                finish_cache(&spawn_task, &root, asset, result, &worker_log);
            });
        if let Err(error) = spawn {
            return Err(mark_spawn_failed(&task, task_log, error));
        }
        Ok(initial)
    }
}
impl Drop for AudioJobs {
    fn drop(&mut self) {
        use crate::poison_observation::{RecoveryScope, RecoverySite};
        static REGISTRY: RecoverySite = RecoverySite::new("audio.drop.registry");
        static TASK: RecoverySite = RecoverySite::new("audio.drop.task");
        let mut recoveries = RecoveryScope::new();
        for (id, task) in recoveries
            .recover(self.tasks.get_mut(), &REGISTRY, "session", "drop_audio")
            .iter()
        {
            let mut t = recoveries.recover(task.lock(), &TASK, id, "drop_audio");
            t.cancel.store(true, Ordering::Release);
            let prepared = t.prepared.take();
            drop(t);
            // Stage cleanup can delete a large tree or call the host logger.
            drop(prepared);
        }
        drop(recoveries);
        self.workers.shutdown();
    }
}
fn update(task: &Arc<Mutex<Task>>, phase: &str, progress: impl Into<Option<f64>>) -> Result<()> {
    let mut t = task.lock().map_err(|_| "audio task poisoned")?;
    if t.cancel.load(Ordering::Acquire) {
        return Err("audio task cancelled".into());
    }
    let progress = checked_progress(progress.into())?;
    if t.status.phase != phase {
        t.status.phase = phase.into();
    }
    t.status.progress = progress;
    Ok(())
}

#[cfg(test)]
mod poison_tests;

#[cfg(test)]
mod drop_failure_tests {
    use super::*;

    #[test]
    fn poisoned_audio_registry_membership_is_observed_after_unlock() {
        use crate::registry_observation::tests::{check, child};
        if !child(
            "jobs::drop_failure_tests::poisoned_audio_registry_membership_is_observed_after_unlock",
        ) {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let jobs = Box::leak(Box::new(
            AudioJobs::new(directory.path().into(), Limits::default()).unwrap(),
        ));
        let healthy = AudioJobs::new(directory.path().into(), Limits::default()).unwrap();
        let (_task, _worker, _initial) = jobs.reserve("present", "test").unwrap();
        check(
            &jobs.tasks,
            "audio.registry.contains",
            |id| jobs.contains(id),
            |id| healthy.contains(id),
        );
        assert_eq!(
            jobs.status("present").unwrap_err().to_string(),
            "audio task registry poisoned"
        );
    }

    #[test]
    fn poisoned_registry_drop_still_cancels_a_blocked_source_worker() {
        let directory = tempfile::tempdir().unwrap();
        let jobs = AudioJobs::new(directory.path().into(), Limits::default()).unwrap();
        let (entered, waiting) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        jobs.start_with_source(move || {
            entered.send(()).unwrap();
            released.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok((Box::new(std::io::Cursor::new(Vec::<u8>::new())) as Box<dyn Read + Send>, None))
        }, ImportOptions {request_id:"drop".into(),at_frame:0,track:None,name:"test".into()}, false).unwrap();
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        let task = jobs.tasks.lock().unwrap().get("drop").unwrap().clone();
        let cancel = task.lock().unwrap().cancel.clone();
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = jobs.tasks.lock().unwrap();
            panic!("injected task registry poison");
        }));
        assert!(panic.is_err());
        // Drop now joins the worker. Release its controlled open after seeing
        // the original cancellation signal, rather than after Drop returns.
        let release_cancel = cancel.clone();
        let resume = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !release_cancel.load(Ordering::Acquire) && Instant::now() < deadline {
                std::thread::yield_now();
            }
            release.send(()).unwrap();
        });
        drop(jobs);
        resume.join().unwrap();
        assert!(cancel.load(Ordering::Acquire), "Drop skipped cancellation of a poisoned registry");
    }
}

#[cfg(test)]
pub(crate) mod completion_tests;

#[cfg(test)]
pub(crate) mod resource_tests;
