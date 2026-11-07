use super::*;
use crate::{PipelinePreparationContext, Renderer};
use std::{sync::mpsc, time::Duration};

fn run(job: DriverPipelineCacheJob) -> Result<DriverPipelineCacheCompletion> {
    let (tx, rx) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let _ = tx.send(job.run());
    });
    let result = rx
        .recv_timeout(Duration::from_secs(60))
        .expect("cache task did not return");
    worker
        .join()
        .expect("cache task escaped its panic boundary");
    result
}
fn loaded(result: DriverPipelineCacheCompletion) -> LoadedDriverPipelineCache {
    match result {
        DriverPipelineCacheCompletion::Loaded(loaded) => loaded,
        _ => panic!("expected a loaded cache receipt"),
    }
}
fn failure(result: Result<DriverPipelineCacheCompletion>) -> DriverPipelineCacheError {
    match result {
        Err(error) => error,
        Ok(_) => panic!("expected cache failure"),
    }
}

#[test]
fn saturated_ordinals_do_not_authorize_obsolete_tasks() {
    let mut state = TaskState::default();
    state.next_request = u64::MAX - 1;
    let first = state.begin(DriverPipelineCacheOperation::Restore);
    let second = state.begin(DriverPipelineCacheOperation::Save);
    assert!(first.status().request_saturated && second.status().request_saturated);
    assert_eq!(first.status().request, second.status().request);
    assert!(!first.same_request(&second));
    assert!(first.cancelled() && !state.accepts(&first));
    assert!(state.accepts(&second));
    drop(state);
    assert!(second.cancelled());
}

#[test]
fn cancelled_atomic_publication_preserves_the_previous_file_and_cleans_temporary() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("cache");
    write_atomic(&path, b"previous").unwrap();
    let checks = std::cell::Cell::new(0);
    assert!(!write_atomic_if(&path, b"cancelled", || {
        checks.set(checks.get() + 1);
        checks.get() == 1
    })
    .unwrap());
    assert_eq!(
        checks.get(),
        2,
        "must check again immediately before publication"
    );
    assert_eq!(fs::read(&path).unwrap(), b"previous");
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    assert!(!write_atomic_if(&root.path().join("absent/cache"), b"unused", || false).unwrap());
    assert!(!root.path().join("absent").exists());
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn worker_cache_roundtrip_preserves_pixels_and_reports_actual_corruption() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("cache");
    let input = super::super::tests::scene();
    let mut cold = pollster::block_on(Renderer::headless()).unwrap();
    cold.set_pointwise_fusion(true);
    let target = cold.capture_target(16, 16).unwrap();
    let expected = cold.capture(&input, &target).unwrap().0;
    assert!(expected.chunks_exact(4).any(|pixel| pixel[3] != 0));
    let supported = cold.driver_pipeline_cache_info().supported;
    let job = cold.driver_pipeline_cache_save_job(&path);
    let save_ticket = job.ticket();
    assert_eq!(save_ticket.status().phase, DriverPipelineCachePhase::Queued);
    assert!(!path.exists(), "creating a save job must not perform IO");
    let snapshot_bytes = match run(job).unwrap() {
        DriverPipelineCacheCompletion::Saved { snapshot_bytes } => {
            assert!(supported);
            assert_eq!(save_ticket.status().phase, DriverPipelineCachePhase::Saved);
            assert_eq!(fs::metadata(&path).unwrap().len(), snapshot_bytes as u64);
            assert!(
                snapshot_bytes > HEADER_BYTES
                    && snapshot_bytes <= HEADER_BYTES + MAX_DRIVER_PIPELINE_CACHE_BYTES
            );
            snapshot_bytes
        }
        DriverPipelineCacheCompletion::Unsupported => {
            assert!(!supported && !path.exists());
            0
        }
        _ => panic!("save failed to complete"),
    };
    let mut warm = pollster::block_on(Renderer::headless()).unwrap();
    warm.set_pointwise_fusion(true);
    let preparation = warm
        .prepare_effect_pipeline_job(
            vec![input.clone()],
            16,
            16,
            PipelinePreparationContext {
                document_generation: 71,
                project_revision: 5,
            },
        )
        .unwrap();
    let preparation_ticket = preparation.ticket();
    // SAFETY: file was produced above in this test's exclusively owned directory.
    let job = unsafe { warm.driver_pipeline_cache_restore_job(&path) };
    let restore_ticket = job.ticket();
    let receipt = loaded(run(job).unwrap());
    let cache_handle = receipt.cache.clone();
    let expected_result = if supported {
        DriverPipelineCacheLoad::Restored
    } else {
        DriverPipelineCacheLoad::Unsupported
    };
    assert_eq!(receipt.result(), expected_result);
    assert_eq!(
        restore_ticket.status().phase,
        DriverPipelineCachePhase::Ready
    );
    assert!(
        !preparation_ticket.status().cancelled,
        "worker load must not mutate the owner"
    );
    assert_eq!(
        warm.install_loaded_driver_pipeline_cache(receipt).unwrap(),
        DriverPipelineCacheInstall::Finished(expected_result)
    );
    assert_eq!(
        preparation_ticket.status().cancelled,
        supported,
        "only an actual cache replacement cancels old preparation"
    );
    drop(preparation);
    assert_eq!(
        restore_ticket.status().phase,
        DriverPipelineCachePhase::Installed
    );
    if supported {
        assert_eq!(
            warm.driver_pipeline_cache.cache, cache_handle,
            "install must use the worker-created native handle"
        );
        assert_eq!(warm.effect_gpu.state.driver_pipeline_cache, cache_handle);
    }
    let warm_target = warm.capture_target(16, 16).unwrap();
    assert_eq!(warm.capture(&input, &warm_target).unwrap().0, expected);
    if supported {
        assert!(warm.driver_pipeline_cache_info().effect_pipeline_creations > 0);
    }
    let cause = if supported {
        let original = fs::read(&path).unwrap();
        let mut corrupt = original.clone();
        corrupt[HEADER_BYTES] ^= 1;
        fs::write(&path, &corrupt).unwrap();
        // SAFETY: controlled bit corruption; checksum must reject before driver access.
        let job = unsafe { warm.driver_pipeline_cache_restore_job(&path) };
        let ticket = job.ticket();
        let error = failure(run(job));
        assert!(matches!(
            error,
            DriverPipelineCacheError::Invalid("checksum")
        ));
        assert_eq!(ticket.status().phase, DriverPipelineCachePhase::Failed);
        assert_eq!(warm.driver_pipeline_cache.cache, cache_handle);
        for _ in 0..65 {
            assert_eq!(ticket.status().phase, DriverPipelineCachePhase::Failed);
        }
        let cause = error.to_string();
        let mut incompatible = original;
        incompatible[8] ^= 1;
        fs::write(&path, &incompatible).unwrap();
        // SAFETY: only the envelope identity changed; original driver bytes untouched.
        let receipt =
            loaded(run(unsafe { warm.driver_pipeline_cache_restore_job(&path) }).unwrap());
        assert_eq!(receipt.result(), DriverPipelineCacheLoad::Incompatible);
        assert_eq!(
            warm.install_loaded_driver_pipeline_cache(receipt).unwrap(),
            DriverPipelineCacheInstall::Finished(DriverPipelineCacheLoad::Incompatible)
        );
        assert_eq!(warm.driver_pipeline_cache.cache, cache_handle);
        fs::remove_file(&path).unwrap();
        // SAFETY: absent private file is a normal miss, never submitted to the driver.
        let receipt =
            loaded(run(unsafe { warm.driver_pipeline_cache_restore_job(&path) }).unwrap());
        assert_eq!(receipt.result(), DriverPipelineCacheLoad::Missing);
        warm.install_loaded_driver_pipeline_cache(receipt).unwrap();
        assert_eq!(warm.driver_pipeline_cache.cache, cache_handle);
        Some(cause)
    } else {
        None
    };
    assert_eq!(warm.capture(&input, &warm_target).unwrap().0, expected);
    warm.check_health().unwrap();
    cold.check_health().unwrap();
    if let Some(evidence) = std::env::var_os("MOTION_DRIVER_TASK_EVIDENCE") {
        let evidence = PathBuf::from(evidence);
        fs::create_dir_all(&evidence).unwrap();
        fs::write(evidence.join("worker-cache-rgba.bin"), &expected).unwrap();
        fs::write(evidence.join("worker-cache.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "adapter": { "name": warm.adapter_info.name, "backend": format!("{:?}", warm.adapter_info.backend),
                "driver": warm.adapter_info.driver, "driverInfo": warm.adapter_info.driver_info },
            "supported": supported, "snapshotBytes": snapshot_bytes,
            "cold": cold.driver_pipeline_cache_info(), "warm": warm.driver_pipeline_cache_info(),
            "save": save_ticket.status(), "restore": restore_ticket.status(), "corruption": cause,
            "scope": "real worker IO and independent devices; not independent-process startup timing"
        })).unwrap()).unwrap();
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn blocking_cancel_drop_and_owner_thread_failures_are_bounded_and_recover() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("cache");
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut job = renderer.driver_pipeline_cache_save_job(&path);
    let ticket = job.ticket();
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    job.before_operation = Some(Box::new(move || {
        entered_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    }));
    let worker = std::thread::spawn(move || job.run());
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(
        ticket.status().phase,
        DriverPipelineCachePhase::Snapshotting
    );
    assert!(!path.exists());
    renderer.cancel_driver_pipeline_cache_task();
    release_tx.send(()).unwrap();
    assert!(matches!(
        worker.join().unwrap().unwrap(),
        DriverPipelineCacheCompletion::Cancelled
    ));
    assert_eq!(ticket.status().phase, DriverPipelineCachePhase::Cancelled);
    assert!(!path.exists());
    let job = renderer.driver_pipeline_cache_save_job(&path);
    let ticket = job.ticket();
    assert!(matches!(
        failure(job.run()),
        DriverPipelineCacheError::OwnerThread
    ));
    assert_eq!(ticket.status().phase, DriverPipelineCachePhase::Failed);
    assert!(!path.exists());
    let mut job = renderer.driver_pipeline_cache_save_job(&path);
    job.before_operation = Some(Box::new(|| panic!("controlled driver cache worker panic")));
    let ticket = job.ticket();
    let error = failure(run(job));
    assert!(
        matches!(error, DriverPipelineCacheError::Panicked { ref reason }
        if reason == "controlled driver cache worker panic")
    );
    assert_eq!(ticket.status().phase, DriverPipelineCachePhase::Failed);
    let job = renderer.driver_pipeline_cache_save_job(&path);
    let ticket = job.ticket();
    drop(job);
    assert!(ticket.status().cancelled);
    assert_eq!(ticket.status().phase, DriverPipelineCachePhase::Cancelled);
    let job = renderer.driver_pipeline_cache_save_job(&path);
    let ticket = job.ticket();
    assert!(matches!(
        run(job).unwrap(),
        DriverPipelineCacheCompletion::Saved { .. } | DriverPipelineCacheCompletion::Unsupported
    ));
    assert!(matches!(
        ticket.status().phase,
        DriverPipelineCachePhase::Saved | DriverPipelineCachePhase::Ready
    ));
    let job = renderer.driver_pipeline_cache_save_job(&root.path().join("owner-gone"));
    let ticket = job.ticket();
    drop(renderer);
    assert!(matches!(
        run(job).unwrap(),
        DriverPipelineCacheCompletion::Cancelled
    ));
    assert_eq!(ticket.status().phase, DriverPipelineCachePhase::Cancelled);
    assert!(!root.path().join("owner-gone").exists());
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn load_receipts_reject_foreign_format_superseded_and_abandoned_owners() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("cache");
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut foreign = pollster::block_on(Renderer::headless()).unwrap();
    let before = renderer.driver_pipeline_cache.cache.clone();
    // SAFETY: all these paths are absent and exclusively owned by this test.
    let restore = |renderer: &mut Renderer| {
        loaded(run(unsafe { renderer.driver_pipeline_cache_restore_job(&path) }).unwrap())
    };
    let receipt = restore(&mut renderer);
    assert!(matches!(
        foreign.install_loaded_driver_pipeline_cache(receipt),
        Err(crate::RenderError::DriverPipelineCache(
            DriverPipelineCacheError::ForeignRenderer
        ))
    ));
    let receipt = restore(&mut renderer);
    let old_format = renderer.target_format;
    renderer.target_format = if old_format == wgpu::TextureFormat::Rgba8Unorm {
        wgpu::TextureFormat::Rgba16Float
    } else {
        wgpu::TextureFormat::Rgba8Unorm
    };
    assert_eq!(
        renderer
            .install_loaded_driver_pipeline_cache(receipt)
            .unwrap(),
        DriverPipelineCacheInstall::Superseded
    );
    renderer.target_format = old_format;
    let receipt = restore(&mut renderer);
    let next = renderer.driver_pipeline_cache_save_job(&path);
    assert_eq!(
        renderer
            .install_loaded_driver_pipeline_cache(receipt)
            .unwrap(),
        DriverPipelineCacheInstall::Superseded
    );
    drop(next);
    let receipt = restore(&mut renderer);
    // SAFETY: same absent private file; sync restore must invalidate old receipts too.
    unsafe { renderer.restore_driver_pipeline_cache(&path) }.unwrap();
    assert_eq!(
        renderer
            .install_loaded_driver_pipeline_cache(receipt)
            .unwrap(),
        DriverPipelineCacheInstall::Superseded
    );
    assert_eq!(renderer.driver_pipeline_cache.cache, before);
    let receipt = restore(&mut renderer);
    let ticket = receipt.ticket.clone();
    drop(receipt);
    assert_eq!(ticket.status().phase, DriverPipelineCachePhase::Cancelled);
    assert!(ticket.status().cancelled);
    let context = PipelinePreparationContext {
        document_generation: 70,
        project_revision: 4,
    };
    let preparation = renderer
        .prepare_effect_pipeline_job(vec![super::super::tests::scene()], 16, 16, context)
        .unwrap();
    let preparation_ticket = preparation.ticket();
    let receipt = restore(&mut renderer);
    renderer
        .install_loaded_driver_pipeline_cache(receipt)
        .unwrap();
    assert!(
        !preparation_ticket.status().cancelled,
        "normal miss must not discard valid preparation"
    );
    drop(preparation);
    renderer.check_health().unwrap();
}
