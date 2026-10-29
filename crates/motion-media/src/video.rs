//! Streaming source transactions; platform probing is supplied by the Android backend.
use crate::progress::checked_progress;
use crate::publication::ImportTarget;
use crate::{contained_dir, Result};
use motion_model::{AudioAsset, Command, Content, EditSink, Layer, LayerTimeline, VideoAsset, VideoClip};
use serde::Serialize;
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
    time::{SystemTime, UNIX_EPOCH},
};

static NONCE: AtomicU64 = AtomicU64::new(1);
static WORKERS: AtomicUsize = AtomicUsize::new(0);
pub struct VideoProbe {
    pub asset: VideoAsset,
    pub timestamps: Vec<u64>,
    pub audio_track: Option<u32>,
    pub first_rgba: Vec<u8>,
    pub tracks: Value,
}
pub type ProbeVideo = Arc<
    dyn Fn(&Path, Option<u32>, Option<u32>, &dyn Fn() -> Result<()>) -> Result<VideoProbe>
        + Send
        + Sync,
>;
#[derive(Clone)]
pub struct VideoImportOptions {
    pub request_id: String,
    pub at_frame: u32,
    pub name: String,
    pub track: Option<u32>,
    pub audio_track: Option<u32>,
    pub with_audio: bool,
}
#[derive(Clone, Serialize)]
pub struct VideoTaskStatus {
    pub request_id: String,
    pub operation: String,
    pub state: String,
    pub phase: String,
    /// Phase-local fraction; None means the total work is not known.
    pub progress: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edit_result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
impl VideoTaskStatus {
    fn terminal(&self) -> bool {
        matches!(self.state.as_str(), "succeeded" | "failed" | "cancelled")
    }
}
struct Stage(PathBuf);
impl Stage {
    fn new(root: &Path) -> Result<Self> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(crate::Error::from)?
            .as_nanos();
        let path = root.join(format!(
            ".video-import-{stamp}-{}",
            NONCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).map_err(crate::Error::from)?;
        Ok(Self(path))
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        crate::publication::cleanup_stage(&self.0);
    }
}
struct Prepared {
    stage: Stage,
    probe: VideoProbe,
    audio: Option<AudioAsset>,
    options: VideoImportOptions,
}
struct Task {
    status: VideoTaskStatus,
    prepared: Option<Prepared>,
    cancel: Arc<AtomicBool>,
}
struct Worker;
impl Drop for Worker {
    fn drop(&mut self) {
        WORKERS.fetch_sub(1, Ordering::AcqRel);
    }
}
pub struct VideoJobs {
    root: PathBuf,
    audio_decode: crate::DecodeAudio,
    tasks: Mutex<HashMap<String, Arc<Mutex<Task>>>>,
    workers: crate::workers::Workers,
}
fn finish_import(
    task: &Mutex<Task>,
    result: std::thread::Result<Result<Prepared>>,
    probe_only: bool,
    worker_log: &crate::task_logging::TaskLog,
) {
    enum Completion {
        Probe(Value),
        Import(Prepared),
    }
    // Probe results retain only metadata. Delete their staging tree before
    // acquiring the task mutex or publishing terminal success.
    let result = result.map(|result| {
        result.map(|prepared| {
            if probe_only {
                let metadata = json!({"video":prepared.probe.asset,"audio":prepared.audio,"audio_tracks":prepared.probe.tracks});
                drop(prepared);
                Completion::Probe(metadata)
            } else {
                Completion::Import(prepared)
            }
        })
    });
    static RECOVERY: crate::poison_observation::RecoverySite =
        crate::poison_observation::RecoverySite::new("video.import.result_lock");
    let mut recoveries = crate::poison_observation::RecoveryScope::new();
    let mut t = recoveries.recover(
        task.lock(),
        &RECOVERY,
        worker_log.request_id(),
        if probe_only {
            "probe_video"
        } else {
            "import_video"
        },
    );
    if t.cancel.load(Ordering::Acquire) {
        drop(t);
        let error = crate::task_logging::cancelled_error(&result, &["video import cancelled"]);
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
            t.status.metadata = Some(
                json!({"video":prepared.probe.asset,"audio":prepared.audio,"audio_tracks":prepared.probe.tracks}),
            );
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
                "video worker failed: {}",
                crate::panic_reason(payload)
            ));
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
    static SPAWN: RecoverySite = RecoverySite::new("video.start.spawn_failure_task");
    let mut recoveries = RecoveryScope::new();
    let reason = error.to_string();
    let mut task = recoveries.recover(
        task.lock(),
        &SPAWN,
        task_log.request_id(),
        "spawn_video_worker",
    );
    task.status.state = "failed".into();
    task.status.phase = "failed".into();
    task.status.error = Some(reason.clone());
    drop(task);
    task_log.finish("spawn", Some(&reason));
    error.into()
}

impl VideoJobs {
    pub fn new(root: PathBuf) -> Result<Self> {
        Self::with_audio_decoder(root, Arc::new(crate::decode_audio))
    }
    pub fn with_audio_decoder(root: PathBuf, audio_decode: crate::DecodeAudio) -> Result<Self> {
        Ok(Self {
            root: root.canonicalize().map_err(crate::Error::from)?,
            audio_decode,
            tasks: Mutex::new(HashMap::new()),
            workers: Default::default(),
        })
    }
    pub fn contains(&self, id: &str) -> bool {
        static OBSERVATION: crate::registry_observation::RegistryObservation =
            crate::registry_observation::RegistryObservation::new();
        OBSERVATION.contains(&self.tasks, id, "video.registry.contains")
    }
    fn task(&self, id: &str) -> Result<Arc<Mutex<Task>>> {
        self.tasks
            .lock()
            .map_err(|_| "video task registry poisoned")?
            .get(id)
            .cloned()
            .ok_or("video task missing".into())
    }
    fn reserve(
        &self,
        id: &str,
        operation: &str,
    ) -> Result<(Arc<Mutex<Task>>, Worker, VideoTaskStatus)> {
        if id.is_empty() || id.len() > 128 {
            return Err("invalid media request id".into());
        }
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| "video task registry poisoned")?;
        if tasks.contains_key(id) || tasks.len() >= 64 {
            return Err("duplicate request id or video task budget exceeded".into());
        }
        let active = tasks.values().try_fold(0, |active, task| -> Result<usize> {
            let task = task.lock().map_err(|_| "video task poisoned")?;
            Ok(active + usize::from(!task.status.terminal()))
        })?;
        if active >= 2 {
            return Err("at most two unfinished video tasks per session".into());
        }
        WORKERS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 2).then_some(n + 1)
            })
            .map_err(|_| "video import worker budget exhausted")?;
        // Return the queued snapshot before publishing the task mutex.
        let initial = VideoTaskStatus {
            request_id: id.into(),
            operation: operation.into(),
            state: "running".into(),
            phase: "queued".into(),
            progress: None,
            metadata: None,
            edit_result: None,
            error: None,
        };
        let t = Arc::new(Mutex::new(Task {
            status: initial.clone(),
            prepared: None,
            cancel: Arc::new(AtomicBool::new(false)),
        }));
        tasks.insert(id.into(), t.clone());
        Ok((t, Worker, initial))
    }
    pub fn status(&self, id: &str) -> Result<VideoTaskStatus> {
        Ok(self
            .task(id)?
            .lock()
            .map_err(|_| "video task poisoned")?
            .status
            .clone())
    }
    pub fn cancel(&self, id: &str) -> Result<VideoTaskStatus> {
        let task = self.task(id)?;
        let mut prepared = None;
        let mut t = task.lock().map_err(|_| "video task poisoned")?;
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
    pub fn release(&self, id: &str) -> Result<()> {
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| "video task registry poisoned")?;
        if !tasks
            .get(id)
            .ok_or("video task missing")?
            .lock()
            .map_err(|_| "video task poisoned")?
            .status
            .terminal()
        {
            return Err("finish or cancel video task before release".into());
        }
        tasks.remove(id);
        Ok(())
    }
    pub fn start(
        &self,
        open: impl FnOnce() -> Result<(Box<dyn Read + Send>, Option<u64>)> + Send + 'static,
        options: VideoImportOptions,
        probe_only: bool,
        probe: ProbeVideo,
    ) -> Result<VideoTaskStatus> {
        if options.name.len() > 1024 {
            return Err("video name exceeds limit".into());
        }
        self.workers.reap()?;
        let (task, worker, initial) = self.reserve(
            &options.request_id,
            if probe_only {
                "probe_video"
            } else {
                "import_video"
            },
        )?;
        let root = self.root.clone();
        let audio_decode = self.audio_decode.clone();
        let task_log = crate::task_logging::TaskLog::new(&initial.request_id, &initial.operation);
        let worker_log = task_log.clone();
        let t = task.clone();
        let spawn=self.workers.spawn("motion-video-import",task_log.clone(),move||{
            let _worker=worker;
            worker_log.start();
            let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||->Result<Prepared>{
                update(&t,"opening",None)?;let(mut reader,size)=open()?;
                if size.is_some_and(|s|s>motion_model::storage::MAX_MEDIA_ASSET){return Err("video exceeds 2 GiB".into());}
                if fs2::available_space(&root).map_err(crate::Error::from)?<size.unwrap_or(0).saturating_add(1024*1024){return Err("insufficient video source storage".into());}
                let stage=Stage::new(&root)?;let source=stage.0.join("source.mp4");let mut file=File::create(&source).map_err(crate::Error::from)?;
                let mut buf=[0u8;motion_model::storage::TRANSFER_BUFFER_BYTES];let mut bytes=0u64;
                loop {update(&t,"copying",size.filter(|s|*s>0).map(|s|bytes as f64/s as f64))?;
                    let n=reader.read(&mut buf).map_err(crate::Error::from)?;if n==0{break;}bytes+=n as u64;
                    if bytes>motion_model::storage::MAX_MEDIA_ASSET{return Err("video exceeds 2 GiB".into());}
                    if bytes%(1024*1024)<n as u64&&fs2::available_space(&root).map_err(crate::Error::from)?<1024*1024{return Err("insufficient video import storage".into());}
                    file.write_all(&buf[..n]).map_err(crate::Error::from)?;
                }
                if bytes==0||size.is_some_and(|s|s!=bytes){return Err("video source length mismatch".into());}
                file.sync_all().map_err(crate::Error::from)?;drop(file);drop(reader);
                update(&t,"probing",None)?;let mut p=probe(&source,options.track,if options.with_audio {options.audio_track}else{Some(u32::MAX)},&||update(&t,"probing",None))?;
                let audio=if options.with_audio {if let Some(track)=p.audio_track {
                    update(&t,"decoding_audio",None)?;
                    let pcm=stage.0.join("decoded.pcm");let a=audio_decode(&source,&pcm,Some(track),crate::Limits::default().cache_bytes,&mut |v|update(&t,"decoding_audio",v))?;
                    crate::mixer::build_waveform(&pcm,&a,||update(&t,"waveform",None))?;Some(a)
                }else{None}}else{None};
                if let Some(a)=&audio{p.asset.duration_us=p.asset.duration_us.max(a.duration_us);}
                p.asset.validate().map_err(crate::Error::from)?;
                save_index(&stage.0.join("index.pts"),&p.asset,&p.timestamps)?;
                image::save_buffer(stage.0.join("thumbnail.png"),&p.first_rgba,p.asset.display_width,p.asset.display_height,image::ColorType::Rgba8).map_err(crate::Error::from)?;
                Ok(Prepared{stage,probe:p,audio,options})
            }));
            finish_import(&t, result, probe_only, &worker_log);
        });
        if let Err(e) = spawn {
            return Err(mark_spawn_failed(&task, task_log, e));
        }
        Ok(initial)
    }
    pub fn commit(&self, id: &str, engine: &mut impl EditSink) -> Result<VideoTaskStatus> {
        self.commit_target(id, engine, ImportTarget::Layer)
    }
    /// Keep all existing clips and publish a validated replacement as one edit.
    pub fn replace(&self, id: &str, engine: &mut impl EditSink, asset: u64) -> Result<VideoTaskStatus> {
        self.commit_target(id, engine, ImportTarget::Replace(asset))
    }
    /// Register video and linked audio together without creating a timeline clip.
    pub fn register(&self, id: &str, engine: &mut impl EditSink, folder: Option<u64>) -> Result<VideoTaskStatus> {
        self.commit_target(id, engine, ImportTarget::Library(folder))
    }
    fn commit_target(&self, id: &str, engine: &mut impl EditSink, target: ImportTarget) -> Result<VideoTaskStatus> {
        let request: String = id.chars().take(128).collect();
        let observation = crate::task_logging::TaskLog::new(&request, "commit_video");
        observation.start();
        let result = self.commit_inner(id, engine, target);
        match &result {
            Ok((status, attempted)) => observation.finish(
                &status.phase,
                if *attempted { status.error.as_deref() } else { None },
            ),
            Err(error) => observation.finish("commit", Some(&error.to_string())),
        }
        result.map(|(status, _)| status)
    }

    fn commit_inner(&self, id: &str, engine: &mut impl EditSink, destination: ImportTarget) -> Result<(VideoTaskStatus, bool)> {
        let target = destination.replacement();
        let task = self.task(id)?;
        // Declare owned resources before the guard so unwind releases it first.
        let mut p;
        let mut publication = crate::publication::PublicationCleanup::default();
        let mut audio_publication = crate::publication::PublicationCleanup::default();
        let mut t = task.lock().map_err(|_| "video task poisoned")?;
        if t.status.terminal() {
            return Ok((t.status.clone(), false));
        }
        if t.status.state != "ready" {
            return Err("video import is not ready".into());
        }
        p = t.prepared.take().ok_or("prepared video missing")?;
        let mut published = Vec::new();
        let result = (|| -> Result<Value> {
            let project = engine.project();
            let at = p.options.at_frame;
            if at >= project.frames {
                return Err("video insertion outside composition".into());
            }
            let next_asset_id = project
                .assets
                .iter()
                .map(|a| a.id)
                .chain(project.audio_assets.iter().map(|a| a.id))
                .chain(project.video_assets.iter().map(|a| a.id))
                .chain(project.model_assets.iter().map(|a| a.id))
                .max()
                .unwrap_or(0)
                .checked_add(1);
            let aid = target.or(next_asset_id).ok_or("asset ID exhausted")?;
            let old_audio = target.and_then(|id| project.video_assets.iter().find(|a| a.id == id)).and_then(|a| a.audio_asset);
            if let Some(id) = target {
                if !project.video_assets.iter().any(|a| a.id == id) {
                    return Err("replacement video asset missing".into());
                }
            }
            let object = if !destination.creates_layer() { 0 } else { project
                .layers
                .iter()
                .map(|l| l.id)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("object ID exhausted")? };
            // The probe records the visible interval on the source presentation
            // timeline, and every decoder entry point rejects samples outside
            // [video_start_us, video_end_us). Layer-local time zero must map to the
            // first visible frame, and the clip may only cover that visible span:
            // duration_us can be longer because audio padding outlasts the picture.
            let offset_us = p.probe.asset.video_start_us;
            let visible_us = p.probe.asset.video_end_us - p.probe.asset.video_start_us;
            let frames = (visible_us * u64::from(project.fps)).div_ceil(1_000_000);
            let out = (u64::from(at) + frames).min(u64::from(project.frames)) as u32;
            let stem = p
                .stage
                .0
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .trim_start_matches('.');
            let ext = crate::source_extension(&p.stage.0.join("source.mp4"))?;
            let path = format!("assets/{stem}.{ext}");
            p.probe.asset.id = aid;
            p.probe.asset.path = path.clone();
            let mut commands = Vec::new();
            if let Some(a) = &mut p.audio {
                a.id = if target.is_some() { old_audio.or(next_asset_id).ok_or("asset ID exhausted")? } else { aid.checked_add(1).ok_or("asset ID exhausted")? };
                a.path = path.clone();
                p.probe.asset.audio_asset = Some(a.id);
                if target.is_none() { commands.push(Command::RegisterAudioAsset { asset: a.clone() }); }
            }
            let mut layer = Layer::solid(
                object,
                &p.options.name,
                [
                    p.probe.asset.display_width as f32,
                    p.probe.asset.display_height as f32,
                ],
                [project.width as f32 / 2.0, project.height as f32 / 2.0, 0.0],
                [1.0; 4],
            );
            let mut video = VideoClip::new(aid);
            video.source_offset_us = offset_us;
            layer.content = Content::Video { video };
            layer.rotation_order = motion_model::RotationOrder::FixedXyz;
            layer.timeline = Some(LayerTimeline {
                in_frame: at as i32,
                out_frame: out as i32,
                offset_frame: at as i32,
            });
            if target.is_some() {
                commands.push(Command::MediaAsset { action: motion_model::media_assets::AssetAction::ReplaceVideo {
                    video: p.probe.asset.clone(), audio: p.audio.clone(),
                } });
            } else {
                commands.push(Command::RegisterVideoAsset { asset: p.probe.asset.clone() });
                match destination {
                    ImportTarget::Layer => commands.push(Command::Add { layer }),
                    ImportTarget::Library(folder) => {
                        let mut items = vec![motion_model::project_library::ProjectItem::Video { id: aid }];
                        if let Some(audio) = &p.audio { items.push(motion_model::project_library::ProjectItem::Audio { id: audio.id }); }
                        commands.push(Command::ProjectLibrary {
                            action: motion_model::project_library::LibraryAction::MoveItems { items, folder },
                        });
                    }
                    ImportTarget::Replace(_) => unreachable!("replacement handled above"),
                }
            }
            contained_dir(&self.root, &self.root.join("assets"))?;
            let cache = video_cache_path(&self.root, &p.probe.asset)?;
            contained_dir(&self.root, cache.parent().unwrap())?;
            let source = self.root.join(&path);
            fs::rename(p.stage.0.join("source.mp4"), &source).map_err(crate::Error::from)?;
            published.push(source);
            publication.publish_cache_pair(
                (&p.stage.0.join("index.pts"), &cache),
                (&p.stage.0.join("thumbnail.png"), &cache.with_extension("png")),
            )?;
            if let Some(a) = &p.audio {
                let pcm = crate::cache_path(&self.root, a)?;
                contained_dir(&self.root, pcm.parent().unwrap())?;
                audio_publication.publish_cache_pair(
                    (&p.stage.0.join("decoded.pcm"), &pcm),
                    (&p.stage.0.join("decoded.wave"), &pcm.with_extension("wave")),
                )?;
            }
            engine
                .apply_batch_saved(commands, &self.root)
                .map_err(crate::Error::from)?;
            Ok(
                json!({"op":destination.operation(),"kind":"video","asset":aid,"object":if destination.creates_layer() {Some(object)} else {None},"in_frame":at,"out_frame":out,"source_offset_us":offset_us,"has_audio":p.audio.is_some(),"audio_asset":p.probe.asset.audio_asset,"truncated_to_composition":u64::from(out-at)<frames}),
            )
        })();
        match result {
            Ok(edit) => {
                t.status.state = "succeeded".into();
                t.status.phase = "complete".into();
                t.status.edit_result = Some(edit);
                t.status.metadata = Some(
                    json!({"video":p.probe.asset,"audio":p.audio,"audio_tracks":p.probe.tracks}),
                );
            }
            Err(e) => {
                let paths = published.iter().map(PathBuf::as_path).collect::<Vec<_>>();
                let e = audio_publication.rollback_published_cache(e);
                let e = publication.rollback_published_cache(e);
                let error = publication.rollback_files(e, &paths);
                t.status.state = "failed".into();
                t.status.phase = "failed".into();
                t.status.error = Some(error.to_string());
            }
        }
        Ok((t.status.clone(), true))
    }
    pub fn prepare_cache(
        &self,
        id: &str,
        expected: VideoAsset,
        probe: ProbeVideo,
    ) -> Result<VideoTaskStatus> {
        let (task, worker, initial) = self.reserve(id, "prepare_video")?;
        let task_log = crate::task_logging::TaskLog::new(&initial.request_id, &initial.operation);
        let worker_log = task_log.clone();
        let t = task.clone();
        let root = self.root.clone();
        let spawn = std::thread::Builder::new()
            .name("motion-video-cache".into())
            .spawn(move || {
                let _worker = worker;
                worker_log.start();
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
                        update(&t, "probing", None)?;
                        let source = root
                            .join(&expected.path)
                            .canonicalize()
                            .map_err(crate::Error::from)?;
                        if !source.starts_with(&root) {
                            return Err("video source outside project".into());
                        }
                        let mut p = probe(&source, Some(expected.track), Some(u32::MAX), &|| {
                            update(&t, "probing", None)
                        })?;
                        p.asset.id = expected.id;
                        p.asset.path = expected.path.clone();
                        p.asset.audio_asset = expected.audio_asset;
                        p.asset.duration_us = expected.duration_us;
                        if p.asset != expected {
                            return Err("owned video metadata changed".into());
                        }
                        let stage = Stage::new(&root)?;
                        save_index(&stage.0.join("index.pts"), &expected, &p.timestamps)?;
                        image::save_buffer(
                            stage.0.join("thumbnail.png"),
                            &p.first_rgba,
                            expected.display_width,
                            expected.display_height,
                            image::ColorType::Rgba8,
                        )
                        .map_err(crate::Error::from)?;
                        let cache = video_cache_path(&root, &expected)?;
                        contained_dir(&root, cache.parent().unwrap())?;
                        let mut publication = crate::publication::PublicationCleanup::default();
                        let mut state = t.lock().map_err(|_| "video task poisoned")?;
                        if state.cancel.load(Ordering::Acquire) {
                            return Err("video cache cancelled".into());
                        }
                        publication.publish_cache_pair(
                            (&stage.0.join("index.pts"), &cache),
                            (&stage.0.join("thumbnail.png"), &cache.with_extension("png")),
                        )?;
                        state.status.state = "succeeded".into();
                        state.status.phase = "complete".into();
                        state.status.progress = Some(1.0);
                        state.status.metadata = Some(json!({"video":expected}));
                        Ok(())
                    }));
                static RECOVERY: crate::poison_observation::RecoverySite =
                    crate::poison_observation::RecoverySite::new("video.cache.result_lock");
                let mut recoveries = crate::poison_observation::RecoveryScope::new();
                let mut state = recoveries.recover(
                    t.lock(), &RECOVERY, worker_log.request_id(), "prepare_video",
                );
                if state.cancel.load(Ordering::Acquire) {
                    drop(state);
                    let error = crate::task_logging::cancelled_error(
                        &result, &["video import cancelled", "video cache cancelled"],
                    );
                    worker_log.finish("cancelled", error.as_deref());
                    return;
                }
                if let Err(error) =
                    result.unwrap_or_else(|payload| Err(crate::Error::Panic {
                        operation: "video cache worker", reason: crate::panic_reason(payload),
                    }))
                {
                    state.status.state = "failed".into();
                    state.status.phase = "failed".into();
                    state.status.error = Some(error.to_string());
                }
                let phase = state.status.phase.clone();
                let error = state.status.error.clone();
                drop(state);
                worker_log.finish(&phase, error.as_deref());
            });
        if let Err(e) = spawn {
            return Err(mark_spawn_failed(&task, task_log, e));
        }
        Ok(initial)
    }
}
impl Drop for VideoJobs {
    fn drop(&mut self) {
        use crate::poison_observation::{RecoveryScope, RecoverySite};
        static REGISTRY: RecoverySite = RecoverySite::new("video.drop.registry");
        static TASK: RecoverySite = RecoverySite::new("video.drop.task");
        let mut recoveries = RecoveryScope::new();
        for (id, task) in recoveries
            .recover(self.tasks.get_mut(), &REGISTRY, "session", "drop_video")
            .iter()
        {
            let mut t = recoveries.recover(task.lock(), &TASK, id, "drop_video");
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
fn update(task: &Mutex<Task>, phase: &str, progress: impl Into<Option<f64>>) -> Result<()> {
    let mut t = task.lock().map_err(|_| "video task poisoned")?;
    if t.cancel.load(Ordering::Acquire) {
        return Err("video import cancelled".into());
    }
    let progress = checked_progress(progress.into())?;
    t.status.phase = phase.into();
    t.status.progress = progress;
    Ok(())
}
pub fn video_cache_path(root: &Path, asset: &VideoAsset) -> Result<PathBuf> {
    motion_model::storage::validate_relative_path(&asset.path).map_err(crate::Error::from)?;
    let stem = Path::new(&asset.path)
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("invalid video source name")?;
    Ok(root.join("cache/video-v2").join(stem).join("index.pts"))
}
pub fn save_index(path: &Path, asset: &VideoAsset, pts: &[u64]) -> Result<()> {
    validate_index(asset, pts)?;
    let mut f = File::create(path).map_err(crate::Error::from)?;
    f.write_all(b"MSVID001")
        .and_then(|_| f.write_all(&asset.bytes.to_le_bytes()))
        .map_err(crate::Error::from)?;
    for t in pts {
        f.write_all(&t.to_le_bytes()).map_err(crate::Error::from)?;
    }
    f.sync_all().map_err(crate::Error::from)
}
pub fn load_video_index(root: &Path, asset: &VideoAsset) -> Result<Vec<u64>> {
    let base = root.canonicalize().map_err(crate::Error::from)?;
    let path = video_cache_path(root, asset)?
        .canonicalize()
        .map_err(|_| "video cache missing; call prepare_video")?;
    if !path.starts_with(base) {
        return Err("video cache outside project".into());
    }
    let mut f = File::open(path).map_err(crate::Error::from)?;
    let size = f.metadata().map_err(crate::Error::from)?.len();
    if size != 16 + u64::from(asset.frame_count) * 8
        || size > 16 + u64::from(motion_model::MAX_VIDEO_FRAMES) * 8
    {
        return Err("video cache invalid; call prepare_video".into());
    }
    let mut bytes = vec![0; size as usize];
    f.read_exact(&mut bytes).map_err(crate::Error::from)?;
    if &bytes[..8] != b"MSVID001"
        || u64::from_le_bytes(bytes[8..16].try_into().unwrap()) != asset.bytes
    {
        return Err("video cache identity mismatch".into());
    }
    let pts = bytes[16..]
        .chunks_exact(8)
        .map(|v| u64::from_le_bytes(v.try_into().unwrap()))
        .collect::<Vec<_>>();
    validate_index(asset, &pts)?;
    Ok(pts)
}
fn validate_index(a: &VideoAsset, pts: &[u64]) -> Result<()> {
    if pts.len() != a.frame_count as usize
        || pts.first() != Some(&a.video_start_us)
        || pts.last().is_none_or(|t| *t >= a.video_end_us)
        || pts.windows(2).any(|v| v[0] >= v[1])
    {
        return Err("invalid video timestamp index".into());
    }
    Ok(())
}

#[cfg(test)]
mod poison_tests;

#[cfg(test)]
mod registry_tests {
    use super::*;

    #[test]
    fn poisoned_video_registry_membership_is_observed_after_unlock() {
        use crate::registry_observation::tests::{check, child};
        if !child(
            "video::registry_tests::poisoned_video_registry_membership_is_observed_after_unlock",
        ) {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let jobs = Box::leak(Box::new(VideoJobs::new(directory.path().into()).unwrap()));
        let healthy = VideoJobs::new(directory.path().into()).unwrap();
        let (_task, _worker, _initial) = jobs.reserve("present", "test").unwrap();
        check(
            &jobs.tasks,
            "video.registry.contains",
            |id| jobs.contains(id),
            |id| healthy.contains(id),
        );
        assert_eq!(
            jobs.status("present").err().map(|error| error.to_string()).as_deref(),
            Some("video task registry poisoned")
        );
    }
}

#[cfg(test)]
mod completion_tests;

#[cfg(test)]
mod resource_tests;
