use super::*;
use motion_render::DriverPipelineCacheLoad;
use std::{
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};

pub(super) fn storage(root: &Path, count: Arc<AtomicUsize>) -> DriverPipelineCacheStorage {
    let root = root.to_owned();
    // SAFETY: this controlled fixture only contains snapshots produced in this
    // test. It is never configured by a project/import or reused across tests.
    unsafe {
        DriverPipelineCacheStorage::new(
            root,
            Arc::new(move |path| {
                count.fetch_add(1, Ordering::SeqCst);
                std::fs::create_dir_all(path.parent().unwrap())
            }),
        )
    }
}

fn wait_cache(state: &mut State, renderer: &mut Renderer, phase: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        state.collect(renderer, key().context).unwrap();
        if state.status()["driverCache"]["task"]["phase"] == phase {
            return;
        }
        assert!(Instant::now() < deadline, "{}", state.status());
        std::thread::yield_now();
    }
}

#[test]
fn persistent_cache_uses_the_same_worker_and_checkpoints_only_completed_new_work() {
    let fixture_dir = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let (project, mut scene) = fixture();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut state = State::default();
    state.configure_cache(storage(fixture_dir.path(), calls.clone()));
    finish(&mut state, &mut renderer, &project, &scene, key());
    if !renderer.driver_pipeline_cache_info().supported {
        assert_eq!(state.status()["driverCache"]["load"], "unsupported");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        return;
    }
    assert_eq!(state.status()["driverCache"]["load"], "missing");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "preparation must not publish before a complete frame"
    );
    let worker = state.worker.as_ref().unwrap().thread.thread().id();
    assert_ne!(worker, std::thread::current().id());
    let target = renderer.capture_target(16, 16).unwrap();
    let before = renderer.effect_pipeline_builds();
    let current = renderer.capture(&scene, &target).unwrap().0;
    scene
        .sample(&project, 30., None, &ExpressionEvaluator)
        .unwrap();
    let future = renderer.capture(&scene, &target).unwrap().0;
    assert_ne!(current, future);
    assert_eq!(
        renderer.effect_pipeline_builds(),
        before,
        "native foreground compilation occurred"
    );
    state.frame_presented(&mut renderer, key().context).unwrap();
    wait_cache(&mut state, &mut renderer, "saved");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let info = renderer.driver_pipeline_cache_info();
    let path = fixture_dir
        .path()
        .join(format!("{}.cache", info.adapter_key.unwrap()));
    assert!(std::fs::metadata(&path).unwrap().len() > 80);
    for _ in 0..65 {
        state.frame_presented(&mut renderer, key().context).unwrap();
        state.collect(&mut renderer, key().context).unwrap();
    }
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "healthy Present performed repeated IO"
    );
    assert_eq!(state.worker.as_ref().unwrap().thread.thread().id(), worker);

    let mut warm = pollster::block_on(Renderer::headless()).unwrap();
    let mut second = State::default();
    second.configure_cache(storage(fixture_dir.path(), calls.clone()));
    scene
        .sample(&project, 0., None, &ExpressionEvaluator)
        .unwrap();
    finish(&mut second, &mut warm, &project, &scene, key());
    assert_eq!(second.status()["driverCache"]["load"], "restored");
    let target = warm.capture_target(16, 16).unwrap();
    let prepared = warm.effect_pipeline_builds();
    assert_eq!(warm.capture(&scene, &target).unwrap().0, current);
    scene
        .sample(&project, 30., None, &ExpressionEvaluator)
        .unwrap();
    assert_eq!(warm.capture(&scene, &target).unwrap().0, future);
    assert_eq!(warm.effect_pipeline_builds(), prepared);
    warm.check_health().unwrap();
    if let Some(evidence) = std::env::var_os("MOTION_HOST_CACHE_EVIDENCE") {
        let evidence = std::path::PathBuf::from(evidence);
        std::fs::create_dir_all(&evidence).unwrap();
        for (name, pixels) in [("current.rgba", &current), ("future.rgba", &future)] {
            std::fs::write(evidence.join(name), pixels).unwrap();
        }
        std::fs::write(evidence.join("persistent.json"), serde_json::to_vec_pretty(&json!({
            "adapter":renderer.adapter_info.name,"backend":format!("{:?}",renderer.adapter_info.backend),
            "driver":renderer.adapter_info.driver,"driverInfo":renderer.adapter_info.driver_info,
            "supported":true,"cold":state.status(),"warm":second.status(),
            "storageCalls":calls.load(Ordering::SeqCst),"snapshotBytes":std::fs::metadata(path).unwrap().len(),
            "allCurrentAndFuturePixelsEqual":true,"foregroundCreatesDuringCapture":0
        })).unwrap()).unwrap();
    }
}

#[test]
fn blocked_restore_is_observable_and_cancelled_results_cannot_install_after_replacement() {
    let fixture_dir = tempfile::tempdir().unwrap();
    let (project, scene) = fixture();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    if !renderer.driver_pipeline_cache_info().supported {
        return;
    }
    let (entered, observed) = mpsc::sync_channel(0);
    let (release, gate) = mpsc::sync_channel(0);
    let gate = std::sync::Mutex::new(Some(gate));
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let root = fixture_dir.path().to_owned();
    // SAFETY: same isolated snapshot fixture as storage() above. The callback
    // blocks only its first operation so cancellation can be tested precisely.
    let storage = unsafe {
        DriverPipelineCacheStorage::new(
            root,
            Arc::new(move |_| {
                count.fetch_add(1, Ordering::SeqCst);
                let wait = gate.lock().unwrap().take();
                if let Some(wait) = wait {
                    entered.send(()).unwrap();
                    wait.recv().unwrap();
                }
                Ok(())
            }),
        )
    };
    let mut state = State::default();
    state.configure_cache(storage);
    assert!(!state
        .ensure(&mut renderer, &project, &scene, None, key())
        .unwrap());
    observed.recv_timeout(Duration::from_secs(60)).unwrap();
    assert_eq!(state.status()["preparing"], true);
    assert_eq!(state.status()["driverCache"]["task"]["phase"], "reading");
    assert_eq!(renderer.effect_pipeline_builds(), 0);
    let worker = state.worker.as_ref().unwrap().thread.thread().id();
    for _ in 0..65 {
        assert!(!state
            .ensure(&mut renderer, &project, &scene, None, key())
            .unwrap());
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    state.cancel();
    let mut replacement = key();
    replacement.context.document_generation += 1;
    assert!(!state
        .ensure(&mut renderer, &project, &scene, None, replacement.clone())
        .unwrap());
    release.send(()).unwrap();
    finish(
        &mut state,
        &mut renderer,
        &project,
        &scene,
        replacement.clone(),
    );
    assert_eq!(
        state.status()["driverCache"]["context"]["documentGeneration"],
        replacement.context.document_generation
    );
    assert_eq!(state.status()["driverCache"]["load"], "missing");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(state.worker.as_ref().unwrap().thread.thread().id(), worker);
}

#[test]
fn corrupt_cache_is_retained_as_a_typed_error_then_replaced_by_a_distinct_checkpoint() {
    let fixture_dir = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let (project, scene) = fixture();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    if !renderer.driver_pipeline_cache_info().supported {
        return;
    }
    let path = fixture_dir.path().join(format!(
        "{}.cache",
        renderer.driver_pipeline_cache_info().adapter_key.unwrap()
    ));
    std::fs::write(&path, b"corrupt envelope, must never reach driver").unwrap();
    let mut state = State::default();
    state.configure_cache(storage(fixture_dir.path(), calls.clone()));
    finish(&mut state, &mut renderer, &project, &scene, key());
    assert_eq!(state.status()["driverCache"]["disabled"], false);
    assert!(state.status()["driverCache"]["lastError"]
        .as_str()
        .unwrap()
        .contains("invalid pipeline cache"));
    let target = renderer.capture_target(16, 16).unwrap();
    let pixels = renderer.capture(&scene, &target).unwrap().0;
    state.frame_presented(&mut renderer, key().context).unwrap();
    wait_cache(&mut state, &mut renderer, "saved");
    assert!(state.status()["driverCache"]["lastError"]
        .as_str()
        .unwrap()
        .contains("invalid pipeline cache"));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let mut cold = pollster::block_on(Renderer::headless()).unwrap();
    // SAFETY: file was atomically replaced by the preceding successful native
    // checkpoint; corrupt fixture bytes were never passed into cache creation.
    assert_eq!(
        unsafe { cold.restore_driver_pipeline_cache(&path) }.unwrap(),
        DriverPipelineCacheLoad::Restored
    );
    let target = cold.capture_target(16, 16).unwrap();
    assert_eq!(cold.capture(&scene, &target).unwrap().0, pixels);
}

#[test]
fn storage_failure_disables_cache_io_without_losing_cause_or_ordinary_rendering() {
    let fixture_dir = tempfile::tempdir().unwrap();
    let (project, scene) = fixture();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    if !renderer.driver_pipeline_cache_info().supported {
        return;
    }
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let missing = fixture_dir.path().join("absent/parent");
    // SAFETY: callback fails on a real missing path before any driver bytes are
    // read. No imported data or shared cache is ever opened by this fixture.
    let storage = unsafe {
        DriverPipelineCacheStorage::new(
            fixture_dir.path().to_owned(),
            Arc::new(move |_| {
                count.fetch_add(1, Ordering::SeqCst);
                std::fs::read(&missing).map(|_| ())
            }),
        )
    };
    let mut state = State::default();
    state.configure_cache(storage);
    finish(&mut state, &mut renderer, &project, &scene, key());
    let original = state.status()["driverCache"]["lastError"].clone();
    assert!(original.as_str().unwrap().contains("prepare storage"));
    assert_eq!(state.status()["driverCache"]["disabled"], true);
    assert_eq!(state.status()["driverCache"]["task"]["phase"], "failed");
    let before = renderer.effect_pipeline_builds();
    let target = renderer.capture_target(16, 16).unwrap();
    assert_eq!(
        renderer.capture(&scene, &target).unwrap().0.len(),
        16 * 16 * 4
    );
    for _ in 0..65 {
        assert!(state
            .ensure(&mut renderer, &project, &scene, None, key())
            .unwrap());
        state.frame_presented(&mut renderer, key().context).unwrap();
        assert_eq!(state.status()["driverCache"]["lastError"], original);
    }
    assert_eq!(renderer.effect_pipeline_builds(), before);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    // Keep the exact underlying OS cause available, not merely its display text.
    let cause = state.cache.failure().unwrap();
    assert!(
        matches!(cause.as_ref(), Error::Render(motion_render::RenderError::DriverPipelineCache(
        motion_render::DriverPipelineCacheError::Io { source, .. })) if source.kind()==std::io::ErrorKind::NotFound)
    );
    renderer.check_health().unwrap();
}
