use super::*;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn compositor_refusal_preserves_cause_and_reaches_default_file_and_stderr() {
    const CHILD: &str = "MOTION_COMPOSITOR_ADMISSION_LOG_CHILD";
    const NAME: &str = "engine::failure::composition_tests::compositing_admission::compositor_refusal_preserves_cause_and_reaches_default_file_and_stderr";
    if let Some(root) = std::env::var_os(CHILD) {
        let _logger = crate::logging::init(PathBuf::from(root)).unwrap();
        let mut project = motion_core::Project::new(32, 32, 30, 60).unwrap();
        project.background = [0.; 4];
        project.camera.created = false;
        project.layers = vec![
            motion_core::Layer::solid(77, "coverage", [16.; 2], [16., 16., 0.], [1.; 4]),
            motion_core::Layer::solid(78, "receiver", [32.; 2], [16., 16., 0.], [1.; 4]),
        ];
        project.layers[0].visible = false;
        project.layers[1].track_matte = Some(motion_core::compositing::TrackMatte {
            source: 77, mode: motion_core::compositing::MatteMode::Alpha, hide_source: true,
        });
        project.validate().unwrap();
        let mut scene = motion_render::Scene::new(&project).unwrap();
        scene.sample(&project, 0., None, &motion_core::ExpressionEvaluator).unwrap();
        let mut renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
        let budget = renderer.allocation_budget().clone();
        let target = renderer.capture_target(32, 32).unwrap();
        // Fund ordinary pass parameters; leave less than even the smallest
        // supported 128-layer matte uniform allocation.
        let limit = budget.info().unwrap().admitted_bytes + 4096;
        budget.set_limit(limit).unwrap();
        CURRENT_COMMAND.set(Some(Flight {
            request: 210, operation: "test_compositor_allocate", started: Instant::now(),
        }));
        let failure = renderer.capture(&scene, &target).unwrap_err();
        assert!(matches!(failure.resource_rejection(), Some(motion_render::ResourceRejection::Budget {
            scope: motion_render::ResourceBudgetScope::RenderAllocations, required_bytes, budget_bytes,
        }) if *budget_bytes == limit && *required_bytes > limit));
        let original = failure.to_string();
        assert!(original.contains("compositing frame=0 phase=initialize"), "{original}");
        let (reply, receive) = channel::<Result<Value, String>>();
        respond(reply, Err(failure.into()));
        CURRENT_COMMAND.set(None);
        assert_eq!(receive.recv_timeout(Duration::from_secs(5)).unwrap().unwrap_err(), original);
        assert_eq!(renderer.compositing_pipeline_set_builds(), 0);
        budget.set_limit(8 << 20).unwrap();
        let recovered = renderer.capture(&scene, &target).unwrap().0;
        assert_eq!(recovered[(16 * 32 + 16) * 4 + 3], 255);
        assert_eq!(recovered[3], 0);
        assert_eq!(renderer.compositing_pipeline_set_builds(), 1);
        assert!(renderer.gpu_error().is_none());
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME]).env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let cause = "admitted render allocations exceed configured combined budget";
    let error = file.lines().find(|line| line.contains("error #210 test_compositor_allocate ")).unwrap();
    for context in [cause, "compositing frame=0 phase=initialize", "phase=allocate label=matte source uniforms", "required_bytes=", "budget_bytes=", "ms:"] {
        assert!(error.contains(context), "missing {context}: {error}");
    }
    assert!(file.contains("logging initialized level=INFO"));
    assert!(stderr.contains(error.split("motion_studio::engine: ").nth(1).unwrap()));
    assert_eq!(stderr.matches(cause).count(), 1);
    assert!(!stderr.contains("logging initialized"));
    for marker in [cause, "error #210", "logging initialized"] {
        assert!(!stdout.contains(marker), "log output leaked to stdout: {stdout}");
    }
}
