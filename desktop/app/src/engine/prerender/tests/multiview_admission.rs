//! Actual profile transition: grouped views never consume an authored-view cache.
use super::*;

fn grouped() -> UiFrame {
    use crate::engine::viewer_frame::{Camera, Cell, Plan};
    use motion_render::multiview::ViewPlacement;
    let mut ui = frame();
    ui.views = Some(Plan {
        active: 0,
        cells: vec![
            Cell {
                camera: Camera::Composition,
                inspection: Default::default(),
                placement: ViewPlacement { viewport: [0., 0., 32., 64.], image: None },
            },
            Cell {
                camera: Camera::Observer(motion_core::Observer::new(16, 16)),
                inspection: Default::default(),
                placement: ViewPlacement { viewport: [32., 0., 32., 64.], image: None },
            },
        ],
    });
    ui
}

fn warmed(engine: &Engine) -> Value {
    let began = Instant::now();
    loop {
        let state = engine.state().unwrap();
        let status = &state["desktop"]["prerender"];
        assert!(status["fault"].is_null(), "{state}");
        if status["cache"]["resident_frames"] == 2 { return state; }
        assert!(began.elapsed() < Duration::from_secs(30), "{state}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "explicit native single-view/grouped-view prerender admission and recovery"]
fn native_multiview_retires_single_view_warmup_and_recovers_after_single_present() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("project");
    let mut project = motion_core::Project::new(16, 16, 30, 6).unwrap();
    project.layers.push(motion_core::Layer::solid(
        1, "View profile control", [16.; 2], [8., 8., 0.], [1., 0., 0., 1.],
    ));
    motion_core::storage::save(&directory, &project).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(events.create_window(
        winit::window::Window::default_attributes().with_visible(false)
            .with_inner_size(winit::dpi::PhysicalSize::new(64, 64)),
    ).unwrap());
    let engine = Engine::start(directory, motion_desktop_media::platform(), 8 << 30).unwrap();
    receive(&mut events, &engine.attach_async(window, 64, 64).unwrap());
    receive(&mut events, &engine.request(|reply| Command::TestStudy {
        operation: |id| motion_host::session::with_session(id, |s| {
            s.preview.set_mode(motion_render::PreviewMode::High);
            Ok(serde_json::json!({}))
        }), reply,
    }).unwrap());
    engine.configure_prerender(motion_host::prerender::Policy {
        enabled: true, window: 2, idle_delay: Duration::ZERO, ..Default::default()
    }).unwrap();
    let before = warmed(&engine);
    let began = Instant::now();
    while !receive(&mut events, &engine.present_async(grouped()).unwrap()) {
        assert!(began.elapsed() < Duration::from_secs(30));
    }
    let began = Instant::now();
    let held = loop {
        let state = engine.state().unwrap();
        let status = &state["desktop"]["prerender"];
        assert!(status["fault"].is_null(), "{state}");
        let quiet = engine.prerender_observation().unwrap().is_some_and(|snapshot|
            snapshot.in_flight.is_none() && snapshot.retained_cpu_bytes == 0);
        if status["hold"] == "unsupported_profile" && status["cache"]["resident_frames"] == 0 && quiet {
            break state;
        }
        if began.elapsed() >= Duration::from_secs(5) { break state; }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(held["desktop"]["prerender"]["hold"], "unsupported_profile",
        "independent views must suspend authored single-view warming: {held}");
    assert_eq!(held["desktop"]["prerender"]["cache"]["resident_frames"], 0);
    let observation = engine.prerender_observation().unwrap().unwrap();
    assert!(observation.in_flight.is_none());
    assert_eq!(observation.retained_cpu_bytes, 0);
    present(&mut events, &engine);
    let recovered = warmed(&engine);
    assert!(recovered["desktop"]["prerender"]["completed"].as_u64().unwrap()
        > before["desktop"]["prerender"]["completed"].as_u64().unwrap());
    let pixels = read_front(&mut events, &engine);
    assert_eq!(pixels["pixels"].as_array().unwrap().len(), 16 * 16 * 4);
    assert!(pixels["pixels"].as_array().unwrap().chunks_exact(4)
        .all(|pixel| pixel == [255, 0, 0, 255]), "{pixels}");
    engine.configure_prerender(Default::default()).unwrap();
}
