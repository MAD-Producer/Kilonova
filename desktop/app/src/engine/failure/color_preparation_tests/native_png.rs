//! Real frozen PNG worker: failures, recovery, and publication before blocking.
use super::*;
use crate::engine::failure::color_tests::runtime_platform::RuntimePlatform;
use motion_core::color_management::ColorTransfer;
use motion_desktop_media::movie::ExportControl;
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
#[ignore = "requires explicit checksum-verified native OCIO prefix and an actual GPU"]
fn managed_png_worker_preserves_default_channels_and_publishes_before_native_work() {
    const CHILD: &str = "MOTION_OCIO_PNG_LOG_CHILD";
    const TEST: &str = "engine::failure::color_preparation_tests::native_authored::png::managed_png_worker_preserves_default_channels_and_publishes_before_native_work";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let active = root.join("active");
        let mut managed = project(&active);
        managed.layers[0].three_d = false;
        let motion_core::color_management::ProjectColorManagement::Ocio { settings } =
            Arc::make_mut(&mut managed.color_management)
        else {
            unreachable!()
        };
        settings.output.transfer = ColorTransfer::ConfigDefined;
        motion_core::storage::save(&active, &managed).unwrap();
        let arm = Arc::new(AtomicBool::new(false));
        let block = arm.clone();
        let (entered, waiting) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let released = std::sync::Mutex::new(released);
        let platform = Arc::new(RuntimePlatform {
            manifest: PathBuf::from(std::env::var_os("MOTION_TEST_OCIO_RUNTIME").unwrap()),
            before_manifest: Some(Arc::new(move || {
                if block.swap(false, Ordering::AcqRel) {
                    entered.send(()).unwrap();
                    released
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(10))
                        .unwrap();
                }
            })),
            desktop: motion_desktop_media::platform(),
        });
        let engine = Engine::start(active.clone(), platform, 8 << 30).unwrap();
        let output = root.join("output.png");
        fs::write(&output, b"existing complete output").unwrap();
        let before = engine.state().unwrap();
        let failure = engine
            .capture_async(output.clone(), ExportControl::default())
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap_err()
            .to_string();
        assert!(
            failure.contains("phase=output_contract") && failure.contains("explicit PNG profile"),
            "{failure}"
        );
        assert_eq!(fs::read(&output).unwrap(), b"existing complete output");
        assert_eq!(engine.state().unwrap()["project"], before["project"]);

        let motion_core::color_management::ProjectColorManagement::Ocio { settings } =
            Arc::make_mut(&mut managed.color_management)
        else {
            unreachable!()
        };
        settings.output.transfer = ColorTransfer::Srgb;
        engine
            .edit(
                serde_json::to_string(&motion_core::Command::SetColorManagement {
                    settings: managed.color_management.clone(),
                })
                .unwrap(),
            )
            .unwrap();
        let before = engine.state().unwrap();
        let resource = &managed.color_management.resources().unwrap()[0];
        let path = active.join(&resource.path);
        let config = fs::read(&path).unwrap();
        fs::remove_file(&path).unwrap();
        let missing = engine
            .capture_async(output.clone(), ExportControl::default())
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap_err()
            .to_string();
        assert!(missing.contains(&resource.path), "{missing}");
        assert_eq!(fs::read(&output).unwrap(), b"existing complete output");
        assert_eq!(engine.state().unwrap()["project"], before["project"]);
        fs::write(&path, config).unwrap();

        // Block the actual export provider only after session preparation.
        arm.store(true, Ordering::Release);
        let reply = engine
            .capture_async(output.clone(), ExportControl::default())
            .unwrap();
        waiting.recv_timeout(Duration::from_secs(30)).unwrap();
        let flight = *engine.frame_flight.lock().unwrap();
        let file = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
        release.send(()).unwrap();
        let flight = flight.expect("PNG in-flight state before native provider");
        assert_eq!(flight.operation, "capture");
        assert!(
            file.contains(&format!(
                "async start #{} capture phase=freeze_render_publish",
                flight.request
            )),
            "{file}"
        );
        let result = reply
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap();
        assert_eq!(result["color"]["storage"]["alpha"], "straight");
        assert_eq!(image::open(&output).unwrap().width(), 64);
        assert_eq!(engine.state().unwrap()["project"], before["project"]);
        let began = Instant::now();
        while engine.frame_capture_in_flight().is_some() {
            assert!(began.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(1));
        }
        let complete = fs::read(&output).unwrap();
        arm.store(true, Ordering::Release);
        let cancel = ExportControl::default();
        let reply = engine
            .capture_async(output.clone(), cancel.clone())
            .unwrap();
        waiting.recv_timeout(Duration::from_secs(30)).unwrap();
        let cancelling = engine.frame_capture_in_flight().unwrap().0;
        assert!(cancel.cancel());
        release.send(()).unwrap();
        assert_eq!(
            reply
                .recv_timeout(Duration::from_secs(30))
                .unwrap()
                .unwrap_err()
                .to_string(),
            "PNG export cancelled"
        );
        assert_eq!(fs::read(&output).unwrap(), complete);
        assert_eq!(engine.state().unwrap()["project"], before["project"]);
        fs::write(
            root.join("report.json"),
            serde_json::to_vec(&serde_json::json!({
                "failures":[failure,missing],"request":flight.request,"result":result,
                "cancelledRequest":cancelling,
            }))
            .unwrap(),
        )
        .unwrap();
        drop(engine);
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            TEST,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .stdout(fs::File::create(root.path().join("child.stdout")).unwrap())
        .stderr(fs::File::create(root.path().join("child.stderr")).unwrap())
        .spawn()
        .unwrap();
    let began = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if began.elapsed() > Duration::from_secs(120) {
            let killed = child.kill();
            let joined = child.wait();
            panic!("PNG log child exceeded 120s: kill={killed:?} wait={joined:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = fs::read_to_string(root.path().join("child.stdout")).unwrap();
    let stderr = fs::read_to_string(root.path().join("child.stderr")).unwrap();
    let file = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    if let Some(directory) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
        let directory = PathBuf::from(directory).join("managed-png-worker");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("motion-studio.log"), &file).unwrap();
        fs::write(directory.join("child.stdout"), &stdout).unwrap();
        fs::write(directory.join("child.stderr"), &stderr).unwrap();
        for name in ["report.json", "output.png"] {
            if root.path().join(name).exists() {
                fs::copy(root.path().join(name), directory.join(name)).unwrap();
            }
        }
    }
    assert!(status.success(), "{status}\n{stdout}\n{stderr}");
    let report: Value =
        serde_json::from_slice(&fs::read(root.path().join("report.json")).unwrap()).unwrap();
    let valid = |file: &str, stderr: &str, stdout: &str| {
        file.contains("logging initialized level=INFO")
            && file.contains(&format!(
                "cancelled #{} capture",
                report["cancelledRequest"].as_u64().unwrap()
            ))
            && !file
                .lines()
                .any(|line| line.contains("ERROR") && line.contains("PNG export cancelled"))
            && !stderr.contains("PNG export cancelled")
            && report["failures"].as_array().unwrap().iter().all(|error| {
                let error = error.as_str().unwrap();
                file.lines().any(|line| {
                    line.contains("ERROR") && line.contains(" capture ") && line.contains(error)
                }) && stderr.contains(error)
                    && !stdout.contains(error)
            })
            && !stdout.contains("async start #")
            && !stderr.contains("async start #")
    };
    assert!(valid(&file, &stderr, &stdout), "{file}\n{stderr}\n{stdout}");
    assert!(!valid(&file.replace("ERROR", "DEBUG"), &stderr, &stdout));
    assert!(!valid(&file, "", &stdout));
    assert!(!valid(&file, &stderr, &format!("{stdout}{stderr}")));
    assert!(!valid(
        &file.replace("explicit PNG profile", "hidden cause"),
        &stderr,
        &stdout
    ));
}
