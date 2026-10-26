use super::*;
use motion_model::{EffectInstance, Layer, Project};

fn fixture(vectors: bool) -> (PlanBuilder, Scene) {
    let package = motion_effects::builtin::package().unwrap();
    let def = package
        .manifest
        .effects
        .iter()
        .find(|d| d.id == "displacement_map")
        .unwrap();
    let mut project = Project::new(32, 24, 30, 60).unwrap();
    for id in 1..=2 {
        let mut layer = Layer::solid(
            id,
            "preparation",
            [32., 24.],
            [16., 12., 0.],
            [0.2, 0.4, 0.6, 0.8],
        );
        layer.effects.push(EffectInstance::new(
            id * 10 + 1,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            def,
            [32., 24.],
        ));
        if vectors {
            layer.content = motion_model::Content::Vector {
                vector: motion_model::vector::VectorContent::shape(
                    motion_model::vector::ShapeKind::Rectangle,
                ),
            };
        }
        project.layers.push(layer);
    }
    project.rebuild_plugin_dependencies();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut builder = PlanBuilder::new(Registry::new_with_builtins().unwrap()).unwrap();
    builder.synchronize(&scene).unwrap();
    (builder, scene)
}

fn cycle(scene: &mut Scene) {
    for (i, effect) in scene.effects.iter_mut().enumerate() {
        effect.image_input = Some(EffectImageInput::Layer {
            layer: if i == 0 { 2 } else { 1 },
            stage: EffectImageStage::Effects,
        });
    }
}

#[test]
fn actual_cycles_are_refused_before_malformed_vector_geometry_and_recover() {
    let (mut builder, mut scene) = fixture(true);
    cycle(&mut scene);
    let healthy = scene.layers[0].vector.clone();
    Arc::make_mut(scene.layers[0].vector.as_mut().unwrap()).paths[0].nodes[0][0] = f32::NAN;
    for preview in [false, true] {
        let error = if preview {
            builder.build_preview(&scene, &[0], 32, 24)
        } else {
            builder.build(&scene, &[0], 32, 24, true)
        }
        .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("phase=effect_ir_preparation"), "{error:?}");
        assert!(
            message.contains("layer=1 effect=Some(11)")
                && message.contains("layer=2 effect=Some(21)")
        );
        let mut cause = &error;
        while let RenderError::Diagnostic { cause: source, .. } = cause {
            cause = source;
        }
        assert!(matches!(
            cause,
            RenderError::ExecutionPlan(ExecutionPlanError::Cycle { total: 2, .. })
        ));
        assert_eq!(builder.cpu_preparation_info().vector_jobs, 0);
        assert!(builder.vector_cache.is_empty() && builder.effect_execution().is_none());
    }
    scene.effects[1].image_input = None;
    let error = builder.build(&scene, &[0], 32, 24, true).unwrap_err();
    assert!(error.to_string().contains("vector rasterize"), "{error:?}");
    scene.layers[0].vector = healthy;
    builder.build(&scene, &[0], 32, 24, true).unwrap();
    assert_eq!(builder.frame.vectors.len(), 2);
    assert!(builder.effect_execution().is_some());
}

#[test]
fn typed_image_inputs_drive_the_actual_native_binding_and_snapshot_selection() {
    let (mut builder, mut scene) = fixture(false);
    for input in [
        None,
        Some(EffectImageInput::Empty),
        Some(EffectImageInput::Asset { asset: 19 }),
        Some(EffectImageInput::Layer {
            layer: 2,
            stage: EffectImageStage::Source,
        }),
        Some(EffectImageInput::Layer {
            layer: 2,
            stage: EffectImageStage::Effects,
        }),
        Some(EffectImageInput::Layer {
            layer: 999,
            stage: EffectImageStage::Effects,
        }),
    ] {
        scene.effects[0].image_input = input;
        builder.build(&scene, &[0, 19], 32, 24, true).unwrap();
        let pass = builder
            .frame
            .passes
            .iter()
            .find(|p| p.resource_input != 0)
            .unwrap_or(&builder.frame.passes[0]);
        let (resource, kind, effected) = match input {
            Some(EffectImageInput::Asset { .. }) => (-2, 3., 0.),
            Some(EffectImageInput::Layer { layer: 2, stage }) => (
                2,
                1.,
                if stage == EffectImageStage::Effects {
                    1.
                } else {
                    0.
                },
            ),
            Some(_) => (0, 2., 0.),
            None => (0, 0., 0.),
        };
        assert_eq!(pass.resource_input, resource);
        assert_eq!(
            (pass.uniform.params[30][0], pass.uniform.params[30][1]),
            (kind, effected)
        );
        assert_eq!(
            builder.effect_execution().unwrap().snapshot_order(),
            if effected == 1. { &[1][..] } else { &[][..] }
        );
        if resource == 2 {
            let expected = if effected == 1. {
                [1.; 4]
            } else {
                [linear(0.2), linear(0.4), linear(0.6), 0.8]
            };
            assert_eq!(pass.uniform.params[31], expected);
        }
    }
}

#[test]
fn invalid_ordinary_preview_effects_keep_their_original_bypass_and_cause() {
    let (mut builder, mut scene) = fixture(false);
    cycle(&mut scene);
    // Invalid candidate is omitted from dependency projection, then its actual
    // cause is delivered to the existing per-effect preview refusal policy.
    scene.effects[1].values[0][0] = f32::NAN;
    let frame = builder.build_preview(&scene, &[0], 32, 24).unwrap();
    assert_eq!(frame.diagnostics.len(), 1);
    assert!(frame.diagnostics[0].contains("layer 2, effect 21"));
    assert!(frame.diagnostics[0].contains("parameter") && !frame.diagnostics[0].contains("cycle"));
    assert!(builder.effect_execution().is_some());
}

#[test]
fn zero_opacity_and_raw_sources_do_not_create_effected_input_cycles() {
    let (mut builder, mut scene) = fixture(false);
    cycle(&mut scene);
    let opacity = scene.effects[1]
        .param_ids
        .iter()
        .position(|p| p == "effect_opacity")
        .unwrap();
    scene.effects[1].values[opacity][0] = 0.;
    builder.build(&scene, &[0], 32, 24, true).unwrap();
    scene.effects[1].values[opacity][0] = 100.;
    scene.effects[1].image_input = Some(EffectImageInput::Layer {
        layer: 1,
        stage: EffectImageStage::Source,
    });
    builder.build(&scene, &[0], 32, 24, true).unwrap();
    assert_eq!(builder.effect_execution().unwrap().snapshot_order(), &[1]);
}

#[test]
fn origin_refusal_is_bounded_and_clears_the_previous_checked_receipt_before_preparation() {
    let (mut builder, mut scene) = fixture(true);
    builder.build(&scene, &[0], 32, 24, true).unwrap();
    assert!(builder.effect_execution().is_some());
    scene.composition_id = "x".repeat(65536);
    let error = builder.build(&scene, &[0], 32, 24, true).unwrap_err();
    assert!(error.to_string().contains("phase=effect_ir_preparation"));
    assert!(error.to_string().len() < 1024);
    assert_eq!(builder.cpu_preparation_info().vector_jobs, 0);
    assert!(builder.effect_execution().is_none());
}

#[test]
fn cycle_diagnostics_exclude_a_downstream_tail() {
    let (builder, mut scene) = fixture(false);
    for effect in &mut scene.effects {
        effect.image_input = Some(EffectImageInput::Layer {
            layer: 2,
            stage: EffectImageStage::Effects,
        });
    }
    let error = builder.prepare_effect_inputs(&scene, &[0]).err().unwrap();
    let text = error.to_string();
    assert!(text.contains("layer=2 effect=Some(21)"));
    assert!(!text.contains("layer=1 effect=Some(11)"));
    let mut cause = &error;
    while let RenderError::Diagnostic { cause: source, .. } = cause {
        cause = source;
    }
    assert!(matches!(
        cause,
        RenderError::ExecutionPlan(ExecutionPlanError::Cycle { total: 1, .. })
    ));
}
