use super::*;
use std::error::Error;

#[test]
fn envelopes_reject_corruption_truncation_and_oversize_without_driver_calls() {
    let identity = [7; 32];
    let bytes = encode(&identity, b"codec fixture only, never sent to GPU").unwrap();
    assert_eq!(
        decode(&bytes, &identity).unwrap().unwrap(),
        &bytes[HEADER_BYTES..]
    );
    assert!(decode(&bytes, &[8; 32]).unwrap().is_none());
    for length in 0..bytes.len() {
        assert!(
            decode(&bytes[..length], &identity).is_err(),
            "truncation={length}"
        );
    }
    for index in [0, 40, 48, HEADER_BYTES] {
        let mut corrupt = bytes.clone();
        corrupt[index] ^= 1;
        assert!(decode(&corrupt, &identity).is_err(), "corrupt byte={index}");
    }
    let oversized = vec![0; MAX_DRIVER_PIPELINE_CACHE_BYTES + HEADER_BYTES + 1];
    assert!(matches!(
        decode(&oversized, &identity),
        Err(DriverPipelineCacheError::TooLarge)
    ));
    assert!(matches!(
        encode(&identity, &oversized),
        Err(DriverPipelineCacheError::TooLarge)
    ));
}

#[test]
fn atomic_publication_keeps_native_io_cause_and_removes_failed_temporary() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("cache");
    write_atomic(&path, b"first").unwrap();
    write_atomic(&path, b"second").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"second");
    let error = write_atomic(&root.path().join("missing/cache"), b"ignored").unwrap_err();
    assert_eq!(
        error
            .source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap()
            .kind(),
        std::io::ErrorKind::NotFound
    );
    let directory_target = root.path().join("directory");
    fs::create_dir(&directory_target).unwrap();
    let error = write_atomic(&directory_target, b"ignored").unwrap_err();
    assert!(error
        .source()
        .unwrap()
        .downcast_ref::<std::io::Error>()
        .is_some());
    let mut names: Vec<_> = fs::read_dir(root.path())
        .unwrap()
        .map(|x| x.unwrap().file_name())
        .collect();
    names.sort();
    assert_eq!(names, ["cache", "directory"]);
    assert_eq!(fs::read(&path).unwrap(), b"second");
}

pub(super) fn scene() -> crate::Scene {
    let package = motion_effects::builtin::package().unwrap();
    let mut project = motion_model::Project::new(16, 16, 30, 60).unwrap();
    let mut layer = motion_model::Layer::solid(
        1,
        "driver cache",
        [16.; 2],
        [8., 8., 0.],
        [0.2, 0.5, 0.8, 0.5],
    );
    for (index, name) in ["invert", "tint"].into_iter().enumerate() {
        let definition = package
            .manifest
            .effects
            .iter()
            .find(|x| x.id == name)
            .unwrap();
        layer.effects.push(motion_model::EffectInstance::new(
            index as u64 + 1,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            definition,
            layer.size,
        ));
    }
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    let mut scene = crate::Scene::new(&project).unwrap();
    scene
        .sample(&project, 0.25, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_effect_cache_reopens_and_rejects_corruption_without_losing_rendering() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("pipeline.cache");
    let scene = scene();
    let mut cold = pollster::block_on(crate::Renderer::headless()).unwrap();
    cold.set_pointwise_fusion(true);
    let target = cold.capture_target(16, 16).unwrap();
    let (expected, _) = cold.capture(&scene, &target).unwrap();
    assert!(cold.effect_pipeline_builds() > 0);
    assert!(expected.chunks_exact(4).any(|p| p[3] != 0));
    let info = cold.driver_pipeline_cache_info();
    if !info.supported {
        assert!(info.adapter_key.is_none());
        assert_eq!(info.effect_pipeline_creations, 0);
        assert!(!cold.save_driver_pipeline_cache(&path).unwrap());
        // SAFETY: this nonexistent private path is created only by this test.
        assert_eq!(
            unsafe { cold.restore_driver_pipeline_cache(&path) }.unwrap(),
            DriverPipelineCacheLoad::Unsupported
        );
        assert!(!path.exists());
        return;
    }
    assert_eq!(cold.adapter_info.backend, wgpu::Backend::Vulkan);
    assert!(info.effect_pipeline_creations > 0);
    assert!(cold.save_driver_pipeline_cache(&path).unwrap());
    let original = fs::read(&path).unwrap();
    assert!(original.len() > HEADER_BYTES);
    assert!(original.len() <= HEADER_BYTES + MAX_DRIVER_PIPELINE_CACHE_BYTES);
    let mut warm = pollster::block_on(crate::Renderer::headless()).unwrap();
    warm.set_pointwise_fusion(true);
    // SAFETY: exact get_data output persisted above, with private directory ownership.
    assert_eq!(
        unsafe { warm.restore_driver_pipeline_cache(&path) }.unwrap(),
        DriverPipelineCacheLoad::Restored
    );
    let warm_target = warm.capture_target(16, 16).unwrap();
    assert_eq!(warm.capture(&scene, &warm_target).unwrap().0, expected);
    assert!(warm.driver_pipeline_cache_info().effect_pipeline_creations > 0);
    let mut corrupt = original.clone();
    corrupt[HEADER_BYTES] ^= 1;
    fs::write(&path, &corrupt).unwrap();
    // SAFETY: controlled accidental-corruption fixture; checksum rejection must
    // precede the unsafe driver call. No forged checksummed blob is submitted.
    let error = unsafe { warm.restore_driver_pipeline_cache(&path) }.unwrap_err();
    assert!(matches!(
        error,
        crate::RenderError::DriverPipelineCache(DriverPipelineCacheError::Invalid("checksum"))
    ));
    assert_eq!(warm.capture(&scene, &warm_target).unwrap().0, expected);
    let mut stale = original;
    stale[8] ^= 1;
    fs::write(&path, &stale).unwrap();
    // SAFETY: original driver data is unchanged; only the application identity differs.
    assert_eq!(
        unsafe { warm.restore_driver_pipeline_cache(&path) }.unwrap(),
        DriverPipelineCacheLoad::Incompatible
    );
    assert_eq!(warm.capture(&scene, &warm_target).unwrap().0, expected);
    fs::remove_file(&path).unwrap();
    // SAFETY: missing private cache is a normal miss, with no GPU data supplied.
    assert_eq!(
        unsafe { warm.restore_driver_pipeline_cache(&path) }.unwrap(),
        DriverPipelineCacheLoad::Missing
    );
    #[cfg(all(debug_assertions, feature = "gpu-tests"))]
    let invalid_shader = {
        let previous_creations = warm.driver_pipeline_cache_info().effect_pipeline_creations;
        warm.debug_inject_pointwise_native_failure();
        let failure = warm.preflight_effects(&scene, 16, 16).unwrap_err();
        assert!(
            failure.to_string().contains("effect program pointwise-v"),
            "{failure}"
        );
        assert_eq!(
            warm.driver_pipeline_cache_info().effect_pipeline_creations,
            previous_creations,
            "failed shader must not be installed/counted as a successful cached pipeline"
        );
        Some(failure.to_string())
    };
    #[cfg(not(all(debug_assertions, feature = "gpu-tests")))]
    let invalid_shader: Option<String> = None;
    assert_eq!(warm.capture(&scene, &warm_target).unwrap().0, expected);
    if let Some(evidence) = std::env::var_os("MOTION_DRIVER_CACHE_EVIDENCE") {
        fs::write(
            evidence,
            serde_json::to_vec_pretty(&serde_json::json!({
                "adapter": { "name": cold.adapter_info.name,
                    "backend": format!("{:?}", cold.adapter_info.backend),
                    "driver": cold.adapter_info.driver, "driverInfo": cold.adapter_info.driver_info },
                "cold": info,
                "warm": warm.driver_pipeline_cache_info(), "snapshotBytes": stale.len(),
                "pixelBytes": expected.len(), "invalidShader": invalid_shader,
                "scope": "same-process independent devices; not a cold-process timing benchmark"
            }))
            .unwrap(),
        )
        .unwrap();
    }
    cold.check_health().unwrap();
    warm.check_health().unwrap();
}
