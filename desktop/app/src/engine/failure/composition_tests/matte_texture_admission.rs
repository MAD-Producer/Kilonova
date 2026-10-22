use super::*;

fn scene(size: u32) -> motion_render::Scene {
    let size = size as f32;
    let mut project = motion_core::Project::new(size as u32, size as u32, 30, 60).unwrap();
    project.background = [0.; 4];
    project.camera.created = false;
    project.layers = vec![
        // Resize the composition, keeping this source snapshot at16x16.
        motion_core::Layer::solid(77, "coverage", [16.; 2], [size * 0.5, size * 0.5, 0.], [1.; 4]),
        motion_core::Layer::solid(78, "receiver", [size; 2], [size * 0.5, size * 0.5, 0.], [1.; 4]),
    ];
    project.layers[0].visible = false;
    project.layers[1].track_matte = Some(motion_core::compositing::TrackMatte {
        source: 77, mode: motion_core::compositing::MatteMode::Alpha, hide_source: true,
    });
    project.validate().unwrap();
    let mut scene = motion_render::Scene::new(&project);
    scene.sample(&project, 0., None, &motion_core::ExpressionEvaluator).unwrap();
    scene
}

#[test]
fn resized_matte_refusal_keeps_old_pixels_and_original_cause_in_default_output_channels() {
    const CHILD: &str = "MOTION_MATTE_TEXTURE_ADMISSION_LOG_CHILD";
    const NAME: &str = "engine::failure::composition_tests::matte_texture_admission::resized_matte_refusal_keeps_old_pixels_and_original_cause_in_default_output_channels";
    if let Some(root) = std::env::var_os(CHILD) {
        let _logger = crate::logging::init(PathBuf::from(root)).unwrap();
        let mut renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
        let budget = renderer.allocation_budget().clone();
        let small = renderer.capture_target(32, 32).unwrap();
        let old_pixels = renderer.capture(&scene(32), &small).unwrap().0;
        assert_eq!(old_pixels[(16 * 32 + 16) * 4 + 3], 255);
        assert_eq!(old_pixels[3], 0);
        let large = renderer.capture_target(64, 64).unwrap();
        // Isolate the resized matte from earlier effect-scratch admission.
        renderer.preflight_effects(&scene(64), 64, 64).unwrap();
        let before = budget.info().unwrap();
        budget.set_limit(before.admitted_bytes).unwrap();
        CURRENT_COMMAND.set(Some(Flight {
            request: 211, operation: "test_matte_texture_allocate", started: Instant::now(),
        }));
        let failure = renderer.capture(&scene(64), &large).unwrap_err();
        assert_eq!(failure.resource_rejection(), Some(&motion_render::ResourceRejection::Budget {
            scope: motion_render::ResourceBudgetScope::RenderAllocations,
            required_bytes: before.admitted_bytes + 64 * 64, budget_bytes: before.admitted_bytes,
        }));
        let original = failure.to_string();
        let (reply, receive) = channel::<Result<Value, String>>();
        respond(reply, Err(failure.into()));
        CURRENT_COMMAND.set(None);
        assert_eq!(receive.recv_timeout(Duration::from_secs(5)).unwrap().unwrap_err(), original);
        assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
        assert_eq!(renderer.read_target(&small).unwrap(), old_pixels);
        assert_eq!(renderer.compositing_pipeline_set_builds(), 1);
        budget.set_limit(8 << 20).unwrap();
        let recovered = renderer.capture(&scene(64), &large).unwrap().0;
        assert_eq!(recovered[(32 * 64 + 32) * 4 + 3], 255);
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
    let error = file.lines().find(|line| line.contains("error #211 test_matte_texture_allocate ")).unwrap();
    for context in [cause, "compositing frame=0 phase=targets", "compositing matte layer=77 luma=false size=64x64 phase=allocate", "required_bytes=", "budget_bytes=", "ms:"] {
        assert!(error.contains(context), "missing {context}: {error}");
    }
    assert!(file.contains("logging initialized level=INFO"));
    assert!(stderr.contains(error.split("motion_studio::engine: ").nth(1).unwrap()));
    assert_eq!(stderr.matches(cause).count(), 1);
    assert!(!stderr.contains("logging initialized"));
    for marker in [cause, "error #211", "logging initialized"] {
        assert!(!stdout.contains(marker), "log output leaked to stdout: {stdout}");
    }
}
