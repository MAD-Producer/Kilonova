use super::*;
use motion_host::depth_snapshot::DepthSnapshot;

fn prepared(root: &Path) -> (Session, mpsc::Receiver<Seen>, DepthSnapshot) {
    let initial = calibrated_source_session(root);
    let mut project = initial.engine.project().clone();
    project.layers[0].three_d = true;
    project.layers[0].size = [80.; 2];
    project.camera.created = true;
    project
        .camera
        .position
        .set_at(0, [16., 16., -120.])
        .unwrap();
    project.camera.position.set_animated(0, true).unwrap();
    project
        .camera
        .position
        .set_at(30, [40., 16., -120.])
        .unwrap();
    project.camera.target.set_at(0, [16., 16., 0.]).unwrap();
    project.camera.fov.set_at(0, 60.).unwrap();
    let mut blue = motion_core::Layer::solid(
        2,
        "Frozen transition metric oracle",
        [80.; 2],
        [16., 16., 0.],
        [0., 0., 1., 1.],
    );
    blue.three_d = true;
    project.layers.push(blue);
    drop(initial);
    let mut session = Session::new(project, root.into(), Arc::new(NoMedia)).unwrap();
    let (echo, seen, _) = backend(false);
    session
        .configure_depth(Scaled {
            echo,
            blocked: None,
            fail: false,
        })
        .unwrap();
    session.request_image_depth(7, geometry(), model()).unwrap();
    wait_done(&mut session, 1);
    session.poll_depth().unwrap().unwrap();
    let command = calibration_command(&mut session);
    session.apply_command_batch(vec![command]).unwrap();
    session.request_image_depth(7, geometry(), model()).unwrap();
    wait_done(&mut session, 2);
    session.poll_depth().unwrap().unwrap();
    let raw = DepthSnapshot::capture(&session).unwrap();
    session
        .begin_image_depth_transition(settled(), policy(), limits())
        .unwrap();
    assert!(DepthSnapshot::capture(&session)
        .unwrap_err()
        .to_string()
        .contains("pending"));
    session
        .request_image_depth_transition(Activity::Interactive, Duration::ZERO)
        .unwrap()
        .unwrap();
    wait_done(&mut session, 3);
    assert!(session.poll_image_depth_transition().unwrap());
    assert_eq!(DepthSnapshot::capture(&session).unwrap().layer_count(), 1);
    session
        .request_image_depth_transition(Activity::Idle, Duration::from_millis(100))
        .unwrap();
    session
        .request_image_depth_transition(Activity::Idle, Duration::from_millis(400))
        .unwrap()
        .unwrap();
    assert!(DepthSnapshot::capture(&session)
        .unwrap_err()
        .to_string()
        .contains("pending"));
    wait_done(&mut session, 4);
    assert!(session.poll_image_depth_transition().unwrap());
    (session, seen, raw)
}

#[test]
fn frozen_transition_selects_current_phase_and_refuses_pending_seek_and_revoked_generations() {
    let root = tempfile::tempdir().unwrap();
    let (mut session, seen, _raw) = prepared(root.path());
    let project = session.engine.project().clone();
    let snapshot = DepthSnapshot::capture(&session).unwrap();
    snapshot.validate_project(&project).unwrap();
    let mut bytes = Vec::new();
    snapshot.write_to(&mut bytes, &|| Ok(())).unwrap();
    let size = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let metadata: serde_json::Value = serde_json::from_slice(&bytes[12..12 + size]).unwrap();
    assert_eq!(
        metadata["entries"][0]["alignment"]["spec"]["phase"],
        "settled"
    );
    assert_eq!(metadata["resources"].as_array().unwrap().len(), 2);
    assert_eq!(
        session.engine.project(),
        &project,
        "capture and serialization must preserve all authored state"
    );
    assert_eq!(session.depth_status().unwrap().unwrap().counts.submitted, 4);
    session.frame = 1.;
    session.sample().unwrap();
    assert!(DepthSnapshot::capture(&session)
        .unwrap_err()
        .to_string()
        .contains("pending"));
    session.frame = 0.;
    session.sample().unwrap();
    assert!(DepthSnapshot::capture(&session)
        .unwrap_err()
        .to_string()
        .contains("pending"));
    session.cancel_depth().unwrap();
    assert!(DepthSnapshot::capture(&session)
        .unwrap_err()
        .to_string()
        .contains("pending"));
    drop(session);
    let restored = DepthSnapshot::read_from(&mut &bytes[..], &|| Ok(())).unwrap();
    restored.validate_project(&project).unwrap();
    assert_eq!(restored.layer_count(), 1);
    assert_eq!(
        seen.try_iter().count(),
        4,
        "freeze, restore and lifecycle revocation never infer or retry"
    );
}

#[test]
fn frozen_transition_fit_start_is_visible_at_default_info_before_cancellation() {
    init_log();
    let root = tempfile::tempdir().unwrap();
    let (mut session, _seen, _raw) = prepared(root.path());
    let snapshot = DepthSnapshot::capture(&session).unwrap();
    let mut bytes = Vec::new();
    snapshot.write_to(&mut bytes, &|| Ok(())).unwrap();
    LOG.0.lock().unwrap().clear();
    let observed = std::cell::Cell::new(false);
    let cause = DepthSnapshot::read_from(&mut &bytes[..], &|| {
        let entered = LOG.0.lock().unwrap().iter().any(|(level, line)| {
            *level == log::Level::Info
                && line.contains("start frozen_depth operation=restore_alignment")
                && line.contains("source_request=4 object=1 source_id=7")
                && line.contains("phase=fit_reconstructed_planes")
        });
        if entered {
            observed.set(true);
            Err("primary observed fit cancellation".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert!(
        observed.get(),
        "Info start must precede the actual fit callback"
    );
    assert_eq!(cause.to_string(), "primary observed fit cancellation");
    assert_eq!(session.depth_status().unwrap().unwrap().counts.submitted, 4);
}

#[test]
fn failed_transition_cannot_freeze_an_older_raw_reference_or_retry_backend() {
    let root = tempfile::tempdir().unwrap();
    let initial = calibrated_source_session(root.path());
    let mut project = initial.engine.project().clone();
    project.layers[0].three_d = true;
    drop(initial);
    let mut session = Session::new(project, root.path().into(), Arc::new(NoMedia)).unwrap();
    let (echo, seen, _) = backend(false);
    session
        .configure_depth(Scaled {
            echo,
            blocked: None,
            fail: true,
        })
        .unwrap();
    session.request_image_depth(7, geometry(), model()).unwrap();
    wait_done(&mut session, 1);
    session.poll_depth().unwrap().unwrap();
    let command = calibration_command(&mut session);
    session.apply_command_batch(vec![command]).unwrap();
    session.request_image_depth(7, geometry(), model()).unwrap();
    wait_done(&mut session, 2);
    session.poll_depth().unwrap().unwrap();
    let raw = DepthSnapshot::capture(&session).unwrap();
    session
        .begin_image_depth_transition(settled(), policy(), limits())
        .unwrap();
    session
        .request_image_depth_transition(Activity::Interactive, Duration::ZERO)
        .unwrap()
        .unwrap();
    wait_done(&mut session, 3);
    assert!(session.poll_image_depth_transition().unwrap());
    session
        .request_image_depth_transition(Activity::Idle, Duration::from_millis(100))
        .unwrap();
    session
        .request_image_depth_transition(Activity::Idle, Duration::from_millis(400))
        .unwrap()
        .unwrap();
    wait_done(&mut session, 4);
    let primary = session.poll_image_depth_transition().unwrap_err();
    assert!(primary
        .to_string()
        .contains("original managed transition failure"));
    for _ in 0..3 {
        assert!(!session.poll_image_depth_transition().unwrap());
        let cause = DepthSnapshot::capture(&session).unwrap_err().to_string();
        assert!(
            cause.contains("object=1") && cause.contains("pending"),
            "{cause}"
        );
    }
    let status = session.depth_status().unwrap().unwrap();
    assert_eq!(status.counts.submitted, 4);
    assert_eq!(status.counts.failed, 1);
    assert_eq!(seen.try_iter().count(), 4);
    raw.validate_project(session.engine.project()).unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn frozen_transition_roundtrip_matches_raw_metric_occlusion_after_session_close_and_copied_source_removal(
) {
    let root = tempfile::tempdir().unwrap();
    let copy = tempfile::tempdir().unwrap();
    let (mut session, seen, raw) = prepared(root.path());
    let project = session.engine.project().clone();
    let snapshot = DepthSnapshot::capture(&session).unwrap();
    let mut bytes = Vec::new();
    snapshot.write_to(&mut bytes, &|| Ok(())).unwrap();
    std::fs::create_dir_all(copy.path().join("assets")).unwrap();
    let rgb = std::fs::read(root.path().join("assets/input.png")).unwrap();
    std::fs::write(copy.path().join("assets/input.png"), &rgb).unwrap();
    session.frame = 1.;
    session.sample().unwrap();
    session.cancel_depth().unwrap();
    drop(session);
    root.close().unwrap();
    let restored = DepthSnapshot::read_from(&mut &bytes[..], &|| Ok(())).unwrap();
    let mut renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
    renderer.configure_assets(&project, copy.path()).unwrap();
    let target = renderer
        .sdr_capture_target(32, 32, motion_render::OutputTransform::SDR_CLIP)
        .unwrap();
    let mut scene = motion_render::Scene::new(&project).unwrap();
    let mut frames = Vec::new();
    for frame in [0., 15., 30.] {
        let mut outputs = Vec::new();
        for frozen in [&raw, &snapshot, &restored] {
            scene
                .sample(&project, frame, None, &motion_core::ExpressionEvaluator)
                .unwrap();
            frozen
                .verify_sources(copy.path(), &|| Ok(()))
                .unwrap()
                .attach(&mut scene)
                .unwrap();
            assert!(renderer
                .prepare_scene_assets(
                    &scene,
                    motion_render::image_resources::Resolution::Full,
                    false
                )
                .unwrap());
            outputs.push(renderer.capture_sdr(&scene, &target).unwrap().0.pixels);
        }
        assert!(
            outputs.windows(2).all(|pair| pair[0] == pair[1]),
            "raw/live-frozen/roundtrip must agree at camera frame={frame}"
        );
        if frame == 0. {
            let at = |x: usize, y: usize| &outputs[0][(y * 32 + x) * 4..][..4];
            assert_eq!(at(16, 10), &[0, 0, 255, 255]);
            assert!(at(16, 22)[0] > 40 && at(16, 22)[2] < 10);
        }
        frames.push(outputs.remove(0));
    }
    assert_ne!(
        frames.first(),
        frames.last(),
        "actual camera animation must change visible output"
    );
    image::RgbaImage::from_pixel(3, 3, image::Rgba([201, 2, 3, 255]))
        .save(copy.path().join("assets/input.png"))
        .unwrap();
    let cause = restored
        .verify_sources(copy.path(), &|| Ok(()))
        .err()
        .unwrap()
        .to_string();
    assert!(
        cause.contains("source_id=7") && cause.contains("fingerprint changed"),
        "{cause}"
    );
    std::fs::write(copy.path().join("assets/input.png"), rgb).unwrap();
    restored.verify_sources(copy.path(), &|| Ok(())).unwrap();
    std::fs::remove_file(copy.path().join("assets/input.png")).unwrap();
    assert!(restored.verify_sources(copy.path(), &|| Ok(())).is_err());
    assert_eq!(
        restored
            .verify_sources(copy.path(), &|| Err(
                "original frozen export cancellation".into()
            ))
            .err()
            .unwrap()
            .to_string(),
        "original frozen export cancellation"
    );
    assert_eq!(seen.try_iter().count(), 4);
}
