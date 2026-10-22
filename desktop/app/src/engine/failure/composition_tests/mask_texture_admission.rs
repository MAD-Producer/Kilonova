use super::*;

#[test]
fn native_mask_texture_refusal_keeps_old_pixels_cause_and_default_output_channels() {
    const CHILD: &str = "MOTION_MASK_TEXTURE_ADMISSION_LOG_CHILD";
    const NAME: &str = "engine::failure::composition_tests::mask_texture_admission::native_mask_texture_refusal_keeps_old_pixels_cause_and_default_output_channels";
    if let Some(root) = std::env::var_os(CHILD) {
        let _logger = crate::logging::init(PathBuf::from(root)).unwrap();
        let mut project = motion_core::Project::new(32, 32, 30, 60).unwrap();
        project.background = [0.; 4];
        let mut layer = motion_core::Layer::solid(77, "masked receiver", [32.; 2], [16., 16., 0.], [1.; 4]);
        let mut mask = motion_core::masks::LayerMask::new(55, motion_core::vector::VectorPath {
            id: 1, closed: true,
            nodes: [[4., 4.], [28., 4.], [28., 28.], [4., 28.]].into_iter().enumerate()
                .map(|(i, p)| motion_core::vector::PathNode {
                    id: i as u64 + 1, geometry: motion_core::Track::constant([p[0], p[1], 0., 0., 0., 0.]),
                }).collect(),
        });
        mask.feather.value = [2.; 2];
        layer.masks.push(mask);
        project.layers.push(layer);
        project.validate().unwrap();
        let mut scene = motion_render::Scene::new(&project);
        scene.sample(&project, 0., None, &motion_core::ExpressionEvaluator).unwrap();
        let mut renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
        let budget = renderer.allocation_budget().clone();
        let target = renderer.capture_target(32, 32).unwrap();
        let pixels = renderer.capture(&scene, &target).unwrap().0;
        assert_eq!(pixels[(16 * 32 + 16) * 4 + 3], 255);
        assert_eq!(pixels[3], 0);
        project.layers[0].size = [64.; 2];
        project.layers[0].transform.position.value = [32., 32., 0.];
        scene.sample(&project, 0., None, &motion_core::ExpressionEvaluator).unwrap();
        let before = budget.info().unwrap();
        budget.set_limit(before.admitted_bytes).unwrap();
        CURRENT_COMMAND.set(Some(Flight {
            request: 215, operation: "test_mask_texture_allocate", started: Instant::now(),
        }));
        let failure = renderer.capture(&scene, &target).unwrap_err();
        assert_eq!(failure.resource_rejection(), Some(&motion_render::ResourceRejection::Budget {
            scope: motion_render::ResourceBudgetScope::RenderAllocations,
            required_bytes: before.admitted_bytes + 4096, budget_bytes: before.admitted_bytes,
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
        assert_eq!(recovered, pixels);
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
    let error = file.lines().find(|line| line.contains("error #215 test_mask_texture_allocate ")).unwrap();
    for context in [cause, "mask layer=77 mask=55 frame=0 phase=scratch_allocate slot=0", "size=64x64 format=R8Unorm phase=allocate", "required_bytes=", "budget_bytes=", "ms:"] {
        assert!(error.contains(context), "missing {context}: {error}");
    }
    assert!(file.contains("logging initialized level=INFO"));
    assert!(stderr.contains(error.split("motion_studio::engine: ").nth(1).unwrap()));
    assert_eq!(stderr.matches(cause).count(), 1);
    assert!(!stderr.contains("logging initialized"));
    for marker in [cause, "error #215", "logging initialized"] {
        assert!(!stdout.contains(marker), "log records leaked to stdout: {stdout}");
    }
}
