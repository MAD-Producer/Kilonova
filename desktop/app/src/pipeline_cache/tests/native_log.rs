use super::*;
use motion_render::{
    DriverPipelineCacheCompletion, DriverPipelineCacheError, DriverPipelineCacheLoad,
    DriverPipelineCachePhase, PipelinePreparationContext, Renderer,
};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

#[test]
fn storage_validation_publishes_inflight_and_preserves_default_file_stderr_boundaries() {
    const CHILD: &str = "MOTION_PRIVATE_CACHE_LOG_CHILD";
    const CASE: &str = "pipeline_cache::tests::native_log::storage_validation_publishes_inflight_and_preserves_default_file_stderr_boundaries";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logging = crate::logging::init(root.join("logs")).unwrap();
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        let supported = renderer.driver_pipeline_cache_info().supported;
        fs::write(
            root.join("backend.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "supported":supported,"adapter":renderer.adapter_info.name,
                "backend":format!("{:?}",renderer.adapter_info.backend)
            }))
            .unwrap(),
        )
        .unwrap();
        if !supported {
            // The same real owner-thread error is observable even when this
            // backend has no driver cache. It does not qualify storage IO.
            let job = renderer.driver_pipeline_cache_save_job(&root.join("unsupported"));
            let ticket = job.ticket();
            assert!(matches!(
                job.run_with_storage(|_| panic!("owner must be rejected before storage")),
                Err(DriverPipelineCacheError::OwnerThread)
            ));
            for _ in 0..65 {
                assert_eq!(ticket.status().phase, DriverPipelineCachePhase::Failed);
            }
            let job = renderer.driver_pipeline_cache_save_job(&root.join("unsupported"));
            assert!(matches!(
                std::thread::spawn(
                    move || job.run_with_storage(|_| panic!("unsupported must not touch storage"))
                )
                .join()
                .unwrap()
                .unwrap(),
                DriverPipelineCacheCompletion::Unsupported
            ));
            crate::logging::flush();
            return;
        }
        assert!(
            renderer.driver_pipeline_cache_info().supported,
            "native Vulkan cache required"
        );
        let path = root.join("insecure/driver-pipeline-cache-v1/cache");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let context = PipelinePreparationContext {
            document_generation: 73,
            project_revision: 42,
        };
        // SAFETY: no data can be read; validation must refuse this inherited
        // public test leaf before any snapshot is handed to the native driver.
        let job =
            unsafe { renderer.driver_pipeline_cache_restore_job(&path) }.with_context(context);
        let ticket = job.ticket();
        let request = ticket.status().request;
        let (entered, observed) = mpsc::sync_channel(0);
        let (release, gate) = mpsc::sync_channel(0);
        let worker = std::thread::spawn(move || {
            job.run_with_storage(|path| {
                entered.send(()).unwrap();
                gate.recv().unwrap();
                prepare(path)
            })
        });
        observed.recv_timeout(Duration::from_secs(60)).unwrap();
        assert_ne!(worker.thread().id(), std::thread::current().id());
        assert_eq!(ticket.status().phase, DriverPipelineCachePhase::Reading);
        crate::logging::flush();
        let start = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
        assert!(start.contains(&format!("driver pipeline cache start request={request}")));
        assert!(start.contains("document_generation: 73, project_revision: 42"));
        assert!(!start.contains("driver pipeline cache failed"));
        fs::write(root.join("published-before-handler-return.txt"), start).unwrap();
        release.send(()).unwrap();
        let error = match worker.join().unwrap() {
            Err(error) => error,
            Ok(_) => panic!("insecure cache accepted"),
        };
        assert!(
            matches!(error, DriverPipelineCacheError::Io { operation: "prepare storage", ref source, .. }
            if source.kind()==io::ErrorKind::PermissionDenied)
        );
        for _ in 0..65 {
            assert_eq!(ticket.status().phase, DriverPipelineCachePhase::Failed);
        }
        // An explicit new operation uses a distinct, correctly protected leaf.
        let healthy = root.join("healthy/driver-pipeline-cache-v1/cache");
        let job =
            unsafe { renderer.driver_pipeline_cache_restore_job(&healthy) }.with_context(context);
        let result = std::thread::spawn(move || job.run_with_storage(prepare))
            .join()
            .unwrap()
            .unwrap();
        let DriverPipelineCacheCompletion::Loaded(loaded) = result else {
            panic!("missing healthy cache must produce a load receipt")
        };
        assert_eq!(loaded.result(), DriverPipelineCacheLoad::Missing);
        renderer
            .install_loaded_driver_pipeline_cache(loaded)
            .unwrap();
        renderer.check_health().unwrap();
        crate::logging::flush();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let stdout = root.path().join("stdout");
    let stderr = root.path().join("stderr");
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", CASE, "--nocapture"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .stdout(fs::File::create(&stdout).unwrap())
        .stderr(fs::File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    let exit = loop {
        if let Some(exit) = child.try_wait().unwrap() {
            break exit;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            child.wait().unwrap();
            panic!("private cache log child did not return");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let errors = fs::read_to_string(&stderr).unwrap();
    assert!(exit.success(), "{errors}");
    let file = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    let backend: serde_json::Value =
        serde_json::from_slice(&fs::read(root.path().join("backend.json")).unwrap()).unwrap();
    if backend["supported"] == true {
        assert!(root
            .path()
            .join("published-before-handler-return.txt")
            .is_file());
        assert!(file.contains("prepare storage") && errors.contains("prepare storage"));
    } else {
        assert!(
            file.contains("must execute away from its renderer owner")
                && errors.contains("must execute away from its renderer owner")
        );
    }
    assert_eq!(file.matches("driver pipeline cache failed").count(), 1);
    assert_eq!(errors.matches("driver pipeline cache failed").count(), 1);
    assert!(!errors.contains("driver pipeline cache start"));
    assert!(
        !file.contains("driver pipeline cache done"),
        "Debug leaked into default Info"
    );
    let output = fs::read_to_string(&stdout).unwrap();
    assert!(
        !output.contains("driver pipeline cache start")
            && !output.contains("driver pipeline cache failed")
    );
    if let Some(evidence) = std::env::var_os("MOTION_PRIVATE_CACHE_LOG_EVIDENCE") {
        let evidence = PathBuf::from(evidence);
        fs::create_dir_all(&evidence).unwrap();
        let mut files = vec![
            ("stdout", "stdout"),
            ("stderr", "stderr"),
            ("logs/motion-studio.log", "motion-studio.log"),
            ("backend.json", "backend.json"),
        ];
        if backend["supported"] == true {
            files.push((
                "published-before-handler-return.txt",
                "published-before-handler-return.txt",
            ));
        }
        for (source, target) in files {
            fs::copy(root.path().join(source), evidence.join(target)).unwrap();
        }
    }
}
