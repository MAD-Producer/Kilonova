use super::*;

fn fixture(root: &Path) -> FrozenMovie {
    fs::create_dir_all(root.join("assets")).unwrap();
    image::RgbaImage::from_pixel(8, 8, image::Rgba([220, 40, 80, 255]))
        .save(root.join("assets/image.png"))
        .unwrap();
    let mut project = Project::new(32, 24, 30, 4).unwrap();
    project.assets.push(motion_core::Asset {
        id: 1,
        path: "assets/image.png".into(),
        width: 8,
        height: 8,
    });
    let mut layer = motion_core::Layer::solid(1, "Durable image", [8.; 2], [16., 12., 0.], [1.; 4]);
    layer.content = motion_core::Content::Image { asset: 1 };
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|effect| effect.id == "dither")
        .unwrap();
    layer.effects.push(motion_core::EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [8.; 2],
    ));
    project.layers.push(layer);
    project.work_area = motion_core::WorkArea {
        in_frame: 1,
        out_frame: 3,
    };
    project.rebuild_plugin_dependencies();
    MovieFreezePlan::new(
        project,
        root,
        Registry::new_with_builtins().unwrap(),
        MemoryPolicy::default(),
        42,
        &ExportControl::default(),
    )
    .unwrap()
    .freeze(&ExportControl::default())
    .unwrap()
    .with_scope(motion_core::FrameScope::WorkArea)
}

#[test]
fn durable_movie_restores_exact_model_range_packages_and_source_bytes_after_owner_exits() {
    let original = tempfile::tempdir().unwrap();
    let queue = tempfile::tempdir().unwrap();
    let frozen = fixture(original.path());
    let project = frozen.project.clone();
    let pixels = fs::read(original.path().join("assets/image.png")).unwrap();
    let prepared = frozen
        .prepare_persistent(queue.path(), &ExportControl::default())
        .unwrap();
    let saved = prepared.publish().unwrap();
    // The original editor, its resource files and the in-memory frozen object go away.
    drop(frozen);
    original.close().unwrap();
    let restored =
        FrozenMovie::restore_persistent_with_request(&saved, &ExportControl::default(), 101)
            .unwrap();
    assert_eq!(restored.observation_request, 101);
    assert_eq!(restored.project, project);
    assert_eq!(restored.revision, 42);
    assert_eq!(restored.frames(), 2);
    assert_eq!(restored.range, project.work_area);
    assert_eq!(restored.memory_policy, MemoryPolicy::default());
    assert_eq!(
        fs::read(restored.assets.path().join("assets/image.png")).unwrap(),
        pixels
    );
    for required in &project.plugin_dependencies {
        let package = restored
            .registry
            .resolve(&required.plugin, &required.version, &required.hash)
            .unwrap();
        assert_eq!(
            package.bytes,
            motion_effects::builtin::package().unwrap().bytes
        );
    }
    assert_eq!(fs::read_dir(queue.path()).unwrap().count(), 1);
}

#[test]
fn durable_movie_preparation_cancel_and_unsaved_queue_drop_clean_only_owned_inputs() {
    let source = tempfile::tempdir().unwrap();
    let queue = tempfile::tempdir().unwrap();
    let foreign = queue.path().join("foreign");
    fs::create_dir(&foreign).unwrap();
    fs::write(foreign.join("keep"), b"unrelated data").unwrap();
    let frozen = fixture(source.path());
    let prepared = frozen
        .prepare_persistent(queue.path(), &ExportControl::default())
        .unwrap();
    let owned = prepared.path().to_owned();
    assert!(owned.join(MANIFEST).is_file());
    // A failed queue document publication drops its still-owned preparation.
    drop(prepared);
    assert!(!owned.exists());
    let cancelled = ExportControl::default();
    assert!(cancelled.cancel());
    assert_eq!(
        frozen
            .prepare_persistent(queue.path(), &cancelled)
            .err()
            .as_deref(),
        Some("movie export cancelled")
    );
    assert_eq!(fs::read_dir(queue.path()).unwrap().count(), 1);
    assert_eq!(fs::read(foreign.join("keep")).unwrap(), b"unrelated data");
    assert!(frozen
        .prepare_persistent(&foreign.join("keep"), &ExportControl::default())
        .is_err());
    assert_eq!(fs::read(foreign.join("keep")).unwrap(), b"unrelated data");
}

#[test]
fn durable_movie_restore_rejects_changed_payload_missing_packages_and_invalid_manifest() {
    let source = tempfile::tempdir().unwrap();
    let queue = tempfile::tempdir().unwrap();
    let frozen = fixture(source.path());
    let saved = frozen
        .prepare_persistent(queue.path(), &ExportControl::default())
        .unwrap()
        .publish()
        .unwrap();
    let manifest_path = saved.join(MANIFEST);
    let original = fs::read(&manifest_path).unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&original).unwrap();
    for mutation in ["version", "traversal", "duplicate", "missing_package"] {
        let mut bad = manifest.clone();
        match mutation {
            "version" => bad["version"] = 999.into(),
            "traversal" => bad["files"][0]["path"] = "../foreign".into(),
            "duplicate" => {
                let entry = bad["files"][0].clone();
                bad["files"].as_array_mut().unwrap().push(entry);
            }
            "missing_package" => bad["files"]
                .as_array_mut()
                .unwrap()
                .retain(|e| !e["path"].as_str().unwrap().starts_with("packages/")),
            _ => unreachable!(),
        }
        fs::write(&manifest_path, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(
            FrozenMovie::restore_persistent(&saved, &ExportControl::default()).is_err(),
            "{mutation}"
        );
    }
    fs::write(&manifest_path, &original).unwrap();
    let asset = saved.join("assets/image.png");
    let bytes = fs::read(&asset).unwrap();
    image::RgbaImage::from_pixel(8, 8, image::Rgba([40, 220, 80, 255]))
        .save(&asset)
        .unwrap();
    let error = FrozenMovie::restore_persistent(&saved, &ExportControl::default())
        .err()
        .unwrap();
    assert!(error.contains("integrity failure"), "{error}");
    fs::write(asset, bytes).unwrap();
    let restored = FrozenMovie::restore_persistent(&saved, &ExportControl::default()).unwrap();
    assert_eq!(restored.project, frozen.project);
    let cancelled = ExportControl::default();
    cancelled.cancel();
    assert_eq!(
        FrozenMovie::restore_persistent(&saved, &cancelled)
            .err()
            .as_deref(),
        Some("movie export cancelled")
    );
}

#[cfg(feature = "ffmpeg")]
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn durable_movie_decoded_pixels_match_original_after_source_and_owner_removal() {
    let source = tempfile::tempdir().unwrap();
    let queue = tempfile::tempdir().unwrap();
    let frozen = fixture(source.path());
    let saved = frozen
        .prepare_persistent(queue.path(), &ExportControl::default())
        .unwrap()
        .publish()
        .unwrap();
    let original_path = queue.path().join("before.mp4");
    frozen
        .export(&original_path, &ExportControl::default(), |_| {})
        .unwrap();
    source.close().unwrap();
    let restored = FrozenMovie::restore_persistent(&saved, &ExportControl::default()).unwrap();
    let reopened_path = queue.path().join("reopened.mp4");
    let result = restored
        .export(&reopened_path, &ExportControl::default(), |_| {})
        .unwrap();
    assert_eq!((result.revision, result.frames), (42, 2));
    let decode = |path: &Path| {
        let probe = crate::av::probe(path, None, None, &|| Ok(())).unwrap();
        assert_eq!(probe.asset.frame_count, 2);
        assert_eq!(probe.asset.video_end_us, 66_667);
        assert_eq!(
            (probe.asset.color_standard, probe.asset.color_range),
            (1, 2)
        );
        let timestamps = probe.timestamps.clone();
        assert_eq!(timestamps.len(), 2);
        let mut decoder = crate::av::Decoder::open(path, probe.asset, probe.timestamps).unwrap();
        timestamps
            .into_iter()
            .map(|pts| {
                decoder
                    .frame(pts, &|| Ok(()))
                    .unwrap()
                    .rgba()
                    .unwrap()
                    .to_vec()
            })
            .collect::<Vec<_>>()
    };
    let expected = decode(&original_path);
    assert!(expected[0]
        .chunks_exact(4)
        .any(|p| p[0] > 100 && p[0] > p[1]));
    assert_eq!(decode(&reopened_path), expected);
}
