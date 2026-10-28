//! Read-only source video playback. Codecs stay on one bounded worker, never on
//! the UI or the serial composition engine. Only one decode can be in flight.
use crate::layer_clipboard::KeyframeSource;
use motion_core::VideoAsset;
use motion_host::platform::{Platform, VideoDecoder};
use motion_ui::paint::Bitmap;
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError},
        Arc,
    },
    time::Instant,
};

static WORKERS: AtomicUsize = AtomicUsize::new(0);
static NEXT: AtomicU64 = AtomicU64::new(1);
const CANCELLED: &str = "source preview cancelled";
fn phase_name(phase: u8) -> &'static str {
    match phase {
        0 => "queued",
        1 => "opening",
        2 => "idle",
        3 => "decode",
        _ => "unknown",
    }
}
impl crate::Shell {
    pub(crate) fn poll_project_preview(&mut self) {
        let preview = self.panels.project.browser.video_preview();
        if preview.is_some_and(|id| self.panels.project.preview.current(&self.state, id)) {
            if self.panels.project.preview.poll(Instant::now()) {
                self.dirty = true;
            }
            if let Some(cause) = self.panels.project.preview.notice.take() {
                self.panels.message = Some(self.workspace.locale.error(&cause));
            }
        } else {
            self.panels.project.preview.close();
        }
    }
}
struct Permit;
impl Drop for Permit {
    fn drop(&mut self) {
        WORKERS.fetch_sub(1, Ordering::AcqRel);
    }
}
struct Worker {
    send: SyncSender<u64>,
    receive: Receiver<Result<Frame, String>>,
    cancel: Arc<AtomicBool>,
    phase: Arc<AtomicU8>,
    request: u64,
    started: Instant,
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}
struct Frame {
    bitmap: Arc<Bitmap>,
    pts: u64,
    end: u64,
}
#[derive(Default)]
pub(crate) struct Player {
    source: Option<(KeyframeSource, u64)>,
    worker: Option<Worker>,
    pending: Option<u64>,
    frame: Option<Frame>,
    start: u64,
    duration: u64,
    cursor: u64,
    playing: Option<(Instant, u64)>,
    pub message: Option<String>,
    pub notice: Option<String>,
    #[cfg(test)]
    before: Option<Box<dyn FnOnce() + Send>>,
}
impl Player {
    pub fn start(&mut self, state: &Value, id: u64) -> Result<(), String> {
        self.close();
        let asset: VideoAsset = serde_json::from_value(
            state["project"]["video_assets"]
                .as_array()
                .and_then(|assets| assets.iter().find(|a| a["id"].as_u64() == Some(id)))
                .ok_or("source preview asset missing")?
                .clone(),
        )
        .map_err(|e| e.to_string())?;
        let root = PathBuf::from(
            state["root"]
                .as_str()
                .ok_or("source preview root missing")?,
        );
        WORKERS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 2).then_some(n + 1)
            })
            .map_err(|_| "source preview workers are still closing")?;
        let permit = Permit;
        let request = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| "source preview identity exhausted")?;
        let (send, commands) = mpsc::sync_channel(1);
        let (complete, receive) = mpsc::sync_channel(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let phase = Arc::new(AtomicU8::new(0));
        let target = Arc::new(AtomicU64::new(asset.video_start_us));
        let started = Instant::now();
        let context = format!(
            "request={request} asset={id} source={}",
            crate::fallback_observation::short(&asset.path)
        );
        let worker_phase = phase.clone();
        let worker_cancel = cancel.clone();
        let worker_asset = asset.clone();
        #[cfg(test)]
        let before = self.before.take();
        self.source = Some((KeyframeSource::of(state), id));
        self.start = asset.video_start_us;
        self.duration = asset.video_end_us.saturating_sub(asset.video_start_us);
        self.worker = Some(Worker {
            send,
            receive,
            cancel,
            phase,
            request,
            started,
        });
        self.queue();
        // Publish the source, phase and pending target before spawn: even a
        // platform open that never returns remains observable and cancellable.
        log::info!("source preview queued {context} phase=queued");
        let spawned=std::thread::Builder::new().name("motion-source-preview".into()).spawn(move || {
            let _permit = permit;
            let mut observation=Observation {context,phase:worker_phase,target:target.clone(),started,done:false};
            observation.phase.store(1, Ordering::Release);
            log::info!("source preview start {} phase=opening",observation.context);
            #[cfg(test)]
            if let Some(before) = before { before(); }
            let result = run(&root, &worker_asset, &worker_cancel, &commands, &complete, &observation);
            match result {
                Err(Failure::Error(cause)) => {
                    log::error!("source preview failed {} phase={} target_us={} elapsed_ms={} cause={cause}", observation.context, phase_name(observation.phase.load(Ordering::Acquire)), target.load(Ordering::Acquire), started.elapsed().as_millis());
                    // Completion after the popup receiver exits is best effort.
                    let _ = complete.send(Err(cause));
                }
                Err(Failure::Cancelled) | Ok(()) => {}
            }
            log::debug!("source preview done {} elapsed_ms={}",observation.context,started.elapsed().as_millis());
            observation.done = true;
            drop(observation);
        });
        if let Err(cause) = spawned {
            self.close();
            return Err(format!(
                "source preview spawn request={request} asset={id}: {cause}"
            ));
        }
        Ok(())
    }
    pub fn close(&mut self) {
        self.worker = None;
        self.source = None;
        self.pending = None;
        self.frame = None;
        self.playing = None;
        self.cursor = 0;
        self.message = None;
        self.notice = None;
    }
    pub fn current(&self, state: &Value, id: u64) -> bool {
        self.source.as_ref() == Some(&(KeyframeSource::of(state), id))
    }
    pub fn bitmap(&self) -> Option<Arc<Bitmap>> {
        self.frame.as_ref().map(|f| f.bitmap.clone())
    }
    pub fn seconds(&self) -> f64 {
        self.cursor as f64 / 1_000_000.
    }
    pub fn duration(&self) -> f64 {
        self.duration as f64 / 1_000_000.
    }
    pub fn playing(&self) -> bool {
        self.playing.is_some()
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some() || self.playing.is_some()
    }
    pub fn toggle(&mut self) {
        if self.message.is_some() || self.worker.is_none() {
            return;
        }
        if self.playing.take().is_none() {
            if self.cursor >= self.duration {
                self.cursor = 0;
            }
            self.playing = Some((Instant::now(), self.cursor));
        }
        self.queue();
    }
    pub fn seek(&mut self, seconds: f64) {
        if !seconds.is_finite() || self.message.is_some() {
            return;
        }
        self.playing = None;
        self.cursor = (seconds.max(0.) * 1_000_000.)
            .round()
            .min(self.duration as f64) as u64;
        self.queue();
    }
    pub fn poll(&mut self, now: Instant) -> bool {
        let mut changed = false;
        if let Some((began, origin)) = self.playing {
            let next = origin
                .saturating_add(
                    now.saturating_duration_since(began)
                        .as_micros()
                        .min(u128::from(u64::MAX)) as u64,
                )
                .min(self.duration);
            changed |= self.cursor != next;
            self.cursor = next;
            if self.cursor == self.duration {
                self.playing = None;
            }
        }
        if let Some(worker) = self.worker.as_ref().filter(|_| self.pending.is_some()) {
            let result = match worker.receive.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => {
                    let cause = format!(
                        "source preview worker disconnected request={} phase={} elapsed_ms={}",
                        worker.request,
                        phase_name(worker.phase.load(Ordering::Acquire)),
                        worker.started.elapsed().as_millis()
                    );
                    log::error!("{cause}");
                    Some(Err(cause))
                }
            };
            if let Some(result) = result {
                let requested = self.pending.take().expect("pending decode was checked");
                match result {
                    Ok(frame) if self.playing() || requested == self.target() => {
                        self.frame = Some(frame)
                    }
                    Ok(_) => {} // A scrub superseded this response; publish only its latest target.
                    Err(cause) => self.fail(cause),
                }
                changed = true;
            }
        }
        self.queue();
        changed
    }
    fn target(&self) -> u64 {
        self.start
            .saturating_add(self.cursor.min(self.duration.saturating_sub(1)))
    }
    fn queue(&mut self) {
        if self.pending.is_some() {
            return;
        }
        let target = self.target();
        if self
            .frame
            .as_ref()
            .is_some_and(|f| f.pts <= target && target < f.end)
        {
            return;
        }
        let Some(worker) = &self.worker else {
            return;
        };
        self.pending = Some(target); // Publish before the worker can enter the decoder.
        if let Err(cause) = worker.send.try_send(target) {
            let cause = format!(
                "source preview dispatch request={} target_us={target}: {cause}",
                worker.request
            );
            log::error!("{cause}");
            self.fail(cause);
        }
    }
    fn fail(&mut self, cause: String) {
        self.message = Some(crate::fallback_observation::short(&cause).to_string());
        self.notice = self.message.clone();
        self.worker = None;
        self.pending = None;
        self.playing = None;
    }
}
struct Observation {
    context: String,
    phase: Arc<AtomicU8>,
    target: Arc<AtomicU64>,
    started: Instant,
    done: bool,
}
impl Drop for Observation {
    fn drop(&mut self) {
        if !self.done {
            log::error!(
                "source preview interrupted {} phase={} target_us={} elapsed_ms={} unwinding={}",
                self.context,
                phase_name(self.phase.load(Ordering::Acquire)),
                self.target.load(Ordering::Acquire),
                self.started.elapsed().as_millis(),
                std::thread::panicking()
            );
        }
    }
}
enum Failure {
    Cancelled,
    Error(String),
}
impl From<std::io::Error> for Failure {
    fn from(e: std::io::Error) -> Self {
        Self::Error(e.to_string())
    }
}
fn media<T>(value: motion_media::Result<T>) -> Result<T, Failure> {
    value.map_err(|e| {
        if matches!(e.primary(),motion_media::Error::Message(s) if s==CANCELLED) {
            Failure::Cancelled
        } else {
            Failure::Error(e.to_string())
        }
    })
}
fn check(cancel: &AtomicBool) -> motion_media::Result<()> {
    if cancel.load(Ordering::Acquire) {
        Err(CANCELLED.into())
    } else {
        Ok(())
    }
}
fn open(
    root: &Path,
    asset: &VideoAsset,
    cancel: &AtomicBool,
) -> Result<Box<dyn VideoDecoder>, Failure> {
    media(check(cancel))?;
    asset
        .validate()
        .map_err(|e| Failure::Error(e.to_string()))?;
    let root = root.canonicalize()?;
    let path = root.join(&asset.path).canonicalize()?;
    if !path.starts_with(&root) {
        return Err(Failure::Error("source preview path outside project".into()));
    }
    let metadata = path.metadata()?;
    if !metadata.is_file() || metadata.len() != asset.bytes {
        return Err(Failure::Error("source preview source size mismatch".into()));
    }
    let backend = motion_desktop_media::DesktopPlatform::new();
    let index_path = media(motion_media::video_cache_path(&root, asset))?;
    let timestamps = match index_path.canonicalize() {
        Ok(_) => media(motion_media::load_video_index(&root, asset))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut probe =
                media(
                    backend
                        .probe_video(&path, Some(asset.track), Some(u32::MAX), &|| check(cancel)),
                )?;
            probe.asset.id = asset.id;
            probe.asset.path = asset.path.clone();
            probe.asset.audio_asset = asset.audio_asset;
            if &probe.asset != asset {
                return Err(Failure::Error(
                    "source preview video metadata mismatch".into(),
                ));
            }
            probe.timestamps
        }
        Err(e) => return Err(e.into()),
    };
    media(check(cancel))?;
    media(backend.open_decoder(&path, asset.clone(), timestamps))
}
fn run(
    root: &Path,
    asset: &VideoAsset,
    cancel: &AtomicBool,
    commands: &Receiver<u64>,
    complete: &SyncSender<Result<Frame, String>>,
    observation: &Observation,
) -> Result<(), Failure> {
    let mut decoder = open(root, asset, cancel)?;
    observation.phase.store(2, Ordering::Release);
    while let Ok(target) = commands.recv() {
        media(check(cancel))?;
        observation.target.store(target, Ordering::Release);
        observation.phase.store(3, Ordering::Release);
        let started = Instant::now();
        let frame = media(decoder.frame(target, &|| check(cancel)))?;
        let rgba = frame.rgba().map_err(|e| Failure::Error(e.to_string()))?;
        let pixels = motion_render::image_resources::preview_rgba(
            frame.width,
            frame.height,
            &rgba,
            128,
            cancel,
        )
        .map_err(|e| match e {
            motion_render::image_resources::Error::Cancelled => Failure::Cancelled,
            _ => Failure::Error(e.to_string()),
        })?;
        let bitmap = Bitmap::new(pixels.width, pixels.height, pixels.rgba)
            .map_err(|e| Failure::Error(e.to_string()))?;
        log::debug!(
            "source preview frame {} target_us={target} pts_us={} elapsed_ms={}",
            observation.context,
            frame.pts,
            started.elapsed().as_millis()
        );
        if complete
            .send(Ok(Frame {
                bitmap,
                pts: frame.pts,
                end: frame.end,
            }))
            .is_err()
        {
            return Ok(());
        }
        observation.phase.store(2, Ordering::Release);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
