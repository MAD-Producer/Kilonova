//! One bounded preset operation for the application, shared by native windows.
//! Snapshot and commit use the serial engine; package IO never blocks a UI frame.
use crate::{
    engine::{Command, Engine},
    layer_clipboard::{Edit, Read, Stamp},
};
use motion_core::{storage::StagedAnimationPreset, AnimationPreset, Property};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    time::Instant,
};
#[cfg(test)]
mod tests;

#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub category: String,
}
#[derive(Debug)]
pub(crate) enum Intent {
    Save {
        object: u64,
        properties: Vec<Property>,
        effects: Vec<u64>,
        destination: PathBuf,
    },
    Apply {
        objects: Vec<u64>,
        source: PathBuf,
    },
}
pub(crate) enum Completion {
    Catalog {
        entries: Vec<Entry>,
        failures: usize,
    },
    Saved(PathBuf),
    Applied(Value),
    Cancelled,
    Superseded,
}
enum Output {
    Complete(Completion),
    Prepared {
        staged: StagedAnimationPreset,
        stamp: Stamp,
        objects: Vec<u64>,
        at_frame: u32,
    },
}
enum Work {
    Scan(PathBuf),
    Document(Read, Intent),
}
enum Stage {
    Reading(
        Receiver<Result<Read, String>>,
        Intent,
        crate::layer_clipboard::KeyframeSource,
    ),
    Working(Job),
    Ready(Output),
    Committing(Receiver<Result<Value, String>>),
}
struct Pending {
    stage: Stage,
    cancel: Arc<AtomicBool>,
}
enum Step {
    Keep(Pending),
    Done(Completion),
}
#[derive(Default)]
pub(crate) struct Controller {
    pending: Option<Pending>,
    pub initialized: bool,
}
impl Drop for Controller {
    fn drop(&mut self) {
        self.cancel();
    }
}
impl Controller {
    pub(crate) fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub(crate) fn committing(&self) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|p| matches!(p.stage, Stage::Committing(_)))
    }
    pub(crate) fn phase(&self) -> Option<&'static str> {
        self.pending.as_ref().map(|p| {
            if p.cancel.load(Ordering::Acquire) {
                "cancelling"
            } else {
                match &p.stage {
                    Stage::Reading(..) => "reading",
                    Stage::Working(job) => job.phase(),
                    Stage::Ready(..) => "ready",
                    Stage::Committing(..) => "committing",
                }
            }
        })
    }
    pub(crate) fn cancel(&self) {
        if let Some(p) = &self.pending {
            p.cancel.store(true, Ordering::Release);
        }
    }
    pub(crate) fn scan(&mut self, root: PathBuf) -> Result<(), String> {
        if self.busy() {
            return Err("animation preset operation already in progress".into());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let job = Job::start(Work::Scan(root), cancel.clone())?;
        self.initialized = true;
        self.pending = Some(Pending {
            stage: Stage::Working(job),
            cancel,
        });
        Ok(())
    }
    pub(crate) fn start(
        &mut self,
        engine: &Engine,
        intent: Intent,
        source: crate::layer_clipboard::KeyframeSource,
    ) -> Result<(), String> {
        if self.busy() {
            return Err("animation preset operation already in progress".into());
        }
        let receive = engine.request(|reply| Command::ClipboardRead {
            objects: vec![],
            reply,
        })?;
        self.pending = Some(Pending {
            stage: Stage::Reading(receive, intent, source),
            cancel: Arc::new(AtomicBool::new(false)),
        });
        Ok(())
    }
    pub(crate) fn poll(
        &mut self,
        engine: &Engine,
        allow_commit: bool,
    ) -> Option<Result<Completion, String>> {
        let pending = self.pending.take()?;
        match Self::advance(pending, engine, allow_commit) {
            Ok(Step::Keep(pending)) => {
                self.pending = Some(pending);
                None
            }
            Ok(Step::Done(value)) => Some(Ok(value)),
            Err(error) => {
                log::error!(
                    "animation preset controller failed phase=poll reason={}",
                    bounded(&error)
                );
                Some(Err(error))
            }
        }
    }
    fn advance(mut pending: Pending, engine: &Engine, allow_commit: bool) -> Result<Step, String> {
        let cancelled = pending.cancel.load(Ordering::Acquire);
        pending.stage = match pending.stage {
            Stage::Reading(receive, intent, source) => match receive.try_recv() {
                Ok(result) => {
                    let read = result?;
                    if cancelled {
                        return Ok(Step::Done(Completion::Cancelled));
                    }
                    if !source.matches(&read.stamp) {
                        return Ok(Step::Done(Completion::Superseded));
                    }
                    Stage::Working(Job::start(
                        Work::Document(read, intent),
                        pending.cancel.clone(),
                    )?)
                }
                Err(mpsc::TryRecvError::Empty) => Stage::Reading(receive, intent, source),
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("preset snapshot worker disconnected".into())
                }
            },
            Stage::Working(job) => match job.receive.try_recv() {
                Ok(Ok(Output::Complete(Completion::Saved(path)))) => {
                    return Ok(Step::Done(Completion::Saved(path)))
                }
                Ok(Err(error))
                    if !(cancelled && error.ends_with("animation preset operation cancelled")) =>
                {
                    return Err(error)
                }
                Ok(result) => {
                    if cancelled {
                        return Ok(Step::Done(Completion::Cancelled));
                    }
                    Stage::Ready(result?)
                }
                Err(mpsc::TryRecvError::Empty) => Stage::Working(job),
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err(format!(
                        "preset IO worker disconnected: request={}",
                        job.request
                    ))
                }
            },
            Stage::Ready(output) => {
                if let Output::Complete(Completion::Saved(path)) = output {
                    return Ok(Step::Done(Completion::Saved(path)));
                }
                if cancelled {
                    return Ok(Step::Done(Completion::Cancelled));
                }
                match output {
                    Output::Complete(completion) => return Ok(Step::Done(completion)),
                    Output::Prepared {
                        staged,
                        stamp,
                        objects,
                        at_frame,
                    } if allow_commit => {
                        let edit = Edit::ApplyPreset {
                            staged,
                            stamp,
                            objects,
                            at_frame,
                            cancel: pending.cancel.clone(),
                        };
                        Stage::Committing(
                            engine.request(|reply| Command::ClipboardCommit { edit, reply })?,
                        )
                    }
                    output => Stage::Ready(output),
                }
            }
            Stage::Committing(receive) => match receive.try_recv() {
                Ok(result) => {
                    let state = result?;
                    return Ok(Step::Done(if state["clipboard_status"] == "superseded" {
                        if cancelled {
                            Completion::Cancelled
                        } else {
                            Completion::Superseded
                        }
                    } else {
                        Completion::Applied(state)
                    }));
                }
                Err(mpsc::TryRecvError::Empty) => Stage::Committing(receive),
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("preset commit worker disconnected".into())
                }
            },
        };
        Ok(Step::Keep(pending))
    }
}
fn bounded(value: &str) -> String {
    value.chars().take(1024).collect()
}
fn io_error(operation: &str, path: &std::path::Path, cause: impl std::fmt::Display) -> String {
    // Reserve space for the original cause even when a path is very long.
    let path: String = path.to_string_lossy().chars().take(384).collect();
    let cause: String = cause.to_string().chars().take(512).collect();
    format!("{operation} path={path}: {cause}")
}
struct Job {
    receive: Receiver<Result<Output, String>>,
    phase: Arc<AtomicU8>,
    request: u64,
}
impl Job {
    fn phase(&self) -> &'static str {
        match self.phase.load(Ordering::Acquire) {
            0 => "queued",
            1 => "io",
            _ => "finished",
        }
    }
    fn start(work: Work, cancel: Arc<AtomicBool>) -> Result<Self, String> {
        Self::start_with(work, cancel, || {})
    }
    fn start_with(
        work: Work,
        cancel: Arc<AtomicBool>,
        before: impl FnOnce() + Send + 'static,
    ) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let request = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| "preset request ID exhausted")?;
        let operation = match &work {
            Work::Scan(_) => "scan",
            Work::Document(_, Intent::Save { .. }) => "save",
            Work::Document(_, Intent::Apply { .. }) => "apply",
        };
        let (send, receive) = mpsc::channel();
        let phase = Arc::new(AtomicU8::new(0));
        let job = Self {
            receive,
            phase: phase.clone(),
            request,
        };
        let (root, revision, composition, generation, frame) = match &work {
            Work::Document(read, _) => (
                read.stamp.root.clone(),
                read.stamp.revision,
                read.stamp.composition.clone(),
                read.stamp.generation,
                read.frame,
            ),
            Work::Scan(root) => (root.clone(), 0, String::new(), 0, 0),
        };
        let (objects, path) = match &work {
            Work::Document(
                _,
                Intent::Save {
                    object,
                    destination,
                    ..
                },
            ) => (vec![*object], destination),
            Work::Document(_, Intent::Apply { objects, source }) => {
                (objects.iter().take(8).copied().collect(), source)
            }
            Work::Scan(root) => (vec![], root),
        };
        log::info!("animation preset start request={request} operation={operation} phase=queued generation={generation} revision={revision} frame={frame} objects={objects:?} composition={} root={} path={}", bounded(&composition), bounded(&root.to_string_lossy()), bounded(&path.to_string_lossy()));
        log::logger().flush();
        std::thread::Builder::new().name("motion-animation-preset".into()).spawn(move || {
            let began = Instant::now(); phase.store(1, Ordering::Release);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| { before(); run(work, &cancel) }))
                .unwrap_or_else(|payload| {
                    let cause = payload.downcast_ref::<String>().map(String::as_str)
                        .or_else(|| payload.downcast_ref::<&str>().copied()).unwrap_or("non-text panic");
                    Err(format!("preset worker panicked: {}", bounded(cause)))
                });
            phase.store(2, Ordering::Release);
            match &result {
                Err(error) if cancel.load(Ordering::Acquire) && error.ends_with("animation preset operation cancelled") => log::debug!("animation preset cancelled request={request} operation={operation} elapsed_ms={}", began.elapsed().as_millis()),
                Err(error) => log::error!("animation preset failed request={request} operation={operation} phase=io elapsed_ms={} reason={}", began.elapsed().as_millis(), bounded(error)),
                Ok(_) => log::debug!("animation preset done request={request} operation={operation} elapsed_ms={}", began.elapsed().as_millis()),
            }
            // Protocol best effort after the UI closes; dropping staged output cleans its owned files.
            let _ = send.send(result);
        }).map_err(|e| format!("start preset worker: {e}"))?;
        Ok(job)
    }
}
fn run(work: Work, cancel: &AtomicBool) -> Result<Output, String> {
    if cancel.load(Ordering::Acquire) {
        return Ok(Output::Complete(Completion::Cancelled));
    }
    match work {
        Work::Scan(root) => scan(root, cancel).map(Output::Complete),
        Work::Document(read, intent) => match intent {
            Intent::Save {
                object,
                properties,
                effects,
                destination,
            } => {
                let name = destination
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or("invalid preset filename")?
                    .to_owned();
                let category = destination
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|s| s.to_str())
                    .unwrap_or("User")
                    .to_owned();
                let preset = AnimationPreset::capture(
                    read.project
                        .as_ref()
                        .ok_or("preset source metadata missing")?,
                    object,
                    name,
                    category,
                    &properties,
                    &effects,
                )
                .map_err(|e| e.to_string())?;
                motion_core::storage::save_animation_preset(
                    &read.stamp.root,
                    &preset,
                    &destination,
                    cancel,
                )
                .map_err(|e| io_error("save preset", &destination, e))?;
                Ok(Output::Complete(Completion::Saved(destination)))
            }
            Intent::Apply { objects, source } => {
                let staged =
                    motion_core::storage::stage_animation_preset(&source, &read.stamp.root, cancel)
                        .map_err(|e| io_error("read preset", &source, e))?;
                // Validate transported pixels on the IO worker even before a GPU
                // window is attached. One bounded decode at a time; failed or
                // cancelled preparation drops the operation's owned files.
                for asset in &staged.preset.assets {
                    if cancel.load(Ordering::Acquire) {
                        return Ok(Output::Complete(Completion::Cancelled));
                    }
                    let path = read.stamp.root.join(&asset.path);
                    let image =
                        motion_render::image_resources::Source::new(&read.stamp.root, asset)
                            .map_err(|e| io_error("preset image source", &path, e))?;
                    motion_render::image_resources::decode_with_budget(
                        &image,
                        motion_render::image_resources::Resolution::Full,
                        motion_render::image_resources::MAX_DECODED_BYTES,
                    )
                    .map_err(|e| io_error("preset image decode", &path, e))?;
                }
                Ok(Output::Prepared {
                    staged,
                    stamp: read.stamp,
                    objects,
                    at_frame: read.frame,
                })
            }
        },
    }
}
fn scan(root: PathBuf, cancel: &AtomicBool) -> Result<Completion, String> {
    let mut entries = Vec::new();
    let mut failures = 0usize;
    if !root
        .try_exists()
        .map_err(|e| io_error("preset catalog", &root, e))?
    {
        std::fs::create_dir_all(&root).map_err(|e| io_error("create preset library", &root, e))?;
        return Ok(Completion::Catalog { entries, failures }); // Empty user library is normal.
    }
    let mut paths = vec![(root, 0)];
    let mut seen = 0usize;
    while let Some((directory, depth)) = paths.pop() {
        for item in
            std::fs::read_dir(&directory).map_err(|e| io_error("preset catalog", &directory, e))?
        {
            if cancel.load(Ordering::Acquire) {
                return Ok(Completion::Cancelled);
            }
            let item = item.map_err(|e| e.to_string())?;
            seen += 1;
            if seen > 512 {
                return Err("preset catalog exceeds 512 filesystem entries".into());
            }
            let kind = item.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                // Categories are immediate child folders; no unbounded recursion.
                if depth == 0 {
                    paths.push((item.path(), 1));
                }
                continue;
            }
            let path = item.path();
            if !kind.is_file() || path.extension().is_none_or(|ext| ext != "msanim") {
                continue;
            }
            if entries.len() >= 128 {
                return Err("preset catalog exceeds 128 presets".into());
            }
            match motion_core::storage::read_animation_preset(&path) {
                Ok(preset) => entries.push(Entry {
                    path,
                    name: preset.name,
                    category: preset.category,
                }),
                Err(error) => {
                    failures += 1;
                    log::error!(
                        "animation preset catalog entry rejected path={} reason={}",
                        bounded(&path.to_string_lossy()),
                        bounded(&error.to_string())
                    );
                }
            }
        }
    }
    entries.sort_by(|a, b| (&a.category, &a.name, &a.path).cmp(&(&b.category, &b.name, &b.path)));
    Ok(Completion::Catalog { entries, failures })
}
