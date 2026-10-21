//! Real command failures through the desktop worker and process logger.
use super::*;
use motion_core::composition::{Composition, MAX_COMPOSITIONS};

mod effect_admission;
mod frame_blend_admission;

#[test]
fn exposure_uniform_admission_failure_reaches_reply_file_and_stderr_at_default_level() {
    const CHILD: &str = "MOTION_EXPOSURE_UNIFORM_ADMISSION_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let _logger = crate::logging::init(PathBuf::from(root)).unwrap();
        let budget = motion_render::AllocationBudget::new(31).unwrap();
        let mut renderer = pollster::block_on(
            motion_render::Renderer::headless_with_allocation_budget(budget.clone()),
        ).unwrap();
        let failure = renderer.exposure_target(1, 1, &[1.]).err().unwrap();
        assert_eq!(failure.resource_rejection(), Some(&motion_render::ResourceRejection::Budget {
            scope: motion_render::ResourceBudgetScope::RenderAllocations,
            required_bytes: 32, budget_bytes: 31,
        }));
        assert_eq!(budget.info().unwrap().admitted_bytes, 4);
        assert_eq!(renderer.exposure_texture_bytes(), 0);
        let original = failure.to_string();
        for context in ["exposure 1x1 phase=allocate", "buffer size=16 phase=allocate label=exposure phase weight", "required_bytes=32", "budget_bytes=31"] {
            assert!(original.contains(context), "missing {context}: {original}");
        }
        let (reply, receive) = channel::<Result<Value, String>>();
        CURRENT_COMMAND.set(Some(Flight {
            request: 203, operation: "test_exposure_target", started: Instant::now(),
        }));
        respond(reply, Err(failure.into()));
        CURRENT_COMMAND.set(None);
        assert_eq!(receive.recv_timeout(Duration::from_secs(5)).unwrap().unwrap_err(), original);
        budget.set_limit(32).unwrap();
        let target = renderer.exposure_target(1, 1, &[1.]).unwrap();
        assert_eq!(budget.info().unwrap().admitted_bytes, 32);
        drop(target);
        assert_eq!(budget.info().unwrap().admitted_bytes, 4);
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "engine::failure::composition_tests::exposure_uniform_admission_failure_reaches_reply_file_and_stderr_at_default_level"])
        .env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let cause = "admitted render allocations exceed configured combined budget";
    let error = file.lines().find(|line| line.contains("error #203 test_exposure_target ")).unwrap();
    for context in [cause, "exposure 1x1 phase=allocate", "buffer size=16 phase=allocate label=exposure phase weight", "required_bytes=32", "budget_bytes=31", "ms:"] {
        assert!(error.contains(context), "missing {context}: {error}");
    }
    assert!(file.contains("logging initialized level=INFO"));
    assert!(stderr.contains(error.split("motion_studio::engine: ").nth(1).unwrap()));
    assert_eq!(stderr.matches(cause).count(), 1);
    assert!(!stderr.contains("logging initialized"));
    for marker in [cause, "error #203", "logging initialized"] {
        assert!(!stdout.contains(marker), "log output leaked to stdout: {stdout}");
    }
}

#[test]
fn video_uniform_admission_failure_reaches_reply_file_and_stderr_at_default_level() {
    const CHILD: &str = "MOTION_VIDEO_UNIFORM_ADMISSION_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let _logger = crate::logging::init(PathBuf::from(root)).unwrap();
        let budget = motion_render::AllocationBudget::new(89).unwrap();
        let mut renderer = pollster::block_on(
            motion_render::Renderer::headless_with_allocation_budget(budget.clone()),
        ).unwrap();
        renderer.upload_video_frame(2, 11, 0, 2, 2, &[0, 255, 0, 255].repeat(4)).unwrap();
        let before = renderer.texture_memory_breakdown();
        let frame = motion_render::Yuv420Frame {
            width: 2, height: 2, rotation: 0, standard: 1, range: 2,
            phase: [0, 0], chroma_layout: motion_render::ChromaLayout::Uv,
            y: vec![128; 4], uv: vec![128; 2],
        };
        let failure = renderer.upload_video_yuv(2, 11, 1, &frame).unwrap_err();
        assert_eq!(failure.resource_rejection(), Some(&motion_render::ResourceRejection::Budget {
            scope: motion_render::ResourceBudgetScope::RenderAllocations,
            required_bytes: 90, budget_bytes: 89,
        }));
        assert_eq!(renderer.texture_memory_breakdown(), before);
        assert_eq!(budget.info().unwrap().admitted_bytes, 20);
        let original = failure.to_string();
        for context in ["object=2 source=11 pts_us=1 phase=yuv_prepare", "buffer size=48 phase=allocate", "required_bytes=90", "budget_bytes=89"] {
            assert!(original.contains(context), "missing {context}: {original}");
        }
        let (reply, receive) = channel::<Result<Value, String>>();
        CURRENT_COMMAND.set(Some(Flight {
            request: 202, operation: "test_upload_yuv", started: Instant::now(),
        }));
        respond(reply, Err(failure.into()));
        CURRENT_COMMAND.set(None);
        assert_eq!(receive.recv_timeout(Duration::from_secs(5)).unwrap().unwrap_err(), original);
        budget.set_limit(90).unwrap();
        renderer.upload_video_yuv(2, 11, 1, &frame).unwrap();
        renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        renderer.check_health().unwrap();
        assert_eq!(renderer.video_gpu_conversions, 1);
        assert_eq!(budget.info().unwrap().admitted_bytes, 74);
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "engine::failure::composition_tests::video_uniform_admission_failure_reaches_reply_file_and_stderr_at_default_level"])
        .env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let cause = "admitted render allocations exceed configured combined budget";
    let error = file.lines().find(|line| line.contains("error #202 test_upload_yuv ")).unwrap();
    for context in [cause, "object=2 source=11 pts_us=1 phase=yuv_prepare", "buffer size=48 phase=allocate", "required_bytes=90", "budget_bytes=89", "ms:"] {
        assert!(error.contains(context), "missing {context}: {error}");
    }
    assert!(file.contains("logging initialized level=INFO"));
    assert!(stderr.contains(error.split("motion_studio::engine: ").nth(1).unwrap()));
    assert_eq!(stderr.matches(cause).count(), 1);
    assert!(!stderr.contains("logging initialized"));
    for marker in [cause, "error #202", "logging initialized"] {
        assert!(!stdout.contains(marker), "log output leaked to stdout: {stdout}");
    }
}

#[test]
fn combined_allocation_failure_reaches_reply_file_and_stderr_at_default_level() {
    const CHILD: &str = "MOTION_COMBINED_ALLOCATION_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let _logger = crate::logging::init(PathBuf::from(root)).unwrap();
        let budget = motion_render::AllocationBudget::new(12).unwrap();
        let mut renderer = pollster::block_on(
            motion_render::Renderer::headless_with_allocation_budget(budget.clone()),
        )
        .unwrap();
        let before = budget.info().unwrap();
        let failure = renderer
            .upload_image(7, 2, 2, &[0, 255, 0, 255].repeat(4))
            .unwrap_err();
        assert!(matches!(
            failure.resource_rejection(),
            Some(motion_render::ResourceRejection::Budget {
                scope: motion_render::ResourceBudgetScope::RenderAllocations,
                required_bytes: 20,
                budget_bytes: 12,
            })
        ));
        assert_eq!(budget.info().unwrap(), before);
        assert_eq!(before.admitted_bytes, 4);
        let original = failure.to_string();
        for context in [
            "Static(7)",
            "size=2x2",
            "phase=allocate",
            "required_bytes=20",
            "budget_bytes=12",
        ] {
            assert!(original.contains(context), "missing {context}: {original}");
        }
        let (reply, receive) = channel::<Result<Value, String>>();
        CURRENT_COMMAND.set(Some(Flight {
            request: 201,
            operation: "test_upload_image",
            started: Instant::now(),
        }));
        respond(reply, Err(failure.into()));
        CURRENT_COMMAND.set(None);
        assert_eq!(
            receive
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap_err(),
            original
        );
        budget.set_limit(20).unwrap();
        renderer
            .upload_image(7, 2, 2, &[0, 255, 0, 255].repeat(4))
            .unwrap();
        assert_eq!(budget.info().unwrap().admitted_bytes, 20);
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "engine::failure::composition_tests::combined_allocation_failure_reaches_reply_file_and_stderr_at_default_level"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let cause = "admitted render allocations exceed configured combined budget";
    let error = file
        .lines()
        .find(|line| line.contains("error #201 test_upload_image "))
        .unwrap();
    for context in [cause, "Static(7)", "phase=allocate", "required_bytes=20", "budget_bytes=12", "ms:"] {
        assert!(error.contains(context), "missing {context}: {error}");
    }
    assert!(stderr.contains(error.split("motion_studio::engine: ").nth(1).unwrap()));
    assert_eq!(stderr.matches(cause).count(), 1);
    assert!(file.contains("logging initialized level=INFO"));
    assert!(!stderr.contains("logging initialized"));
    for marker in [cause, "error #201", "logging initialized"] {
        assert!(!stdout.contains(marker), "log output leaked to stdout: {stdout}");
    }
}

#[test]
fn video_handoff_budget_failure_reaches_reply_file_and_stderr_at_default_level() {
    const CHILD: &str = "MOTION_VIDEO_HANDOFF_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let _logger = crate::logging::init(PathBuf::from(root)).unwrap();
        let mut renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
        let frame = motion_render::Yuv420Frame {
            width: 16,
            height: 16,
            rotation: 0,
            standard: 1,
            range: 2,
            phase: [0, 0],
            chroma_layout: motion_render::ChromaLayout::Uv,
            y: vec![128; 256],
            uv: vec![128; 128],
        };
        renderer.upload_video_yuv(1, 1, 0, &frame).unwrap();
        let before = renderer.texture_memory_breakdown();
        renderer
            .set_memory_policy(motion_render::resource_policy::MemoryPolicy {
                source_texture_budget_bytes: 4,
                ..Default::default()
            })
            .unwrap();
        let failure = renderer
            .upload_video_frame(1, 1, 1, 32, 16, &[255; 2048])
            .unwrap_err();
        assert!(matches!(
            failure.resource_rejection(),
            Some(motion_render::ResourceRejection::Budget {
                scope: motion_render::ResourceBudgetScope::SourceTextures,
                required_bytes: 2052,
                budget_bytes: 4,
            })
        ));
        assert_eq!(renderer.texture_memory_breakdown(), before);
        let original = failure.to_string();
        for context in [
            "object=1",
            "source=1",
            "pts_us=1",
            "phase=rgba_upload",
            "required_bytes: 2052",
            "budget_bytes: 4",
        ] {
            assert!(original.contains(context), "missing {context}: {original}");
        }
        let (reply, receive) = channel::<Result<Value, String>>();
        // Exercise an actual GPU allocation-budget refusal through the existing
        // process sink/reply boundary with a controlled origin request.
        CURRENT_COMMAND.set(Some(Flight {
            request: 200,
            operation: "present",
            started: Instant::now(),
        }));
        respond(reply, Err(failure.into()));
        CURRENT_COMMAND.set(None);
        assert_eq!(
            receive
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap_err(),
            original
        );
        renderer.set_memory_policy(Default::default()).unwrap();
        renderer
            .upload_video_frame(1, 1, 1, 32, 16, &[255; 2048])
            .unwrap();
        assert_eq!(renderer.texture_memory_breakdown().video_planes, 0);
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "engine::failure::composition_tests::video_handoff_budget_failure_reaches_reply_file_and_stderr_at_default_level"])
        .env(CHILD,root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let cause = "decoded preview textures exceed configured source texture budget";
    let error = file
        .lines()
        .find(|line| line.contains("error #200 present "))
        .unwrap();
    assert!(error.contains(cause) && error.contains("ms:"));
    assert!(stderr.contains(error.split("motion_studio::engine: ").nth(1).unwrap()));
    assert_eq!(stderr.matches(cause).count(), 1);
    assert!(file.contains("logging initialized level=INFO"));
    for marker in [cause, "error #200", "logging initialized"] {
        assert!(
            !stdout.contains(marker),
            "log output leaked to stdout: {stdout}"
        );
    }
}

#[test]
fn source_time_failures_reach_reply_file_and_stderr_at_default_level() {
    const CHILD: &str = "MOTION_SOURCE_TIME_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let _logger = crate::logging::init(PathBuf::from(root)).unwrap();
        let mut project = motion_core::Project::new(64, 64, 30, 120).unwrap();
        project.layers.push(motion_core::Layer::solid(
            1, "Selected", [32.; 2], [0.; 3], [1.; 4],
        ));
        let result = project
            .edit_composition(motion_core::CompositionAction::Precompose {
                objects: vec![1],
                name: "Nested".into(),
                range: "composition".into(),
            })
            .unwrap();
        let child = result["composition"].as_str().unwrap();
        project
            .compositions
            .iter_mut()
            .find(|c| c.id == child)
            .unwrap()
            .fps = 0;
        let mut scene = motion_render::Scene::new(&project);
        // Ordinary project loading rejects this state. Exercise the actual
        // public API failures with controlled origin context at the production
        // reply boundary, without installing an invalid engine session.
        let errors = [
            scene
                .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
                .unwrap_err(),
            motion_render::image_resources::upcoming_assets(&project, 0.).unwrap_err(),
            motion_core::CompositionClip::new(child.into())
                .source_frame(f64::NAN, 30, 60)
                .unwrap_err(),
        ];
        for (index, error) in errors.into_iter().enumerate() {
            let original = error.to_string();
            let (reply, receive) = channel::<Result<Value, String>>();
            CURRENT_COMMAND.set(Some(Flight {
                request: 100 + index as u64,
                operation: "present",
                started: Instant::now(),
            }));
            respond(reply, Err(error.into()));
            CURRENT_COMMAND.set(None);
            let received = receive
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap_err();
            assert_eq!(received, original);
            let envelope = motion_host::ops::error_envelope(&received);
            assert_eq!(envelope["error_detail"]["composition"], child);
            assert_eq!(
                envelope["error_detail"]["code"],
                if index < 2 {
                    "invalid_frame_rate"
                } else {
                    "invalid_time"
                }
            );
        }
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "engine::failure::composition_tests::source_time_failures_reach_reply_file_and_stderr_at_default_level"])
        .env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(file.contains("logging initialized level=INFO"));
    for (request, phase) in [(100, "scene_sample"), (101, "image_prefetch"), (102, "NaN")] {
        let line = file
            .lines()
            .find(|line| line.contains(&format!("error #{request} present ")))
            .unwrap();
        assert!(line.contains(phase) && line.contains("ms:") && line.contains("comp-1"));
        assert!(stderr.contains(line.split("motion_studio::engine: ").nth(1).unwrap()));
    }
    assert_eq!(stderr.matches("ERROR motion_studio::engine:").count(), 3);
    assert!(!stderr.contains("logging initialized"));
    for marker in [
        "composition_error:",
        "invalid_frame_rate",
        "invalid_time",
        "logging initialized",
    ] {
        assert!(
            !stdout.contains(marker),
            "log output leaked to stdout: {stdout}"
        );
    }
}

#[test]
fn capacity_failure_reaches_reply_file_and_stderr_at_default_level() {
    const CHILD: &str = "MOTION_COMPOSITION_BOUNDARY_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let mut project = motion_core::Project::new(64, 64, 30, 120).unwrap();
        project.layers.push(motion_core::Layer::solid(
            1,
            "Selected",
            [32.; 2],
            [32., 32., 0.],
            [1.; 4],
        ));
        project.compositions = (1..MAX_COMPOSITIONS)
            .map(|id| Composition {
                id: format!("comp-{id}"),
                name: format!("Library {id}"),
                width: 64,
                height: 64,
                fps: 30,
                frames: 120,
                background: [0.; 4],
                camera: motion_core::Camera::new(64, 64),
                layers: vec![],
                expressions: vec![],
            })
            .collect();
        let project_root = root.join("project");
        motion_core::storage::save(&project_root, &project).unwrap();
        let saved = std::fs::read(project_root.join("project.json")).unwrap();
        let engine = Engine::start(
            project_root.clone(),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let before = engine.state().unwrap();
        let commands = serde_json::json!([
            {"op":"rename", "object":1, "name":"private-command-text-must-not-be-logged"},
            {"op":"composition", "composition":"comp-main",
             "action":{"kind":"precompose", "objects":[1], "name":"Nested"}}
        ])
        .to_string();
        for _ in 0..2 {
            let error = engine.edit(commands.clone()).unwrap_err();
            let reply = motion_host::ops::error_envelope(&error);
            assert_eq!(reply["error_detail"]["code"], "resource_limit");
            assert_eq!(reply["error_detail"]["composition"], "comp-main");
            assert!(error.contains("Composition limit exceeded"));
            let after = engine.state().unwrap();
            for field in ["project", "revision", "history", "canUndo", "canRedo"] {
                assert_eq!(after[field], before[field], "changed {field}");
            }
            assert!(!engine.failed());
        }
        assert_eq!(
            std::fs::read(project_root.join("project.json")).unwrap(),
            saved
        );
        let after = engine
            .edit(r#"{"op":"rename","object":1,"name":"Still editable"}"#.into())
            .unwrap();
        assert_eq!(after["revision"], before["revision"].as_u64().unwrap() + 1);
        let deadline = Instant::now() + Duration::from_secs(5);
        while engine.in_flight.lock().unwrap().is_some() {
            assert!(
                Instant::now() < deadline,
                "completed command remained in flight"
            );
            std::thread::yield_now();
        }
        drop(engine);
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "engine::failure::composition_tests::capacity_failure_reaches_reply_file_and_stderr_at_default_level"])
        .env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(file.contains("logging initialized level=INFO"));
    let errors: Vec<_> = file
        .lines()
        .filter(|line| line.contains("Composition limit exceeded"))
        .collect();
    assert_eq!(errors.len(), 2, "{file}");
    for error in errors {
        assert!(error.contains("ERROR") && error.contains("error #") && error.contains(" edit "));
        assert!(
            error.contains("ms:")
                && error.contains("resource_limit")
                && error.contains("comp-main")
        );
        let request = error
            .split("error #")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        let start = format!("start #{request} edit");
        assert!(file.find(&start).unwrap() < file.find(error).unwrap());
        assert!(stderr.contains(error.split("motion_studio::engine: ").nth(1).unwrap()));
    }
    assert_eq!(stderr.matches("Composition limit exceeded").count(), 2);
    assert!(!stderr.contains("start #"));
    assert!(!file.contains("private-command-text-must-not-be-logged"));
    assert!(!stderr.contains("private-command-text-must-not-be-logged"));
    for marker in [
        "Composition limit exceeded",
        "resource_limit",
        "start #",
        "logging initialized",
    ] {
        assert!(
            !stdout.contains(marker),
            "log records leaked to stdout: {stdout}"
        );
    }
}
