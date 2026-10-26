use super::*;

fn png(path: &Path, rgba: [u8; 4]) {
    image::RgbaImage::from_pixel(8, 6, image::Rgba(rgba))
        .save(path)
        .unwrap();
}
fn images(temp: &Path) -> (Engine, Value, Vec<PathBuf>) {
    let engine = engine(&temp.join("project"));
    let mut sources = Vec::new();
    for index in 0..3 {
        let path = temp.join(format!("source-{index}.png"));
        png(&path, [255, index as u8, 0, 255]);
        engine.import_image(path.clone()).unwrap();
        sources.push(path);
    }
    let before = engine.save().unwrap();
    (engine, before, sources)
}
fn candidates(before: &Value, sources: &[PathBuf]) -> Vec<Value> {
    before["project"]["assets"]
        .as_array()
        .unwrap()
        .iter()
        .zip(sources)
        .map(|(a, path)| json!({"target":{"kind":"image","id":a["id"]},"path":path}))
        .collect()
}
fn begin(e: &Engine, id: &str, before: &Value, candidates: Vec<Value>) -> Value {
    request(
        e,
        json!({"op":"start_asset_relink","request_id":id,"revision":before["revision"],"document_generation":before["document_generation"],"candidates":candidates}),
    )
}
fn ready(e: &Engine, id: &str) -> Value {
    let start = Instant::now();
    loop {
        let status = request(e, json!({"op":"media_status","request_id":id}));
        if status["state"] != "running" {
            return status;
        }
        assert!(start.elapsed() < Duration::from_secs(30), "{status}");
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn finish_batch(e: &Engine, id: &str) -> Result<Value, String> {
    e.media(json!({"op":"finish_asset_relink","request_id":id}).to_string())
}
fn files(path: &Path) -> Vec<String> {
    fn walk(root: &Path, folder: &Path, out: &mut Vec<String>) {
        for entry in fs::read_dir(folder).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                walk(root, &entry.path(), out);
            } else {
                out.push(
                    entry
                        .path()
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into(),
                );
            }
        }
    }
    let mut out = Vec::new();
    walk(path, path, &mut out);
    out.sort();
    out
}

#[test]
fn media_asset_relink_recovers_multiple_missing_sources_in_one_edit_and_reopens() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let (e, before, sources) = images(temp.path());
    for a in before["project"]["assets"].as_array().unwrap() {
        fs::remove_file(root.join(a["path"].as_str().unwrap())).unwrap();
    }
    assert_eq!(
        request(&e, json!({"op":"missing_media_assets"}))["assets"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let single = replace_image(
        &e,
        sources[0].clone(),
        1,
        before["revision"].as_u64().unwrap(),
    );
    assert!(
        single.is_err(),
        "one replacement must not bypass the other missing sources"
    );
    assert_eq!(e.state().unwrap()["project"], before["project"]);
    let document = fs::read(root.join("project.json")).unwrap();
    begin(&e, "recover-all", &before, candidates(&before, &sources));
    assert_eq!(ready(&e, "recover-all")["state"], "ready");
    assert_eq!(
        e.state().unwrap()["project"],
        before["project"],
        "staging must not edit the live document"
    );
    assert_eq!(fs::read(root.join("project.json")).unwrap(), document);
    let after = finish_batch(&e, "recover-all").unwrap()["state"].clone();
    assert_eq!(
        after["revision"].as_u64().unwrap(),
        before["revision"].as_u64().unwrap() + 1
    );
    assert_eq!(after["project"]["layers"], before["project"]["layers"]);
    for (index, a) in after["project"]["assets"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        assert_eq!(a["id"], before["project"]["assets"][index]["id"]);
        assert_eq!(
            image::open(root.join(a["path"].as_str().unwrap()))
                .unwrap()
                .to_rgba8()
                .get_pixel(0, 0)
                .0,
            [255, index as u8, 0, 255]
        );
    }
    release(&e, "recover-all");
    e.save().unwrap();
    assert_eq!(motion_core::storage::load(&root).unwrap().assets.len(), 3);
    assert!(request(&e, json!({"op":"missing_media_assets"}))["assets"]
        .as_array()
        .unwrap()
        .is_empty());
    drop(e);
    assert_eq!(engine(&root).state().unwrap()["project"], after["project"]);
}

#[test]
fn media_asset_relink_decode_failure_cancel_stale_and_commit_failure_preserve_the_document() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let (e, before, sources) = images(temp.path());
    let initial_files = files(&root);
    let document = fs::read(root.join("project.json")).unwrap();
    let bad = temp.path().join("broken.png");
    fs::write(&bad, b"not an image").unwrap();
    let mut inputs = candidates(&before, &sources);
    inputs[1]["path"] = json!(bad);
    begin(&e, "bad-later", &before, inputs);
    let failed = ready(&e, "bad-later");
    assert_eq!(failed["state"], "failed");
    assert!(failed["error"].as_str().unwrap().contains("phase=stage"));
    assert!(finish_batch(&e, "bad-later").is_err());
    release(&e, "bad-later");
    assert_eq!(files(&root), initial_files);
    assert_eq!(e.state().unwrap()["project"], before["project"]);
    assert_eq!(fs::read(root.join("project.json")).unwrap(), document);
    begin(&e, "cancel-ready", &before, candidates(&before, &sources));
    assert_eq!(ready(&e, "cancel-ready")["state"], "ready");
    assert_eq!(
        request(
            &e,
            json!({"op":"cancel_media_task","request_id":"cancel-ready"})
        )["state"],
        "cancelled"
    );
    release(&e, "cancel-ready");
    assert_eq!(files(&root), initial_files);
    begin(&e, "stale-ready", &before, candidates(&before, &sources));
    assert_eq!(ready(&e, "stale-ready")["state"], "ready");
    let changed=e.edit(json!({"op":"rename","object":before["project"]["layers"][0]["id"],"name":"Edited while preparing"}).to_string()).unwrap();
    let stale = finish_batch(&e, "stale-ready").unwrap();
    assert_eq!(stale["asset_operation_cancelled"], true);
    assert_eq!(stale["state"]["project"], changed["project"]);
    release(&e, "stale-ready");
    assert_eq!(files(&root), initial_files);
    // Exclude one missing source to force the unchanged full-catalog preflight
    // to reject publication after two candidates have really been decoded.
    let missing = root.join(changed["project"]["assets"][2]["path"].as_str().unwrap());
    fs::remove_file(missing).unwrap();
    let expected_files = files(&root);
    begin(
        &e,
        "incomplete-set",
        &changed,
        candidates(&changed, &sources[..2]),
    );
    assert_eq!(ready(&e, "incomplete-set")["state"], "ready");
    assert!(finish_batch(&e, "incomplete-set").is_err());
    release(&e, "incomplete-set");
    assert_eq!(
        files(&root),
        expected_files,
        "failed publication must release only newly owned files"
    );
    let actual = e.state().unwrap();
    assert_eq!(actual["project"], changed["project"]);
    assert_eq!(actual["revision"], changed["revision"]);
    assert_eq!(fs::read(root.join("project.json")).unwrap(), document);
}

#[test]
#[cfg(feature = "ffmpeg")]
fn media_asset_relink_mixed_sources_keep_linked_audio_caches_and_one_step_history() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let e = engine(&root);
    let image = temp.path().join("red.png");
    png(&image, [255, 0, 0, 255]);
    e.import_image(image.clone()).unwrap();
    let audio = temp.path().join("sound.wav");
    fs::write(&audio, wave(96_000)).unwrap();
    let video = fixture("video/sound-24fps.mp4");
    for (id, path, kind) in [("audio", &audio, "audio"), ("video", &video, "video")] {
        assert_eq!(start(&e, id, path, kind)["state"], "ready");
        request(&e, json!({"op":"finish_media_import","request_id":id}));
        release(&e, id);
    }
    let before = e.save().unwrap();
    let project: motion_core::Project = serde_json::from_value(before["project"].clone()).unwrap();
    let old_sources: Vec<_> = project
        .assets
        .iter()
        .map(|a| a.path.clone())
        .chain(project.audio_assets.iter().map(|a| a.path.clone()))
        .chain(project.video_assets.iter().map(|a| a.path.clone()))
        .map(|p| (p.clone(), fs::read(root.join(p)).unwrap()))
        .collect();
    let inputs = vec![
        json!({"target":{"kind":"image","id":project.assets[0].id},"path":image}),
        json!({"target":{"kind":"audio","id":project.audio_assets[0].id},"path":audio}),
        json!({"target":{"kind":"video","id":project.video_assets[0].id},"path":video}),
    ];
    begin(&e, "mixed", &before, inputs);
    assert_eq!(ready(&e, "mixed")["state"], "ready");
    assert_eq!(e.state().unwrap()["project"], before["project"]);
    let after = finish_batch(&e, "mixed").unwrap()["state"].clone();
    release(&e, "mixed");
    let final_project: motion_core::Project =
        serde_json::from_value(after["project"].clone()).unwrap();
    assert_eq!(final_project.layers, project.layers);
    assert_eq!(
        final_project.video_assets[0].audio_asset,
        project.video_assets[0].audio_asset
    );
    assert_eq!(final_project.audio_assets.len(), project.audio_assets.len());
    for a in &final_project.audio_assets {
        assert!(motion_media::cache_path(&root, a).unwrap().is_file());
    }
    assert!(
        motion_media::video_cache_path(&root, &final_project.video_assets[0])
            .unwrap()
            .is_file()
    );
    for (path, bytes) in old_sources {
        assert_eq!(fs::read(root.join(path)).unwrap(), bytes);
    }
    assert_eq!(
        after["revision"].as_u64().unwrap(),
        before["revision"].as_u64().unwrap() + 1
    );
    e.save().unwrap();
    assert_eq!(e.history(0).unwrap()["project"], before["project"]);
    assert_eq!(e.history(1).unwrap()["project"], after["project"]);
    e.save().unwrap();
    drop(e);
    assert_eq!(engine(&root).state().unwrap()["project"], after["project"]);
}

#[test]
fn media_asset_relink_default_errors_preserve_causes_and_normal_supersession_stays_quiet() {
    const CHILD: &str = "MOTION_ASSET_RELINK_ERROR_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let (e, before, sources) = images(&root);
        let mut inputs = candidates(&before, &sources);
        inputs[1]["path"] = json!(root.join("actual-missing-relink.png"));
        begin(&e, "default-relink-error", &before, inputs);
        let failed = ready(&e, "default-relink-error");
        assert_eq!(failed["state"], "failed");
        // A terminal poll is read-only and must not repeat the stage Error.
        assert_eq!(ready(&e, "default-relink-error"), failed);
        release(&e, "default-relink-error");
        let mut stale = before.clone();
        stale["revision"] = json!(0);
        let inputs = vec![
            json!({"target":{"kind":"image","id":1},"path":root.join("never-open-stale-relink.png")}),
        ];
        assert_eq!(
            begin(&e, "normal-supersession", &stale, inputs)["asset_operation_cancelled"],
            true
        );
        let mut other = before.clone();
        other["document_generation"] = json!(before["document_generation"].as_u64().unwrap() + 1);
        assert_eq!(
            begin(
                &e,
                "normal-document-replacement",
                &other,
                candidates(&before, &sources)
            )["asset_operation_cancelled"],
            true
        );
        assert_eq!(e.state().unwrap()["project"], before["project"]);
        begin(
            &e,
            "normal-live-document-replacement",
            &before,
            candidates(&before, &sources),
        );
        assert_eq!(
            ready(&e, "normal-live-document-replacement")["state"],
            "ready"
        );
        let changed = e
            .new_project(
                serde_json::to_string(&motion_core::Project::new(640, 360, 30, 120).unwrap())
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(
            request(
                &e,
                json!({"op":"media_status","request_id":"normal-live-document-replacement"})
            )["state"],
            "cancelled"
        );
        assert_eq!(
            finish_batch(&e, "normal-live-document-replacement").unwrap()["state"]["project"],
            changed["project"]
        );
        release(&e, "normal-live-document-replacement");
        fs::write(root.join("failure.json"), failed.to_string()).unwrap();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_ASSET_RELINK_ERROR_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().to_owned());
    fs::create_dir_all(&root).unwrap();
    let output=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","workflow::media_asset_tests::relink::media_asset_relink_default_errors_preserve_causes_and_normal_supersession_stays_quiet"])
        .env_remove("MOTION_LOG").env(CHILD,&root).output().unwrap();
    fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let status: Value =
        serde_json::from_slice(&fs::read(root.join("failure.json")).unwrap()).unwrap();
    let cause = status["error"].as_str().unwrap();
    let log = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        cause.contains("actual-missing-relink.png")
            && cause.contains("asset=")
            && cause.contains("phase=stage")
    );
    assert!(log.contains(cause) && stderr.contains(cause));
    assert_eq!(
        log.matches("asset relink request=default-relink-error asset=")
            .count(),
        1
    );
    assert!(!stdout.contains(cause) && !stdout.contains("motion_studio::engine"));
    assert!(!stderr.contains("normal-live-document-replacement"));
    assert!(
        !log.contains("never-open-stale-relink.png") && !stderr.contains("normal-supersession")
    );
    assert!(
        log.contains("asset relink start request=default-relink-error")
            && !stderr.contains("asset relink start")
    );
    assert!(
        log.find("asset relink start request=default-relink-error")
            .unwrap()
            < log.find(cause).unwrap()
    );
}

#[test]
fn media_asset_relink_cache_collision_preserves_foreign_cache_and_rolls_back_new_sources() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let e = engine(&root);
    let audio = temp.path().join("original.wav");
    fs::write(&audio, wave(96_000)).unwrap();
    assert_eq!(start(&e, "original", &audio, "audio")["state"], "ready");
    let before = request(
        &e,
        json!({"op":"finish_media_import","request_id":"original"}),
    )["state"]
        .clone();
    release(&e, "original");
    begin(
        &e,
        "cache-collision",
        &before,
        vec![
            json!({"target":{"kind":"audio","id":before["project"]["audio_assets"][0]["id"]},"path":audio}),
        ],
    );
    assert_eq!(ready(&e, "cache-collision")["state"], "ready");
    let workspace = fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".asset-relink-")
        })
        .unwrap();
    let cache = workspace.join(format!("cache/audio-v{}", motion_media::PCM_VERSION));
    let key = fs::read_dir(cache)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .file_name();
    let foreign = root
        .join(format!("cache/audio-v{}", motion_media::PCM_VERSION))
        .join(key);
    fs::create_dir(&foreign).unwrap();
    fs::write(foreign.join("decoded.pcm"), b"foreign PCM").unwrap();
    fs::write(foreign.join("decoded.wave"), b"foreign waveform").unwrap();
    let before_assets = files(&root.join("assets"));
    assert!(finish_batch(&e, "cache-collision").is_err());
    release(&e, "cache-collision");
    assert_eq!(
        fs::read(foreign.join("decoded.pcm")).unwrap(),
        b"foreign PCM"
    );
    assert_eq!(
        fs::read(foreign.join("decoded.wave")).unwrap(),
        b"foreign waveform"
    );
    assert_eq!(files(&root.join("assets")), before_assets);
    assert_eq!(e.state().unwrap()["project"], before["project"]);
    e.save().unwrap();
}

#[test]
fn media_asset_relink_document_replacement_keeps_cancellation_queryable_and_releases_old_workspace()
{
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let (e, before, sources) = images(temp.path());
    begin(
        &e,
        "document-replacement",
        &before,
        candidates(&before, &sources),
    );
    assert_eq!(ready(&e, "document-replacement")["state"], "ready");
    let project = motion_core::Project::new(640, 360, 30, 120).unwrap();
    let changed = e
        .new_project(serde_json::to_string(&project).unwrap())
        .unwrap();
    assert_ne!(
        changed["document_generation"],
        before["document_generation"]
    );
    let status = request(
        &e,
        json!({"op":"media_status","request_id":"document-replacement"}),
    );
    assert_eq!(status["state"], "cancelled");
    let result = finish_batch(&e, "document-replacement").unwrap();
    assert_eq!(result["asset_operation_cancelled"], true);
    assert_eq!(result["state"]["project"], changed["project"]);
    release(&e, "document-replacement");
    assert!(!fs::read_dir(root).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".asset-relink-")));
    assert_eq!(e.state().unwrap()["project"], changed["project"]);
}
