//! Explicit playback demand through the actual native Engine presentation path.
use super::*;

#[test]
#[ignore = "explicit native Engine playback prerender acceptance"]
fn native_playback_ignores_idle_delay_and_presents_a_warm_exact_frame() {
    const NAME: &str = "engine::prerender::tests::playback::native_playback_ignores_idle_delay_and_presents_a_warm_exact_frame";
    const CHILD: &str = "MOTION_PRERENDER_PLAYBACK_CHILD";
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
        if let Some(destination) = std::env::var_os("MOTION_PRERENDER_PLAYBACK_ARTIFACTS") {
            let destination = PathBuf::from(destination);
            std::fs::create_dir_all(&destination).unwrap();
            for name in ["report.json", "motion-studio.log"] {
                let source = root.path().join(name);
                if source.exists() {
                    std::fs::copy(source, destination.join(name)).unwrap();
                }
            }
            std::fs::write(destination.join("stdout.log"), &output.stdout).unwrap();
            std::fs::write(destination.join("stderr.log"), &output.stderr).unwrap();
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
    let directory = root.join("project");
    let mut project = motion_core::Project::new(16, 16, 30, 12).unwrap();
    project.layers.push(motion_core::Layer::solid(
        1,
        "Playback control",
        [16.; 2],
        [8., 8., 0.],
        [1., 0., 0., 1.],
    ));
    motion_core::storage::save(&directory, &project).unwrap();
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
    let engine = Engine::start(directory, motion_desktop_media::platform(), 8 << 30).unwrap();
    receive(&mut events, &engine.attach_async(window, 64, 64).unwrap());
    receive(
        &mut events,
        &engine
            .request(|reply| Command::TestStudy {
                operation: |id| {
                    motion_host::session::with_session(id, |s| {
                        s.preview.set_mode(motion_render::PreviewMode::High);
                        Ok(serde_json::json!({}))
                    })
                },
                reply,
            })
            .unwrap(),
    );
    present(&mut events, &engine);
    let cold = read_front(&mut events, &engine);
    assert_eq!(cold["cachedPresented"], 0);
    engine
        .configure_prerender(motion_host::prerender::Policy {
            enabled: true,
            window: 2,
            idle_delay: Duration::from_secs(3600),
            ..Default::default()
        })
        .unwrap();
    engine.set_prerender_playing(true).unwrap();
    let began = Instant::now();
    let warm_state = loop {
        let state = engine.state().unwrap();
        let status = &state["desktop"]["prerender"];
        assert!(status["fault"].is_null(), "{state}");
        assert_eq!(status["playing"], true);
        if status["cache"]["resident_frames"].as_u64() == Some(2) {
            break state;
        }
        assert!(began.elapsed() < Duration::from_secs(30), "{state}");
        std::thread::sleep(Duration::from_millis(10));
    };
    // Queue both foreground commands while the serial owner is held. This
    // gives the current frame priority over admitting a later cache eviction.
    let (entered, waiting) = channel();
    let (release, released) = channel();
    let held = engine
        .request(|reply| Command::TestWait {
            entered,
            release: released,
            reply,
        })
        .unwrap();
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    let sought = engine.seek_async(1.).unwrap();
    let pending = engine.present_async(frame()).unwrap();
    assert!(engine.queue_depth() >= 2);
    release.send(()).unwrap();
    receive(&mut events, &held);
    receive(&mut events, &sought);
    assert!(receive(&mut events, &pending));
    let warm = read_front(&mut events, &engine);
    assert_eq!(warm["cachedPresented"], 1);
    assert_eq!(warm["pixels"], cold["pixels"]);
    assert!(warm["pixels"]
        .as_array()
        .unwrap()
        .chunks_exact(4)
        .all(|p| p == [255, 0, 0, 255]));
    engine.set_prerender_playing(false).unwrap();
    assert_eq!(
        engine.state().unwrap()["desktop"]["prerender"]["playing"],
        false
    );
    engine.configure_prerender(Default::default()).unwrap();
    std::fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"cold":cold,"warmState":warm_state,"warm":warm,
            "idleDelaySeconds":3600,"warmupDeadlineSeconds":30,
            "scope":"actual native warm playback presentation; not a paced performance benchmark"}),
        )
        .unwrap(),
    )
    .unwrap();
}
