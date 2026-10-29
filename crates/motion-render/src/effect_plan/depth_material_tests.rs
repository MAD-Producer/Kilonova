use super::*;
use motion_model::{EffectInstance, Layer, Project};

fn scene(id: &str) -> Scene {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == id)
        .unwrap();
    let mut project = Project::new(16, 16, 30, 1).unwrap();
    let mut layer = Layer::solid(1, "Material proof", [16.; 2], [8., 8., 0.], [1.; 4]);
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
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}

#[test]
fn depth_material_proof_tracks_alpha_modes_disabled_effects_and_external_inputs() {
    let mut builder = PlanBuilder::new(Registry::new_with_builtins().unwrap()).unwrap();
    let mut invert = scene("invert");
    let channel = invert.effects[0]
        .param_ids
        .iter()
        .position(|p| p == "channel")
        .unwrap();
    for mode in 1..=5 {
        invert.effects[0].values[channel][0] = mode as f32;
        assert_eq!(
            builder.depth_material_alpha(&invert, 1).unwrap(),
            Some(mode != 5)
        );
    }
    let blend = invert.effects[0]
        .param_ids
        .iter()
        .position(|p| p == "blend")
        .unwrap();
    invert.effects[0].values[blend][0] = 100.;
    assert_eq!(
        builder.depth_material_alpha(&invert, 1).unwrap(),
        Some(true)
    );
    invert.effects[0].image_input = Some(motion_model::EffectImageInput::Layer {
        layer: 99,
        stage: motion_model::EffectImageStage::Effects,
    });
    assert_eq!(builder.depth_material_alpha(&invert, 1).unwrap(), None);
    let mut unknown = scene("fill");
    assert_eq!(builder.depth_material_alpha(&unknown, 1).unwrap(), None);
    unknown.effects[0].enabled = false;
    assert_eq!(
        builder.depth_material_alpha(&unknown, 1).unwrap(),
        Some(true)
    );
    assert_eq!(
        builder.depth_material_alpha(&scene("tint"), 1).unwrap(),
        Some(true)
    );
}

#[test]
fn depth_material_proof_refuses_replaced_native_code_and_preserves_program_failure() {
    let mut builder = PlanBuilder::new(Registry::new_with_builtins().unwrap()).unwrap();
    let mut source = scene("invert");
    assert_eq!(
        builder.depth_material_alpha(&source, 1).unwrap(),
        Some(true)
    );
    let program = builder.resolved[0].as_ref().unwrap().programs[0];
    let original = builder.programs[program as usize].shader.clone();
    let mut changed = (*original).clone();
    changed
        .wgsl
        .push_str("\n// changed native implementation\n");
    builder.programs[program as usize].shader = Arc::new(changed);
    assert_eq!(builder.depth_material_alpha(&source, 1).unwrap(), None);
    builder.programs[program as usize].shader = original;
    builder
        .program_errors
        .insert(program, "controlled native material program failure".into());
    let error = builder.depth_material_alpha(&source, 1).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("controlled native material program failure"),
        "{error}"
    );
    builder.program_errors.clear();
    assert_eq!(
        builder.depth_material_alpha(&source, 1).unwrap(),
        Some(true)
    );
    source.effects[0].plugin = "missing.material.package".into();
    assert!(builder.depth_material_alpha(&source, 1).is_err());
}
