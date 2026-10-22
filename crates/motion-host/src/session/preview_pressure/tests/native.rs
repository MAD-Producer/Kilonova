//! Inject advisory readings, but exercise actual Session/native resources and pixels.
use super::*;
use motion_core::{Asset, Content, Layer};
use motion_render::{image_resources::Resolution, Presenter};
use winit::platform::windows::EventLoopBuilderExtWindows;

fn front(graphics: &Graphics) -> Vec<u8> {
    let renderer = &graphics.renderer;
    let target = renderer
        .capture_target(graphics.scratch.width, graphics.scratch.height)
        .unwrap();
    Presenter::new_admitted(
        renderer,
        &graphics.scratch.view,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    )
    .draw(renderer, &target.view)
    .unwrap();
    renderer.read_target(&target).unwrap()
}

#[test]
fn actual_session_evicts_idle_before_downsampling_and_preserves_front_project_and_full_capture() {
    const NAME: &str = "session::preview_pressure::tests::native::actual_session_evicts_idle_before_downsampling_and_preserves_front_project_and_full_capture";
    const CHILD: &str = "MOTION_NATIVE_PREVIEW_PRESSURE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let events = winit::event_loop::EventLoop::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    for exposure in [false, true] {
        exercise(&events, exposure);
    }
}

fn exercise(events: &winit::event_loop::EventLoop<()>, exposure: bool) {
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                winit::window::Window::default_attributes()
                    .with_title("Motion Studio preview pressure validation")
                    .with_inner_size(winit::dpi::PhysicalSize::new(64, 64))
                    .with_visible(false),
            )
            .unwrap(),
    );
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("assets")).unwrap();
    for (id, color) in [(1, [255, 0, 0, 255]), (2, [0, 255, 0, 255])] {
        let pixels = color.repeat(2048 * 64);
        image::save_buffer(
            root.path().join(format!("assets/{id}.png")),
            &pixels,
            2048,
            64,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
    let mut project = Project::new(64, 64, 30, 6).unwrap();
    for id in 1..=2 {
        project.assets.push(Asset {
            id,
            path: format!("assets/{id}.png"),
            width: 2048,
            height: 64,
        });
    }
    let mut layer = Layer::solid(1, "Active preview image", [64.; 2], [32., 32., 0.], [1.; 4]);
    layer.content = Content::Image { asset: 1 };
    if exposure {
        layer.sampling.motion_blur = Some(motion_core::render_sampling::Shutter {
            frames: 0.5,
            samples: 2,
        });
    }
    project.layers.push(layer);
    let mut session = Session::new(
        project,
        root.path().to_owned(),
        Arc::new(crate::session::last_error_observation_tests::Stub),
    )
    .unwrap();
    session.preview.mode = PreviewMode::High;
    session.sample().unwrap();
    assert_eq!(session.sampling.required(), exposure);
    let original = session.engine.snapshot();
    let revision = session.engine.revision();
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let surface = instance.create_surface(window).unwrap();
    let renderer = pollster::block_on(Renderer::new_profiled_with_allocation_budget(
        &instance,
        Some(&surface),
        wgpu::TextureFormat::Rgba8UnormSrgb,
        false,
        session.allocation_budget().clone(),
    ))
    .unwrap();
    let config = surface
        .get_default_config(&renderer.adapter, 64, 64)
        .unwrap();
    session
        .attach(SurfaceTarget {
            instance,
            surface,
            config,
            renderer,
        })
        .unwrap();
    let (saved, occupied, idle) = {
        let graphics = session.graphics.as_mut().unwrap();
        let mut other = original.clone();
        other.layers[0].content = Content::Image { asset: 2 };
        let mut idle_scene = Scene::new(&other);
        idle_scene
            .sample(&other, 0., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        graphics
            .renderer
            .prepare_scene_assets(&idle_scene, Resolution::Preview(2048), false)
            .unwrap();
        graphics
            .renderer
            .prepare_scene_assets(&session.scene, Resolution::Preview(2048), false)
            .unwrap();
        let target = graphics
            .scratch
            .prepare(&graphics.renderer, 64, 64)
            .unwrap();
        let mut encoder = graphics
            .renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        graphics
            .renderer
            .encode_preview(&session.scene, &target.view, 64, 64, &mut encoder, None)
            .unwrap();
        graphics
            .renderer
            .submit_allocated(encoder.finish(), |_| {})
            .unwrap();
        graphics.scratch.commit_submitted();
        graphics
            .renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        let pixels = front(graphics);
        let idle = graphics.renderer.image_idle_bytes();
        assert_eq!(idle, 2048 * 64 * 4);
        (
            pixels,
            session.allocation_budget().info().unwrap().admitted_bytes,
            idle,
        )
    };
    let center = (32 * 64 + 32) * 4;
    assert!(saved[center] > 240 && saved[center + 1] < 4 && saved[center + 2] < 4);
    // Test clock is independent of wall time: no sleep or physical VRAM
    // exhaustion is needed. The same production helper consumes these readings.
    assert!(!session
        .observe_preview_pressure_with(Duration::from_secs(1000), |_| high())
        .unwrap());
    let graphics = session.graphics.as_ref().unwrap();
    assert_eq!(graphics.renderer.image_idle_bytes(), 0);
    assert_eq!(graphics.renderer.image_dimensions(1), Some((2048, 64)));
    assert_eq!(front(graphics), saved);
    assert!(session.allocation_budget().info().unwrap().admitted_bytes <= occupied - idle);
    assert_eq!(session.preview_memory.quality(true).level, 0);
    let view = session.view_revision;
    assert!(session
        .observe_preview_pressure_with(Duration::from_secs(1001), |_| high())
        .unwrap());
    assert_eq!(session.view_revision, view + 1);
    assert_eq!(front(session.graphics.as_ref().unwrap()), saved);
    assert_eq!(session.preview_memory.quality(true).level, 1);
    assert!(!session.preview_memory.allows_image_prefetch());
    let adapting = session.preview_info()["memoryAdaptation"].clone();
    assert_eq!(adapting["status"], "degraded");
    assert_eq!(adapting["nativePressure"]["status"], "high");
    assert_eq!(adapting["nativePressure"]["level"], 1);
    assert_eq!(adapting["nativePressure"]["worstPool"]["index"], 3);
    assert_eq!(adapting["refusalCount"], 0);
    assert!(adapting["rejection"].is_null());
    assert!(adapting["hasLastValidFrame"].as_bool().unwrap());
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if session
            .render_with_view(0., Some([0., 0., 64., 64.]), None, |_, _, _| {})
            .unwrap()
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "native pressure preview did not commit its smaller full effect result"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    let graphics = session.graphics.as_ref().unwrap();
    graphics
        .renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert_eq!((graphics.scratch.width, graphics.scratch.height), (32, 32));
    assert_eq!(graphics.actual_preview_quality.unwrap().level, 1);
    assert_eq!(graphics.renderer.image_dimensions(1), Some((1024, 32)));
    let reduced = front(graphics);
    let center = (16 * 32 + 16) * 4;
    assert!(reduced[center] > 240 && reduced[center + 1] < 4 && reduced[center + 2] < 4);
    assert_eq!(session.engine.snapshot(), original);
    assert_eq!(session.engine.revision(), revision);
    assert!(!session.engine.can_undo());
    assert_eq!(session.preview_memory.refusal_count, 0);
    let adapted = session.preview_info()["memoryAdaptation"].clone();
    assert_eq!(adapted["requested"]["level"], 1);
    assert_eq!(adapted["actual"]["level"], 1);
    assert_eq!(adapted["actualWidth"], 32);
    assert_eq!(adapted["actualHeight"], 32);
    let graphics = session.graphics.as_mut().unwrap();
    graphics
        .renderer
        .prepare_scene_assets(&session.scene, Resolution::Full, false)
        .unwrap();
    let target = graphics.renderer.capture_target(64, 64).unwrap();
    let output = graphics
        .renderer
        .capture(&session.scene, &target)
        .unwrap()
        .0;
    assert_eq!(graphics.renderer.image_dimensions(1), Some((2048, 64)));
    assert_eq!(output.len(), 64 * 64 * 4);
    assert_eq!(
        output, saved,
        "formal capture keeps full dimensions and original pixels"
    );
}
