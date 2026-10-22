use super::*;
use motion_core::{render_sampling::Shutter, Command, Ease, Layer};

#[test]
fn frozen_png_shares_live_session_admission_and_recovers_at_full_quality() {
    let root = tempfile::tempdir().unwrap();
    let mut project = Project::new(32, 24, 30, 6).unwrap();
    project.background = [0., 1., 0., 1.];
    let mut session = Session::new(project, root.path().to_owned(), crate::platform()).unwrap();
    let budget = session.allocation_budget().clone();
    // A real live renderer holds the same native fixed buffers that an editor
    // preview needs. The dedicated export renderer must compete for their bytes.
    session.editor_renderer = Some(pollster::block_on(
        Renderer::headless_with_allocation_budget(budget.clone()),
    ).unwrap());
    let before = session.engine.snapshot();
    let revision = session.engine.revision();
    let view = session.view_revision;
    let occupied = budget.info().unwrap();
    assert!(occupied.admitted_bytes > 0);
    let output = root.path().join("shared-envelope.png");
    fs::write(&output, b"previous publication").unwrap();
    let control = ExportControl::default();
    let plan = FrameFreezePlan::capture(&session, Some(output.clone()), &control).unwrap();
    let mut policy = session.effects.memory_policy();
    policy.session_allocation_budget_bytes = occupied.admitted_bytes;
    session.configure_memory_policy(policy).unwrap();
    // Pressure after freeze must still constrain the output worker. No preview
    // quality change, resampling or overwrite is allowed to make it fit.
    let error = plan.export(&control).expect_err("PNG must use the live session allowance");
    assert!(error.contains("PNG GPU initialization") && error.contains("render allocation"), "{error}");
    assert_eq!(fs::read(&output).unwrap(), b"previous publication");
    assert_eq!(budget.info().unwrap().admitted_bytes, occupied.admitted_bytes);
    assert_eq!(budget.info().unwrap().live_allocations, occupied.live_allocations);
    assert_eq!(session.engine.snapshot(), before);
    assert_eq!(session.engine.revision(), revision);
    assert_eq!(session.view_revision, view + 1);
    assert!(!session.engine.can_undo());
    policy.session_allocation_budget_bytes = 512 << 20;
    session.configure_memory_policy(policy).unwrap();
    let control = ExportControl::default();
    let result = FrameFreezePlan::capture(&session, Some(output.clone()), &control)
        .unwrap().export(&control).unwrap();
    assert_eq!((result["width"].as_u64(), result["height"].as_u64()), (Some(32), Some(24)));
    let image = image::open(&output).unwrap().into_rgba8();
    assert_eq!(image.dimensions(), (32, 24));
    assert!(image.pixels().all(|p| p.0 == [0, 255, 0, 255]));
    assert_eq!(session.engine.snapshot(), before);
    assert_eq!(session.engine.revision(), revision);
    assert!(!session.engine.can_undo());
}

fn project() -> Project {
    let mut p = Project::new(32, 32, 30, 6).unwrap();
    p.version = motion_core::CURRENT_PROJECT_VERSION;
    p.background = [0., 0., 0., 1.];
    for (id, y, color) in [(1, 8., [1., 0., 0., 1.]), (2, 24., [0., 1., 0., 1.])] {
        let mut layer = Layer::solid(id, "Frozen PNG", [4.; 2], [16., y, 0.], color);
        for (frame, x) in [(0, 8.), (2, 24.)] {
            layer
                .transform
                .position
                .upsert(frame, [x, y, 0.], Ease::Linear)
                .unwrap();
        }
        if id == 1 {
            layer.sampling.motion_blur = Some(Shutter {
                frames: 2.,
                samples: 8,
            });
        }
        p.layers.push(layer);
    }
    p
}

#[test]
fn png_uses_frozen_fractional_frame_and_all_samples_without_preview_mutation() {
    let root = tempfile::tempdir().unwrap();
    let mut session = Session::new(project(), root.path().to_owned(), crate::platform()).unwrap();
    session.frame = 1.25;
    let control = ExportControl::default();
    let plan = FrameFreezePlan::capture(&session, None, &control).unwrap();
    let frozen = session.engine.snapshot();
    let revision = session.engine.revision();
    session
        .engine
        .apply(Command::SetLayerSampling {
            object: 1,
            sampling: Default::default(),
        })
        .unwrap();
    session.frame = 4.;
    let before = serde_json::to_value(session.engine.project()).unwrap();
    let started = Instant::now();
    let result = plan.export(&control).unwrap();
    let elapsed = started.elapsed();
    assert_eq!(result["frame"], 1.25);
    assert_eq!(result["revision"], revision);
    assert_eq!(session.frame, 4.);
    assert_eq!(
        serde_json::to_value(session.engine.project()).unwrap(),
        before
    );
    assert!(
        !control.cancel(),
        "publication must win over later cancellation"
    );
    let actual = image::open(result["path"].as_str().unwrap())
        .unwrap()
        .into_rgba8()
        .into_raw();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut center = Scene::new(&frozen).unwrap();
    center
        .sample(&frozen, 1.25, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut oracle = renderer.exposure_target(32, 32, &[0.125; 8]).unwrap();
    // Independently authored midpoint times; only the red receiver is blurred.
    for time in [0.375, 0.625, 0.875, 1.125, 1.375, 1.625, 1.875, 2.125] {
        let mut sample = Scene::new(&frozen).unwrap();
        sample
            .sample(&frozen, time, None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let mut phase = center.clone();
        phase.layers[0] = sample.layers[0].clone();
        renderer
            .accumulate_exposure(&mut oracle, Some(&phase), false)
            .unwrap();
    }
    let expected = renderer.capture_exposure(&oracle).unwrap();
    assert!(actual
        .iter()
        .zip(&expected)
        .all(|(a, b)| a.abs_diff(*b) <= 1));
    let target = renderer.capture_target(32, 32).unwrap();
    let center_pixels = renderer.capture(&center, &target).unwrap().0;
    assert!(
        actual
            .iter()
            .zip(center_pixels)
            .filter(|(a, b)| **a != *b)
            .count()
            > 24
    );
    if let Some(dir) = std::env::var_os("MOTION_FRAME_EXPORT_ARTIFACTS") {
        let dir = PathBuf::from(dir);
        fs::create_dir_all(&dir).unwrap();
        fs::copy(
            result["path"].as_str().unwrap(),
            dir.join("frozen-fractional.png"),
        )
        .unwrap();
        fs::write(dir.join("frozen-fractional.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "result": result, "exportUs": elapsed.as_micros(), "samples":8,
            "scope":"snapshot copy, dedicated renderer initialization, exposure, readback, PNG encode/verify/publish"
        })).unwrap()).unwrap();
    }
}

#[test]
fn png_cancel_failure_and_source_protection_preserve_existing_destination() {
    let root = tempfile::tempdir().unwrap();
    let mut session = Session::new(project(), root.path().to_owned(), crate::platform()).unwrap();
    for frame in [f64::NAN, -1., 6., f64::INFINITY] {
        session.frame = frame;
        assert!(
            FrameFreezePlan::capture(&session, None, &ExportControl::default())
                .err()
                .unwrap()
                .contains("PNG capture frame is invalid")
        );
    }
    session.frame = 0.;
    let output = root.path().join("previous.png");
    fs::write(&output, b"previous publication").unwrap();
    let control = ExportControl::default();
    let plan = FrameFreezePlan::capture(&session, Some(output.clone()), &control).unwrap();
    assert!(control.cancel());
    assert_eq!(plan.export(&control).unwrap_err(), "PNG export cancelled");
    assert_eq!(fs::read(&output).unwrap(), b"previous publication");

    let control = ExportControl::default();
    let mut plan = FrameFreezePlan::capture(&session, Some(output.clone()), &control).unwrap();
    plan.sources.memory_policy.source_texture_budget_bytes = 1;
    let error = plan.export(&control).unwrap_err();
    assert!(
        error.contains("PNG export revision=0 frame=0") && error.contains("budget"),
        "{error}"
    );
    assert_eq!(fs::read(&output).unwrap(), b"previous publication");
    assert!(fs::read_dir(root.path()).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".motion-frame-")));

    fs::write(root.path().join("project.json"), b"protected project").unwrap();
    let plan = FrameFreezePlan::capture(
        &session,
        Some(root.path().join("project.json")),
        &ExportControl::default(),
    )
    .unwrap();
    assert!(plan
        .export(&ExportControl::default())
        .unwrap_err()
        .contains("PNG export must not replace a project source"));
    assert_eq!(
        fs::read(root.path().join("project.json")).unwrap(),
        b"protected project"
    );
}

#[test]
fn png_freezes_sources_before_replacement_and_exports_without_ffmpeg_or_pcm() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("assets")).unwrap();
    let source = root.path().join("assets/image.png");
    image::save_buffer(&source, &[255, 0, 0, 255], 1, 1, image::ColorType::Rgba8).unwrap();
    let mut p = Project::new(32, 32, 30, 6).unwrap();
    p.assets.push(motion_core::Asset {
        id: 1,
        path: "assets/image.png".into(),
        width: 1,
        height: 1,
    });
    p.audio_assets.push(motion_core::AudioAsset {
        id: 2,
        path: "assets/missing.wav".into(),
        mime: "audio/wav".into(),
        bytes: 4,
        track: 0,
        sample_rate: 48_000,
        channels: 2,
        sample_frames: 48_000,
        duration_us: 1_000_000,
    });
    let mut audio = Layer::solid(2, "Unavailable sound", [0.; 2], [0.; 3], [1.; 4]);
    audio.content = motion_core::Content::Audio {
        audio: motion_core::AudioClip::new(2),
    };
    p.layers.push(audio);
    let mut layer = Layer::solid(1, "Frozen image", [32.; 2], [16., 16., 0.], [1.; 4]);
    layer.content = motion_core::Content::Image { asset: 1 };
    p.layers.push(layer);
    let session = Session::new(p, root.path().to_owned(), crate::platform()).unwrap();
    let control = ExportControl::default();
    let plan = FrameFreezePlan::capture(&session, None, &control).unwrap();
    fs::rename(&source, root.path().join("old.png")).unwrap();
    image::save_buffer(&source, &[0, 0, 255, 255], 1, 1, image::ColorType::Rgba8).unwrap();
    let result = plan.export(&control).unwrap();
    let actual = image::open(result["path"].as_str().unwrap())
        .unwrap()
        .into_rgba8();
    assert_eq!(actual.get_pixel(16, 16).0, [255, 0, 0, 255]);
    assert_eq!(
        image::open(source).unwrap().into_rgba8().get_pixel(0, 0).0,
        [0, 0, 255, 255]
    );
}
