//! Native fault injection plus actual desktop sink/Session ordering. This is
//! diagnostic GPU-buffer corruption, not evidence of a driver/hardware fault.
use super::*;
use motion_render::{AllocationFrameOutcome, FrameOperation};

const NATIVE_RECORD: &str = "render GPU command readback";

fn inject(renderer: &Renderer, request: u64) {
    let frame = renderer
        .allocation_budget()
        .begin_frame_at(request, FrameOperation::Render, 1.25)
        .unwrap()
        .unwrap();
    let identity = renderer.debug_inject_invalid_command_words().unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    renderer.check_health().unwrap();
    frame.finish(AllocationFrameOutcome::Skipped);
    let snapshot = renderer.command_observer().unwrap().snapshot();
    let record = snapshot
        .commands
        .recent
        .iter()
        .find(|record| record.epoch == identity.epoch && record.id == identity.id)
        .unwrap();
    assert_eq!(
        record.phase,
        motion_render::gpu_commands::CommandPhase::Invalid
    );
    assert_eq!(record.timestamp_words, Some([0, 0]));
    assert!(record.gpu_elapsed_us.is_none());
    assert_eq!(record.contexts[0].request, Some(request));
}

#[test]
fn native_readback_sixty_errors_reach_default_sinks_only_after_guard_exit_and_recover() {
    const CHILD: &str = "MOTION_GPU_NATIVE_FILE_CHILD";
    const TEST: &str = "logging::gpu_command_tests::native_readback::native_readback_sixty_errors_reach_default_sinks_only_after_guard_exit_and_recover";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        assert!(std::env::var_os("MOTION_LOG").is_none());
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
        fs::create_dir(&project_root).unwrap();
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
            for occurrence in 1..=59 {
                inject(renderer, 7000 + occurrence);
            }
            assert!(!fs::read_to_string(&path).unwrap().contains(NATIVE_RECORD));
            Ok(())
        })
        .unwrap();
        assert!(!fs::read_to_string(&path).unwrap().contains(NATIVE_RECORD));
        let primary = motion_host::session::with_session::<()>(id, |session| {
            inject(session.editor_renderer.as_ref().unwrap(), 7060);
            assert_eq!(
                observer.snapshot().commands.consecutive_readback_failures,
                60
            );
            assert!(
                !fs::read_to_string(&path).unwrap().contains(NATIVE_RECORD),
                "sink ran under real Session guard"
            );
            Err("original failure after diagnostic readback".into())
        })
        .unwrap_err();
        assert_eq!(
            primary.to_string(),
            "original failure after diagnostic readback"
        );
        assert_eq!(
            fs::read_to_string(&path)
                .unwrap()
                .matches(NATIVE_RECORD)
                .count(),
            1
        );
        motion_host::session::with_session(id, |session| {
            let renderer = session.editor_renderer.as_ref().unwrap();
            for occurrence in 61..=70 {
                inject(renderer, 7000 + occurrence);
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(
            fs::read_to_string(&path)
                .unwrap()
                .matches(NATIVE_RECORD)
                .count(),
            1
        );
        motion_host::session::with_session(id, |session| {
            let renderer = session.editor_renderer.as_ref().unwrap();
            let target = renderer.capture_target(8, 8)?;
            let mut encoder =
                renderer.allocation_command_encoder(&wgpu::CommandEncoderDescriptor::default())?;
            drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("diagnostic threshold real recovery"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 1.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            }));
            renderer.submit_allocated(encoder.finish(), |_| {})?;
            renderer
                .device
                .poll(wgpu::PollType::wait_indefinitely())
                .map_err(|cause| {
                    motion_host::Error::with_context("diagnostic recovery poll", cause)
                })?;
            assert_eq!(
                observer.snapshot().commands.consecutive_readback_failures,
                0
            );
            assert!(renderer
                .read_target(&target)?
                .chunks_exact(4)
                .all(|pixel| pixel == [255, 0, 0, 255]));
            Ok(())
        })
        .unwrap();
        motion_host::session::with_session(id, |session| {
            let renderer = session.editor_renderer.as_ref().unwrap();
            for occurrence in 1..=59 {
                inject(renderer, 9000 + occurrence);
            }
            assert_eq!(
                fs::read_to_string(&path)
                    .unwrap()
                    .matches(NATIVE_RECORD)
                    .count(),
                1
            );
            Ok(())
        })
        .unwrap();
        assert_eq!(
            fs::read_to_string(&path)
                .unwrap()
                .matches(NATIVE_RECORD)
                .count(),
            1
        );
        motion_host::session::with_session(id, |session| {
            inject(session.editor_renderer.as_ref().unwrap(), 9060);
            assert_eq!(
                fs::read_to_string(&path)
                    .unwrap()
                    .matches(NATIVE_RECORD)
                    .count(),
                1
            );
            Ok(())
        })
        .unwrap();
        let snapshot = observer.snapshot();
        assert_eq!(snapshot.commands.counts.invalid, 130);
        assert_eq!(snapshot.readback.invalid_samples, 130);
        assert!(snapshot.readback.valid_samples >= 1);
        assert_eq!(snapshot.commands.consecutive_readback_failures, 60);
        assert!(!snapshot.commands.observation_fault);
        fs::write(
            root.join("commands.json"),
            serde_json::to_vec_pretty(&snapshot).unwrap(),
        )
        .unwrap();
        motion_host::session::close(id);
        flush();
        let file = fs::read_to_string(&path).unwrap();
        assert_eq!(file.matches(NATIVE_RECORD).count(), 2);
        assert_eq!(file.matches("count=60").count(), 2);
        assert!(!file.contains("count=1 words="));
        assert!(file.contains("request: Some(7060)"));
        assert!(file.contains("request: Some(9060)"));
        assert!(file.contains("cause=invalid native timestamp words"));
        fs::write(root.join("report.json"),serde_json::to_vec_pretty(&serde_json::json!({
            "available":true,"actualInvalidNativeSamples":130,"actualValidNativeSamples":snapshot.readback.valid_samples,
            "actualRecoveryPixelsChecked":64,"defaultInfo":true,"realSession":true,"errorEpisodes":2,
            "guardExitRequiredForFileRecord":true,"primaryFailurePreserved":true,
            "scope":"actual native diagnostic-word corruption, occurrence60 Error suppression/reset and real default desktop file/stderr sinks; not a driver/hardware fault"
        })).unwrap()).unwrap();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = match std::env::var_os("MOTION_GPU_NATIVE_FILE_ARTIFACTS") {
        Some(path) => {
            let path = PathBuf::from(path);
            assert!(!path.exists());
            fs::create_dir(&path).unwrap();
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
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("native readback file child timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = fs::read_to_string(stdout).unwrap();
    let console = fs::read_to_string(stderr).unwrap();
    assert!(status.success(), "child status={status}\n{output}\n{console}");
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("report.json")).unwrap()).unwrap();
    if report["available"] != true {
        eprintln!("GPU_NATIVE_FILE available=false; no native threshold file acceptance");
        return;
    }
    let file = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    for (text, count) in [(&file, 2), (&console, 2), (&output, 0)] {
        assert_eq!(text.matches(NATIVE_RECORD).count(), count, "{text}");
    }
    eprintln!("GPU_NATIVE_FILE {report}");
}
