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
        check: &dyn Fn() -> Result<(), motion_media::Error>,
    ) -> Result<DecodedFrame, motion_media::Error> {
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
    fn media_capabilities(&self, _: Option<&motion_host::VideoQuery>) -> Result<Value, motion_media::Error> {
        Ok(serde_json::json!({}))
    }
    fn attach_surface(
        &self,
        window: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        width: u32,
        height: u32,
        _budget: motion_render::AllocationBudget,
    ) -> motion_host::Result<motion_host::SurfaceTarget> {
        motion_desktop_media::platform().attach_surface(window, width, height, _budget)
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
        inspection: Default::default(),
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
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn png_export_freezes_frame_without_blocking_engine_and_observes_failure_and_cancel() {
    const CHILD: &str = "MOTION_PNG_EXPORT_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        for (blur, fail, cancel) in [(false, false, false), (true, false, false), (true, true, false), (true, false, true)] {
            let directory = root.join(format!("png-{blur}-{fail}-{cancel}"));
            fixture(&directory, false);
            if blur {
                let mut project = motion_core::storage::load(&directory).unwrap();
                project.version = motion_model::CURRENT_PROJECT_VERSION;
                project.layers[0].sampling.motion_blur = Some(motion_core::render_sampling::Shutter { frames:0.5, samples:8 });
                motion_core::storage::save(&directory, &project).unwrap();
            }
            let (entered, waiting) = channel();
            let gate = Arc::new(Gate { released:Mutex::new(false), ready:Condvar::new(), entered, fail });
            let engine = Engine::start(directory.clone(), Arc::new(Backend(gate.clone())), 8 << 30).unwrap();
            engine.seek(1.).unwrap();
            let before = engine.state().unwrap();
            let output = directory.join("previous.png");
            std::fs::write(&output, b"previous file").unwrap();
            let control = motion_desktop_media::movie::ExportControl::default();
            let capture = engine.capture_async(output.clone(), control.clone()).unwrap();
            waiting.recv_timeout(Duration::from_secs(30)).unwrap();
            let flight = engine.frame_capture_in_flight().expect("publish in-flight before decoder enters");
            let peaks=engine.allocation_frame_peaks().unwrap();
            let window=peaks.active.iter().find(|w|w.operation==motion_render::FrameOperation::Export).unwrap();
            assert_eq!(window.request,flight.0);
            assert_eq!(window.source_frame,Some(1.));
            assert!(window.outcome.is_none());
            assert!(window.peak_admitted_bytes>0);
            let log = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
            assert!(log.contains(&format!("async start #{} capture", flight.0)));
            let pending = engine.request(Command::State).unwrap().recv_timeout(Duration::from_secs(2)).unwrap().unwrap();
            assert_eq!(pending["desktop"]["frameExport"]["request"], flight.0);
            assert!(!pending["desktop"]["frameExport"]["workerFinished"].as_bool().unwrap());
            assert_eq!(pending["lastPresentedFrame"], before["lastPresentedFrame"]);
            let seek = engine.request(|reply| Command::Seek { frame:1.5, reply }).unwrap();
            seek.recv_timeout(Duration::from_secs(2)).unwrap().unwrap();
            assert!(capture.try_recv().is_err(), "blocked decoder must not publish a partial PNG");
            let duplicate = engine.capture_async(directory.join("duplicate.png"), Default::default()).unwrap();
            assert!(duplicate.recv_timeout(Duration::from_secs(2)).unwrap().unwrap_err().contains("still running"));
            if cancel {
                assert!(control.cancel());
                std::thread::sleep(Duration::from_millis(40));
                assert!(capture.try_recv().is_err(), "retain export slot until native decoder closes");
                assert!(engine.frame_capture_in_flight().is_some());
            }
            *gate.released.lock().unwrap() = true;
            gate.ready.notify_all();
            let result = capture.recv_timeout(Duration::from_secs(30)).unwrap();
            if cancel {
                assert_eq!(result.unwrap_err(), "PNG export cancelled");
            } else if fail {
                let error = result.unwrap_err();
                assert!(error.contains("controlled first video frame failure") && error.contains("PNG export revision=") && error.contains("frame=1"), "{error}");
            } else {
                let result = result.unwrap();
                assert_eq!(result["frame"], 1.);
                let pixels = image::open(&output).unwrap().into_rgba8();
                assert_eq!(pixels.dimensions(), (32,32));
                assert_eq!(pixels.get_pixel(16,16).0, [255;4]);
                assert!(!control.cancel());
            }
            if fail || cancel { assert_eq!(std::fs::read(&output).unwrap(), b"previous file"); }
            assert!(!directory.join("duplicate.png").exists());
            assert_eq!(engine.state().unwrap()["frame"], 1.5);
            let deadline = Instant::now() + Duration::from_secs(2);
            while engine.frame_capture_in_flight().is_some() {
                assert!(Instant::now() < deadline);
                std::thread::yield_now();
            }
            assert!(engine.state().unwrap()["desktop"]["frameExport"].is_null());
            let cancelled = motion_desktop_media::movie::ExportControl::default();
            assert!(cancelled.cancel());
            let reply = engine.capture_async(directory.join("cancelled-before-start.png"), cancelled).unwrap();
            assert_eq!(reply.recv_timeout(Duration::from_secs(2)).unwrap().unwrap_err(), "PNG export cancelled");
            assert!(!directory.join("cancelled-before-start.png").exists());
        }
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "engine::pending_video_tests::png_export_freezes_frame_without_blocking_engine_and_observes_failure_and_cancel"])
        .env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    let log = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for cause in ["controlled first video frame failure", "PNG export revision=", "PNG export is still running"] {
        assert!(log.contains(cause) && stderr.contains(cause) && !stdout.contains(cause), "{cause}");
    }
    assert!(log.contains("logging initialized level=INFO") && log.contains("cancelled #"));
    assert!(!stderr.contains("PNG export cancelled"));
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
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
                for viewport in [[-1.,0.,8.,8.],[0.,0.,0.,8.],[60.,0.,8.,8.],[f32::NAN,0.,8.,8.]] {
                    let mut invalid = ui();
                    invalid.composition = Some(viewport);
                    let error = engine.present_async(invalid).unwrap()
                        .recv_timeout(Duration::from_secs(5)).unwrap().unwrap_err();
                    for context in ["invalid composition viewport", "viewport=", "surface=64x64", "phase=render", "window=0", "logical_size=[64.0, 64.0]"] {
                        assert!(error.contains(context), "missing {context}: {error}");
                    }
                }
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
    for context in ["invalid composition viewport", "surface=64x64", "window=0", "phase=render"] {
        assert!(file.contains(context));
        assert!(String::from_utf8_lossy(&output.stderr).contains(context));
        assert!(!String::from_utf8_lossy(&output.stdout).contains(context));
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn exposure_preview_keeps_progress_across_native_windows_and_cancels_on_explicit_seek() {
    const CHILD:&str="MOTION_NATIVE_EXPOSURE_PREVIEW_CHILD";
    if let Some(root)=std::env::var_os(CHILD) {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let root=PathBuf::from(root);
        let _logger=crate::logging::init(root.clone()).unwrap();
        let event_loop=winit::event_loop::EventLoop::builder().with_any_thread(true).build().unwrap();
        for (floating,fail) in [(false,false),(true,false),(false,true),(true,true)] {
            #[allow(deprecated)]
            let window=Arc::new(event_loop.create_window(winit::window::Window::default_attributes().with_visible(false)).unwrap());
            let directory=root.join(format!("exposure-{floating}-{fail}"));
            fixture(&directory,false);
            let mut project=motion_core::storage::load(&directory).unwrap();
            project.version = motion_model::CURRENT_PROJECT_VERSION;
            project.layers[0].sampling.motion_blur=Some(motion_core::render_sampling::Shutter {frames:0.5,samples:4});
            for (frame,x) in [(0,8.),(2,24.)] {project.layers[0].transform.position.upsert(frame,[x,16.,0.],motion_core::Ease::Linear).unwrap();}
            motion_core::storage::save(&directory,&project).unwrap();
            let (entered,waiting)=channel();
            let gate=Arc::new(Gate {released:Mutex::new(false),ready:Condvar::new(),entered,fail});
            let engine=Engine::start(directory,Arc::new(Backend(gate.clone())),8<<30).unwrap();
            assert_eq!(engine.state().unwrap()["preview"]["exposureRequired"],true);
            engine.attach_async(window,64,64).unwrap().recv_timeout(Duration::from_secs(30)).unwrap().unwrap();
            assert_eq!(engine.state().unwrap()["preview"]["exposureRequired"],true);
            let key=if floating {
                #[allow(deprecated)]
                let window=Arc::new(event_loop.create_window(winit::window::Window::default_attributes().with_visible(false)).unwrap());
                engine.float_window(11,window,64,64).unwrap();11
            } else {0};
            let draw=|| {let mut frame=ui();frame.window=key;engine.present_async(frame).unwrap().recv_timeout(Duration::from_secs(10)).unwrap()};
            let state=engine.seek(1.).unwrap();
            assert_eq!(state["preview"]["exposureRequired"],true);
            let before=engine.state().unwrap();
            assert!(!draw().unwrap());
            waiting.recv_timeout(Duration::from_secs(5)).unwrap();
            let pending=engine.state().unwrap();
            assert_eq!(pending["preview"]["exposure"]["state"],"pending");
            assert!(pending["preview"]["memoryBreakdown"]["exposureTargetBytes"].as_u64().unwrap()>0);
            let flight=pending["preview"]["video"]["decodeInFlight"].clone();
            for _ in 0..4 {
                assert!(!draw().unwrap());
                if floating {
                    let mut chrome=ui();chrome.composition=None;chrome.composition_image=None;
                    engine.present_async(chrome).unwrap().recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
                }
                let state=engine.state().unwrap();
                assert_eq!(state["lastPresentedFrame"],before["lastPresentedFrame"]);
                assert_eq!(state["preview"]["exposure"]["workUnits"],pending["preview"]["exposure"]["workUnits"]);
                assert_eq!(state["project"],before["project"]);
            }
            // Actual decoder state is published before the blocking native call.
            let logs=std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
            assert!(logs.contains("video decoder open start object=1 source=1"));
            assert_eq!(flight.as_array().unwrap().len(),1);
            assert_eq!(flight[0]["object"],1);assert_eq!(flight[0]["source"],1);
            assert_eq!(flight[0]["pts_us"],0);assert_eq!(flight[0]["phase"],"decode");
            let expected=if fail {1.} else {
                let changed=engine.seek(1.25).unwrap();
                assert!(changed["preview"]["exposure"].is_null());
                assert_eq!(changed["preview"]["memoryBreakdown"]["exposureTargetBytes"],0);
                1.25
            };
            *gate.released.lock().unwrap()=true;gate.ready.notify_all();
            let deadline=Instant::now()+Duration::from_secs(10);
            loop {
                match draw() {
                    Ok(false)=>assert!(Instant::now()<deadline,"{}",engine.state().unwrap()["preview"]),
                    Ok(true)=>{assert!(!fail);break;},
                    Err(error)=>{
                        assert!(fail && error.contains("controlled first video frame failure") && error.contains("preview exposure") && error.contains("media_prepare"),"{error}");
                        let failed=engine.state().unwrap();assert_eq!(failed["preview"]["exposure"]["state"],"failed");
                        let cpu=failed["preview"]["exposure"]["cpuPollUs"].clone();
                        assert!(draw().unwrap_err().contains("controlled first video frame failure"));
                        assert_eq!(engine.state().unwrap()["preview"]["exposure"]["cpuPollUs"],cpu);
                        break;
                    }
                }
            }
            if !fail {
                let complete=engine.state().unwrap();
                assert_eq!(complete["lastPresentedFrame"],expected);
                assert_eq!(complete["preview"]["exposure"]["state"],"ready");
                // Read the actual committed preview through the same screenshot
                // presenter used by desktop acceptance, at one output pixel per
                // composition pixel. The independent oracle uses four ordinary
                // scenes with an authored constant white source.
                let path=root.join(format!("native-exposure-{floating}.png"));
                engine.capture_ui(UiFrame {paint:Default::default(),atlas:Some(AtlasUpload {pixels:vec![255;4],width:1,height:1}),
                    logical_size:[64.,64.],composition:Some([0.,0.,32.,32.]),inspection: Default::default(), composition_image:None,window:key,node_preview:None},path.clone()).unwrap();
                let screenshot=image::open(&path).unwrap().to_rgba8();
                let actual=image::imageops::crop_imm(&screenshot,0,0,32,32).to_image().into_raw();
                let mut reference=project.clone();reference.layers[0].content=Content::Solid {color:[1.;4]};
                let mut renderer=pollster::block_on(motion_render::Renderer::headless()).unwrap();
                let mut target=renderer.exposure_target(32,32,&[0.25;4]).unwrap();
                for offset in [-0.1875,-0.0625,0.0625,0.1875] {
                    let mut scene=motion_render::Scene::new(&reference).unwrap();
                    scene.sample(&reference,expected+offset,None,&motion_core::ExpressionEvaluator).unwrap();
                    renderer.accumulate_exposure(&mut target,Some(&scene),true).unwrap();
                }
                let oracle=renderer.capture_exposure(&target).unwrap();
                let difference=actual.iter().zip(&oracle).map(|(a,b)|a.abs_diff(*b)).max().unwrap();
                assert!(difference<=1,"native preview pixels differ from ordinary-scene oracle: {difference}");
                if let Some(output)=std::env::var_os("MOTION_EXPOSURE_NATIVE_ARTIFACTS") {
                    let output=PathBuf::from(output);std::fs::create_dir_all(&output).unwrap();
                    image::save_buffer(output.join(format!("preview-{floating}.png")),&actual,32,32,image::ColorType::Rgba8).unwrap();
                    std::fs::write(output.join(format!("preview-{floating}.json")),serde_json::to_vec_pretty(&complete["preview"]).unwrap()).unwrap();
                }
                engine.seek(1.5).unwrap();
                while !draw().unwrap() {assert!(Instant::now()<deadline);}
                assert_eq!(engine.state().unwrap()["lastPresentedFrame"],1.5);
            }
            if floating {engine.close_window(key).unwrap();}
        }
        return;
    }
    let root=tempfile::tempdir().unwrap();
    let output=std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact","engine::pending_video_tests::exposure_preview_keeps_progress_across_native_windows_and_cancels_on_explicit_seek"])
        .env(CHILD,root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(output.status.success(),"{}\n{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
    let logs=std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr=String::from_utf8_lossy(&output.stderr);let stdout=String::from_utf8_lossy(&output.stdout);
    for marker in ["controlled first video frame failure","preview exposure","phase=media_prepare"] {
        assert!(logs.contains(marker));assert!(stderr.contains(marker));assert!(!stdout.contains(marker));
    }
    assert!(logs.contains("logging initialized level=INFO"));
    assert!(!stderr.contains("video decoder open start"));
}

mod prerender_logging;
mod prerender_disk_logging;

mod prerender_runtime;
