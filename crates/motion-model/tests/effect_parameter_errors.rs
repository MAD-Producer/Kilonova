use motion_model::{effects, EffectAction, EffectInstance, Layer, Project};

const FRAMES: u32 = 60;

/// A one-layer document holding one builtin effect instance, already valid.
fn document(effect: &str) -> Project {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|d| d.id == effect)
        .unwrap();
    let mut layer = Layer::solid(1, "test", [64.0; 2], [32.0, 32.0, 0.0], [1.0; 4]);
    layer.effects.push(EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [64.0; 2],
    ));
    let mut project = Project::new(64, 64, 30, FRAMES).unwrap();
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    project
}

fn set(project: &mut Project, param: &str, value: [f32; 4]) -> motion_model::Result<()> {
    effects::apply(
        &mut project.layers[0],
        EffectAction::Set {
            effect: 1,
            param: param.into(),
            frame: 0,
            value,
        },
        FRAMES,
    )
}

#[test]
fn effect_parameter_writes_reject_out_of_range_and_non_integral_values() {
    let mut project = document("gaussian_blur");
    let before = project.clone();
    // gaussian_blur: p0001 is a 0..=30000 float, p0002 a 1..=3 enum, p0003 a 0..=1 bool.
    for (param, value, expected) in [
        ("p0001", [30_001.0, 0.0, 0.0, 0.0], "exceeds range"),
        ("p0002", [2.5, 0.0, 0.0, 0.0], "integral"),
        ("p0003", [0.5, 0.0, 0.0, 0.0], "integral"),
    ] {
        let motion_model::Error::Invalid(message) = set(&mut project, param, value).unwrap_err()
        else {
            panic!("{param}: expected a model validation refusal");
        };
        assert!(message.contains(expected), "{param}: {message}");
        // The rejected write never reaches the track, so the edit is atomic even
        // without an engine snapshot to roll back to.
        assert_eq!(project, before, "{param}: rejected write mutated the document");
    }
    set(&mut project, "p0001", [12.5, 0.0, 0.0, 0.0]).unwrap();
    project.validate().unwrap();
    assert_eq!(
        project.layers[0].effects[0].params["p0001"]
            .sample(0.0)
            .unwrap()[0],
        12.5
    );
    // `animatable` governs keyframes only: `EffectParam::validate` requires a
    // non-animatable parameter to stay unkeyed, not to be read-only.
    project.layers[0].effects[0]
        .params
        .get_mut("p0003")
        .unwrap()
        .animatable = false;
    set(&mut project, "p0003", [1.0, 0.0, 0.0, 0.0]).unwrap();
    project.validate().unwrap();
}
