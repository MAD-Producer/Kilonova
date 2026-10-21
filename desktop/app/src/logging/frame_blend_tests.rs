//! Exercise the host's required-right failure through the real desktop log sink.
use super::*;
use motion_core::{render_sampling::FrameBlend, Content, Layer, Project, VideoAsset, VideoClip};
use motion_host::{
    video_frame::{DecodedFrame, VideoPixels},
    video_frames::VideoFrames,
    Platform, VideoDecoder,
};
use motion_render::Scene;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Instant,
};

struct Gate {
    entered: mpsc::Sender<()>,
    released: Mutex<mpsc::Receiver<()>>,
    fail: AtomicBool,
}
struct Backend(Arc<Gate>);
struct Decoder(Arc<Gate>);
impl VideoDecoder for Decoder {
    fn frame(
        &mut self,
        time: u64,
        check: &dyn Fn() -> Result<(), motion_media::Error>,
    ) -> Result<DecodedFrame, motion_media::Error> {
        check()?;
        if time == 33333 {
            self.0.entered.send(()).map_err(|e| e.to_string())?;
            self.0
                .released
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .map_err(|e| e.to_string())?;
            check()?;
            if self.0.fail.load(Ordering::Acquire) {
                return Err("controlled adjacent-frame cause".into());
            }
        }
        Ok(DecodedFrame {
            pixels: VideoPixels::Rgba(vec![255; 16]),
            pts: time,
            end: if time == 0 { 33333 } else { 66666 },
            width: 2,
            height: 2,
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
        "adjacent-frame-observation"
    }
    fn probe_video(
        &self,
        _: &Path,
        _: Option<u32>,
        _: Option<u32>,
        _: &dyn Fn() -> Result<(), motion_media::Error>,
    ) -> Result<motion_media::VideoProbe, motion_media::Error> {
        Err("unused".into())
    }
    fn open_decoder(
        &self,
        _: &Path,
        _: VideoAsset,
        _: Vec<u64>,
    ) -> Result<Box<dyn VideoDecoder>, motion_media::Error> {
        Ok(Box::new(Decoder(self.0.clone())))
    }
    fn decode_audio(
        &self,
        _: &Path,
        _: &Path,
        _: Option<u32>,
        _: u64,
        _: &mut dyn FnMut(Option<f64>) -> Result<(), motion_media::Error>,
    ) -> Result<motion_core::AudioAsset, motion_media::Error> {
        Err("unused".into())
    }
    fn media_capabilities(
        &self,
        _: Option<&motion_host::VideoQuery>,
    ) -> Result<serde_json::Value, motion_media::Error> {
        Err("unused".into())
    }
    fn attach_surface(
        &self,
        _: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        _: u32,
        _: u32,
    ) -> motion_host::Result<motion_host::SurfaceTarget> {
        Err("unused".into())
    }
}
fn wait(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn fixture(root: &Path) -> Project {
    fs::create_dir_all(root.join("assets")).unwrap();
    fs::write(root.join("assets/controlled.mp4"), b"1234").unwrap();
    let asset: VideoAsset = serde_json::from_value(serde_json::json!({
        "id":1,"path":"assets/controlled.mp4","bytes":4,"mime":"video/avc","track":0,
        "width":2,"height":2,"rotation":0,"display_width":2,"display_height":2,
        "video_start_us":0,"video_end_us":66666,"duration_us":66666,
        "frame_count":2,"variable_frame_rate":true,"nominal_frame_rate":30.,
        "color_standard":1,"color_range":2,"audio_asset":null
    }))
    .unwrap();
    let path = motion_media::video_cache_path(root, &asset).unwrap();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    motion_media::save_index(&path, &asset, &[0, 33333]).unwrap();
    let mut p = Project::new(32, 32, 30, 2).unwrap();
    p.version = motion_model::CURRENT_PROJECT_VERSION;
    p.video_assets.push(asset);
    let mut layer = Layer::solid(7, "Controlled", [2.; 2], [16., 16., 0.], [1.; 4]);
    layer.content = Content::Video {
        video: VideoClip::new(1),
    };
    layer.sampling.frame_blend = FrameBlend::Linear;
    p.layers.push(layer);
    p.validate().unwrap();
    p
}

#[test]
fn required_right_reports_default_file_stderr_and_pending_threshold_recovery() {
    const CHILD: &str = "MOTION_FRAME_BLEND_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = init(root.clone()).unwrap();
        let project = fixture(&root);
        let mut scene = Scene::new(&project);
        scene
            .sample(&project, 0.5, None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let (entered, waiting) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let gate = Arc::new(Gate {
            entered,
            released: Mutex::new(released),
            fail: AtomicBool::new(true),
        });
        let mut frames = VideoFrames::new(Arc::new(Backend(gate.clone())));
        let path = root.join("motion-studio.log");
        let marker = "persistent video pending object=7 source=1 ";
        for episode in 0..2 {
            assert!(frames
                .prepare_scene_exact(&project, &root, &scene)
                .unwrap()
                .is_none());
            waiting.recv_timeout(Duration::from_secs(5)).unwrap();
            let metrics = frames.metrics();
            assert_eq!(metrics["decodeInFlight"][0]["pts_us"], 33333);
            assert_eq!(metrics["decodeInFlight"][0]["phase"], "decode");
            assert_eq!(metrics["decodeInFlight"][0]["speculative"], false);
            assert!(fs::read_to_string(&path)
                .unwrap()
                .contains("video decoder open start object=7 source=1"));
            for _ in 0..120 {
                assert!(frames
                    .prepare_scene_exact(&project, &root, &scene)
                    .unwrap()
                    .is_none());
            }
            let logs = fs::read_to_string(&path).unwrap();
            assert_eq!(logs.matches(marker).count(), episode + 1);
            assert!(logs.contains("count=60 phase=decode"));
            assert!(logs.contains("active_pts_us=Some(33333)"));
            release.send(()).unwrap();
            if episode == 0 {
                wait(|| {
                    fs::read_to_string(&path)
                        .unwrap()
                        .contains("reason=controlled adjacent-frame cause")
                });
                let error = frames
                    .prepare_scene_exact(&project, &root, &scene)
                    .err()
                    .unwrap();
                assert_eq!(error.to_string(), "controlled adjacent-frame cause");
                assert_eq!(frames.metrics()["prefetchFailures"], 0);
                // No automatic retry: changing the user's requirement to Hold
                // and back explicitly requests the right frame again.
                let mut hold = scene.clone();
                hold.layers[0].sampling.frame_blend = FrameBlend::Hold;
                assert!(frames
                    .prepare_scene_exact(&project, &root, &hold)
                    .unwrap()
                    .is_some());
                gate.fail.store(false, Ordering::Release);
            } else {
                wait(|| {
                    frames
                        .prepare_scene_exact(&project, &root, &scene)
                        .unwrap()
                        .is_some()
                });
                for _ in 0..120 {
                    assert!(frames
                        .prepare_scene_exact(&project, &root, &scene)
                        .unwrap()
                        .is_some());
                }
            }
        }
        drop(frames);
        flush();
        let logs = fs::read_to_string(path).unwrap();
        assert_eq!(
            logs.matches("video decode failed object=7 source=1 ")
                .count(),
            1
        );
        assert!(logs.contains("pts_us=33333") && logs.contains("phase=decode speculative=false"));
        assert!(logs.contains("requested_pts_us=0 discarded=false"));
        assert!(!logs.contains("DEBUG") && !logs.contains("video decode done"));
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact","logging::frame_blend_tests::required_right_reports_default_file_stderr_and_pending_threshold_recovery"])
        .env(CHILD,root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(!stdout.contains("video pending") && !stdout.contains("video decode failed"));
    assert_eq!(
        stderr
            .matches("persistent video pending object=7 source=1 ")
            .count(),
        2
    );
    assert_eq!(
        stderr
            .matches("video decode failed object=7 source=1 ")
            .count(),
        1
    );
    assert!(stderr.contains("reason=controlled adjacent-frame cause"));
    assert!(!stderr.contains("video decoder open start"));
}

#[test]
fn persistent_exposure_media_uses_default_decoder_logs_inflight_and_keeps_failure_terminal() {
    use motion_host::{preview_video::PreviewVideoCache,render_sampling::{ExposureMedia,ExposureMediaPolicy}};
    use motion_render::{exposure_scene::{ExposureDocument,ExposureScenes},ExposureFrame,ExposurePoll,ExposureStage,Renderer};
    const CHILD:&str = "MOTION_EXPOSURE_FRAME_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = init(root.clone()).unwrap();
        let mut project = fixture(&root);
        project.layers[0].sampling.motion_blur = Some(motion_core::render_sampling::Shutter {frames:0.5,samples:2});
        let (entered,waiting) = mpsc::channel();
        let (release,released) = mpsc::channel();
        let gate = Arc::new(Gate {entered,released:Mutex::new(released),fail:AtomicBool::new(true)});
        let mut videos = VideoFrames::new(Arc::new(Backend(gate)));
        let mut proxies = PreviewVideoCache::default();
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        let document = ExposureDocument::new(Arc::new(project.clone())).unwrap();
        let scenes = ExposureScenes::new(document,&project.composition_id,0.5,false,None,motion_core::ExpressionEvaluator).unwrap();
        let mut task = ExposureFrame::new(&mut renderer,scenes,32,32).unwrap();
        let mut tick = |task:&mut ExposureFrame<motion_core::ExpressionEvaluator>,videos:&mut VideoFrames| {
            let mut media = ExposureMedia {project:&project,root:&root,videos,proxies:&mut proxies,policy:ExposureMediaPolicy::FULL};
            task.poll(&mut renderer,Duration::from_millis(8),|renderer,scene|media.prepare(renderer,scene))
        };
        assert!(matches!(tick(&mut task,&mut videos).unwrap(),ExposurePoll::Pending(_)));
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        let path = root.join("motion-studio.log");
        let logs = fs::read_to_string(&path).unwrap();
        assert!(logs.contains("video decoder open start object=7 source=1"));
        assert!(logs.contains("video decoder open start object=7 source=1 sequence=1 pts_us=0"));
        assert_eq!(logs.matches("video decoder open start object=7 source=1 ").count(),1);
        let flight = videos.metrics();
        assert_eq!(flight["decodeInFlight"][0]["pts_us"],33333);
        assert_eq!(flight["decodeInFlight"][0]["phase"],"decode");
        assert_eq!(flight["decodeInFlight"][0]["speculative"],false);
        assert_eq!(flight["decodeInFlight"][0]["sequence"],1);
        for _ in 0..120 {
            assert!(matches!(tick(&mut task,&mut videos).unwrap(),ExposurePoll::Pending(_)));
            assert_eq!(task.target().phase_index(),0);
        }
        let logs = fs::read_to_string(&path).unwrap();
        assert_eq!(logs.matches("persistent video pending object=7 source=1 ").count(),1);
        assert!(logs.contains("count=60 phase=decode"));
        release.send(()).unwrap();
        wait(||fs::read_to_string(&path).unwrap().contains("reason=controlled adjacent-frame cause"));
        let error = tick(&mut task,&mut videos).unwrap_err().to_string();
        assert!(error.contains("controlled adjacent-frame cause") && error.contains("media_prepare") && error.contains("sample_frame"),"{error}");
        assert_eq!(task.progress().stage,ExposureStage::Failed);
        for _ in 0..60 {
            assert!(tick(&mut task,&mut videos).unwrap_err().to_string().contains("controlled adjacent-frame cause"));
        }
        drop(tick);
        assert!(renderer.capture_exposure(task.target()).is_err());
        drop(task);
        assert_eq!(renderer.exposure_texture_bytes(),0);
        drop(videos);
        flush();
        let logs = fs::read_to_string(path).unwrap();
        assert_eq!(logs.matches("video decode failed object=7 source=1 ").count(),1);
        assert_eq!(logs.matches("persistent video pending object=7 source=1 ").count(),1);
        assert!(!logs.contains("DEBUG") && !logs.contains("video decode done"));
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact","logging::frame_blend_tests::persistent_exposure_media_uses_default_decoder_logs_inflight_and_keeps_failure_terminal"])
        .env(CHILD,root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(output.status.success(),"{}\n{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(!stdout.contains("persistent video pending") && !stdout.contains("video decode failed"));
    assert_eq!(stderr.matches("persistent video pending object=7 source=1 ").count(),1);
    assert_eq!(stderr.matches("video decode failed object=7 source=1 ").count(),1);
    assert!(stderr.contains("reason=controlled adjacent-frame cause"));
    assert!(!stderr.contains("video decode start") && !stderr.contains("video decoder open start"));
}
