use super::*;
use motion_host::depth_snapshot::DepthSnapshot;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn frozen_image_depth_keeps_results_after_seek_close_and_source_removal_and_verifies_copied_rgb() {
    let original = tempfile::tempdir().unwrap();
    let copied = tempfile::tempdir().unwrap();
    let initial = calibrated_source_session(original.path());
    let mut project = initial.engine.project().clone();
    project.layers[0].three_d = true;
    project.layers[0].size = [80.; 2];
    project.camera.created = true;
    project
        .camera
        .position
        .set_at(0, [16., 16., -120.])
        .unwrap();
    project.camera.target.set_at(0, [16., 16., 0.]).unwrap();
    project.camera.fov.set_at(0, 60.).unwrap();
    let mut plane = motion_core::Layer::solid(
        2,
        "Metric depth control",
        [80.; 2],
        [16., 16., 0.],
        [0., 0., 1., 1.],
    );
    plane.three_d = true;
    project.layers.push(plane);
    drop(initial);
    let mut session = Session::new(project, original.path().into(), Arc::new(NoMedia)).unwrap();
    let (worker, seen, _) = backend(false);
    session.configure_depth(worker).unwrap();
    session.request_image_depth(7, geometry(), model()).unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    wait_done(&mut session, 1);
    session.poll_depth().unwrap().unwrap();
    let command = calibration_command(&mut session);
    session.apply_command_batch(vec![command]).unwrap();
    assert!(DepthSnapshot::capture(&session)
        .unwrap_err()
        .to_string()
        .contains("pending"));
    session.request_image_depth(7, geometry(), model()).unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    wait_done(&mut session, 2);
    let estimate = session.poll_depth().unwrap().unwrap();
    let weak = Arc::downgrade(&estimate);
    let snapshot = DepthSnapshot::capture(&session).unwrap();
    assert_eq!(snapshot.layer_count(), 1);
    let project = session.engine.project().clone();
    snapshot.validate_project(&project).unwrap();
    let mut relinked = project.clone();
    relinked.assets[0].path = "assets/another.png".into();
    assert!(snapshot.validate_project(&relinked).is_err());
    let mut saved = Vec::new();
    snapshot.write_to(&mut saved, &|| Ok(())).unwrap();
    let restored = DepthSnapshot::read_from(&mut &saved[..], &|| Ok(())).unwrap();
    restored.validate_project(&project).unwrap();
    std::fs::create_dir_all(copied.path().join("assets")).unwrap();
    let bytes = std::fs::read(original.path().join("assets/input.png")).unwrap();
    std::fs::write(copied.path().join("assets/input.png"), &bytes).unwrap();
    drop(estimate);
    session.frame = 1.;
    session.sample().unwrap();
    assert!(
        session.poll_depth().unwrap().is_none(),
        "live installation remains frame-bound"
    );
    assert_eq!(DepthSnapshot::capture(&session).unwrap().layer_count(), 1,
        "already admitted immutable static sources remain available at another camera time");
    session.cancel_depth().unwrap();
    assert!(DepthSnapshot::capture(&session)
        .unwrap_err()
        .to_string()
        .contains("pending"));
    drop(session);
    original.close().unwrap();
    assert!(
        weak.upgrade().is_some(),
        "independent snapshot owns the admitted scalar planes"
    );

    let verified = snapshot.verify_sources(copied.path(), &|| Ok(())).unwrap();
    let mut renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
    renderer.configure_assets(&project, copied.path()).unwrap();
    let target = renderer
        .sdr_capture_target(32, 32, motion_render::OutputTransform::SDR_CLIP)
        .unwrap();
    let mut scene = motion_render::Scene::new(&project).unwrap();
    let mut frames = Vec::new();
    for frame in [0., 1., 30.] {
        scene
            .sample(&project, frame, None, &motion_core::ExpressionEvaluator)
            .unwrap();
        verified.attach(&mut scene).unwrap();
        assert!(renderer
            .prepare_scene_assets(
                &scene,
                motion_render::image_resources::Resolution::Full,
                false
            )
            .unwrap());
        let pixels = renderer.capture_sdr(&scene, &target).unwrap().0.pixels;
        let p = |x: usize, y: usize| &pixels[(y * 32 + x) * 4..][..4];
        assert_eq!(p(16, 10), &[0, 0, 255, 255]);
        let near = p(16, 22);
        assert!(near[0] > 40 && near[1] < 10 && near[2] < 10 && near[3] == 255);
        frames.push(pixels);
    }
    assert!(frames.windows(2).all(|pair| pair[0] == pair[1]));
    restored
        .verify_sources(copied.path(), &|| Ok(()))
        .unwrap()
        .attach(&mut scene)
        .unwrap();
    assert_eq!(
        renderer.capture_sdr(&scene, &target).unwrap().0.pixels,
        frames[0]
    );
    drop(verified);

    // A modified copy cannot reuse the old numeric result, even when the
    // registered asset metadata, crop, model and source PTS are identical.
    image::RgbaImage::from_pixel(3, 3, image::Rgba([201, 2, 3, 255]))
        .save(copied.path().join("assets/input.png"))
        .unwrap();
    let cause = snapshot
        .verify_sources(copied.path(), &|| Ok(()))
        .err()
        .unwrap()
        .to_string();
    assert!(
        cause.contains("source_id=7") && cause.contains("fingerprint changed"),
        "{cause}"
    );
    std::fs::write(copied.path().join("assets/input.png"), &bytes).unwrap();
    snapshot.verify_sources(copied.path(), &|| Ok(())).unwrap();
    std::fs::remove_file(copied.path().join("assets/input.png")).unwrap();
    let cause = snapshot
        .verify_sources(copied.path(), &|| Err("primary export cancellation".into()))
        .err()
        .unwrap();
    assert_eq!(cause.to_string(), "primary export cancellation");
    drop(snapshot);
    assert!(
        weak.upgrade().is_none(),
        "geometry cache tokens cannot retain scalar planes"
    );
}
