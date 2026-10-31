//! The actual serial worker invokes the host batch API through its existing
//! test-only operation gateway; the product UI is not activated by this test.
use super::*;
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows};

fn render(id: i64, missing_camera: bool) -> motion_host::Result<Value> {
    motion_host::session::with_session(id, |session| {
        use motion_host::session::viewer::View;
        use motion_render::{multiview::ViewPlacement, ViewCamera};
        let observer = motion_core::Observer::new(32, 32);
        let mut views = [
            ViewCamera::Composition,
            ViewCamera::Observer(&observer),
            ViewCamera::Composition,
            ViewCamera::Observer(&observer),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, camera)| View {
            camera,
            placement: ViewPlacement {
                viewport: [(index % 2) as f32 * 48., (index / 2) as f32 * 48., 48., 48.],
                image: None,
            },
        })
        .collect::<Vec<_>>();
        if missing_camera {
            views[2].camera = ViewCamera::Camera(998);
        }
        let before = session.presented;
        let mut overlays = 0;
        let complete =
            session.render_views_with_overlay(17, 0., &views, |_, _, _| overlays += 1)?;
        Ok(serde_json::json!({"complete":complete,"overlays":overlays,
            "presentedDelta":session.presented-before,"preview":session.preview_info()}))
    })
}
fn healthy(id: i64) -> motion_host::Result<Value> {
    render(id, false)
}
fn invalid(id: i64) -> motion_host::Result<Value> {
    render(id, true)
}
type Gates = (Sender<()>, std::sync::mpsc::Receiver<()>);
static GATES: Mutex<Option<Gates>> = Mutex::new(None);
fn blocked(id: i64) -> motion_host::Result<Value> {
    let (entered, release) = GATES.lock().unwrap().take().unwrap();
    entered
        .send(())
        .map_err(|e| format!("test entry signal: {e}"))?;
    release
        .recv()
        .map_err(|e| format!("test release signal: {e}"))?;
    healthy(id)
}

#[test]
#[cfg_attr(
    not(feature = "gpu-tests"),
    ignore = "requires native Windows surface and GPU"
)]
fn native_worker_group_uses_default_error_channels_and_recovers_without_project_edits() {
    const NAME:&str="engine::multiview_tests::native_worker_group_uses_default_error_channels_and_recovers_without_project_edits";
    const CHILD: &str = "MOTION_MULTIVIEW_WORKER_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let directory = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env(CHILD, directory.path())
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        if let Some(destination) = std::env::var_os("MOTION_MULTIVIEW_WORKER_ARTIFACTS") {
            let destination = PathBuf::from(destination);
            std::fs::create_dir_all(&destination).unwrap();
            for name in ["motion-studio.log", "report.json"] {
                if directory.path().join(name).exists() {
                    std::fs::copy(directory.path().join(name), destination.join(name)).unwrap();
                }
            }
            std::fs::write(destination.join("stdout.log"), &output.stdout).unwrap();
            std::fs::write(destination.join("stderr.log"), &output.stderr).unwrap();
        }
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let log = std::fs::read_to_string(directory.path().join("motion-studio.log")).unwrap();
        assert!(log.contains("logging initialized level=INFO"));
        let errors: Vec<_> = log
            .lines()
            .filter(|line| line.contains("error #") && line.contains("viewer_camera object=998"))
            .collect();
        assert_eq!(errors.len(), 1);
        let error = &errors[0][errors[0].find("error #").unwrap()..];
        assert!(String::from_utf8_lossy(&output.stderr).contains(error));
        assert!(!String::from_utf8_lossy(&output.stdout).contains(error));
        assert!(!log
            .lines()
            .any(|line| line.contains("start #") && line.ends_with(" present")));
        return;
    };
    let root = PathBuf::from(root);
    let _logging = crate::logging::init(root.clone()).unwrap();
    let project_root = root.join("project");
    let mut project = motion_core::Project::new(32, 32, 30, 60).unwrap();
    project.background = [0., 1., 0., 1.];
    motion_core::storage::save(&project_root, &project).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                winit::window::Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(96, 96)),
            )
            .unwrap(),
    );
    let engine = Engine::start(project_root, motion_desktop_media::platform(), 8 << 30).unwrap();
    let pending = engine.attach_async(window, 96, 96).unwrap();
    crate::native_test_support::receive(&mut events, &pending, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let before = engine.state().unwrap();
    let mut last = Value::Null;
    let mut attempts = None;
    for _ in 0..2 {
        let pending = engine
            .request(|reply| Command::TestStudy {
                operation: healthy,
                reply,
            })
            .unwrap();
        last = crate::native_test_support::receive(&mut events, &pending, Duration::from_secs(30))
            .unwrap()
            .unwrap();
        assert_eq!(last["complete"], true);
        assert_eq!(last["overlays"], 1);
        assert_eq!(last["presentedDelta"], 1);
        let current = last["preview"]["video"]["renderAttempts"].as_u64().unwrap();
        if let Some(previous) = attempts {
            assert_eq!(
                current, previous,
                "retained presentation must not count a content render"
            );
        } else {
            assert!(current > 0, "the new content render must be counted");
        }
        attempts = Some(current);
    }
    let (entered_tx, entered_rx) = channel();
    let (release_tx, release_rx) = channel();
    *GATES.lock().unwrap() = Some((entered_tx, release_rx));
    let pending = engine
        .request(|reply| Command::TestStudy {
            operation: blocked,
            reply,
        })
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(30)).unwrap();
    let flight = engine.native_wait_snapshot();
    assert_eq!(flight["in_flight"]["operation"], "test_study");
    let request = flight["in_flight"]["request"].as_u64().unwrap();
    let log = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
    assert!(
        log.contains(&format!("start #{request} test_study")),
        "the default start signal must precede the blocked handler"
    );
    release_tx.send(()).unwrap();
    let blocked_result =
        crate::native_test_support::receive(&mut events, &pending, Duration::from_secs(30))
            .unwrap()
            .unwrap();
    assert_eq!(blocked_result["complete"], true);
    let pending = engine
        .request(|reply| Command::TestStudy {
            operation: invalid,
            reply,
        })
        .unwrap();
    let error = crate::native_test_support::receive(&mut events, &pending, Duration::from_secs(30))
        .unwrap()
        .unwrap_err();
    assert!(error.contains("viewer_camera object=998"));
    let pending = engine
        .request(|reply| Command::TestStudy {
            operation: healthy,
            reply,
        })
        .unwrap();
    let recovered =
        crate::native_test_support::receive(&mut events, &pending, Duration::from_secs(30))
            .unwrap()
            .unwrap();
    assert_eq!(recovered["complete"], true);
    let after = engine.state().unwrap();
    for key in ["project", "revision", "frame", "canUndo", "canRedo"] {
        assert_eq!(before[key], after[key], "group rendering changed {key}");
    }
    drop(engine);
    crate::logging::flush();
    std::fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
        "healthy":last,"recovered":recovered,"error":error,"authoredStateUnchanged":true,
        "blockedHandlerInFlight":flight,
            "gateway":"existing test-only worker operation; product multiview UI still pending"
        }))
        .unwrap(),
    )
    .unwrap();
}
