//! A blocked native decoder must not block presentation of editor chrome.
use super::*;
use motion_core::{Content, Layer, Project, VideoAsset, VideoClip};
use motion_host::{platform::VideoDecoder, video_frame::VideoPixels};
use std::{path::Path, sync::Condvar};

struct Gate {
    released: Mutex<bool>,
    ready: Condvar,
    entered: Sender<()>,
    fail: bool,
}
struct Backend(Arc<Gate>);
struct Decoder(Arc<Gate>);
impl VideoDecoder for Decoder {
    fn frame(
        &mut self,
        _: u64,
        check: &dyn Fn() -> Result<(), String>,
    ) -> Result<DecodedFrame, String> {
        self.0.entered.send(()).map_err(|e| e.to_string())?;
        let gate = self.0.released.lock().unwrap();
        let (gate, timeout) = self
            .0
            .ready
            .wait_timeout_while(gate, Duration::from_secs(10), |v| !*v)
            .unwrap();
        if timeout.timed_out() {
            return Err("controlled decoder gate timed out".into());
        }
        drop(gate);
        check()?;
        if self.0.fail {
            return Err("controlled first video frame failure".into());
        }
        Ok(DecodedFrame {
            pixels: VideoPixels::Rgba(vec![255; 32]),
            pts: 0,
            end: 100_000,
            width: 2,
            height: 4,
            decode_us: 0,
            codec_us: 0,
            transfer_us: 0,
            pack_us: 0,
            source_transfer: "controlled",
            decoder_name: "controlled".into(),
        })
    }
}
impl Platform for Backend {
    fn name(&self) -> &'static str {
        "pending-test"
    }
    fn probe_video(
        &self,
        _: &Path,
        _: Option<u32>,
        _: Option<u32>,
        _: &dyn Fn() -> Result<(), String>,
    ) -> Result<motion_media::VideoProbe, String> {
        Err("unused".into())
    }
    fn open_decoder(
        &self,
        _: &Path,
        _: VideoAsset,
        _: Vec<u64>,
    ) -> Result<Box<dyn VideoDecoder>, String> {
        Ok(Box::new(Decoder(self.0.clone())))
    }
    fn decode_audio(
        &self,
        _: &Path,
        _: &Path,
        _: Option<u32>,
        _: u64,
        _: &mut dyn FnMut(f64) -> Result<(), String>,
    ) -> Result<motion_core::AudioAsset, String> {
        Err("unused".into())
    }
    fn media_capabilities(&self, _: Option<&motion_host::VideoQuery>) -> Result<Value, String> {
        Ok(serde_json::json!({}))
    }
    fn attach_surface(
        &self,
        window: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        width: u32,
        height: u32,
    ) -> Result<motion_host::SurfaceTarget, String> {
        motion_desktop_media::platform().attach_surface(window, width, height)
    }
}
fn fixture(root: &Path, delayed: bool) {
    std::fs::create_dir_all(root.join("assets")).unwrap();
    std::fs::write(root.join("assets/source.mp4"), b"123").unwrap();
    let asset = VideoAsset {
        id: 1,
        path: "assets/source.mp4".into(),
        bytes: 3,
        mime: "video/avc".into(),
        track: 0,
        width: 2,
        height: 4,
        rotation: 0,
        display_width: 2,
        display_height: 4,
        video_start_us: 0,
        video_end_us: 100_000,
        duration_us: 100_000,
        frame_count: 1,
        variable_frame_rate: false,
        nominal_frame_rate: 10.,
        color_standard: 1,
        color_range: 2,
        audio_asset: None,
    };
    let cache = motion_media::video_cache_path(root, &asset).unwrap();
    std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
    motion_media::save_index(&cache, &asset, &[0]).unwrap();
    let mut project = Project::new(32, 32, 30, 6).unwrap();
    project.video_assets.push(asset);
    let mut layer = Layer::solid(1, "Video", [2., 4.], [16., 16., 0.], [1.; 4]);
    layer.content = Content::Video {
        video: VideoClip::new(1),
    };
    layer.timeline = Some(motion_core::LayerTimeline {
        in_frame: 0,
        out_frame: 3,
        offset_frame: 0,
    });
    if delayed {
        layer.timeline = Some(motion_core::LayerTimeline {
            in_frame: 1,
            out_frame: 4,
            offset_frame: 1,
        });
    }
    project.layers.push(layer);
    motion_core::storage::save(root, &project).unwrap();
}
fn ui() -> UiFrame {
    let mut paint = motion_ui::PaintList::default();
    motion_ui::paint::rect(
        &mut paint,
        motion_ui::Rect::new(0., 0., 8., 64.),
        motion_ui::Color([0.2, 0.4, 0.6, 1.]),
    );
    UiFrame {
        paint,
        atlas: None,
        logical_size: [64., 64.],
        composition_image: Some([-16., -8., 96., 192.]),
        composition: Some([8., 8., 48., 48.]),
        window: 0,
        node_preview: None,
    }
}
fn present(engine: &Engine) -> Result<bool, String> {
    engine
        .present_async(ui())
        .unwrap()
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
}

#[test]
fn pending_video_presents_chrome_without_acknowledging_an_unready_composition() {
    const CHILD: &str = "MOTION_PENDING_VIDEO_CHROME_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let event_loop = winit::event_loop::EventLoop::builder()
            .with_any_thread(true)
            .build()
            .unwrap();
        for (delayed, fail) in [(false, false), (true, false), (true, true)] {
            #[allow(deprecated)]
            let window = Arc::new(
                event_loop
                    .create_window(winit::window::Window::default_attributes().with_visible(false))
                    .unwrap(),
            );
            let directory = root.join(format!("{delayed}-{fail}"));
            fixture(&directory, delayed);
            let (entered, waiting) = channel();
            let gate = Arc::new(Gate {
                released: Mutex::new(false),
                ready: Condvar::new(),
                entered,
                fail,
            });
            let engine =
                Engine::start(directory, Arc::new(Backend(gate.clone())), 8 << 30).unwrap();
            engine
                .attach_async(window, 64, 64)
                .unwrap()
                .recv_timeout(Duration::from_secs(30))
                .unwrap()
                .unwrap();
            if !delayed {
                let before = engine.state().unwrap();
                let mut invalid = ui();
                invalid.composition_image = Some([0.,0.,0.,1.]);
                let error = engine.present_async(invalid).unwrap()
                    .recv_timeout(Duration::from_secs(5)).unwrap().unwrap_err();
                assert!(error.contains("invalid composition canvas image"),"{error}");
                let after = engine.state().unwrap();
                assert_eq!(before["project"],after["project"]);
                assert_eq!(before["revision"],after["revision"]);
            }
            let deadline = Instant::now() + Duration::from_secs(5);
            if delayed {
                while !present(&engine).unwrap() {
                    assert!(Instant::now() < deadline);
                }
                engine.seek(1.).unwrap();
            }
            let before = engine.state().unwrap();
            assert!(!present(&engine).unwrap());
            waiting.recv_timeout(Duration::from_secs(5)).unwrap();
            loop {
                // A drawable surface can arrive after the first hidden-window
                // acquisition. A presented UI must still report composition pending.
                assert!(!present(&engine).unwrap());
                let state = engine.state().unwrap();
                assert_eq!(state["lastPresentedFrame"], before["lastPresentedFrame"]);
                assert_eq!(state["revision"], before["revision"]);
                if state["presented"].as_u64().unwrap() > before["presented"].as_u64().unwrap() {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "no editor chrome was presented while the video decoder was blocked"
                );
            }
            *gate.released.lock().unwrap() = true;
            gate.ready.notify_all();
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                match present(&engine) {
                    Ok(false) => assert!(Instant::now() < deadline),
                    Ok(true) => {
                        assert!(!fail);
                        break;
                    }
                    Err(error) => {
                        assert!(
                            fail && error.contains("controlled first video frame failure"),
                            "{error}"
                        );
                        break;
                    }
                }
            }
            if !fail {
                assert_eq!(
                    engine.state().unwrap()["lastPresentedFrame"],
                    if delayed { 1. } else { 0. }
                );
            }
        }
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","engine::pending_video_tests::pending_video_presents_chrome_without_acknowledging_an_unready_composition"])
        .env(CHILD,root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let error = "controlled first video frame failure";
    assert!(file.contains("logging initialized level=INFO") && file.contains(error));
    assert!(String::from_utf8_lossy(&output.stderr).contains(error));
    assert!(!String::from_utf8_lossy(&output.stdout).contains(error));
    let invalid = "invalid composition canvas image";
    assert!(file.contains(invalid));
    assert!(String::from_utf8_lossy(&output.stderr).contains(invalid));
    assert!(!String::from_utf8_lossy(&output.stdout).contains(invalid));
}
