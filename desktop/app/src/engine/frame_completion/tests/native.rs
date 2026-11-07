use super::*;
use std::{fs, sync::atomic::AtomicUsize};
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows, window::Window};
mod preparation_status;

fn frame(window: u64) -> UiFrame {
    UiFrame {
        views: None,
        paint: Default::default(),
        atlas: Some(AtlasUpload {
            pixels: vec![255; 4],
            width: 1,
            height: 1,
        }),
        logical_size: [64.; 2],
        composition: Some([0., 0., 64., 64.]),
        composition_image: None,
        inspection: Default::default(),
        window,
        node_preview: None,
    }
}
fn receive<T>(
    events: &mut EventLoop<()>,
    reply: &std::sync::mpsc::Receiver<Result<T, Error>>,
) -> T {
    crate::native_test_support::receive(events, reply, Duration::from_secs(30))
        .unwrap()
        .unwrap()
}

#[test]
#[cfg_attr(
    not(feature = "gpu-tests"),
    ignore = "requires native Windows surfaces and GPU"
)]
fn native_pending_frames_complete_without_resampling_or_starving_edits_and_cancel_stale_work() {
    const CASE: &str = "engine::frame_completion::tests::native::native_pending_frames_complete_without_resampling_or_starving_edits_and_cancel_stale_work";
    const CHILD: &str = "MOTION_FRAME_COMPLETION_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let root = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CASE, "--nocapture", "--test-threads=1"])
            .env(CHILD, root.path())
            .env_remove("MOTION_LOG")
            .stdout(fs::File::create(root.path().join("stdout")).unwrap())
            .stderr(fs::File::create(root.path().join("stderr")).unwrap())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(120);
        let exit = loop {
            if let Some(exit) = child.try_wait().unwrap() {
                break exit;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!(
                    "frame completion child did not return: {}",
                    fs::read_to_string(root.path().join("stderr")).unwrap()
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let stdout = fs::read(root.path().join("stdout")).unwrap();
        let stderr = fs::read(root.path().join("stderr")).unwrap();
        if let Some(destination) = std::env::var_os("MOTION_FRAME_COMPLETION_EVIDENCE") {
            fs::create_dir_all(&destination).unwrap();
            for name in ["report.json", "motion-studio.log", "completed.png"] {
                let source = root.path().join(name);
                if source.exists() {
                    fs::copy(source, PathBuf::from(&destination).join(name)).unwrap();
                }
            }
            fs::write(PathBuf::from(&destination).join("stdout"), &stdout).unwrap();
            fs::write(PathBuf::from(&destination).join("stderr"), &stderr).unwrap();
        }
        assert!(
            exit.success(),
            "{}\n{}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
        let log = fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
        assert!(log.contains("logging initialized level=INFO"));
        assert!(!log
            .lines()
            .any(|line| line.contains("start #") && line.ends_with(" present")));
        assert!(!String::from_utf8_lossy(&stdout).contains("pipeline cache start"));
        return;
    };
    let root = PathBuf::from(root);
    let _logging = crate::logging::init(root.clone()).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    let mut report = Vec::new();
    for scenario in ["complete", "edit", "reopen", "shutdown"] {
        let case = root.join(scenario);
        let mut project = motion_core::Project::new(32, 32, 30, 60).unwrap();
        project.layers.push(motion_core::Layer::solid(
            1,
            "Completion",
            [24.; 2],
            [16., 16., 0.],
            [0.8, 0.2, 0.1, 1.],
        ));
        let package = motion_effects::builtin::package().unwrap();
        let effect = package
            .manifest
            .effects
            .iter()
            .find(|effect| effect.id == "invert")
            .unwrap();
        project.layers[0]
            .effects
            .push(motion_core::EffectInstance::new(
                1,
                &package.manifest.id,
                &package.manifest.version,
                &package.hash,
                effect,
                [24.; 2],
            ));
        project.rebuild_plugin_dependencies();
        motion_core::storage::save(&case.join("project"), &project).unwrap();
        let (entered, observed) = channel();
        let (release, gate) = channel();
        let gate = Mutex::new(gate);
        let calls = Arc::new(AtomicUsize::new(0));
        let callback_calls = calls.clone();
        // SAFETY: this test-owned private leaf receives only the native cache
        // tasks; the normal desktop validator establishes its OS protection.
        let storage = unsafe {
            motion_host::session::DriverPipelineCacheStorage::new(
                case.join("private/driver-pipeline-cache-v1"),
                Arc::new(move |path| {
                    if callback_calls.fetch_add(1, Ordering::AcqRel) == 0 {
                        entered.send(()).map_err(std::io::Error::other)?;
                        gate.lock()
                            .map_err(|_| std::io::Error::other("fixture gate poisoned"))?
                            .recv()
                            .map_err(std::io::Error::other)?;
                    }
                    crate::pipeline_cache::prepare_test_storage(path)
                }),
            )
        };
        let engine = Engine::start_with_pipeline_cache(
            case.join("project"),
            motion_desktop_media::platform(),
            8 << 30,
            Some(storage),
        )
        .unwrap();
        #[allow(deprecated)]
        let main = Arc::new(
            events
                .create_window(
                    Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(64, 64)),
                )
                .unwrap(),
        );
        #[allow(deprecated)]
        let floating = Arc::new(
            events
                .create_window(
                    Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(64, 64)),
                )
                .unwrap(),
        );
        receive(&mut events, &engine.attach_async(main, 64, 64).unwrap());
        engine.float_window(7, floating.clone(), 64, 64).unwrap();
        let supported = engine.call(|reply|Command::TestStudy { operation: |id| motion_host::session::with_session(id, |s|
            Ok(serde_json::json!({"supported":s.graphics.as_ref().unwrap().renderer.driver_pipeline_cache_info().supported}))), reply }).unwrap()["supported"] == true;
        let first = engine.present_async(frame(0)).unwrap();
        if !supported {
            assert!(receive(&mut events, &first));
            report.push(serde_json::json!({"supported":false,"ordinaryFirstPresentCompleted":true,"blockedRestoreQualified":false}));
            break;
        }
        observed.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            matches!(first.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty)),
            "first Present replied before preparation completed"
        );
        let second = engine.present_async(frame(7)).unwrap();
        let rendered = engine
            .request(|reply| Command::Render { frame: 0., reply })
            .unwrap();
        assert!(
            !receive(
                &mut events,
                &engine.present_poll_async(frame(0), false).unwrap()
            ),
            "Shell polling must yield while the composition is preparing"
        );
        let mut chrome = frame(0);
        chrome.composition = None;
        assert!(
            receive(&mut events, &engine.present_async(chrome).unwrap()),
            "chrome-only presentation was queued behind pipeline preparation"
        );
        assert!(matches!(
            first.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        let before = engine.preview_info().unwrap()["video"]["renderAttempts"].clone();
        for _ in 0..65 {
            assert!(engine.preview_info().unwrap()["pipelinePreparation"]["preparing"] == true);
            let state = engine.state().unwrap();
            assert_eq!(state["desktop"]["frameCompletions"]["pending"], 3);
        }
        assert_eq!(
            engine.preview_info().unwrap()["video"]["renderAttempts"],
            before,
            "waiting polls sampled or rendered the project again"
        );
        assert_eq!(
            calls.load(Ordering::Acquire),
            1,
            "blocked cache restore was restarted"
        );
        if scenario == "shutdown" {
            engine.request_shutdown().unwrap();
            assert!(!receive(&mut events, &first));
            assert!(!receive(&mut events, &second));
            assert!(!receive(&mut events, &rendered));
            release.send(()).unwrap();
            report.push(serde_json::json!({"supported":true,"scenario":scenario,"cancelledBeforeWorkerRelease":true}));
            continue;
        } else if scenario == "reopen" {
            engine.close_window(7).unwrap();
            engine.float_window(7, floating, 64, 64).unwrap();
            assert!(
                !receive(&mut events, &second),
                "closed owner received a stale frame after key reuse"
            );
            assert!(matches!(
                first.try_recv(),
                Err(std::sync::mpsc::TryRecvError::Empty)
            ));
            let replacement = engine.present_async(frame(7)).unwrap();
            release.send(()).unwrap();
            assert!(receive(&mut events, &first));
            assert!(receive(&mut events, &rendered));
            assert!(receive(&mut events, &replacement));
        } else if scenario == "edit" {
            engine
                .edit(
                    serde_json::json!({"op":"rename","object":1,"name":"Edited while preparing"})
                        .to_string(),
                )
                .unwrap();
            assert!(
                !receive(&mut events, &first),
                "stale main frame was presented after edit"
            );
            assert!(
                !receive(&mut events, &second),
                "stale floating frame was presented after edit"
            );
            assert!(
                !receive(&mut events, &rendered),
                "stale Render was completed after edit"
            );
            assert_eq!(
                engine.state().unwrap()["project"]["layers"][0]["name"],
                "Edited while preparing"
            );
            let latest = engine.present_async(frame(0)).unwrap();
            release.send(()).unwrap();
            assert!(receive(&mut events, &latest));
        } else {
            release.send(()).unwrap();
            assert!(
                receive(&mut events, &first),
                "original Present must finish its composition"
            );
            assert!(
                receive(&mut events, &second),
                "floating Present must not cancel the main window preparation"
            );
            assert!(
                receive(&mut events, &rendered),
                "Render must wait for the original frame preparation"
            );
            crate::native_test_support::capture_ui(
                &mut events,
                &engine,
                frame(7),
                root.join("completed.png"),
            )
            .unwrap();
            let image = image::open(root.join("completed.png")).unwrap().to_rgba8();
            let pixel = image.get_pixel(32, 32);
            assert!(
                pixel[1] > 100 && pixel[1] > pixel[0],
                "completed reply had no inverted authored composition pixels: {pixel:?}"
            );
        }
        let state = engine.state().unwrap();
        assert_eq!(state["desktop"]["frameCompletions"]["pending"], 0);
        report.push(serde_json::json!({"supported":true,"scenario":scenario,"beforeAttempts":before,
            "after":engine.preview_info().unwrap(),"completion":state["desktop"]["frameCompletions"]}));
    }
    fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    crate::logging::flush();
}
