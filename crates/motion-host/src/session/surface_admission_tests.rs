use super::*;
use motion_render::{AllocationBudget, ResourceBudgetScope, ResourceRejection};
use winit::platform::windows::EventLoopBuilderExtWindows;

fn surface(
    events: &winit::event_loop::EventLoop<()>,
    budget: &AllocationBudget,
    size: u32,
) -> SurfaceTarget {
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                winit::window::Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(size, size)),
            )
            .unwrap(),
    );
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let surface = instance.create_surface(window).unwrap();
    let renderer = pollster::block_on(Renderer::new_profiled_with_allocation_budget(
        &instance,
        Some(&surface),
        wgpu::TextureFormat::Rgba8UnormSrgb,
        false,
        budget.clone(),
    ))
    .unwrap();
    let config = surface
        .get_default_config(&renderer.adapter, size, size)
        .unwrap();
    SurfaceTarget {
        instance,
        surface,
        config,
        renderer,
    }
}
fn pixels(graphics: &Graphics) -> Vec<u8> {
    let renderer = &graphics.renderer;
    let target = renderer.capture_target(8, 8).unwrap();
    Presenter::new_admitted(renderer, &graphics.scratch.view, renderer.target_format)
        .unwrap()
        .draw(renderer, &target.view)
        .unwrap();
    renderer.read_target(&target).unwrap()
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_presenter_refusal_during_surface_replacement_preserves_front_and_recovers() {
    const NAME: &str = "session::surface_admission_tests::native_presenter_refusal_during_surface_replacement_preserves_front_and_recovers";
    const CHILD: &str = "MOTION_SURFACE_ADMISSION_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
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
    let root = tempfile::tempdir().unwrap();
    let mut project = Project::new(8, 8, 30, 6).unwrap();
    project.background = [0., 1., 0., 1.];
    let mut session = Session::new(
        project.clone(),
        root.path().into(),
        Arc::new(last_error_observation_tests::Stub),
    )
    .unwrap();
    let budget = session.allocation_budget().clone();
    session.attach(surface(&events, &budget, 32)).unwrap();
    let before_pixels = {
        let graphics = session.graphics.as_mut().unwrap();
        let target = graphics.scratch.prepare(&graphics.renderer, 8, 8).unwrap();
        let mut encoder = graphics
            .renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        graphics
            .renderer
            .encode_preview(&session.scene, &target.view, 8, 8, &mut encoder, None)
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
        pixels(graphics)
    };
    assert!(before_pixels
        .chunks_exact(4)
        .all(|pixel| pixel == [0, 255, 0, 255]));
    let before = budget.info().unwrap();
    let epoch = session.surface_epoch;
    let presented = session.last_presented_frame;
    let revision = session.engine.revision();
    let replacement = surface(&events, &budget, 64);
    let replacement_device = replacement.renderer.device.clone();
    let replacing = budget.info().unwrap().admitted_bytes;
    // One RGBA8 placeholder costs4bytes; the two uniforms need64more. Refuse
    // the second uniform by one byte, after the replacement placeholder exists.
    budget.set_limit(replacing + 4 + 63).unwrap();
    let error = session.attach(replacement).unwrap_err();
    match &error {
        crate::Error::Render(error) => assert_eq!(
            error.resource_rejection(),
            Some(&ResourceRejection::Budget {
                scope: ResourceBudgetScope::RenderAllocations,
                required_bytes: replacing + 4 + 64,
                budget_bytes: replacing + 4 + 63,
            })
        ),
        other => panic!("native presenter refusal lost its cause: {other:?}"),
    }
    assert!(error.to_string().contains("canvas navigation"));
    let old = session
        .graphics
        .as_ref()
        .expect("failed replacement detached the previous front");
    assert!(old.scratch.valid());
    assert_eq!([old.scratch.width, old.scratch.height], [8, 8]);
    assert_eq!(old.config.width, 32);
    assert_eq!(session.surface_epoch, epoch);
    assert_eq!(session.last_presented_frame, presented);
    replacement_device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    assert_eq!(
        budget.info().unwrap().live_allocations,
        before.live_allocations
    );
    budget.set_limit(before.budget_bytes).unwrap();
    assert_eq!(pixels(session.graphics.as_ref().unwrap()), before_pixels);
    assert_eq!(session.engine.snapshot(), project);
    assert_eq!(session.engine.revision(), revision);
    assert!(!session.engine.can_undo());
    session.attach(surface(&events, &budget, 64)).unwrap();
    assert_eq!(session.graphics.as_ref().unwrap().config.width, 64);
    assert_eq!(session.surface_epoch, epoch + 1);
    assert_eq!(session.engine.snapshot(), project);
    assert_eq!(session.engine.revision(), revision);
    assert!(!session.engine.can_undo());
    let last_device = session.graphics.as_ref().unwrap().renderer.device.clone();
    session.detach();
    // Renderer teardown submits a remaining startup write after detach's poll.
    last_device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, 0);
}
