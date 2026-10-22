use super::*;

#[test]
fn native_vector_texture_refusal_keeps_old_pixels_cause_and_default_output_channels() {
    const CHILD: &str = "MOTION_VECTOR_TEXTURE_ADMISSION_LOG_CHILD";
    const NAME: &str = "engine::failure::composition_tests::vector_texture_admission::native_vector_texture_refusal_keeps_old_pixels_cause_and_default_output_channels";
    if let Some(root) = std::env::var_os(CHILD) {
        let _logger = crate::logging::init(PathBuf::from(root)).unwrap();
        let mut project = motion_core::Project::new(64, 64, 30, 60).unwrap();
        project.background = [0.; 4];
        let mut vector = motion_core::vector::VectorContent::shape(motion_core::vector::ShapeKind::Rectangle);
        vector.fill = Some(motion_core::Track::constant([0., 1., 0., 1.]));
        let mut layer = motion_core::Layer::solid(77, "vector receiver", [32.; 2], [32., 32., 0.], [1.; 4]);
        layer.content = motion_core::Content::Vector { vector };
        project.layers.push(layer);
        project.validate().unwrap();
        let mut scene = motion_render::Scene::new(&project).unwrap();
        scene.sample(&project, 0., None, &motion_core::ExpressionEvaluator).unwrap();
        let mut renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
        let budget = renderer.allocation_budget().clone();
        let target = renderer.capture_target(64, 64).unwrap();
        let pixels = renderer.capture(&scene, &target).unwrap().0;
        assert_eq!(&pixels[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 4], &[0, 255, 0, 255]);
        assert_eq!(pixels[3], 0);
        project.layers[0].size = [64.; 2];
        scene.sample(&project, 0., None, &motion_core::ExpressionEvaluator).unwrap();
        let before = budget.info().unwrap();
        budget.set_limit(before.admitted_bytes).unwrap();
        CURRENT_COMMAND.set(Some(Flight {
            request: 216, operation: "test_vector_texture_allocate", started: Instant::now(),
        }));
        let failure = renderer.capture(&scene, &target).unwrap_err();
        assert_eq!(failure.resource_rejection(), Some(&motion_render::ResourceRejection::Budget {
            scope: motion_render::ResourceBudgetScope::RenderAllocations,
            required_bytes: before.admitted_bytes + 66 * 66 * 4, budget_bytes: before.admitted_bytes,
        }));
        let original = failure.to_string();
        let (reply, receive) = channel::<Result<Value, String>>();
        respond(reply, Err(failure.into()));
        CURRENT_COMMAND.set(None);
        assert_eq!(receive.recv_timeout(Duration::from_secs(5)).unwrap().unwrap_err(), original);
        assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
        assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations);
        assert_eq!(renderer.read_target(&target).unwrap(), pixels);
        budget.set_limit(8 << 20).unwrap();
        let recovered = renderer.capture(&scene, &target).unwrap().0;
        assert_ne!(recovered, pixels);
        assert_eq!(&recovered[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 4], &[0, 255, 0, 255]);
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
    let error = file.lines().find(|line| line.contains("error #216 test_vector_texture_allocate ")).unwrap();
    for context in [cause, "vector layer=77 frame=0 size=66x66 format=Rgba8UnormSrgb phase=source_allocate", "required_bytes=", "budget_bytes=", "ms:"] {
        assert!(error.contains(context), "missing {context}: {error}");
    }
    assert!(file.contains("logging initialized level=INFO"));
    assert!(stderr.contains(error.split("motion_studio::engine: ").nth(1).unwrap()));
    assert_eq!(stderr.matches(cause).count(), 1);
    assert!(!stderr.contains("logging initialized"));
    for marker in [cause, "error #216", "logging initialized"] {
        assert!(!stdout.contains(marker), "log records leaked to stdout: {stdout}");
    }
}
