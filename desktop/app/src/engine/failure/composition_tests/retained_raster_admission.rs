use super::*;

fn scene() -> motion_render::Scene {
    let mut project = motion_core::Project::new(64, 64, 30, 60).unwrap();
    project.background = [0.; 4];
    project.camera.created = false;
    project.layers.push(motion_core::Layer::solid(77, "receiver", [32.; 2], [32., 32., 0.], [0., 1., 0., 1.]));
    project.validate().unwrap();
    let mut scene = motion_render::Scene::new(&project).unwrap();
    scene.sample(&project, 0., None, &motion_core::ExpressionEvaluator).unwrap();
    scene
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn retained_receiver_refusal_preserves_budgets_pixels_cause_and_default_output_channels() {
    const CHILD: &str = "MOTION_RETAINED_RASTER_ADMISSION_LOG_CHILD";
    const NAME: &str = "engine::failure::composition_tests::retained_raster_admission::retained_receiver_refusal_preserves_budgets_pixels_cause_and_default_output_channels";
    if let Some(root) = std::env::var_os(CHILD) {
        let _logger = crate::logging::init(PathBuf::from(root)).unwrap();
        let mut renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
        let scene = scene();
        let budget = renderer.allocation_budget().clone();
        let target = renderer.capture_target(64, 64).unwrap();
        let old = renderer.materialize_layer(&scene, 77, 64, 64, false).unwrap().unwrap().0;
        assert_eq!(old.size(), [1, 1]);
        renderer.draw_layer_rasters(&scene, &[&old], &target.view, 64, 64).unwrap();
        let pixels = renderer.read_target(&target).unwrap();
        let center = (32 * 64 + 32) * 4;
        assert_eq!(&pixels[center..center + 4], &[0, 255, 0, 255]);
        assert_eq!(pixels[3], 0);
        let before = budget.info().unwrap();
        let category = renderer.exposure_texture_bytes();
        budget.set_limit(before.admitted_bytes).unwrap();
        CURRENT_COMMAND.set(Some(Flight {
            request: 214, operation: "test_retained_raster_allocate", started: Instant::now(),
        }));
        let failure = renderer.materialize_layer(&scene, 77, 64, 64, false).err().unwrap();
        assert_eq!(failure.resource_rejection(), Some(&motion_render::ResourceRejection::Budget {
            scope: motion_render::ResourceBudgetScope::RenderAllocations,
            required_bytes: before.admitted_bytes + 4, budget_bytes: before.admitted_bytes,
        }));
        let original = failure.to_string();
        let (reply, receive) = channel::<Result<Value, String>>();
        respond(reply, Err(failure.into()));
        CURRENT_COMMAND.set(None);
        assert_eq!(receive.recv_timeout(Duration::from_secs(5)).unwrap().unwrap_err(), original);
        assert_eq!(renderer.exposure_texture_bytes(), category);
        assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
        assert_eq!(renderer.read_target(&target).unwrap(), pixels);
        budget.set_limit(8 << 20).unwrap();
        let recovered = renderer.materialize_layer(&scene, 77, 64, 64, false).unwrap().unwrap().0;
        assert_eq!(recovered.size(), [1, 1]);
        renderer.draw_layer_rasters(&scene, &[&recovered], &target.view, 64, 64).unwrap();
        assert_eq!(renderer.read_target(&target).unwrap(), pixels);
        assert_eq!(renderer.exposure_texture_bytes(), category + 4);
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
    let error = file.lines().find(|line| line.contains("error #214 test_retained_raster_allocate ")).unwrap();
    for context in [cause, "exposure composition=comp-main frame=0 object=77 size=1x1", "format=Rgba8UnormSrgb phase=local_constant_allocate", "required_bytes=", "budget_bytes=", "ms:"] {
        assert!(error.contains(context), "missing {context}: {error}");
    }
    assert!(file.contains("logging initialized level=INFO"));
    assert!(stderr.contains(error.split("motion_studio::engine: ").nth(1).unwrap()));
    assert_eq!(stderr.matches(cause).count(), 1);
    assert!(!stderr.contains("logging initialized"));
    for marker in [cause, "error #214", "logging initialized"] {
        assert!(!stdout.contains(marker), "log output leaked to stdout: {stdout}");
    }
}
