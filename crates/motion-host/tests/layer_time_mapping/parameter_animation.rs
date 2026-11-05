use super::NoMedia;
use motion_core::{Content, EffectInstance, FrameRate, Layer, LayerTimeline, Project};
use motion_host::Session;
use serde_json::{json, Value};
use std::{path::Path, sync::Arc};

fn session(root: &Path, effect: &str) -> Session {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|value| value.id == effect)
        .unwrap();
    let mut project =
        Project::new_with_frame_rate(32, 32, FrameRate::new(30000, 1001).unwrap(), 120).unwrap();
    let mut layer = Layer::solid(7, "Parameter contract", [16.; 2], [16., 16., 0.], [1.; 4]);
    layer.timeline = Some(LayerTimeline {
        in_frame: 10,
        out_frame: 110,
        offset_frame: 10,
    });
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
    Session::new(project, root.into(), Arc::new(NoMedia)).unwrap()
}
fn support(state: &Value, param: &str) -> Value {
    let entries = state["effectParameterCapabilities"].as_array().unwrap();
    let effect = entries
        .iter()
        .find(|entry| entry["layer"] == 7 && entry["instance"] == 1)
        .unwrap();
    effect["parameters"][param].clone()
}
fn animate(param: &str) -> Value {
    json!({"op":"effect","object":7,"action":{"kind":"animate","effect":1,"param":param,"frame":10,"enabled":true}})
}
fn set(param: &str, frame: u32, value: [f32; 4]) -> Value {
    json!({"op":"effect","object":7,"action":{"kind":"set","effect":1,"param":param,"frame":frame,"value":value}})
}
fn expression(param: &str) -> Value {
    json!({"op":"set_expression","frame":10,"expression":{"target":{"kind":"effect","object":7,"effect":1,"param":param},
        "source":"value","enabled":true,"seed":0,"profile":motion_core::EXPRESSION_PROFILE}})
}
fn apply(session: &mut Session, value: Value) -> motion_host::Result<Value> {
    session.apply_commands(&value.to_string())
}

#[test]
fn declared_parameter_matrix_matches_actual_key_and_expression_admission() {
    for (effect, param, kind, interpolation, expressions) in [
        (
            "brightness_contrast",
            "p0001",
            "float",
            Some("numeric"),
            true,
        ),
        ("mirror", "p0001", "vec2", Some("numeric"), true),
        ("tint", "p0001", "color", Some("numeric"), true),
        ("brightness_contrast", "p0003", "bool", Some("hold"), false),
        ("exposure", "p0001", "enum", Some("hold"), false),
        ("curves", "p0001", "curve", Some("color_curve_lut"), false),
        ("hue_saturation", "p0007", "bool", None, false),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let mut session = session(directory.path(), effect);
        let initial = session.snapshot().unwrap();
        let contract = support(&initial, param);
        assert_eq!(contract["kind"], kind);
        assert_eq!(
            contract["animation"],
            json!({"keyframes":interpolation.is_some(),
            "expressions":expressions,"interpolation":interpolation})
        );
        assert_eq!(
            initial["capabilities"]["effect_parameters"]["image_input_animation"],
            false
        );
        assert!(matches!(
            session.engine.project().layers[0].content,
            Content::Solid { .. }
        ));
        let before = session.engine.snapshot();
        let keyed = apply(&mut session, animate(param));
        if interpolation.is_some() {
            keyed.unwrap();
        } else {
            assert!(keyed.unwrap_err().to_string().contains("not animatable"));
            assert_eq!(session.engine.project(), &before);
        }
        let before_expression = session.engine.snapshot();
        let can_undo = session.engine.can_undo();
        let result = apply(&mut session, expression(param));
        if expressions {
            result.unwrap();
        } else {
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("continuous effect parameter"));
            assert_eq!(session.engine.project(), &before_expression);
            assert_eq!(session.engine.can_undo(), can_undo);
        }
        assert_eq!(support(&session.snapshot().unwrap(), param), contract);
    }
}

#[test]
fn numeric_and_discrete_tracks_keep_offsets_atomic_history_and_reopen_contract() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = session(directory.path(), "brightness_contrast");
    session.frame = 15.5;
    session.sample().unwrap();
    let original = session.engine.snapshot();
    let original_contract = session.snapshot().unwrap()["effectParameterCapabilities"].clone();
    apply(
        &mut session,
        json!([
            animate("p0001"),
            set("p0001", 20, [100., 0., 0., 0.]),
            animate("p0003"),
            set("p0003", 20, [1., 0., 0., 0.])
        ]),
    )
    .unwrap();
    let edited = session.engine.snapshot();
    let params = &edited.layers[0].effects[0].params;
    assert_eq!(
        params["p0001"]
            .track
            .keys
            .iter()
            .map(|key| key.frame)
            .collect::<Vec<_>>(),
        vec![0, 10]
    );
    assert!(params["p0003"]
        .track
        .keys
        .iter()
        .all(|key| key.ease == motion_core::Ease::Hold));
    let state = session.snapshot().unwrap();
    assert_eq!(state["sampledEffects"][0]["values"]["p0001"][0], 55.);
    assert_eq!(state["sampledEffects"][0]["values"]["p0003"][0], 0.);
    let invalid = apply(
        &mut session,
        json!([
            set("p0001", 20, [50., 0., 0., 0.]),
            set("p0003", 20, [0.5, 0., 0., 0.])
        ]),
    );
    assert!(invalid.unwrap_err().to_string().contains("integral"));
    assert_eq!(session.engine.project(), &edited);
    session.engine.undo().unwrap();
    assert_eq!(session.engine.project(), &original);
    session.engine.redo().unwrap();
    assert_eq!(session.engine.project(), &edited);
    motion_core::storage::save(directory.path(), &edited).unwrap();
    let reopened = motion_core::storage::load(directory.path()).unwrap();
    assert_eq!(reopened, edited);
    let mut reopened = Session::new(reopened, directory.path().into(), Arc::new(NoMedia)).unwrap();
    reopened.frame = 15.5;
    reopened.sample().unwrap();
    let restored = reopened.snapshot().unwrap();
    assert_eq!(restored["effectParameterCapabilities"], original_contract);
    assert_eq!(restored["sampledEffects"], state["sampledEffects"]);
}

#[test]
fn curve_tracks_advertise_lut_interpolation_and_sample_it_after_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = session(directory.path(), "curves");
    let mut curve = motion_core::CurveObject::default();
    curve.channels[0] = vec![[0., 0.], [1., 0.]];
    apply(&mut session,json!([animate("p0001"),
        {"op":"effect","object":7,"action":{"kind":"set_curve_object","effect":1,"param":"p0001","frame":20,"value":curve}}])).unwrap();
    session.frame = 15.;
    session.sample().unwrap();
    let state = session.snapshot().unwrap();
    assert_eq!(
        support(&state, "p0001")["animation"]["interpolation"],
        "color_curve_lut"
    );
    let actual = state["sampledEffects"][0]["curve_lut"][128][0]
        .as_f64()
        .unwrap();
    assert!(
        (actual - 64. / 255.).abs() < 1e-6,
        "curve midpoint {actual}"
    );
    motion_core::storage::save(directory.path(), session.engine.project()).unwrap();
    let project = motion_core::storage::load(directory.path()).unwrap();
    let mut reopened = Session::new(project, directory.path().into(), Arc::new(NoMedia)).unwrap();
    reopened.frame = 15.;
    reopened.sample().unwrap();
    let restored = reopened.snapshot().unwrap();
    assert_eq!(
        restored["effectParameterCapabilities"],
        state["effectParameterCapabilities"]
    );
    assert_eq!(restored["sampledEffects"], state["sampledEffects"]);
}

#[test]
fn enum_keys_hold_until_the_boundary_and_layer_lock_is_separate_from_schema_support() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = session(directory.path(), "exposure");
    let contract = support(&session.snapshot().unwrap(), "p0001");
    apply(
        &mut session,
        json!([animate("p0001"), set("p0001", 20, [2., 0., 0., 0.])]),
    )
    .unwrap();
    for (frame, expected) in [(15.5, 1.), (19.999, 1.), (20., 2.)] {
        session.frame = frame;
        session.sample().unwrap();
        assert_eq!(
            session.snapshot().unwrap()["sampledEffects"][0]["values"]["p0001"][0],
            expected
        );
    }
    apply(
        &mut session,
        json!({"op":"flags","object":7,"visible":true,"locked":true}),
    )
    .unwrap();
    let before = session.engine.snapshot();
    let history = session.engine.history_diagnostics();
    let error = apply(&mut session, set("p0001", 20, [1., 0., 0., 0.])).unwrap_err();
    assert!(error.to_string().contains("locked"), "{error}");
    assert_eq!(session.engine.project(), &before);
    assert_eq!(session.engine.history_diagnostics(), history);
    assert_eq!(support(&session.snapshot().unwrap(), "p0001"), contract);
}

#[test]
fn declared_cross_parameter_references_use_local_keys_and_reject_cycles_atomically() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = session(directory.path(), "brightness_contrast");
    apply(
        &mut session,
        json!([animate("p0001"), set("p0001", 20, [100., 0., 0., 0.])]),
    )
    .unwrap();
    let target = |param: &str| motion_core::ExpressionTarget::Effect {
        object: 7,
        effect: 1,
        param: param.into(),
    };
    let source = motion_core::expression_reference_source(&target("p0001")).unwrap();
    let mut linked = expression("p0002");
    linked["expression"]["source"] = json!(format!("({source}) * 0.5"));
    apply(&mut session, linked).unwrap();
    session.frame = 15.5;
    session.sample().unwrap();
    let state = session.snapshot().unwrap();
    assert_eq!(
        state["capabilities"]["effect_parameters"]["cross_parameter_expressions"],
        true
    );
    assert_eq!(
        state["capabilities"]["property_expressions"]["cross_property_references"],
        true
    );
    assert_eq!(state["sampledEffects"][0]["values"]["p0002"][0], 27.5);
    let before = session.engine.snapshot();
    let history = session.engine.history_diagnostics();
    let mut cycle = expression("p0001");
    cycle["expression"]["source"] =
        json!(motion_core::expression_reference_source(&target("p0002")).unwrap());
    let error = apply(&mut session, cycle).unwrap_err();
    assert!(error.to_string().contains("dependency cycle"), "{error}");
    assert_eq!(session.engine.project(), &before);
    assert_eq!(session.engine.history_diagnostics(), history);
    motion_core::storage::save(directory.path(), &before).unwrap();
    let project = motion_core::storage::load(directory.path()).unwrap();
    let mut reopened = Session::new(project, directory.path().into(), Arc::new(NoMedia)).unwrap();
    reopened.frame = 15.5;
    reopened.sample().unwrap();
    assert_eq!(
        reopened.snapshot().unwrap()["sampledEffects"],
        state["sampledEffects"]
    );
}
