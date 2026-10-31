use super::*;
use motion_core::{render_sampling::Shutter, Content, Ease, Layer, Observer};
use winit::platform::windows::EventLoopBuilderExtWindows;
mod video;

fn fixture(blur: bool) -> Project {
    let mut project = Project::new(32, 32, 30, 60).unwrap();
    project.background = [0., 0., 0., 1.];
    for (id, z) in [(1, -50.), (2, 50.)] {
        let mut layer = Layer::camera(id, "View camera", 32, 32);
        if let Content::Camera { camera } = &mut layer.content {
            camera.position.value = [16., 16., z];
        }
        project.layers.push(layer);
    }
    for (id, z, color) in [(7, -5., [1., 0., 0., 1.]), (8, 5., [0., 0., 1., 1.])] {
        let mut layer = Layer::solid(id, "View plane", [12.; 2], [16., 16., z], color);
        layer.three_d = true;
        layer
            .transform
            .position
            .upsert(0, [12., 16., z], Ease::Linear)
            .unwrap();
        layer
            .transform
            .position
            .upsert(20, [20., 16., z], Ease::Linear)
            .unwrap();
        if blur {
            layer.sampling.motion_blur = Some(Shutter {
                frames: 2.,
                samples: 4,
            });
        }
        project.layers.push(layer);
    }
    project.validate().unwrap();
    project
}
fn views(observer: &Observer) -> [View<'_>; 4] {
    [
        ViewCamera::Composition,
        ViewCamera::Camera(1),
        ViewCamera::Camera(2),
        ViewCamera::Observer(observer),
    ]
    .map(|camera| View {
        camera,
        placement: ViewPlacement {
            viewport: [0., 0., 32., 32.],
            image: None,
        },
    })
    .into_iter()
    .enumerate()
    .map(|(index, mut view)| {
        view.placement.viewport = [(index % 2) as f32 * 34., (index / 2) as f32 * 34., 32., 32.];
        view
    })
    .collect::<Vec<_>>()
    .try_into()
    .ok()
    .unwrap()
}
fn attach(session: &mut Session, events: &winit::event_loop::EventLoop<()>) {
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                winit::window::Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(66, 66)),
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
        session.allocation_budget().clone(),
    ))
    .unwrap();
    let config = surface
        .get_default_config(&renderer.adapter, 66, 66)
        .unwrap();
    session
        .attach(SurfaceTarget {
            instance,
            surface,
            renderer,
            config,
        })
        .unwrap();
    session.preview.mode = PreviewMode::High;
}
fn pixels(g: &Graphics, index: usize) -> Vec<u8> {
    let target = g.renderer.capture_target(32, 32).unwrap();
    Presenter::new_admitted(
        &g.renderer,
        &g.views.targets.output(index).unwrap().view,
        g.renderer.target_format,
    )
    .unwrap()
    .draw(&g.renderer, &target.view)
    .unwrap();
    g.renderer.read_target(&target).unwrap()
}
fn ready(session: &mut Session, owner: u64, frame: f64, views: &[View<'_>]) {
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if session
            .render_views_with_overlay(owner, frame, views, |_, _, _| {})
            .unwrap()
        {
            break;
        }
        assert!(Instant::now() < until, "multiview never became ready");
    }
}
fn child(name: &str, key: &str) -> bool {
    if std::env::var_os(key).is_some() {
        return true;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env(key, "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    false
}

#[test]
#[cfg_attr(
    not(feature = "gpu-tests"),
    ignore = "requires native Windows surfaces and GPU"
)]
fn native_group_cameras_canvas_reuse_and_invalid_inputs_preserve_host_and_fronts() {
    if !child("session::viewer::tests::native_group_cameras_canvas_reuse_and_invalid_inputs_preserve_host_and_fronts",
        "MOTION_VIEW_GROUP_CHILD") {return;}
    let events = winit::event_loop::EventLoop::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let project = fixture(false);
    let mut s = Session::new(
        project.clone(),
        root.path().into(),
        Arc::new(crate::session::last_error_observation_tests::Stub),
    )
    .unwrap();
    attach(&mut s, &events);
    let observer = Observer::new(32, 32);
    let mut cells = views(&observer);
    ready(&mut s, 41, 10., &cells);
    let g = s.graphics.as_ref().unwrap();
    let original: Vec<_> = (0..4).map(|i| pixels(g, i)).collect();
    assert_ne!(
        original[1], original[2],
        "opposite cameras must see different planes"
    );
    assert_eq!(
        original[0], original[2],
        "composition uses its active authored camera"
    );
    let identities: Vec<_> = (0..4)
        .map(|i| g.views.targets.output(i).unwrap().view.texture().clone())
        .collect();
    let revision = s.engine.revision();
    let global = s.observer.clone();
    let before = s.presented;
    let overlays = std::cell::Cell::new(0);
    cells[0].placement.image = Some([3., 5., 20., 20.]);
    assert!(s
        .render_views_with_overlay(41, 10., &cells, |_, _, _| overlays.set(overlays.get() + 1))
        .unwrap());
    assert_eq!(overlays.get(), 1);
    assert_eq!(s.presented, before + 1);
    for (index, identity) in identities.iter().enumerate() {
        assert_eq!(
            s.graphics
                .as_ref()
                .unwrap()
                .views
                .targets
                .output(index)
                .unwrap()
                .view
                .texture(),
            identity
        );
    }
    drop(identities);
    let graphics = s.graphics.as_ref().unwrap();
    assert_eq!(
        s.preview_info()["memoryBreakdown"]["previewTargetBytes"].as_u64(),
        Some(graphics.scratch.texture_bytes() + graphics.views.texture_bytes())
    );
    assert!(graphics.views.texture_bytes() > 0);
    for count in [0, 3] {
        assert!(s
            .render_views_with_overlay(41, 10., &cells[..count], |_, _, _| panic!("invalid draw"))
            .is_err());
    }
    let mut bad = cells;
    bad[1].placement.viewport = bad[0].placement.viewport;
    assert!(s
        .render_views_with_overlay(41, 10., &bad, |_, _, _| panic!("overlap draw"))
        .is_err());
    bad = cells;
    bad[1].camera = ViewCamera::Camera(999);
    assert!(s
        .render_views_with_overlay(41, 10., &bad, |_, _, _| panic!("missing camera draw"))
        .unwrap_err()
        .to_string()
        .contains("999"));
    assert_eq!(s.engine.revision(), revision);
    assert_eq!(s.engine.project(), &project);
    assert_eq!(
        s.observer.pose(32, 32).unwrap().view_projection,
        global.pose(32, 32).unwrap().view_projection
    );
    for (index, expected) in original.iter().enumerate() {
        assert_eq!(&pixels(s.graphics.as_ref().unwrap(), index), expected);
    }
    for count in [2, 1, 4] {
        ready(&mut s, 41, 11., &cells[..count]);
        assert_eq!(s.graphics.as_ref().unwrap().views.targets.len(), count);
    }
    s.detach();
    assert!(s.graphics.is_none());
    assert_eq!(s.allocation_budget().info().unwrap().admitted_bytes, 0);
    attach(&mut s, &events);
    ready(&mut s, 99, 11., &cells);
    assert_eq!(
        s.graphics
            .as_ref()
            .unwrap()
            .views
            .published
            .as_ref()
            .unwrap()
            .owner,
        99
    );
    let generation = s.document_generation;
    s.replace_project(Engine::new(project.clone()).unwrap(), root.path().into())
        .unwrap();
    assert_ne!(generation, s.document_generation);
    let state = &s.graphics.as_ref().unwrap().views;
    assert!(state.request.is_none() && state.published.is_none() && state.targets.is_empty());
    ready(&mut s, 99, 11., &cells);
    assert_eq!(
        s.graphics
            .as_ref()
            .unwrap()
            .views
            .published
            .as_ref()
            .unwrap()
            .document,
        s.document_generation
    );
}

#[test]
#[cfg_attr(
    not(feature = "gpu-tests"),
    ignore = "requires native Windows surfaces and GPU"
)]
fn native_exposure_group_matches_independent_camera_oracles_and_failed_request_is_terminal() {
    if !child("session::viewer::tests::native_exposure_group_matches_independent_camera_oracles_and_failed_request_is_terminal",
        "MOTION_VIEW_EXPOSURE_CHILD") {return;}
    let events = winit::event_loop::EventLoop::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let project = fixture(true);
    let mut s = Session::new(
        project.clone(),
        root.path().into(),
        Arc::new(crate::session::last_error_observation_tests::Stub),
    )
    .unwrap();
    attach(&mut s, &events);
    let observer = Observer::new(32, 32);
    let cells = views(&observer);
    ready(&mut s, 4, 10., &cells);
    let expected: Vec<_> = (0..4)
        .map(|i| pixels(s.graphics.as_ref().unwrap(), i))
        .collect();
    assert_ne!(expected[1], expected[2]);
    // Each group output must match a separately driven single-camera exposure.
    for index in 0..4 {
        let mut cell = cells[index];
        cell.placement.viewport = [0., 0., 32., 32.];
        ready(&mut s, 4, 10., &[cell]);
        assert_eq!(pixels(s.graphics.as_ref().unwrap(), 0), expected[index]);
    }
    ready(&mut s, 4, 10., &cells);
    let old: Vec<_> = (0..4)
        .map(|i| pixels(s.graphics.as_ref().unwrap(), i))
        .collect();
    // Refuse a fresh exposure task before any group can be published. A second
    // identical request returns the retained original cause, with no new task.
    let baseline = s.allocation_budget().info().unwrap();
    s.allocation_budget()
        .set_limit(baseline.admitted_bytes)
        .unwrap();
    let error = s
        .render_views_with_overlay(4, 11., &cells, |_, _, _| panic!("failed group drew"))
        .unwrap_err();
    assert!(matches!(&error,crate::Error::Render(e) if e.resource_rejection().is_some()));
    let retained = s
        .graphics
        .as_ref()
        .unwrap()
        .views
        .request
        .as_ref()
        .unwrap()
        .failure
        .as_ref()
        .unwrap()
        .clone();
    let second = s
        .render_views_with_overlay(4, 11., &cells, |_, _, _| panic!("failed group restarted"))
        .unwrap_err();
    assert_eq!(second.to_string(), error.to_string());
    assert!(Arc::ptr_eq(
        &retained,
        s.graphics
            .as_ref()
            .unwrap()
            .views
            .request
            .as_ref()
            .unwrap()
            .failure
            .as_ref()
            .unwrap()
    ));
    s.allocation_budget()
        .set_limit(baseline.budget_bytes)
        .unwrap();
    for (index, expected) in old.iter().enumerate() {
        assert_eq!(&pixels(s.graphics.as_ref().unwrap(), index), expected);
    }
    // A changed frame is an explicit replacement, not an automatic retry.
    ready(&mut s, 4, 12., &cells);
    assert_eq!(s.engine.project(), &project);
}
