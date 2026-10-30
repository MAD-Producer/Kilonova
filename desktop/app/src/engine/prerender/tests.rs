//! Actual Engine-owned production, native presentation and changed-source control.
use crate::engine::*;
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows};
mod playback;
mod performance;

fn frame() -> UiFrame {
    UiFrame {
        paint: Default::default(),
        atlas: None,
        logical_size: [64.; 2],
        composition: Some([0., 0., 64., 64.]),
        composition_image: None,
        window: 0,
        node_preview: None,
    }
}
fn receive<T>(
    events: &mut EventLoop<()>,
    pending: &std::sync::mpsc::Receiver<Result<T, String>>,
) -> T {
    crate::native_test_support::receive(events, pending, Duration::from_secs(60))
        .unwrap()
        .unwrap()
}
fn present(events: &mut EventLoop<()>, engine: &Engine) {
    let began = Instant::now();
    loop {
        if receive(events, &engine.present_async(frame()).unwrap()) {
            break;
        }
        assert!(
            began.elapsed() < Duration::from_secs(60),
            "native front did not complete"
        );
    }
}
fn front(id: i64) -> motion_host::Result<Value> {
    motion_host::session::with_session(id, |s| {
        let g = s.graphics.as_ref().ok_or("surface missing")?;
        assert!(g.scratch.valid());
        let target = g.renderer.image_capture_target(
            16,
            16,
            motion_effects::image_contract::ImageContract::new(
                motion_effects::image_contract::ImageMeaning::Color {
                    space: motion_effects::WorkingSpace::Linear,
                    alpha: motion_effects::AlphaMode::Premultiplied,
                },
                motion_effects::image_contract::ChannelPrecision::Unorm8,
            ),
        )?;
        motion_render::Presenter::new_admitted(
            &g.renderer,
            &g.scratch.view,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        )?
        .draw(&g.renderer, target.view())?;
        let pixels = g.renderer.read_image_target(&target)?.pixels;
        Ok(serde_json::json!({"pixels":pixels,"cachedPresented":s.cached_presented}))
    })
}
fn read_front(events: &mut EventLoop<()>, engine: &Engine) -> Value {
    receive(
        events,
        &engine
            .request(|reply| Command::TestStudy {
                operation: front,
                reply,
            })
            .unwrap(),
    )
}

#[test]
#[ignore = "explicit native Engine idle prerender acceptance with actual retained-front pixels"]
fn native_idle_owner_presents_a_warm_frame_and_rejects_an_edited_source() {
    const NAME:&str="engine::prerender::tests::native_idle_owner_presents_a_warm_frame_and_rejects_an_edited_source";
    const CHILD: &str = "MOTION_PRERENDER_ENGINE_CHILD";
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
        if let Some(destination) = std::env::var_os("MOTION_PRERENDER_ENGINE_ARTIFACTS") {
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
            "child status={}\n{}\n{}",
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
        "Cache control",
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
    assert_eq!(
        engine.state().unwrap()["desktop"]["prerender"]["enabled"],
        false
    );
    assert!(engine.prerender_observation().unwrap().is_none());
    present(&mut events, &engine);
    let cold = read_front(&mut events, &engine);
    assert_eq!(cold["cachedPresented"], 0);
    let policy = motion_host::prerender::Policy {
        enabled: true,
        window: 2,
        ..Default::default()
    };
    engine.configure_prerender(policy).unwrap();
    let began = Instant::now();
    let warm_state = loop {
        let state = engine.state().unwrap();
        assert!(state["desktop"]["prerender"]["fault"].is_null(), "{state}");
        if state["desktop"]["prerender"]["cache"]["resident_frames"]
            .as_u64()
            .unwrap_or(0)
            > 0
        {
            break state;
        }
        assert!(
            began.elapsed() < Duration::from_secs(60),
            "warmup deadline: {state}"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    engine.seek(1.).unwrap();
    present(&mut events, &engine);
    let warm = read_front(&mut events, &engine);
    assert_eq!(warm["cachedPresented"], 1, "{warm}");
    assert_eq!(
        engine.state().unwrap()["desktop"]["prerender"]["cachedPresented"],
        1
    );
    assert_eq!(warm["pixels"], cold["pixels"]);
    assert!(warm["pixels"]
        .as_array()
        .unwrap()
        .chunks_exact(4)
        .all(|p| p == [255, 0, 0, 255]));
    engine
        .edit(
            serde_json::to_string(&motion_core::Command::SetColor {
                object: 1,
                value: [0., 1., 0., 1.],
            })
            .unwrap(),
        )
        .unwrap();
    present(&mut events, &engine);
    let edited = read_front(&mut events, &engine);
    assert_eq!(edited["cachedPresented"], 1);
    assert_eq!(
        engine.state().unwrap()["desktop"]["prerender"]["cachedPresented"],
        1
    );
    assert!(edited["pixels"]
        .as_array()
        .unwrap()
        .chunks_exact(4)
        .all(|p| p == [0, 255, 0, 255]));
    engine.configure_prerender(Default::default()).unwrap();
    assert_eq!(
        engine.state().unwrap()["desktop"]["prerender"]["enabled"],
        false
    );
    std::fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"cold":cold,"warmState":warm_state,"warm":warm,"edited":edited}),
        )
        .unwrap(),
    )
    .unwrap();
}
