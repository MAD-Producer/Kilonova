//! Ordinary window source identity and inspection-independent authored picks.
use super::*;

#[test]
#[ignore = "requires actual Windows surface, GPU and pinned native OCIO runtime"]
fn managed_window_picks_exclude_display_overlays_and_stale_cells() {
    const CHILD: &str = "MOTION_MANAGED_PICK_CHILD";
    const TEST: &str =
        "authored::window::pick::managed_window_picks_exclude_display_overlays_and_stale_cells";
    if std::env::var_os(CHILD).is_none() {
        let output = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                TEST,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .stdout(std::fs::File::create(output.path().join("stdout")).unwrap())
            .stderr(std::fs::File::create(output.path().join("stderr")).unwrap())
            .spawn()
            .unwrap();
        let began = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if began.elapsed() > Duration::from_secs(60) {
                let killed = child.kill();
                let waited = child.wait();
                panic!("managed pick test exceeded 60s: kill={killed:?} wait={waited:?}");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let stdout = std::fs::read_to_string(output.path().join("stdout")).unwrap();
        let stderr = std::fs::read_to_string(output.path().join("stderr")).unwrap();
        assert!(status.success(), "{status}\n{stdout}\n{stderr}");
        assert!(stdout.contains("managed window color pick: one/two/four cells"));
        return;
    }
    let events = winit::event_loop::EventLoop::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    let reference = reference();
    let root = tempfile::tempdir().unwrap();
    let mut project = document(&reference, 0, AlphaMode::Straight, root.path());
    settings(&mut project).display.transfer = ColorTransfer::Srgb;
    let color = [0.25, 0.5, 0.75, 0.5];
    project.layers.push(Layer::solid(
        1,
        "picked authored color",
        [32., 64.],
        [16., 32., 0.],
        color,
    ));
    let before = serde_json::to_value(&project).unwrap();
    let mut session = Session::new(
        project,
        root.path().to_owned(),
        Arc::new(session_installation::NativePlatform(super::super::runtime())),
    )
    .unwrap();
    let pick = |s: &Session, owner, index, point| {
        s.sample_view_cell(owner, index, ViewCamera::Composition, point)
    };
    assert!(pick(&session, 77, 0, [0.25, 0.5]).unwrap().is_none());
    attach(&mut session, &events);
    let backend = session
        .graphics
        .as_ref()
        .unwrap()
        .renderer
        .adapter
        .get_info()
        .backend;
    match std::env::var("WGPU_BACKEND").as_deref() {
        Ok("vulkan") => assert_eq!(backend, wgpu::Backend::Vulkan),
        Ok("dx12") => assert_eq!(backend, wgpu::Backend::Dx12),
        _ => {}
    }
    assert!(pick(&session, 77, 0, [0.25, 0.5]).unwrap().is_none());
    for count in [1, 2, 4] {
        let cells = views(count);
        let inspections: Vec<_> = (0..count)
            .map(|_| ViewInspection {
                channel: motion_render::ViewChannel::Red,
                region: Some([0.75, 0.75, 0.2, 0.2]),
                ..Default::default()
            })
            .collect();
        capture_inspected(&mut session, 0.5, &cells, false, 77, Some(&inspections));
        let attempts = session.render_attempts;
        for index in 0..count {
            let actual = pick(&session, 77, index, [0.25, 0.5]).unwrap().unwrap();
            assert!(
                close(actual, color),
                "count={count} cell={index}: {actual:?}"
            );
            assert_eq!(
                pick(&session, 77, index, [0.75, 0.5]).unwrap(),
                Some([0.; 4])
            );
        }
        let reads = session
            .graphics
            .as_ref()
            .unwrap()
            .renderer
            .image_readback_info();
        assert!(pick(&session, 78, 0, [0.25, 0.5]).unwrap().is_none());
        assert!(pick(&session, 77, count, [0.25, 0.5]).unwrap().is_none());
        assert!(pick(&session, 77, 0, [f64::NAN, 0.5]).is_err());
        assert!(pick(&session, 77, 0, [1., 0.5]).is_err());
        assert!(session
            .sample_view_cell(77, 0, ViewCamera::Camera(999), [0.25, 0.5])
            .unwrap()
            .is_none());
        session.frame = 1.5;
        assert!(pick(&session, 77, 0, [0.25, 0.5]).unwrap().is_none());
        session.frame = 0.5;
        let revision = session.view_revision;
        session.view_revision += 1;
        assert!(pick(&session, 77, 0, [0.25, 0.5]).unwrap().is_none());
        session.view_revision = revision;
        assert_eq!(
            session
                .graphics
                .as_ref()
                .unwrap()
                .renderer
                .image_readback_info(),
            reads
        );
        assert_eq!(
            session.render_attempts, attempts,
            "picks must not trigger rendering"
        );
    }
    assert_eq!(
        serde_json::to_value(session.engine.project()).unwrap(),
        before
    );
    session.detach();
    assert!(pick(&session, 77, 0, [0.25, 0.5]).unwrap().is_none());
    println!(
        "managed window color pick: one/two/four cells; no overlays, stale reads or project edits"
    );
}
