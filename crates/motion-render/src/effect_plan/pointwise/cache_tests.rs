use super::*;
use motion_model::{EffectInstance, Layer, Project};

fn fixture() -> (PlanBuilder, Scene) {
    let registry = Registry::new_with_builtins().unwrap();
    let package = motion_effects::builtin::package().unwrap();
    let mut project = Project::new(32, 32, 30, 60).unwrap();
    let mut layer = Layer::solid(1, "fusion", [32.; 2], [16., 16., 0.], [0.2, 0.3, 0.7, 1.]);
    for (i, name) in ["invert", "tint"].into_iter().enumerate() {
        let d = package
            .manifest
            .effects
            .iter()
            .find(|e| e.id == name)
            .unwrap();
        layer.effects.push(EffectInstance::new(
            i as u64 + 1,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            d,
            layer.size,
        ));
    }
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut builder = PlanBuilder::new(registry).unwrap();
    builder.set_pointwise_fusion(true);
    builder.build(&scene, &[0], 32, 32, true).unwrap();
    assert_eq!(builder.pointwise_fusion_report().groups.len(), 1);
    (builder, scene)
}

#[test]
fn fusion_private_success_proof_survives_public_program_mutation_and_registry_reset() {
    let (mut builder, scene) = fixture();
    let id = builder.pointwise_fusion_report().groups[0].program as usize;
    let original = builder.programs[id].shader.clone();
    Arc::make_mut(&mut builder.programs[id].shader)
        .wgsl
        .push_str("\ninvalid shader {");
    builder.build(&scene, &[0], 32, 32, true).unwrap();
    assert!(Arc::ptr_eq(&builder.programs[id].shader, &original));
    assert_eq!(builder.pointwise_fusion_report().portable_compiles, 1);
    builder.set_registry(builder.registry.clone());
    builder.build(&scene, &[0], 32, 32, true).unwrap();
    let report = builder.pointwise_fusion_report();
    assert_eq!(report.portable_compiles, 1);
    assert_eq!(report.portable_reuses, 1);
    assert_eq!(report.retired_cache_entries, 0);
    assert!(Arc::ptr_eq(
        &builder.programs[report.groups[0].program as usize].shader,
        &original
    ));
    // A success proof must not consume explicit real-parser failure injection.
    #[cfg(all(debug_assertions, feature = "gpu-tests"))]
    {
        builder.debug_inject_pointwise_portable_failure();
        let error = builder.build(&scene, &[0], 32, 32, true).unwrap_err();
        assert!(std::error::Error::source(&error).is_some());
        assert_eq!(builder.fusion.compiles, 2);
        builder.set_registry(builder.registry.clone());
        builder.build(&scene, &[0], 32, 32, true).unwrap();
        assert_eq!(builder.pointwise_fusion_report().portable_compiles, 3);
    }
}

#[test]
fn fusion_retired_code_is_bounded_and_cannot_exhaust_future_live_groups() {
    let (mut builder, scene) = fixture();
    let Compilation::Ready {
        shader, metrics, ..
    } = builder.fusion.compiled.values().next().unwrap()
    else {
        panic!("actual successful compilation is required")
    };
    let shader = shader.clone();
    let metrics = metrics.clone();
    builder.fusion.compiled.clear();
    // Capacity fixture aliases a real successful compiler result. Fake keys
    // are never executed; the next real group must evict one retired entry.
    for id in 0..MAX_COMPILED_GROUPS {
        builder.fusion.compiled.insert(
            format!("retired-{id}"),
            Compilation::Ready {
                program: None,
                shader: shader.clone(),
                metrics: metrics.clone(),
            },
        );
    }
    builder.set_registry(builder.registry.clone());
    builder.build(&scene, &[0], 32, 32, true).unwrap();
    let report = builder.pointwise_fusion_report();
    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.portable_compiles, 2);
    assert_eq!(report.cache_entries, MAX_COMPILED_GROUPS);
    assert_eq!(report.retired_cache_entries, MAX_COMPILED_GROUPS - 1);
    let mut oversized = (*shader).clone();
    oversized.wgsl = " ".repeat(MAX_RETIRED_CODE_BYTES + 1);
    builder.fusion.compiled.insert(
        "oversized".into(),
        Compilation::Ready {
            program: None,
            shader: Arc::new(oversized),
            metrics,
        },
    );
    builder.set_registry(builder.registry.clone());
    assert!(!builder.fusion.compiled.contains_key("oversized"));
    let mut report = FusionFrameReport::default();
    builder.fusion.cache_report(&mut report);
    assert!(report.retired_code_bytes <= MAX_RETIRED_CODE_BYTES);
    assert!(report.cache_entries <= MAX_COMPILED_GROUPS);
}
