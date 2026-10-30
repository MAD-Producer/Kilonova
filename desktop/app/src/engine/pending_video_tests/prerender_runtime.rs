//! Actual blocked decoder remains independent of a blocked serial Engine.
use super::*;
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows};

#[test]
#[ignore = "explicit controlled native decoder/Engine cancellation ownership proof"]
fn blocked_prerender_is_observable_and_canceled_before_a_foreground_handler_runs() {
    const NAME: &str = "engine::pending_video_tests::prerender_runtime::blocked_prerender_is_observable_and_canceled_before_a_foreground_handler_runs";
    const CHILD: &str = "MOTION_PRERENDER_RUNTIME_BLOCKED_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                NAME,
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, root.path())
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        if let Some(destination) = std::env::var_os("MOTION_PRERENDER_BLOCKED_ARTIFACTS") {
            std::fs::create_dir_all(&destination).unwrap();
            for name in ["report.json", "motion-studio.log"] {
                let source = root.path().join(name);
                if source.exists() {
                    std::fs::copy(source, PathBuf::from(&destination).join(name)).unwrap();
                }
            }
            std::fs::write(
                PathBuf::from(&destination).join("stdout.log"),
                &output.stdout,
            )
            .unwrap();
            std::fs::write(
                PathBuf::from(&destination).join("stderr.log"),
                &output.stderr,
            )
            .unwrap();
        }
        assert!(
            output.status.success(),
            "status={}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.clone()).unwrap();
    let project = root.join("project");
    fixture(&project, false);
    let (entered, waiting) = channel();
    let gate = Arc::new(Gate {
        released: Mutex::new(false),
        ready: Condvar::new(),
        entered,
        fail: false,
    });
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                winit::window::Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(64, 64)),
            )
            .unwrap(),
    );
    let engine = Engine::start(project, Arc::new(Backend(gate.clone())), 8 << 30).unwrap();
    let attached = engine.attach_async(window, 64, 64).unwrap();
    crate::native_test_support::receive(&mut events, &attached, Duration::from_secs(60))
        .unwrap()
        .unwrap();
    engine
        .call(|reply| Command::TestStudy {
            operation: |id| {
                motion_host::session::with_session(id, |s| {
                    s.preview.set_mode(motion_render::PreviewMode::High);
                    Ok(serde_json::json!({}))
                })
            },
            reply,
        })
        .unwrap();
    engine
        .configure_prerender(motion_host::prerender::Policy {
            enabled: true,
            window: 1,
            ..Default::default()
        })
        .unwrap();
    crate::native_test_support::receive(&mut events, &waiting, Duration::from_secs(60)).unwrap();
    let native = engine.prerender_observation().unwrap().unwrap();
    let flight = native.in_flight.unwrap();
    assert_eq!((flight.operation, flight.phase), ("render", "video"));
    assert!(native.retained_cpu_bytes > 0);
    assert!(engine.state().unwrap()["desktop"]["prerender"]["fault"].is_null());

    let (serial_entered, serial_waiting) = channel();
    let (release, blocked) = channel();
    let serial = engine
        .request(|reply| Command::TestWait {
            entered: serial_entered,
            release: blocked,
            reply,
        })
        .unwrap();
    serial_waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(engine.in_flight().unwrap().0, "test_wait");
    // A queued State could not answer here; this independent API must answer.
    let independent = engine.prerender_observation().unwrap().unwrap();
    assert_eq!(independent.in_flight.unwrap().id, flight.id);
    let seek = engine.seek_async(2.).unwrap();
    let disabled = engine
        .request(|reply| Command::ConfigurePrerender {
            policy: Default::default(),
            reply,
        })
        .unwrap();
    assert!(engine.queue_depth() >= 2);
    // Dispatch already canceled the native control, before Seek can execute.
    *gate.released.lock().unwrap() = true;
    gate.ready.notify_all();
    release.send(()).unwrap();
    serial
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    seek.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    disabled
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    let began = Instant::now();
    loop {
        let native = engine.prerender_observation().unwrap().unwrap();
        if native.stopped {
            assert_eq!(native.retained_cpu_bytes, 0);
            break;
        }
        assert!(
            began.elapsed() < Duration::from_secs(30),
            "native owner failed to retire"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let after = engine.state().unwrap();
    assert_eq!(after["desktop"]["prerender"]["enabled"], false);
    assert_eq!(after["desktop"]["prerender"]["completed"], 0);
    assert!(after["desktop"]["prerender"]["fault"].is_null());
    assert_eq!(after["desktop"]["prerender"]["cache"]["resident_frames"], 0);
    std::fs::write(root.join("report.json"),serde_json::to_vec_pretty(&serde_json::json!({
        "blockedTicket":flight.id,"blockedPhase":flight.phase,"retainedCpuBytes":native.retained_cpu_bytes,"after":after["desktop"]["prerender"]
    })).unwrap()).unwrap();
}
