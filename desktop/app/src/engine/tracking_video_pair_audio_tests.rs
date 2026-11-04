//! Real audio/cache publication accompanies the plate and original solve.
use super::*;

fn audio_solve(source: &Path, fixture: &str, x: f64) -> Asset {
    let mut data = solve(source, "video-ctts.mp4", x);
    let manifest: Value = serde_json::from_slice(
        &fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../crates/motion-media/tests/fixtures/video/manifest.json"),
        )
        .unwrap(),
    )
    .unwrap();
    // The independent fixture manifest records the native 1/1,000,000 clock.
    // Parse decimal text exactly, rather than infer PTS from the nominal fps.
    data.ticks_per_second = 1_000_000;
    data.nominal_rate = motion_host::tracking::asset::Rate::new(24, 1).unwrap();
    data.source_pts = manifest[fixture]["frames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|frame| {
            let (seconds, fraction) = frame["best_effort_timestamp_time"]
                .as_str()
                .unwrap()
                .split_once('.')
                .unwrap();
            assert_eq!(fraction.len(), 6);
            seconds.parse::<i64>().unwrap() * 1_000_000 + fraction.parse::<i64>().unwrap()
        })
        .collect();
    assert_eq!(data.source_pts.len(), 72);
    let sample = data.tracks[0].samples[0].clone();
    data.tracks[0].samples = (0..data.source_pts.len())
        .map(|i| Sample {
            source_index: i as u32,
            ..sample.clone()
        })
        .collect();
    data.validate().unwrap();
    data
}

fn prepare_audio(engine: &Engine, request: &str, source: &Path) {
    media(
        engine,
        json!({"op":"import_media","request_id":request,"path":source,
        "kind":"video","at_frame":0,"with_audio":true}),
    )
    .unwrap();
    ready(engine, request);
}

fn mix(project: &motion_core::Project, root: &Path, sample: u64) -> Vec<f32> {
    let mut mixer = motion_media::AudioMixer::new(project.clone(), root).unwrap();
    let mut samples = vec![0.; 2048];
    assert_eq!(mixer.mix(sample, &mut samples).unwrap(), 1024);
    samples
}

#[test]
fn native_audio_and_tracking_replace_rollback_undo_redo_and_reopen_together() {
    let observations = std::env::var_os("MOTION_TRACKED_PAIR_AUDIO_DIR").map(PathBuf::from);
    let _logger = observations
        .as_ref()
        .map(|p| crate::logging::init(p.join("logs")).unwrap());
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/motion-media/tests/fixtures/video");
    let old_source = fixtures.join("sound-24fps.mp4");
    let new_source = fixtures.join("audio-delayed.mp4");
    let old_data = audio_solve(&old_source, "sound-24fps.mp4", 0.);
    let new_data = audio_solve(&new_source, "audio-delayed.mp4", 0.5);
    let mut project = authored::camera_fixture();
    let mut binding = project.camera.tracking.take().unwrap();
    project.tracking_assets.clear();
    project.video_assets.clear();
    fs::create_dir_all(&root).unwrap();
    motion_core::storage::save(&root, &project).unwrap();
    let engine = Engine::start(root.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
    prepare_audio(&engine, "pair-audio-old", &old_source);
    let imported = media(
        &engine,
        json!({"op":"finish_media_import","request_id":"pair-audio-old"}),
    )
    .unwrap();
    assert_eq!(imported["task"]["state"], "succeeded");
    let state = engine.state().unwrap();
    let video_id = state["project"]["video_assets"][0]["id"].as_u64().unwrap();
    let old_document = temp.path().join("old.mstrack");
    fs::write(
        &old_document,
        motion_host::tracking::encode::to_vec(&old_data).unwrap(),
    )
    .unwrap();
    let state = engine
        .import_tracking(
            old_document,
            AssetRef {
                kind: AssetKind::Video,
                id: video_id,
            },
            state["revision"].as_u64().unwrap(),
            state["document_generation"].as_u64().unwrap(),
        )
        .unwrap();
    binding.asset = state["project"]["tracking_assets"][0]["id"]
        .as_u64()
        .unwrap();
    binding.source_anchor_pts = old_data.source_pts[0];
    binding.crop = [
        0.,
        0.,
        old_data.source_extent.pixels()[0] as f64,
        old_data.source_extent.pixels()[1] as f64,
    ];
    engine
        .edit_at_revision(
            json!({"op":"set_tracking_camera","object":0,"binding":binding}).to_string(),
            state["revision"].as_u64().unwrap(),
        )
        .unwrap();
    engine.save().unwrap();
    let before_state = engine.state().unwrap();
    let before: motion_core::Project =
        serde_json::from_value(before_state["project"].clone()).unwrap();
    let old_audio = before.video_assets[0].audio_asset.unwrap();
    let old_pcm = mix(&before, &root, 6_000);
    assert!(old_pcm.iter().any(|v| v.abs() > 0.01));
    let warm = captured(&engine, 12.);
    let new_document = temp.path().join("new.mstrack");
    let raw = motion_host::tracking::encode::to_vec(&new_data).unwrap();
    fs::write(&new_document, &raw).unwrap();
    let request = |id: &str| {
        json!({"op":"finish_tracked_video_replacement","request_id":id,
        "target":{"kind":"video","id":video_id},"revision":before_state["revision"],
        "document_generation":before_state["document_generation"],
        "solves":[{"asset":binding.asset,"path":new_document}],"bindings":[]})
    };

    // These are actual model-validation and filesystem save failures after the
    // media publisher has moved the source, PCM, waveform, index and thumbnail.
    for (id, save_failure) in [("pair-audio-binding", false), ("pair-audio-save", true)] {
        prepare_audio(&engine, id, &new_source);
        let files = published_files(&root);
        let mut failed = request(id);
        if save_failure {
            fs::create_dir(root.join("project.json.tmp")).unwrap();
        } else {
            let mut invalid = binding.clone();
            invalid.track = "missing-camera".into();
            failed["bindings"] = json!([{"kind":"camera","composition":before.composition_id,
                "object":0,"binding":invalid}]);
        }
        let reason = media(&engine, failed).unwrap_err();
        if save_failure {
            assert!(reason.contains("save_tracked_video_project"), "{reason}");
            fs::remove_dir(root.join("project.json.tmp")).unwrap();
        } else {
            assert!(reason.contains("camera track missing"), "{reason}");
        }
        assert_eq!(
            published_files(&root),
            files,
            "audio rollback leaked files: {id}"
        );
        let unchanged = engine.state().unwrap();
        assert_eq!(unchanged["project"], before_state["project"]);
        assert_eq!(unchanged["revision"], before_state["revision"]);
        assert_eq!(motion_core::storage::load(&root).unwrap(), before);
        assert_eq!(mix(&before, &root, 6_000), old_pcm);
        assert_eq!(captured(&engine, 12.), warm);
    }
    prepare_audio(&engine, "pair-audio-new", &new_source);
    assert_eq!(
        media(&engine, request("pair-audio-new")).unwrap()["task"]["state"],
        "succeeded"
    );
    let after_state = engine.state().unwrap();
    let after: motion_core::Project =
        serde_json::from_value(after_state["project"].clone()).unwrap();
    assert_eq!(
        after_state["revision"].as_u64().unwrap(),
        before_state["revision"].as_u64().unwrap() + 1
    );
    assert_eq!(after.video_assets[0].id, video_id);
    assert_eq!(after.video_assets[0].audio_asset, Some(old_audio));
    assert_eq!(after.audio_assets.len(), before.audio_assets.len());
    let audio = after
        .audio_assets
        .iter()
        .find(|a| a.id == old_audio)
        .unwrap();
    assert_eq!(audio.path, after.video_assets[0].path);
    let pcm = motion_media::cache_path(&root, audio).unwrap();
    assert!(pcm.with_extension("wave").is_file());
    assert_ne!(
        audio.path,
        before
            .audio_assets
            .iter()
            .find(|a| a.id == old_audio)
            .unwrap()
            .path
    );
    let reference = temp.path().join("reference.pcm");
    motion_desktop_media::platform()
        .decode_audio(
            &new_source,
            &reference,
            None,
            motion_media::Limits::default().cache_bytes,
            &mut |_| Ok(()),
        )
        .unwrap();
    assert_eq!(fs::read(&pcm).unwrap(), fs::read(reference).unwrap());
    let new_pcm = mix(&after, &root, 6_000);
    assert!(new_pcm.iter().all(|v| v.abs() < 0.001));
    assert!(mix(&after, &root, 19_200).iter().any(|v| v.abs() > 0.01));
    assert_eq!(
        fs::read(root.join(&after.video_assets[0].path)).unwrap(),
        fs::read(new_source).unwrap()
    );
    assert_eq!(
        fs::read(root.join(&after.tracking_assets[0].path)).unwrap(),
        raw
    );
    assert_eq!(captured(&engine, 12.), warm);
    assert_eq!(
        engine.history(0).unwrap()["project"],
        before_state["project"]
    );
    assert_eq!(mix(&before, &root, 6_000), old_pcm);
    assert_eq!(
        engine.history(1).unwrap()["project"],
        after_state["project"]
    );
    assert_eq!(mix(&after, &root, 6_000), new_pcm);
    engine.save().unwrap();
    drop(engine);
    fs::remove_file(new_document).unwrap();
    let reopened = Engine::start(root.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
    assert_eq!(reopened.state().unwrap()["project"], after_state["project"]);
    assert_eq!(mix(&after, &root, 6_000), new_pcm);
    assert_eq!(captured(&reopened, 12.), warm);
    if let Some(out) = observations {
        warm.save(out.join("audio-plate.png")).unwrap();
        let log = fs::read_to_string(out.join("logs/motion-studio.log")).unwrap();
        for id in ["pair-audio-binding", "pair-audio-save"] {
            assert!(log.contains(&format!("tracked video replacement failed request={id}")));
        }
        assert!(log.contains("tracked video replacement start request=pair-audio-new"));
    }
}
