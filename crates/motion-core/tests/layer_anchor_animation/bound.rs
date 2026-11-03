use super::*;
use motion_core::{AnchorKeyBinding, CommandRegistry};
fn project(reverse: bool) -> Project {
    let mut p = mapped::project(reverse, false, 0);
    fn fixed<T: motion_core::Tween>(track: &mut Track<T>) {
        track.enable_roving(&[3]).unwrap();
        let spatial = track.spatial_animation.as_mut().unwrap();
        let key = spatial.keys()[1].id;
        spatial.set_fixed_time(key, 3.).unwrap();
    }
    fixed(&mut p.layers[1].transform.anchor);
    fixed(&mut p.layers[1].transform.position);
    p.validate().unwrap();
    p
}
fn binding(p: &Project) -> AnchorKeyBinding {
    let t = &p.layers[1].transform;
    AnchorKeyBinding {
        anchor: Some(t.anchor.spatial_animation.as_ref().unwrap().keys()[1].id),
        position: Some(t.position.spatial_animation.as_ref().unwrap().keys()[1].id),
    }
}
fn command(keys: AnchorKeyBinding, anchor: [f32; 2]) -> Command {
    Command::AnchorAt {
        object: 2,
        frame: 6.5,
        anchor,
        keys: Some(keys),
    }
}
fn replace<T: motion_core::Tween>(track: &mut Track<T>) {
    let spatial = track.spatial_animation.as_mut().unwrap();
    let key = spatial.keys()[1].clone();
    spatial.delete_key(key.id).unwrap();
    spatial.set_at_time(3., key.value).unwrap();
}
#[test]
fn bound_pivot_gestures_preserve_pose_ids_history_and_saved_project_in_both_directions() {
    for reverse in [false, true] {
        for commit in [false, true] {
            let p = project(reverse);
            let keys = binding(&p);
            let mut engine = Engine::new(p.clone()).unwrap();
            let pose = model(&p, 6.5);
            engine.begin_gesture().unwrap();
            for anchor in [[0.55, 0.35], [0.7, 0.2], [0.8, 0.15]] {
                engine.apply(command(keys.clone(), anchor)).unwrap();
                close(pose, model(engine.project(), 6.5));
                assert_eq!(binding(engine.project()), keys);
                assert_eq!(
                    engine.project().layers[1]
                        .transform
                        .anchor
                        .spatial_animation
                        .as_ref()
                        .unwrap()
                        .keys()
                        .len(),
                    3
                );
                assert_eq!(
                    engine.project().layers[1]
                        .transform
                        .position
                        .spatial_animation
                        .as_ref()
                        .unwrap()
                        .keys()
                        .len(),
                    3
                );
                let mut expected = serde_json::to_value(&p).unwrap();
                let actual = serde_json::to_value(engine.project()).unwrap();
                let base = &mut expected["layers"][1]["transform"];
                base["anchor"]["spatial_animation"]["keys"][1]["value"] = serde_json::json!(anchor);
                base["position"]["spatial_animation"]["keys"][1]["value"] = actual["layers"][1]
                    ["transform"]["position"]["spatial_animation"]["keys"][1]["value"]
                    .clone();
                assert_eq!(
                    actual, expected,
                    "only the two original key values may change"
                );
            }
            engine.end_gesture(commit).unwrap();
            if commit {
                let after = engine.snapshot();
                let root = tempfile::tempdir().unwrap();
                motion_core::storage::save(root.path(), &after).unwrap();
                assert_eq!(motion_core::storage::load(root.path()).unwrap(), after);
                assert!(engine.undo().unwrap());
                assert_eq!(engine.snapshot(), p);
                assert!(!engine.undo().unwrap());
                assert!(engine.redo().unwrap());
                assert_eq!(engine.snapshot(), after);
            } else {
                assert_eq!(engine.snapshot(), p);
                assert!(!engine.can_undo());
            }
        }
    }
}
#[test]
fn replaced_keys_are_rejected_before_either_channel_changes_then_fresh_binding_recovers() {
    for reverse in [false, true] {
        for channel in ["anchor", "position"] {
            let mut p = project(reverse);
            let stale = binding(&p);
            if channel == "anchor" {
                replace(&mut p.layers[1].transform.anchor)
            } else {
                replace(&mut p.layers[1].transform.position)
            }
            p.validate().unwrap();
            let before = p.clone();
            let pose = model(&p, 6.5);
            let mut engine = Engine::new(p).unwrap();
            let error = engine
                .apply(command(stale, [0.7, 0.2]))
                .unwrap_err()
                .to_string();
            assert!(
                error.contains("anchor_key_binding")
                    && error.contains("key=2")
                    && error.contains("no longer exists"),
                "{error}"
            );
            assert_eq!(engine.snapshot(), before);
            assert!(!engine.can_undo() && !engine.can_redo());
            engine
                .apply(command(binding(engine.project()), [0.7, 0.2]))
                .unwrap();
            close(pose, model(engine.project(), 6.5));
            assert!(engine.undo().unwrap());
            assert_eq!(engine.snapshot(), before);
        }
    }
}
#[test]
fn moved_keys_missing_storage_and_empty_or_zero_bindings_are_atomic_errors() {
    for reverse in [false, true] {
        for channel in ["anchor", "position"] {
            for cause in ["moved", "static", "empty", "zero"] {
                let mut p = project(reverse);
                let mut keys = binding(&p);
                match (channel, cause) {
                    ("anchor", "moved") => p.layers[1]
                        .transform
                        .anchor
                        .spatial_animation
                        .as_mut()
                        .unwrap()
                        .set_fixed_time(keys.anchor.unwrap(), 4.)
                        .unwrap(),
                    (_, "moved") => p.layers[1]
                        .transform
                        .position
                        .spatial_animation
                        .as_mut()
                        .unwrap()
                        .set_fixed_time(keys.position.unwrap(), 4.)
                        .unwrap(),
                    ("anchor", "static") => {
                        p.layers[1].transform.anchor = Track::constant([0.4, 0.6])
                    }
                    (_, "static") => {
                        p.layers[1].transform.position = Track::constant([20., 30., 0.])
                    }
                    (_, "empty") => keys = AnchorKeyBinding::default(),
                    ("anchor", "zero") => keys.anchor = Some(0),
                    (_, "zero") => keys.position = Some(0),
                    _ => unreachable!(),
                }
                p.validate().unwrap();
                let mut engine = Engine::new(p.clone()).unwrap();
                assert!(
                    engine.apply(command(keys, [0.7, 0.2])).is_err(),
                    "{channel} {cause}"
                );
                assert_eq!(engine.snapshot(), p);
                assert!(!engine.can_undo() && !engine.can_redo());
            }
        }
    }
}
#[test]
fn bound_roving_compensation_keeps_the_existing_constant_speed_policy_on_rejection() {
    let mut p = mapped::project(false, false, 0);
    // Exactly representable equally spaced values put both solved keys at 3.
    for (frame, value) in [(0, [0.25, 0.75]), (3, [0.5, 0.5]), (6, [0.75, 0.25])] {
        p.layers[1]
            .transform
            .anchor
            .upsert(frame, value, Ease::Linear)
            .unwrap();
    }
    p.layers[1].transform.anchor.enable_roving(&[3]).unwrap();
    p.layers[1].transform.position.enable_roving(&[3]).unwrap();
    let keys = binding(&p);
    assert_eq!(
        p.layers[1]
            .transform
            .anchor
            .spatial_animation
            .as_ref()
            .unwrap()
            .prepared()
            .keys()
            .nth(1)
            .unwrap()
            .1,
        3.
    );
    assert_eq!(
        p.layers[1]
            .transform
            .position
            .spatial_animation
            .as_ref()
            .unwrap()
            .prepared()
            .keys()
            .nth(1)
            .unwrap()
            .1,
        3.
    );
    let mut engine = Engine::new(p.clone()).unwrap();
    let error = engine
        .apply(command(keys, [0.7, 0.2]))
        .unwrap_err()
        .to_string();
    assert!(error.contains("retime spatial keys"), "{error}");
    assert_eq!(engine.snapshot(), p);
    assert!(!engine.can_undo());
}
#[test]
fn optional_binding_is_typed_in_the_catalogue_and_round_trips_without_changing_unbound_payloads() {
    let old = r#"{"op":"anchor_at","object":2,"frame":6.5,"anchor":[0.75,0.25]}"#;
    let raw: Command = serde_json::from_str(old).unwrap();
    let expected: serde_json::Value = serde_json::from_str(old).unwrap();
    assert_eq!(serde_json::to_value(raw).unwrap(), expected);
    let wrapped = CommandRegistry::parse(old).unwrap().remove(0);
    assert_eq!(
        serde_json::to_value(wrapped).unwrap(),
        serde_json::json!({
        "op":"in_composition","composition":"comp-main","command":expected})
    );
    let catalogue = CommandRegistry::catalogue();
    let desc = catalogue["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "anchor_at")
        .unwrap();
    assert!(!desc["inputSchema"]["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "keys"));
    assert!(desc["inputSchema"]["properties"]["keys"]
        .to_string()
        .contains("minimum"));
    let mut engine = Engine::new(project(false)).unwrap();
    let value = command(binding(engine.project()), [0.7, 0.2]);
    let text = serde_json::to_string(&value).unwrap();
    let commands = CommandRegistry::parse(&text).unwrap();
    let pose = model(engine.project(), 6.5);
    CommandRegistry::run(&mut engine, commands).unwrap();
    close(pose, model(engine.project(), 6.5));
    assert!(CommandRegistry::parse(r#"{"op":"anchor_at","object":2,"frame":6.5,"anchor":[0.7,0.2],"keys":{"anchor":2,"position":2,"other":9}}"#).is_err());
}
