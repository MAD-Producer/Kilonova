//! Actual surface/Session path; the gate blocks the production preparation worker.
use super::*;
use crate::session::{Session, SurfaceTarget};
use motion_render::Presenter;
use winit::platform::windows::EventLoopBuilderExtWindows;

fn attach(events: &winit::event_loop::EventLoop<()>, session: &mut Session) {
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
}
fn front(session: &Session) -> Vec<u8> {
    let g = session.graphics.as_ref().unwrap();
    assert!(g.scratch.valid());
    let target = g
        .renderer
        .capture_target(g.scratch.width, g.scratch.height)
        .unwrap();
    Presenter::new_admitted(&g.renderer, &g.scratch.view, g.renderer.target_format)
        .unwrap()
        .draw(&g.renderer, &target.view)
        .unwrap();
    g.renderer.read_target(&target).unwrap()
}
fn complete(session: &mut Session, frame: f64) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !session
        .render_with_view(frame, Some([0., 0., 64., 64.]), None, |_, _, _| {})
        .unwrap()
    {
        assert!(Instant::now() < deadline, "{}", session.preview_info());
        std::thread::yield_now();
    }
}
fn gate(session: &mut Session) -> (mpsc::Receiver<()>, mpsc::SyncSender<()>) {
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    session.pipeline_preparation.before_sample = Some(Box::new(move || {
        started_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    }));
    (started_rx, release_tx)
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_surface_keeps_chrome_and_front_responsive_until_current_revision_is_prepared() {
    const NAME:&str="session::pipeline_preparation::tests::native::actual_surface_keeps_chrome_and_front_responsive_until_current_revision_is_prepared";
    const CHILD: &str = "MOTION_PIPELINE_SURFACE_CHILD";
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
    let (project, _) = fixture();
    let mut session = Session::new(
        project.clone(),
        root.path().into(),
        Arc::new(crate::session::last_error_observation_tests::Stub),
    )
    .unwrap();
    session.preview.mode = motion_render::PreviewMode::High;
    attach(&events, &mut session);
    let original = session.engine.snapshot();
    let revision = session.engine.revision();
    let (started, release) = gate(&mut session);
    let mut chrome = 0;
    assert!(!session
        .render_with_view(0., Some([0., 0., 64., 64.]), None, |_, _, _| chrome += 1)
        .unwrap());
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(
        session.preview_info()["pipelinePreparation"]["preparing"],
        true
    );
    assert_eq!(
        session.preview_info()["pipelinePreparation"]["task"]["phase"],
        "planning"
    );
    assert!(!session.graphics.as_ref().unwrap().scratch.valid());
    let began = Instant::now();
    for _ in 0..8 {
        assert!(!session
            .render_with_view(0., Some([0., 0., 64., 64.]), None, |_, _, _| chrome += 1)
            .unwrap());
    }
    assert_eq!(
        chrome, 9,
        "pending composition stopped presenting editor chrome"
    );
    assert!(
        began.elapsed() < Duration::from_secs(2),
        "owner waited on a blocked sampler"
    );
    let worker = session
        .pipeline_preparation
        .worker
        .as_ref()
        .unwrap()
        .thread
        .thread()
        .id();
    release.send(()).unwrap();
    complete(&mut session, 0.);
    let first = front(&session);
    assert!(first.chunks_exact(4).any(|pixel| pixel[3] != 0));
    let builds = session
        .graphics
        .as_ref()
        .unwrap()
        .renderer
        .effect_pipeline_builds();
    complete(&mut session, 30.);
    let future = front(&session);
    assert_ne!(future, first);
    assert_eq!(
        session
            .graphics
            .as_ref()
            .unwrap()
            .renderer
            .effect_pipeline_builds(),
        builds,
        "entering the upcoming layer compiled in foreground"
    );
    assert_eq!(session.engine.snapshot(), original);
    assert_eq!(session.engine.revision(), revision);

    // A real view revision keeps the previous completed front while its new
    // parameters prepare. Neither project edits nor history are involved.
    let retained = front(&session);
    session.view_revision += 1;
    let (started, release) = gate(&mut session);
    assert!(!session
        .render_with_view(30., Some([0., 0., 64., 64.]), None, |_, _, _| chrome += 1)
        .unwrap());
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(front(&session), retained);
    assert!(session.graphics.as_ref().unwrap().scratch.valid());
    release.send(()).unwrap();
    complete(&mut session, 30.);
    assert_eq!(front(&session), retained);
    assert_eq!(
        session
            .pipeline_preparation
            .worker
            .as_ref()
            .unwrap()
            .thread
            .thread()
            .id(),
        worker
    );

    // Detach cancels the old renderer ticket; attach creates a different native
    // owner while retaining the same single session worker.
    let ticket = session
        .pipeline_preparation
        .ticket
        .as_ref()
        .unwrap()
        .clone();
    session.detach();
    assert!(ticket.status().cancelled);
    attach(&events, &mut session);
    complete(&mut session, 30.);
    assert_eq!(front(&session), retained);
    assert_eq!(
        session
            .pipeline_preparation
            .worker
            .as_ref()
            .unwrap()
            .thread
            .thread()
            .id(),
        worker
    );
    assert_eq!(session.engine.snapshot(), original);
    assert_eq!(session.engine.revision(), revision);
    if let Some(root) = std::env::var_os("MOTION_PIPELINE_SURFACE_EVIDENCE") {
        let root = std::path::PathBuf::from(root);
        std::fs::create_dir(&root).unwrap();
        let g = session.graphics.as_ref().unwrap();
        for (name, pixels) in [
            ("first.png", &first),
            ("future.png", &future),
            ("reattached.png", &retained),
        ] {
            image::save_buffer(
                root.join(name),
                pixels,
                g.scratch.width,
                g.scratch.height,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
        std::fs::write(root.join("surface.json"),serde_json::to_vec_pretty(&serde_json::json!({
            "adapter":g.renderer.adapter_info.name,"backend":format!("{:?}",g.renderer.adapter_info.backend),
            "driver":g.renderer.adapter_info.driver,"driverInfo":g.renderer.adapter_info.driver_info,
            "chromeCalls":chrome,"firstRevisionNativeCreates":builds,
            "dimensions":[g.scratch.width,g.scratch.height],"preview":session.preview_info(),
            "projectUnchanged":session.engine.snapshot()==original,"revisionUnchanged":session.engine.revision()==revision,
            "workerRetained":session.pipeline_preparation.worker.as_ref().unwrap().thread.thread().id()==worker
        })).unwrap()).unwrap();
    }
    session.detach();
}
