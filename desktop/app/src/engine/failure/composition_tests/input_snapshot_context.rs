use super::*;

fn scene(size: u32) -> motion_render::Scene {
    let mut project = motion_core::Project::new(size, size, 30, 60).unwrap();
    project.background = [0.; 4];
    project.camera.created = false;
    let center = size as f32 * 0.5;
    let extent = size as f32 * 0.5;
    project.layers = vec![
        motion_core::Layer::solid(77, "coverage", [extent; 2], [center, center, 0.], [1.; 4]),
        motion_core::Layer::solid(76, "back", [extent; 2], [center, center, 0.], [0.2, 0.2, 0.2, 1.]),
        motion_core::Layer::solid(78, "front", [extent; 2], [center, center, 0.], [0.7, 0.7, 0.7, 0.5]),
    ];
    project.layers[0].visible = false;
    project.layers[2].track_matte = Some(motion_core::compositing::TrackMatte {
        source: 77, mode: motion_core::compositing::MatteMode::Alpha, hide_source: true,
    });
    project.layers[2].blend = motion_core::compositing::LayerBlend {
        mode: motion_core::compositing::BlendMode::Multiply,
        space: motion_core::compositing::BlendSpace::Srgb,
    };
    project.validate().unwrap();
    let mut scene = motion_render::Scene::new(&project);
    scene.sample(&project, 0., None, &motion_core::ExpressionEvaluator).unwrap();
    scene
}

#[test]
fn snapshot_refusal_reports_frame_layer_extent_and_original_cause_at_default_level() {
    const CHILD: &str = "MOTION_INPUT_SNAPSHOT_CONTEXT_LOG_CHILD";
    const NAME: &str = "engine::failure::composition_tests::input_snapshot_context::snapshot_refusal_reports_frame_layer_extent_and_original_cause_at_default_level";
    if let Some(root) = std::env::var_os(CHILD) {
        let _logger = crate::logging::init(PathBuf::from(root)).unwrap();
        let mut renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
        let budget = renderer.allocation_budget().clone();
        let small = renderer.capture_target(32, 32).unwrap();
        let old = renderer.capture(&scene(32), &small).unwrap().0;
        assert_eq!(old[(16 * 32 + 16) * 4 + 3], 255);
        assert!(old[(16 * 32 + 16) * 4].abs_diff(43) <= 3);
        let large = renderer.capture_target(64, 64).unwrap();
        renderer.preflight_effects(&scene(64), 64, 64).unwrap();
        let before = budget.info().unwrap();
        budget.set_limit(before.admitted_bytes).unwrap();
        CURRENT_COMMAND.set(Some(Flight {
            request: 213, operation: "test_input_snapshot_allocate", started: Instant::now(),
        }));
        let failure = renderer.capture(&scene(64), &large).unwrap_err();
        assert_eq!(failure.resource_rejection(), Some(&motion_render::ResourceRejection::Budget {
            scope: motion_render::ResourceBudgetScope::RenderAllocations,
            required_bytes: before.admitted_bytes + 32 * 32 * 4, budget_bytes: before.admitted_bytes,
        }));
        let original = failure.to_string();
        let (reply, receive) = channel::<Result<Value, String>>();
        respond(reply, Err(failure.into()));
        CURRENT_COMMAND.set(None);
        assert_eq!(receive.recv_timeout(Duration::from_secs(5)).unwrap().unwrap_err(), original);
        assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
        assert_eq!(renderer.read_target(&small).unwrap(), old);
        budget.set_limit(8 << 20).unwrap();
        let recovered = renderer.capture(&scene(64), &large).unwrap().0;
        assert_eq!(recovered[(32 * 64 + 32) * 4 + 3], 255);
        assert!(recovered[(32 * 64 + 32) * 4].abs_diff(43) <= 3);
        assert_eq!(recovered[3], 0);
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
    let error = file.lines().find(|line| line.contains("error #213 test_input_snapshot_allocate ")).unwrap();
    for context in [cause, "effects frame=0 phase=input_snapshots", "input snapshot layer=77 size=32x32 phase=allocate", "required_bytes=", "budget_bytes=", "ms:"] {
        assert!(error.contains(context), "missing {context}: {error}");
    }
    assert!(file.contains("logging initialized level=INFO"));
    assert!(stderr.contains(error.split("motion_studio::engine: ").nth(1).unwrap()));
    assert_eq!(stderr.matches(cause).count(), 1);
    assert!(!stderr.contains("logging initialized"));
    for marker in [cause, "error #213", "logging initialized"] {
        assert!(!stdout.contains(marker), "log output leaked to stdout: {stdout}");
    }
}
