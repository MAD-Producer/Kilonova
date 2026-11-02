//! Actual native decoding and GPU capture across an explicit paired replacement.
use super::*;
use motion_host::tracking::asset::{Asset, Observation};

fn solve(source: &Path, name: &str, x: f64) -> Asset {
    let platform = motion_desktop_media::platform();
    let probe = platform
        .probe_video(source, None, Some(u32::MAX), &|| Ok(()))
        .unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/motion-media/tests/fixtures/video");
    let timings: Value =
        serde_json::from_slice(&fs::read(fixtures.join("native-timing.json")).unwrap()).unwrap();
    let timing = &timings[name];
    let mut data = (*authored::camera_fixture().tracking_assets[0].data).clone();
    data.source_extent = Extent::new(probe.asset.width, probe.asset.height).unwrap();
    let digest = motion_core::storage::resource_sha256(&fs::read(source).unwrap());
    data.source_sha256 =
        std::array::from_fn(|i| u8::from_str_radix(&digest[i * 2..i * 2 + 2], 16).unwrap());
    data.ticks_per_second = timing["timescale"].as_u64().unwrap() as u32;
    data.source_pts = timing["samples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s[0].as_i64().unwrap())
        .collect();
    data.source_pts.sort_unstable();
    let mut sample = data.tracks[0].samples[0].clone();
    if let Some(Observation::Camera {
        camera_to_world,
        calibration,
    }) = &mut sample.observation
    {
        camera_to_world.position[0] = x;
        *calibration = motion_host::tracking::plate::Calibration::new(
            data.source_extent,
            [180., 160.],
            [128., 72.],
            [0.; 3],
            [0.; 2],
        )
        .unwrap();
    }
    data.tracks[0].samples = (0..data.source_pts.len())
        .map(|i| Sample {
            source_index: i as u32,
            ..sample.clone()
        })
        .collect();
    data
}

fn captured(engine: &Engine, at: f64) -> image::RgbaImage {
    let began = Instant::now();
    loop {
        match engine.render_preview(Some(at)) {
            Ok(result) => {
                return image::open(result["path"].as_str().unwrap())
                    .unwrap()
                    .to_rgba8()
            }
            Err(reason) if reason == "video capture pending; request frames and retry" => {
                assert!(began.elapsed() < Duration::from_secs(30), "{reason}");
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(reason) => panic!("paired native capture failed: {reason}"),
        }
    }
}

// Include original files and derived caches; prepared tasks own separate staging
// directories which are expected to disappear when their publication fails.
fn published_files(root: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    fn visit(base: &Path, path: &Path, out: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if !path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".video-import-")
                {
                    visit(base, &path, out);
                }
            } else {
                out.insert(
                    path.strip_prefix(base).unwrap().to_owned(),
                    fs::read(&path).unwrap(),
                );
            }
        }
    }
    let mut result = std::collections::BTreeMap::new();
    for directory in ["assets", "cache"] {
        let path = root.join(directory);
        if path.is_dir() {
            visit(root, &path, &mut result);
        }
    }
    // Session lock files are intentionally held open and are not publications.
    result.insert(
        PathBuf::from("project.json"),
        fs::read(root.join("project.json")).unwrap(),
    );
    result
}

#[test]
fn fresh_plate_and_solve_are_atomic_native_verified_undoable_and_reopenable() {
    let observations = std::env::var_os("MOTION_TRACKED_PAIR_NATIVE_DIR").map(PathBuf::from);
    let _logger = observations
        .as_ref()
        .map(|p| crate::logging::init(p.join("logs")).unwrap());
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir_all(root.join("assets")).unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/motion-media/tests/fixtures/video");
    let source = root.join("assets/original.mp4");
    fs::copy(fixtures.join("video-ctts.mp4"), &source).unwrap();
    let old_solve = solve(&source, "video-ctts.mp4", 0.);
    let document = temp.path().join("old.mstrack");
    let original = motion_host::tracking::encode::to_vec(&old_solve).unwrap();
    fs::write(&document, &original).unwrap();
    let platform = motion_desktop_media::platform();
    let mut probe = platform
        .probe_video(&source, None, Some(u32::MAX), &|| Ok(()))
        .unwrap();
    probe.asset.id = 1;
    probe.asset.path = "assets/original.mp4".into();
    let mut project = authored::camera_fixture();
    let mut binding = project.camera.tracking.take().unwrap();
    binding.source_anchor_pts = old_solve.source_pts[0];
    binding.crop = [0., 0., probe.asset.width as f64, probe.asset.height as f64];
    project.tracking_assets.clear();
    project.video_assets = vec![probe.asset.clone()];
    let mut layer = motion_core::Layer::solid(50, "Plate", [256., 144.], [320., 240., 0.], [1.; 4]);
    layer.three_d = true;
    layer.content = motion_core::Content::Video {
        video: motion_core::VideoClip::for_source(&probe.asset, false),
    };
    layer.timeline = Some(motion_core::LayerTimeline {
        in_frame: 0,
        out_frame: 48,
        offset_frame: 0,
    });
    project.layers.push(layer);
    motion_core::storage::save(&root, &project).unwrap();
    let index = motion_media::video_cache_path(&root, &probe.asset).unwrap();
    fs::create_dir_all(index.parent().unwrap()).unwrap();
    motion_media::save_index(&index, &probe.asset, &probe.timestamps).unwrap();
    let engine = Engine::start(root.clone(), platform, 8 << 30).unwrap();
    let state = engine.state().unwrap();
    let state = engine
        .import_tracking(
            document.clone(),
            AssetRef {
                kind: AssetKind::Video,
                id: 1,
            },
            state["revision"].as_u64().unwrap(),
            state["document_generation"].as_u64().unwrap(),
        )
        .unwrap();
    engine
        .edit_at_revision(
            json!({"op":"set_tracking_camera","object":0,"binding":binding}).to_string(),
            state["revision"].as_u64().unwrap(),
        )
        .unwrap();
    engine.save().unwrap();
    let before = engine.state().unwrap();
    let warm = captured(&engine, 24.);
    let fresh = fixtures.join("video-delayed.mp4");
    let new_solve = solve(&fresh, "video-delayed.mp4", 0.5);
    let new_document = temp.path().join("new.mstrack");
    let mut raw = b" \n".to_vec();
    raw.extend(motion_host::tracking::encode::to_vec(&new_solve).unwrap());
    raw.extend(b"\r\n ");
    fs::write(&new_document, &raw).unwrap();
    let asset = before["project"]["tracking_assets"][0]["id"]
        .as_u64()
        .unwrap();
    let mut next_binding = binding.clone();
    next_binding.source_anchor_pts = new_solve.source_pts[0];
    let request = |id: &str, path: &Path, current: &Value| {
        json!({"op":"finish_tracked_video_replacement","request_id":id,
        "target":{"kind":"video","id":1},"revision":current["revision"],"document_generation":current["document_generation"],
        "solves":[{"asset":asset,"path":path}],"bindings":[{"kind":"camera","composition":project.composition_id,"object":0,"binding":next_binding}]})
    };
    prepare(&engine, "pair-fresh", &fresh);
    // Correct new encoded bytes do not authorize the previous original solve.
    let reason = media(&engine, request("pair-fresh", &document, &before)).unwrap_err();
    assert!(
        reason.contains("tracking source content differs"),
        "{reason}"
    );
    assert_eq!(engine.state().unwrap()["project"], before["project"]);
    assert_eq!(
        media(
            &engine,
            json!({"op":"media_status","request_id":"pair-fresh"})
        )
        .unwrap()["state"],
        "ready"
    );
    // Even a source-matching solve must match the exact native clock.
    let mut wrong_pts = new_solve.clone();
    for pts in &mut wrong_pts.source_pts {
        *pts += 1;
    }
    let wrong = temp.path().join("wrong-pts.mstrack");
    fs::write(
        &wrong,
        motion_host::tracking::encode::to_vec(&wrong_pts).unwrap(),
    )
    .unwrap();
    let reason = media(&engine, request("pair-fresh", &wrong, &before)).unwrap_err();
    assert!(
        reason.contains("PTS") || reason.contains("timestamp"),
        "{reason}"
    );
    assert_eq!(engine.state().unwrap()["revision"], before["revision"]);
    let mut stale = request("pair-fresh", &new_document, &before);
    stale["revision"] = json!(before["revision"].as_u64().unwrap() - 1);
    assert!(media(&engine, stale).is_err());
    assert_eq!(engine.state().unwrap()["project"], before["project"]);
    // Both failures below happen AFTER the source/PTS checks and original
    // staging, when the ordinary publisher has already moved the new video
    // and caches. They must roll back those publications as well as the solve.
    for (id, save_failure) in [("pair-invalid-binding", false), ("pair-save-failure", true)] {
        prepare(&engine, id, &fresh);
        let files = published_files(&root);
        let mut failed = request(id, &new_document, &before);
        if save_failure {
            fs::create_dir(root.join("project.json.tmp")).unwrap();
        } else {
            failed["bindings"][0]["binding"]["track"] = json!("missing-new-camera");
        }
        let reason = media(&engine, failed).unwrap_err();
        if save_failure {
            // The real save encounters a directory where its temporary file
            // must be created. No fake successful persistence return is used.
            assert!(reason.contains("save_tracked_video_project"), "{reason}");
            fs::remove_dir(root.join("project.json.tmp")).unwrap();
        } else {
            assert!(reason.contains("camera track missing"), "{reason}");
        }
        let status = media(&engine, json!({"op":"media_status","request_id":id})).unwrap();
        assert_eq!(status["state"], "failed", "{status}");
        assert_eq!(
            published_files(&root),
            files,
            "publication leaked files for {id}"
        );
        let unchanged = engine.state().unwrap();
        assert_eq!(unchanged["project"], before["project"]);
        assert_eq!(unchanged["revision"], before["revision"]);
        assert_eq!(
            motion_core::storage::load(&root).unwrap(),
            serde_json::from_value::<motion_core::Project>(before["project"].clone()).unwrap()
        );
    }
    let result = media(&engine, request("pair-fresh", &new_document, &before)).unwrap();
    assert_eq!(result["task"]["state"], "succeeded");
    let after = engine.state().unwrap();
    assert_eq!(
        after["revision"].as_u64().unwrap(),
        before["revision"].as_u64().unwrap() + 1
    );
    assert_eq!(after["project"]["video_assets"][0]["id"], 1);
    assert_eq!(after["project"]["tracking_assets"][0]["id"], asset);
    assert_eq!(
        after["project"]["camera"]["position"],
        before["project"]["camera"]["position"]
    );
    let new_frame = captured(&engine, 24.);
    assert_ne!(new_frame, warm);
    let old_path = before["project"]["tracking_assets"][0]["path"]
        .as_str()
        .unwrap();
    let new_path = after["project"]["tracking_assets"][0]["path"]
        .as_str()
        .unwrap();
    assert_ne!(new_path, old_path);
    assert_eq!(fs::read(root.join(old_path)).unwrap(), original);
    assert_eq!(fs::read(root.join(new_path)).unwrap(), raw);
    assert_eq!(
        fs::read(&source).unwrap(),
        fs::read(fixtures.join("video-ctts.mp4")).unwrap()
    );
    assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
    assert_eq!(captured(&engine, 24.), warm);
    assert_eq!(engine.history(1).unwrap()["project"], after["project"]);
    assert_eq!(captured(&engine, 24.), new_frame);
    engine.save().unwrap();
    drop(engine);
    fs::remove_file(new_document).unwrap();
    let reopened = Engine::start(root.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
    assert_eq!(reopened.state().unwrap()["project"], after["project"]);
    assert_eq!(captured(&reopened, 24.), new_frame);
    if let Some(dir) = observations {
        warm.save(dir.join("before.png")).unwrap();
        new_frame.save(dir.join("after.png")).unwrap();
        let log = fs::read_to_string(dir.join("logs/motion-studio.log")).unwrap();
        assert!(
            log.contains(" INFO ")
                && log.contains("tracked video replacement start request=pair-fresh")
        );
        assert!(log.contains("phase=verify_original"));
        assert!(log.contains("tracking source content differs"));
    }
}
