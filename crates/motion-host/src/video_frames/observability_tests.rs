use super::*;
mod shutdown_tests;
use std::{
    fs::File,
    io::{BufWriter, Write},
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};

struct FileCapture(Mutex<BufWriter<File>>);
impl log::Log for FileCapture {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let mut file = self.0.lock().unwrap();
        writeln!(file, "{} {}", record.level(), record.args()).unwrap();
        if record.level() <= log::Level::Warn {
            file.flush().unwrap();
        }
    }
    fn flush(&self) {
        self.0.lock().unwrap().flush().unwrap();
    }
}

fn isolated(name: &str, variable: &str, body: impl FnOnce(&Path)) {
    if let Some(root) = std::env::var_os(variable) {
        let root = PathBuf::from(root);
        let sink = Box::leak(Box::new(FileCapture(Mutex::new(BufWriter::new(
            File::create(root.join("events.log")).unwrap(),
        )))));
        log::set_logger(sink).unwrap();
        log::set_max_level(log::LevelFilter::Info);
        body(&root);
        log::logger().flush();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            &format!("video_frames::observability_tests::{name}"),
        ])
        .env(variable, root.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("video decoder "));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("video decoder "));
}

struct Release(Sender<()>);
impl Drop for Release {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}
struct Gate {
    phase: &'static str,
    entered: Sender<&'static str>,
    release: Mutex<Receiver<()>>,
    error: Option<&'static str>,
}
impl Gate {
    fn block(&self) -> Result<()> {
        self.entered.send(self.phase).map_err(|e| e.to_string())?;
        self.release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(15))
            .map_err(|e| e.to_string().into())
    }
}
struct Backend(Arc<Gate>);
impl crate::platform::Platform for Backend {
    fn name(&self) -> &'static str {
        "observation-test"
    }
    fn probe_video(
        &self,
        _: &Path,
        _: Option<u32>,
        _: Option<u32>,
        _: &dyn Fn() -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_media::VideoProbe> {
        Err("not used".into())
    }
    fn open_decoder(&self, _: &Path, _: VideoAsset, _: Vec<u64>) -> motion_media::Result<Box<dyn VideoDecoder>> {
        if self.0.phase == "open_error" {
            self.0.block()?;
            return Err(self.0.error.unwrap_or("controlled open failure").into());
        }
        if self.0.phase == "open" {
            self.0.block()?;
        }
        Ok(Box::new(Decoder(self.0.clone())))
    }
    fn decode_audio(
        &self,
        _: &Path,
        _: &Path,
        _: Option<u32>,
        _: u64,
        _: &mut dyn FnMut(Option<f64>) -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_core::AudioAsset> {
        Err("not used".into())
    }
    fn media_capabilities(&self, _: Option<&crate::platform::VideoQuery>) -> motion_media::Result<Value> {
        Err("not used".into())
    }
    fn attach_surface(
        &self,
        _: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        _: u32,
        _: u32,
        _budget: motion_render::AllocationBudget,
    ) -> crate::Result<crate::platform::SurfaceTarget> {
        Err("not used".into())
    }
}
struct Decoder(Arc<Gate>);
impl Drop for Decoder {
    fn drop(&mut self) {
        if self.0.phase == "close" { self.0.block().unwrap(); }
    }
}
impl VideoDecoder for Decoder {
    fn frame(&mut self, time: u64, check: &dyn Fn() -> motion_media::Result<()>) -> motion_media::Result<DecodedFrame> {
        check()?;
        if matches!(self.0.phase, "decode" | "oversized") {
            self.0.block()?;
        }
        if let Some(error) = self.0.error {
            return Err(error.into());
        }
        check()?;
        Ok(DecodedFrame {
            pixels: crate::video_frame::VideoPixels::Rgba(vec![
                255;
                if self.0.phase == "oversized" {
                    crate::video_cache::CACHE_BYTES + 1
                } else {
                    16
                }
            ]),
            pts: time,
            end: if time == 0 { 33333 } else { 66666 },
            width: 2,
            height: 2,
            decode_us: 0,
            codec_us: 0,
            transfer_us: 0,
            pack_us: 0,
            source_transfer: "test",
            decoder_name: "controlled".into(),
        })
    }
    fn seeks(&self) -> u64 {
        if self.0.phase == "seeks" {
            self.0.block().unwrap();
            panic!("controlled seek telemetry panic");
        }
        0
    }
}
fn fixture(root: &Path) -> VideoAsset {
    let asset: VideoAsset = serde_json::from_value(json!({
        "id":1,"path":"assets/test.mp4","bytes":4,"mime":"video/avc","track":0,
        "width":2,"height":2,"rotation":0,"display_width":2,"display_height":2,
        "video_start_us":0,"video_end_us":66666,"duration_us":66666,
        "frame_count":2,"variable_frame_rate":false,"nominal_frame_rate":30.0,
        "color_standard":1,"color_range":1,"audio_asset":null
    }))
    .unwrap();
    std::fs::create_dir_all(root.join("assets")).unwrap();
    std::fs::write(root.join(&asset.path), b"1234").unwrap();
    let path = motion_media::video_cache_path(root, &asset).unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    motion_media::save_index(&path, &asset, &[0, 33333]).unwrap();
    asset
}
fn wait_until(mut condition: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(
            Instant::now() < end,
            "decoder observation deadline exceeded"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn blocked_decoder_exposes_open_and_frame_flights_and_escalates_pending_once() {
    isolated(
        "blocked_decoder_exposes_open_and_frame_flights_and_escalates_pending_once",
        "MOTION_VIDEO_FLIGHT_CHILD",
        |root| {
            for (object, phase) in [(7, "open"), (8, "decode")] {
                let folder = root.join(phase);
                let asset = fixture(&folder);
                let (entered, waiting) = mpsc::channel();
                let (release, receive) = mpsc::channel();
                let release = Release(release);
                let backend = Arc::new(Backend(Arc::new(Gate {
                    phase,
                    entered,
                    release: Mutex::new(receive),
                    error: None,
                })));
                let stream = Stream::new(folder, asset, backend, object).unwrap();
                let mut frames = VideoFrames::default();
                frames.streams.insert(object, stream);
                let stream = &frames.streams[&object];
                let episodes = if phase == "decode" { 2 } else { 1 };
                for episode in 0..episodes {
                    let sequence = 11 + episode;
                    let time = episode * 33333;
                    stream.request(time, sequence, false).unwrap();
                    assert_eq!(waiting.recv_timeout(Duration::from_secs(5)).unwrap(), phase);
                    let snapshot = stream.shared.0.lock().unwrap();
                    let report = status(&snapshot);
                    drop(snapshot);
                    assert_eq!(report["state"], "pending");
                    assert_eq!(
                        report["in_flight"]["phase"], phase,
                        "decode flight must be visible before provider completion"
                    );
                    assert_eq!(report["in_flight"]["pts_us"], time);
                    assert_eq!(report["in_flight"]["sequence"], sequence);
                    assert_eq!(report["in_flight"]["speculative"], false);
                    assert!(report["in_flight"]["elapsed_ms"].is_u64());
                    let metrics = frames.metrics();
                    assert_eq!(metrics["decodeInFlight"][0]["object"], object);
                    assert_eq!(metrics["decodeInFlight"][0]["source"], 1);
                    assert_eq!(metrics["decodeInFlight"][0]["sequence"], sequence);
                    // Read without flushing from this thread: Info start must have
                    // been flushed by the worker before entering either provider.
                    assert!(std::fs::read_to_string(root.join("events.log")).unwrap()
                    .contains(&format!("INFO video decoder open start object={object} source=1 sequence=11 pts_us=0")));
                    for _ in 1..120 {
                        stream.request(time, sequence, false).unwrap();
                    }
                    let text = std::fs::read_to_string(root.join("events.log")).unwrap();
                    let marker =
                        format!("ERROR persistent video pending object={object} source=1 ");
                    assert_eq!(text.matches(&marker).count() as u64, episode + 1);
                    assert!(text.contains(&format!("count=60 phase={phase} active_sequence=Some({sequence}) active_pts_us=Some({time}) decode_elapsed_ms=")));
                    release.0.send(()).unwrap();
                    wait_until(|| stream.ready(sequence).is_ok());
                    let ready = stream.request(time, sequence, false).unwrap();
                    assert_eq!(ready["state"], "ready");
                    assert_eq!(ready["pending_polls"], 0);
                    assert!(ready["in_flight"].is_null());
                    assert!(frames.metrics()["decodeInFlight"]
                        .as_array()
                        .unwrap()
                        .is_empty());
                }
                drop(frames);
                wait_until(|| {
                    std::fs::read_to_string(root.join("events.log"))
                        .unwrap()
                        .contains(&format!("video decoder worker stopped object={object}"))
                });
            }
            let text = std::fs::read_to_string(root.join("events.log")).unwrap();
            assert!(!text.contains("DEBUG"));
            assert!(!text.contains("video decode done"));
        },
    );
}

#[test]
fn decoder_stop_preserves_discarded_failure_but_not_expected_supersession() {
    isolated(
        "decoder_stop_preserves_discarded_failure_but_not_expected_supersession",
        "MOTION_VIDEO_STOP_CHILD",
        |root| {
            for (object, error) in [
                (9, "controlled decoder failed after stop"),
                (10, "video target superseded"),
            ] {
                let folder = root.join(object.to_string());
                let asset = fixture(&folder);
                let (entered, waiting) = mpsc::channel();
                let (release, receive) = mpsc::channel();
                let release = Release(release);
                let backend = Arc::new(Backend(Arc::new(Gate {
                    phase: "decode",
                    entered,
                    release: Mutex::new(receive),
                    error: Some(error),
                })));
                let stream = Stream::new(folder, asset, backend, object).unwrap();
                stream.request(0, 31, false).unwrap();
                waiting.recv_timeout(Duration::from_secs(5)).unwrap();
                let shared = stream.shared.clone();
                // This case verifies failure-vs-cancellation after stop. Drop
                // now waits for a bounded join, so release the controlled native
                // operation after stop and before testing normal disposal. The
                // shutdown suite separately keeps it blocked past that budget.
                stream.stop().unwrap();
                release.0.send(()).unwrap();
                wait_until(|| stream.worker.as_ref().unwrap().is_finished());
                drop(stream);
                wait_until(|| {
                    std::fs::read_to_string(root.join("events.log")).unwrap()
                .contains(&format!("video decoder worker stopped object={object} source=1 reason=stop_after_decode"))
                });
                let state = shared.0.lock().unwrap();
                assert!(state.stop);
                assert!(state.worker_stopped);
                assert!(
                    state.error.is_none(),
                    "stopped stream must keep its existing discard behavior"
                );
                assert!(state.in_flight.is_none());
            }
            let text = std::fs::read_to_string(root.join("events.log")).unwrap();
            let marker =
                "ERROR video decode discarded failure object=9 source=1 sequence=31 pts_us=0 ";
            assert!(text.contains(marker));
            assert!(text.contains("ms stopped=true reason=controlled decoder failed after stop"));
            assert!(!text.contains("discarded failure object=10"));
            assert_eq!(
                text.lines()
                    .filter(|line| line.starts_with("ERROR"))
                    .count(),
                1
            );
        },
    );
}

#[test]
fn unpolled_required_failures_and_outer_unwind_leave_file_evidence() {
    isolated(
        "unpolled_required_failures_and_outer_unwind_leave_file_evidence",
        "MOTION_VIDEO_UNPOLLED_FAILURE_CHILD",
        |root| {
            for (object, phase, error, expected_phase) in [
                (11, "open_error", Some("controlled open failure"), "open"),
                (
                    12,
                    "decode",
                    Some("controlled required frame failure"),
                    "decode",
                ),
                (13, "oversized", None, "cache"),
                (14, "seeks", None, "decode"),
            ] {
                let folder = root.join(object.to_string());
                let asset = fixture(&folder);
                let (entered, waiting) = mpsc::channel();
                let (release, receive) = mpsc::channel();
                let release = Release(release);
                let backend = Arc::new(Backend(Arc::new(Gate {
                    phase,
                    entered,
                    release: Mutex::new(receive),
                    error,
                })));
                let stream = Stream::new(folder, asset, backend, object).unwrap();
                stream.request(0, 41, false).unwrap();
                waiting.recv_timeout(Duration::from_secs(5)).unwrap();
                release.0.send(()).unwrap();
                let marker = if phase == "seeks" {
                    format!("ERROR video decoder worker exited unexpectedly object={object} source=1 reason=unwind sequence=Some(41) pts_us=Some(0) phase=decode ")
                } else {
                    format!(
                        "ERROR video decode failed object={object} source=1 sequence=41 pts_us=0 "
                    )
                };
                // No status/ready poll after release: the failure itself must flush.
                wait_until(|| {
                    std::fs::read_to_string(root.join("events.log"))
                        .unwrap()
                        .contains(&marker)
                });
                let text = std::fs::read_to_string(root.join("events.log")).unwrap();
                assert_eq!(text.matches(&marker).count(), 1);
                if phase == "seeks" {
                    let mut frames = VideoFrames::default();
                    frames.streams.insert(object, stream);
                    assert_eq!(frames.metrics()["decodeWorkersStopped"], 1);
                    let state = frames.streams[&object]
                        .shared
                        .0
                        .lock()
                        .unwrap_or_else(|p| p.into_inner());
                    assert!(state.worker_stopped);
                    assert!(state.in_flight.is_none());
                    assert!(
                        state.error.is_none(),
                        "observation must preserve the existing outer-panic result"
                    );
                    assert_eq!(status(&state)["worker_stopped"], true);
                } else {
                    assert!(text.contains(&format!("ms phase={expected_phase} speculative=false requested_sequence=41 requested_pts_us=0 discarded=false")));
                    let reason =
                        error.unwrap_or("video source frame exceeds its stream cache budget");
                    assert!(text.contains(&format!("reason={reason}")));
                    assert_eq!(stream.ready(41).err().map(|error| error.to_string()).as_deref(), Some(reason));
                    let state = stream.shared.0.lock().unwrap();
                    assert!(!state.worker_stopped);
                    assert!(state.in_flight.is_none());
                }
            }
        },
    );
}
