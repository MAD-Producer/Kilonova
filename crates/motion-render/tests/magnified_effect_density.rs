use motion_core::{EffectInstance, Layer, Project};
use motion_render::{effect_plan::PlanBuilder, ResourceRejection, Scene};

#[test]
fn portable_matte_programs_validate_without_enabling_native_fringe_coverage() {
    let programs = motion_render::compositing_plan::plane_programs().unwrap();
    assert_eq!(programs.len(), 3);
    for program in programs {
        assert!(program.vertex.contains("#version 300 es"));
        assert!(program.fragment.contains("#version 300 es"));
    }
}

fn fixture() -> Project {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|d| d.id == "rays")
        .unwrap();
    let mut project = Project::new(512, 512, 30, 60).unwrap();
    project.camera.created = false;
    let mut layer = Layer::solid(1, "Magnified effect", [64.; 2], [256., 256., 0.], [1.; 4]);
    layer.effects.push(EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        layer.size,
    ));
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    project
}
fn sample(project: &Project) -> Scene {
    let mut scene = Scene::new(project);
    scene
        .sample(project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}

#[test]
fn density_buckets_preserve_logical_coordinates_cache_and_portable_geometry() {
    let mut project = fixture();
    let mut builder =
        PlanBuilder::new(motion_effects::Registry::new_with_builtins().unwrap()).unwrap();
    for (scales, expected) in [
        ([100., 100., 100.], 1.),
        ([200., 50., 100.], 2.),
        ([-650., 650., 100.], 8.),
        ([9000., 9000., 100.], 8.),
        ([100., 100., 100.], 1.),
    ] {
        project.layers[0].transform.scale.value = scales;
        project.layers[0].transform.rotation.value[2] = 21.;
        let scene = sample(&project);
        let preview = builder
            .build_preview_guarded(&scene, &[0], 512, 512, true)
            .unwrap();
        assert!(preview.diagnostics.is_empty());
        let pass = preview.passes.last().unwrap();
        assert_eq!(pass.uniform.output_mode[3], expected, "{scales:?}");
        assert_eq!(&pass.uniform.size[..2], &[64.; 2]);
        assert_eq!(pass.uniform.region, [0., 0., 64., 64.]);
        assert_eq!([pass.width, pass.height], [(64. * expected) as u32; 2]);
        // No ABI or geometry change for GLES consumers that have not opted
        // into the native wgpu boundary-coverage shader.
        assert!(preview
            .vertices
            .iter()
            .all(|v| v.uv.iter().all(|u| (0.0..=1.0).contains(u))));
        let first = pass.uniform;
        builder
            .build_preview_guarded(&scene, &[0], 512, 512, true)
            .unwrap();
        assert_eq!(
            builder.frame.passes.last().unwrap().uniform.size,
            first.size
        );
        let formal = builder.build(&scene, &[0], 512, 512, true).unwrap();
        assert_eq!(formal.passes.last().unwrap().uniform.size, first.size);
        assert_eq!(formal.passes.last().unwrap().uniform.region, first.region);
    }
    assert_eq!(builder.preview_cache_hits, 5);
    // Fit the physical target, rather than multiplying a quality reduction
    // twice or continuing to allocate the previous zoom's raster dimensions.
    project.layers[0].transform.scale.value = [650., 650., 100.];
    let scene = sample(&project);
    let reduced = builder
        .build_preview_guarded(&scene, &[0], 128, 128, true)
        .unwrap();
    assert_eq!(reduced.passes.last().unwrap().uniform.output_mode[3], 2.);
}

#[test]
fn magnification_obeys_dimension_guards_and_recovers_after_a_refusal() {
    let mut project = fixture();
    project.layers[0].transform.scale.value = [650., 650., 100.];
    let scene = sample(&project);
    let mut builder =
        PlanBuilder::new(motion_effects::Registry::new_with_builtins().unwrap()).unwrap();
    builder.device_dimension = 256;
    let error = builder
        .build_preview_guarded(&scene, &[0], 512, 512, true)
        .err()
        .unwrap();
    assert!(matches!(
        error.resource_rejection(),
        Some(ResourceRejection::TextureDimensions {
            max_dimension: 256,
            ..
        })
    ));
    assert!(error.to_string().contains("layer 1"));
    builder.device_dimension = 8192;
    let plan = builder
        .build_preview_guarded(&scene, &[0], 512, 512, true)
        .unwrap();
    assert!(plan.diagnostics.is_empty());
    assert_eq!(plan.passes.last().unwrap().width, 512);
    assert!(builder.last_resource_rejection.is_none());
}

#[test]
fn magnification_reports_capacity_refusals_instead_of_bypassing_the_effect() {
    let mut project = fixture();
    project.layers[0].size = [1024.; 2];
    project.layers[0].transform.scale.value = [650., 650., 100.];
    let mut builder =
        PlanBuilder::new(motion_effects::Registry::new_with_builtins().unwrap()).unwrap();
    let error = builder
        .build_preview_guarded(&sample(&project), &[0], 512, 512, true)
        .err()
        .unwrap();
    assert!(
        matches!(error.resource_rejection(),Some(ResourceRejection::Budget{
        scope:motion_render::ResourceBudgetScope::EffectScratch,required_bytes,budget_bytes,
    }) if required_bytes>budget_bytes)
    );
    assert!(error.to_string().contains("layer 1"));
    project.layers[0].size = [64.; 2];
    assert!(builder
        .build_preview_guarded(&sample(&project), &[0], 512, 512, true)
        .unwrap()
        .diagnostics
        .is_empty());
    assert!(builder.last_resource_rejection.is_none());
}

#[test]
fn enlarged_transfer_target_uses_coverage_at_the_working_canvas_resolution() {
    use motion_core::compositing::{BlendMode, BlendSpace, LayerBlend};
    use motion_render::Renderer;
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    project.camera.created = false;
    project.background = [0.; 4];
    let mut layer = Layer::solid(1, "Transfer", [32.; 2], [32.25, 32.25, 0.], [1.; 4]);
    layer.transform.rotation.value[2] = 3.7;
    layer.blend = LayerBlend {
        mode: BlendMode::Multiply,
        space: BlendSpace::Linear,
    };
    project.layers.push(layer);
    let scene = sample(&project);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(64, 64).unwrap();
    let reference = renderer.capture(&scene, &target).unwrap().0;
    let target = renderer.capture_target(512, 512).unwrap();
    let actual = renderer.capture(&scene, &target).unwrap().0;
    // Independent bilinear resampling of the captured working-canvas alpha.
    // This checks the composition stage, not the already-tested fringe formula.
    let mut max_error = 0_f64;
    for y in 0..512 {
        for x in 0..512 {
            let sample = |v: usize| ((v as f64 + 0.5) / 8. - 0.5).clamp(0., 63.);
            let u = sample(x);
            let v = sample(y);
            let x0 = u.floor() as usize;
            let y0 = v.floor() as usize;
            let x1 = (x0 + 1).min(63);
            let y1 = (y0 + 1).min(63);
            let at = |x: usize, y: usize| f64::from(reference[(y * 64 + x) * 4 + 3]);
            let mix = |a: f64, b: f64, t: f64| a + (b - a) * t;
            let expected = mix(
                mix(at(x0, y0), at(x1, y0), u.fract()),
                mix(at(x0, y1), at(x1, y1), u.fract()),
                v.fract(),
            );
            max_error = max_error.max((f64::from(actual[(y * 512 + x) * 4 + 3]) - expected).abs());
        }
    }
    assert!(
        max_error <= 2.,
        "working-canvas edge coverage changed when presenting a larger target: {max_error}"
    );
    assert!(renderer.gpu_error().is_none());
}
