//! Real desktop sink and real Session guard ordering for deferred metadata
//! faults. Native map-corruption/threshold tests remain separate evidence.
use super::*;
use motion_render::{gpu_commands::ExposureCommandContext, Renderer};
use std::process::{Command, Stdio};

#[cfg(all(debug_assertions, feature = "gpu-tests"))]
mod native_readback;

const RECORD: &str = "render GPU command observation invariant";

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn deferred_command_faults_reach_default_file_and_console_after_the_real_session_guard() {
    const CHILD: &str = "MOTION_GPU_COMMAND_FILE_CHILD";
    const TEST: &str = "logging::gpu_command_tests::deferred_command_faults_reach_default_file_and_console_after_the_real_session_guard";
    if let Some(root) = std::env::var_os(CHILD) {
        assert!(std::env::var_os("MOTION_LOG").is_none());
        let root = PathBuf::from(root);
        let _logger = init(root.join("logs")).unwrap();
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let mut renderer = pollster::block_on(Renderer::new_profiled(
            &instance,
            None,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            true,
        ))
        .unwrap();
        let adapter = renderer.adapter.get_info();
        fs::write(root.join("adapter.json"),serde_json::to_vec_pretty(&serde_json::json!({
            "name":adapter.name,"driver":adapter.driver,"driverInfo":adapter.driver_info,
            "backend":format!("{:?}",adapter.backend),"vendor":adapter.vendor,"device":adapter.device,
        })).unwrap()).unwrap();
        let Some(observer) = renderer.start_command_profiling(1).unwrap() else {
            fs::write(root.join("report.json"), b"{\"available\":false}").unwrap();
            return;
        };
        let project_root = root.join("project");
        fs::create_dir_all(&project_root).unwrap();
        let id = motion_host::session::open(
            project_root,
            motion_core::Project::new(32, 32, 30, 60).unwrap(),
            motion_desktop_media::platform(),
        )
        .unwrap();
        motion_host::session::with_session(id, |session| {
            session.editor_renderer = Some(renderer);
            Ok(())
        })
        .unwrap();
        let path = root.join("logs/motion-studio.log");
        motion_host::session::with_session(id, |session| {
            let renderer = session.editor_renderer.as_ref().unwrap();
            let _healthy = renderer.exposure_command_scope(ExposureCommandContext {
                request: Some(1),
                poll: Some(1),
            });
            Ok(())
        })
        .unwrap();
        assert!(!fs::read_to_string(&path).unwrap().contains(RECORD));
        let primary = motion_host::session::with_session::<()>(id, |session| {
            let renderer = session.editor_renderer.as_ref().unwrap();
            for _ in 0..60 {
                let _unknown = renderer.exposure_command_scope(ExposureCommandContext {
                    request: None,
                    poll: None,
                });
            }
            // Check the real file before returning the real business guard.
            // No logger is called here: the actual deferred path must wait.
            assert!(!fs::read_to_string(&path).unwrap().contains(RECORD));
            Err("original command test failure".into())
        })
        .unwrap_err();
        assert_eq!(primary.to_string(), "original command test failure");
        assert_eq!(
            fs::read_to_string(&path).unwrap().matches(RECORD).count(),
            1
        );
        assert_eq!(
            observer.snapshot().commands.counts.omitted_exposure_scopes,
            60
        );
        assert!(observer
            .snapshot()
            .commands
            .active_exposure_scopes
            .is_empty());
        // This is a sticky epoch invariant, not a consecutive-readback episode.
        // More invalid metadata in the same epoch must not flood either sink.
        motion_host::session::with_session(id, |session| {
            for _ in 0..60 {
                let _unknown = session
                    .editor_renderer
                    .as_ref()
                    .unwrap()
                    .exposure_command_scope(ExposureCommandContext {
                        request: None,
                        poll: None,
                    });
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(
            observer.snapshot().commands.counts.omitted_exposure_scopes,
            120
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap().matches(RECORD).count(),
            1
        );
        let new_epoch = motion_host::session::with_session(id, |session| {
            let renderer = session.editor_renderer.as_mut().unwrap();
            let observer = renderer.start_command_profiling(1)?.unwrap();
            let _unknown = renderer.exposure_command_scope(ExposureCommandContext {
                request: None,
                poll: None,
            });
            Ok(observer.snapshot().commands.epoch)
        })
        .unwrap();
        assert_ne!(new_epoch, observer.snapshot().commands.epoch);
        assert_eq!(
            fs::read_to_string(&path).unwrap().matches(RECORD).count(),
            2
        );
        fs::write(
            root.join("first-epoch.json"),
            serde_json::to_vec_pretty(&observer.snapshot()).unwrap(),
        )
        .unwrap();
        motion_host::session::close(id);
        flush();
        fs::write(root.join("report.json"),serde_json::to_vec_pretty(&serde_json::json!({
            "available":true,"metadataFaultEpochs":2,"firstEpochOmissions":120,
            "defaultInfo":true,"realSession":true,"guardExitRequiredForFileRecord":true,
            "originalFailurePreserved":true,
            "scope":"actual deferred metadata-invariant faults through real Session and desktop logger; not native map failure or occurrence-60 file acceptance"
        })).unwrap()).unwrap();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = match std::env::var_os("MOTION_GPU_COMMAND_FILE_ARTIFACTS") {
        Some(path) => {
            let path = PathBuf::from(path);
            assert!(!path.exists(), "acceptance output must be new");
            fs::create_dir_all(&path).unwrap();
            path
        }
        None => temporary.path().to_path_buf(),
    };
    let stdout = root.join("stdout.txt");
    let stderr = root.join("stderr.txt");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture"])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
        .stdout(Stdio::from(File::create(&stdout).unwrap()))
        .stderr(Stdio::from(File::create(&stderr).unwrap()))
        .spawn()
        .unwrap();
    let began = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if began.elapsed() > Duration::from_secs(60) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("deferred command file child did not finish");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = fs::read_to_string(stdout).unwrap();
    let console = fs::read_to_string(stderr).unwrap();
    assert!(status.success(), "{output}\n{console}");
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("report.json")).unwrap()).unwrap();
    if report["available"] != true {
        eprintln!("GPU_COMMAND_FILE available=false; no actual file integration acceptance");
        return;
    }
    let file = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    for (text, count) in [(&file, 2), (&console, 2), (&output, 0)] {
        assert_eq!(text.matches(RECORD).count(), count, "{text}");
    }
    assert!(!output.contains("exact_coverage=false"));
    assert!(file.contains("phase=metadata exact_coverage=false"));
    eprintln!("GPU_COMMAND_FILE {report}");
}
