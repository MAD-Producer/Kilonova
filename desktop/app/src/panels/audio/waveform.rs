use super::*;
use motion_host::platform::Platform;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc,
    },
    time::Instant,
};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Request {
    root: std::path::PathBuf,
    composition: String,
    object: u64,
    asset: motion_core::AudioAsset,
    begin: f64,
    end: f64,
    columns: usize,
}
impl Request {
    pub fn from_state(
        state: &Value,
        layer: &Value,
        columns: usize,
    ) -> Result<Option<Self>, String> {
        let kind = layer["content"]["kind"].as_str().unwrap_or("");
        let source = &layer["content"][kind];
        let id = match kind {
            "audio" => source["asset"].as_u64(),
            "video" => state["project"]["video_assets"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|a| a["id"] == source["asset"])
                .and_then(|a| a["audio_asset"].as_u64()),
            _ => return Ok(None),
        };
        let Some(id) = id else {
            return Ok(None);
        };
        let asset = state["project"]["audio_assets"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|a| a["id"] == id)
            .ok_or("waveform audio asset missing")?;
        let asset: motion_core::AudioAsset =
            serde_json::from_value(asset.clone()).map_err(|e| e.to_string())?;
        asset.validate().map_err(|e| e.to_string())?;
        let fps = state["project"]["fps"]
            .as_f64()
            .filter(|v| *v > 0.)
            .ok_or("waveform fps missing")?;
        let frames = state["project"]["frames"]
            .as_f64()
            .ok_or("waveform duration missing")?;
        let begin = layer["subframe_range"]["in_frame"]
            .as_f64()
            .or_else(|| layer["timeline"]["in_frame"].as_f64())
            .unwrap_or(0.)
            .max(0.);
        let end = layer["subframe_range"]["out_frame"]
            .as_f64()
            .or_else(|| layer["timeline"]["out_frame"].as_f64())
            .unwrap_or(frames)
            .min(frames);
        if end <= begin {
            return Ok(None);
        }
        let offset = layer["timeline"]["offset_frame"].as_f64().unwrap_or(0.);
        let source_offset = source["source_offset_us"].as_f64().unwrap_or(0.) / 1_000_000.;
        Ok(Some(Self {
            root: state["root"]
                .as_str()
                .ok_or("waveform root missing")?
                .into(),
            composition: state["composition"].as_str().unwrap_or("").into(),
            object: layer["id"].as_u64().ok_or("waveform layer missing")?,
            asset,
            begin: source_offset + (begin - offset) / fps,
            end: source_offset + (end - offset) / fps,
            columns: columns.clamp(1, motion_media::MAX_WAVEFORM_COLUMNS),
        }))
    }
}
struct Task {
    id: u64,
    request: Request,
    cancelled: Arc<AtomicBool>,
    receive: mpsc::Receiver<Result<Vec<motion_media::WaveBucket>, String>>,
    began: Instant,
    slow: bool,
}
struct InFlight {
    id: u64,
    object: u64,
    source: u64,
    began: Instant,
    finished: bool,
}
fn prepare_missing_cache(work: &Request, id: u64, stop: &AtomicBool) -> Result<(), String> {
    if stop.load(Ordering::Acquire) {
        return Err("cancelled".into());
    }
    let cache = motion_media::cache_path(&work.root, &work.asset)
        .map_err(|e| e.to_string())?
        .with_extension("wave");
    match std::fs::metadata(&cache) {
        Ok(_) => return Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("waveform metadata: {error}")),
    }
    // A normal cold cache is prepared once with the same bounded, cancellable
    // desktop decoder as import. An invalid existing cache remains an error;
    // this is not a decoder restart or a retry of a failed read.
    let platform = motion_desktop_media::platform();
    let jobs = motion_media::AudioJobs::with_decoder(
        work.root.clone(),
        Default::default(),
        Arc::new(move |path, pcm, track, bytes, check| {
            platform.decode_audio(path, pcm, track, bytes, check)
        }),
    )
    .map_err(|e| e.to_string())?;
    let request = format!("desktop-waveform-{id}");
    jobs.prepare_cache(&request, work.asset.clone())
        .map_err(|e| e.to_string())?;
    let result = loop {
        if stop.load(Ordering::Acquire) {
            break jobs
                .cancel(&request)
                .map(|_| ())
                .map_err(|e| e.to_string())
                .and(Err("cancelled".into()));
        }
        let status = match jobs.status(&request) {
            Ok(status) => status,
            Err(error) => break Err(error.to_string()),
        };
        match status.state.as_str() {
            "succeeded" => break Ok(()),
            "failed" => {
                break Err(status
                    .error
                    .unwrap_or_else(|| "audio waveform preparation failed without a cause".into()))
            }
            "cancelled" => break Err("cancelled".into()),
            "queued" | "running" => std::thread::sleep(std::time::Duration::from_millis(10)),
            other => {
                break Err(format!(
                    "unexpected audio waveform preparation state: {other}"
                ))
            }
        }
    };
    if let Err(cleanup) = jobs.release(&request) {
        log::error!("audio waveform cleanup failed request={id} operation=waveform_view object={} source={} phase=release cause={cleanup}", work.object, work.asset.id);
        if result.is_ok() {
            return Err(cleanup.to_string());
        }
    }
    result.map_err(|error| {
        if error == "cancelled" {
            error
        } else {
            format!(
                "prepare waveform source {}: {error}",
                work.asset.path.chars().take(512).collect::<String>()
            )
        }
    })
}
impl Drop for InFlight {
    fn drop(&mut self) {
        if !self.finished {
            log::error!("audio waveform worker exited request={} operation=waveform_view object={} source={} elapsed_ms={} phase={}",
                self.id, self.object, self.source, self.began.elapsed().as_millis(), if std::thread::panicking() {"panic"} else {"unexpected_return"});
        }
    }
}
#[derive(Default)]
pub(super) struct Waveform {
    wanted: Option<Request>,
    running: Option<Task>,
    pub values: Vec<motion_media::WaveBucket>,
    pub error: Option<String>,
    completed: bool,
}
impl Drop for Waveform {
    fn drop(&mut self) {
        self.cancel();
    }
}
impl Waveform {
    pub fn refresh(&mut self) {
        self.cancel();
        self.completed = false;
        self.error = None;
        self.values.clear();
    }
    pub fn cancel(&mut self) {
        if let Some(task) = &self.running {
            task.cancelled.store(true, Ordering::Release);
        }
    }
    pub fn request(&mut self, request: Option<Request>) {
        if self.wanted != request {
            self.cancel();
            self.wanted = request;
            self.values.clear();
            self.error = None;
            self.completed = false;
        }
        self.poll();
        if self.running.is_none() && !self.completed {
            if let Some(request) = self.wanted.clone() {
                self.start(request);
            }
        }
    }
    pub fn loading(&self) -> bool {
        self.wanted.is_some() && !self.completed
    }
    pub fn busy(&self) -> bool {
        self.running.is_some() || self.loading()
    }
    fn start(&mut self, request: Request) {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let (send, receive) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let stop = cancelled.clone();
        let work = request.clone();
        // Publish in-flight state before the worker can enter filesystem IO.
        self.running = Some(Task {
            id,
            request,
            cancelled,
            receive,
            began: Instant::now(),
            slow: false,
        });
        log::info!("audio waveform start request={id} operation=waveform_view object={} source={} columns={} begin={} end={} composition={} root={:?} source_path={:?}",
            work.object, work.asset.id, work.columns, work.begin, work.end,
            work.composition.chars().take(128).collect::<String>(),
            work.root.to_string_lossy().chars().take(512).collect::<String>(),
            work.asset.path.chars().take(512).collect::<String>());
        let spawn = std::thread::Builder::new().name("audio-waveform-view".into()).spawn(move || {
            let mut flight = InFlight { id, object: work.object, source: work.asset.id, began: Instant::now(), finished: false };
            let result = prepare_missing_cache(&work, id, &stop).and_then(|_| motion_media::waveform_view(&work.root, &work.asset, work.begin, work.end, work.columns, &mut || {
                if stop.load(Ordering::Acquire) { Err("cancelled".into()) } else { Ok(()) }
            }).map_err(|e| e.to_string())).map_err(|error| error.chars().take(2048).collect::<String>());
            match &result {
                Err(error) if error != "cancelled" => log::error!("audio waveform failed request={id} operation=waveform_view object={} source={} elapsed_ms={} cause={}", work.object, work.asset.id, flight.began.elapsed().as_millis(), error.chars().take(2048).collect::<String>()),
                _ => log::debug!("audio waveform done request={id} operation=waveform_view object={} source={} elapsed_ms={} cancelled={}", work.object, work.asset.id, flight.began.elapsed().as_millis(), stop.load(Ordering::Acquire)),
            }
            let _ = send.send(result); // The owning panel may have been dropped.
            flight.finished = true;
        });
        if let Err(error) = spawn {
            log::error!(
                "audio waveform spawn failed request={id} operation=waveform_view cause={error}"
            );
            self.running = None;
            self.error = Some(error.to_string());
            self.completed = true;
        }
    }
    pub fn poll(&mut self) -> bool {
        let Some(mut task) = self.running.take() else {
            return false;
        };
        let result = match task.receive.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                if !task.slow && task.began.elapsed().as_secs() >= 2 {
                    log::warn!("audio waveform still in flight request={} operation=waveform_view object={} source={} elapsed_ms={}", task.id, task.request.object, task.request.asset.id, task.began.elapsed().as_millis());
                    task.slow = true;
                }
                self.running = Some(task);
                return false;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                log::error!("audio waveform disconnected request={} operation=waveform_view object={} source={} elapsed_ms={}", task.id, task.request.object, task.request.asset.id, task.began.elapsed().as_millis());
                Err("audio waveform worker disconnected".into())
            }
        };
        if self.wanted.as_ref() == Some(&task.request) && !task.cancelled.load(Ordering::Acquire) {
            match result {
                Ok(values) => self.values = values,
                Err(error) => self.error = Some(error),
            }
            self.completed = true;
        }
        // A changed selection waits for this one worker to exit, then starts
        // only its most recent request. Rapid resizing cannot grow a queue.
        if !self.completed {
            if let Some(request) = self.wanted.clone() {
                self.start(request);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(root: &std::path::Path, object: u64) -> Request {
        Request {
            root: root.into(),
            composition: "comp-main".into(),
            object,
            asset: motion_core::AudioAsset {
                id: 8,
                path: "assets/source.wav".into(),
                bytes: 4,
                mime: "audio/wav".into(),
                track: 0,
                sample_rate: 48_000,
                channels: 2,
                sample_frames: 48_000,
                duration_us: 1_000_000,
            },
            begin: 0.,
            end: 1.,
            columns: 64,
        }
    }
    fn wait(wave: &mut Waveform) {
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        while wave.running.is_some() {
            assert!(Instant::now() < deadline);
            wave.poll();
            std::thread::yield_now();
        }
    }
    #[test]
    fn audio_waveform_publishes_one_inflight_request_cancels_and_discards_superseded_reply() {
        let root = tempfile::tempdir().unwrap();
        let old = request(root.path(), 1);
        let (send, receive) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut wave = Waveform::default();
        wave.wanted = Some(old.clone());
        wave.running = Some(Task {
            id: 0,
            request: old,
            cancelled: cancelled.clone(),
            receive,
            began: Instant::now(),
            slow: false,
        });
        for object in 2..100 {
            wave.request(Some(request(root.path(), object)));
        }
        assert!(cancelled.load(Ordering::Acquire));
        assert_eq!(
            wave.running.as_ref().unwrap().id,
            0,
            "no second worker while a filesystem operation is blocked"
        );
        assert_eq!(wave.wanted.as_ref().unwrap().object, 99);
        send.send(Ok(vec![motion_media::WaveBucket {
            min: -0.5,
            max: 0.5,
            rms: 0.25,
        }]))
        .unwrap();
        wave.poll();
        assert!(
            wave.values.is_empty(),
            "the old layer's peaks must never appear on the current layer"
        );
        wait(&mut wave);
        assert!(wave
            .error
            .as_deref()
            .unwrap()
            .contains("prepare waveform source assets/source.wav"));
        assert!(wave.completed);
        wave.request(None);
        assert!(wave.error.is_none() && wave.values.is_empty() && !wave.loading());
    }
    #[test]
    fn audio_waveform_real_failure_is_visible_and_uses_default_file_stderr_without_stdout_logs() {
        const CHILD: &str = "MOTION_AUDIO_WAVEFORM_ERROR_CHILD";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = std::path::PathBuf::from(root);
            let _logging = crate::logging::init(root.join("logs")).unwrap();
            let mut wave = Waveform::default();
            wave.request(Some(request(&root, 17)));
            assert!(
                wave.running.is_some(),
                "inflight state is published before observing completion"
            );
            wait(&mut wave);
            assert!(wave
                .error
                .as_deref()
                .unwrap()
                .contains("prepare waveform source assets/source.wav"));
            for _ in 0..100 {
                wave.request(wave.wanted.clone());
            }
            crate::logging::flush();
            let logs = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
            assert_eq!(
                logs.matches("audio waveform start ").count(),
                1,
                "failed requests are not automatically retried every frame"
            );
            assert!(
                logs.contains("INFO")
                    && logs.contains("operation=waveform_view object=17 source=8")
            );
            assert!(
                logs.contains("ERROR")
                    && logs.contains("cause=prepare waveform source assets/source.wav")
            );
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "panels::audio::waveform::tests::audio_waveform_real_failure_is_visible_and_uses_default_file_stderr_without_stdout_logs", "--nocapture"])
            .env_remove("MOTION_LOG").env(CHILD, temp.path()).output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("audio waveform failed"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("operation=waveform_view"));
        if let Some(path) = std::env::var_os("MOTION_AUDIO_MONITOR_ERROR_EVIDENCE") {
            let path = std::path::PathBuf::from(path);
            std::fs::create_dir_all(&path).unwrap();
            std::fs::copy(
                temp.path().join("logs/motion-studio.log"),
                path.join("file.log"),
            )
            .unwrap();
            std::fs::write(path.join("stderr.log"), output.stderr).unwrap();
            std::fs::write(path.join("stdout.log"), output.stdout).unwrap();
        }
    }
    #[test]
    fn audio_waveform_request_maps_video_audio_offset_subframe_clip_and_composition_identity() {
        let asset = serde_json::to_value(request(std::path::Path::new("."), 7).asset).unwrap();
        let state = serde_json::json!({"root":"project", "composition":"comp-two", "project": {
            "fps":30,"frames":300,"audio_assets":[asset],"video_assets":[{"id":9,"audio_asset":8}] }});
        let layer = serde_json::json!({"id":7,"content":{"kind":"video","video":{"asset":9,"source_offset_us":250000}},
            "timeline":{"in_frame":15,"out_frame":90,"offset_frame":10},"subframe_range":{"in_frame":15.5,"out_frame":75.5}});
        let key = Request::from_state(&state, &layer, 4000).unwrap().unwrap();
        assert!((key.begin - (0.25 + 5.5 / 30.)).abs() < 1e-12);
        assert!((key.end - (0.25 + 65.5 / 30.)).abs() < 1e-12);
        assert_eq!(key.composition, "comp-two");
        assert_eq!(key.columns, motion_media::MAX_WAVEFORM_COLUMNS);
    }
}
