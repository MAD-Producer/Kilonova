use super::*;
use motion_core::{Command, Layer};
use motion_effects::{
    AlphaMode, WorkingSpace,
    image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
};
use winit::platform::windows::EventLoopBuilderExtWindows;

pub(super) fn surface(events: &winit::event_loop::EventLoop<()>, session: &Session) -> SurfaceTarget {
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                winit::window::Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(32, 32)),
            )
            .unwrap(),
    );
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let surface = instance.create_surface(window).unwrap();
    let renderer = pollster::block_on(Renderer::new_profiled_with_precision_changes(
        &instance,
        Some(&surface),
        wgpu::TextureFormat::Rgba8UnormSrgb,
        false,
        session.allocation_budget().clone(),
    ))
    .unwrap();
    let config = surface
        .get_default_config(&renderer.adapter, 32, 32)
        .unwrap();
    SurfaceTarget {
        instance,
        surface,
        renderer,
        config,
    }
}
fn project(exposure: bool) -> Project {
    let mut project = Project::new(16, 16, 30, 12).unwrap();
    project.working_precision = ChannelPrecision::Float16;
    project.background = [0., 1., 0., 1.];
    let mut layer = Layer::solid(
        1,
        "Stationary precision probe",
        [8.; 2],
        [8., 8., 0.],
        [1., 0., 0., 1.],
    );
    if exposure {
        layer.sampling.motion_blur = Some(motion_core::render_sampling::Shutter {
            frames: 0.5,
            samples: 4,
        });
    }
    project.layers.push(layer);
    project
}
fn render(session: &mut Session) {
    for _ in 0..1000 {
        if session.render(4.).unwrap() {
            return;
        }
    }
    panic!(
        "precision preview did not complete: {:?}",
        session.last_error
    );
}
pub(super) fn pixels(session: &Session) -> Vec<u8> {
    let g = session.graphics.as_ref().unwrap();
    assert!(g.scratch.valid());
    let target = g
        .renderer
        .image_capture_target(
            16,
            16,
            ImageContract::new(
                ImageMeaning::Color {
                    space: WorkingSpace::Linear,
                    alpha: AlphaMode::Premultiplied,
                },
                ChannelPrecision::Unorm8,
            ),
        )
        .unwrap();
    Presenter::new_admitted(
        &g.renderer,
        &g.scratch.view,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    )
    .unwrap()
    .draw(&g.renderer, target.view())
    .unwrap();
    g.renderer.read_image_target(&target).unwrap().pixels
}
fn assert_pixels(pixels: &[u8]) {
    assert_eq!(&pixels[..4], &[0, 255, 0, 255]);
    assert_eq!(
        &pixels[(8 * 16 + 8) * 4..(8 * 16 + 8) * 4 + 4],
        &[255, 0, 0, 255]
    );
}
fn set_precision(session: &mut Session, precision: ChannelPrecision) {
    session
        .engine
        .apply_batch(vec![Command::SetWorkingPrecision { precision }])
        .unwrap();
    session.sample().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_surface_preview_changes_precision_without_replacing_the_device_and_retains_sdr_fronts() {
    const NAME: &str = "session::precision_preview_tests::actual_surface_preview_changes_precision_without_replacing_the_device_and_retains_sdr_fronts";
    const CHILD: &str = "MOTION_PRECISION_PREVIEW_CHILD";
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
    for exposure in [false, true] {
        exercise(&events, exposure);
    }
}
fn exercise(events: &winit::event_loop::EventLoop<()>, exposure: bool) {
    let root = tempfile::tempdir().unwrap();
    let mut session = Session::new(
        project(exposure),
        root.path().into(),
        Arc::new(last_error_observation_tests::Stub),
    )
    .unwrap();
    session.preview.mode = PreviewMode::High;
    assert_eq!(session.effects.precision(), ChannelPrecision::Float16);
    session.attach(surface(events, &session)).unwrap();
    let g = session.graphics.as_ref().unwrap();
    assert_eq!(g.renderer.working_precision(), ChannelPrecision::Float16);
    let device = g.renderer.device.clone();
    let queue = g.renderer.queue.clone();
    let surface_format = g.config.format;
    render(&mut session);
    let original = pixels(&session);
    assert_pixels(&original);
    for precision in [
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float32,
        ChannelPrecision::Unorm8,
        ChannelPrecision::Float16,
    ] {
        set_precision(&mut session, precision);
        assert_eq!(session.effects.precision(), precision);
        // The old SDR front stays displayable before the requested new frame.
        assert_eq!(pixels(&session), original);
        render(&mut session);
        render(&mut session);
        let g = session.graphics.as_ref().unwrap();
        assert_eq!(g.renderer.working_precision(), precision);
        assert_eq!(
            g.renderer.target_format,
            motion_render::image_formats::native_format(
                motion_render::image_formats::color_storage(precision)
            )
        );
        assert_eq!(g.renderer.device, device);
        assert_eq!(g.renderer.queue, queue);
        assert_eq!(g.config.format, surface_format);
        let bpp = match precision {
            ChannelPrecision::Unorm8 => 4,
            ChannelPrecision::Unorm16 | ChannelPrecision::Float16 => 12,
            ChannelPrecision::Float32 => 20,
        };
        assert_eq!(g.scratch.texture_bytes(), 2 * 16 * 16 * bpp);
        assert_eq!(pixels(&session), original);
        assert_pixels(&pixels(&session));
    }
    // Requested project metadata is never silently rendered at the previous
    // precision when same-device pipeline admission fails.
    set_precision(&mut session, ChannelPrecision::Float32);
    let budget = session.allocation_budget().clone();
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let before = budget.info().unwrap();
    budget.set_limit(before.admitted_bytes).unwrap();
    let revision = session.engine.revision();
    let error = session.render(4.).unwrap_err();
    assert!(error.to_string().contains("preview-synchronize"), "{error}");
    assert_eq!(session.engine.revision(), revision);
    assert_eq!(
        session.engine.project().working_precision,
        ChannelPrecision::Float32
    );
    assert_eq!(
        session
            .graphics
            .as_ref()
            .unwrap()
            .renderer
            .working_precision(),
        ChannelPrecision::Float16
    );
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    budget.set_limit(before.budget_bytes).unwrap();
    assert_eq!(pixels(&session), original);
    render(&mut session);
    assert_eq!(
        session
            .graphics
            .as_ref()
            .unwrap()
            .renderer
            .working_precision(),
        ChannelPrecision::Float32
    );
    assert_eq!(pixels(&session), original);
    session.engine.undo().unwrap();
    render(&mut session);
    assert_eq!(
        session
            .graphics
            .as_ref()
            .unwrap()
            .renderer
            .working_precision(),
        ChannelPrecision::Float16
    );
    session.engine.redo().unwrap();
    render(&mut session);
    assert_eq!(
        session
            .graphics
            .as_ref()
            .unwrap()
            .renderer
            .working_precision(),
        ChannelPrecision::Float32
    );
    assert_eq!(pixels(&session), original);
    session.detach();
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
}
