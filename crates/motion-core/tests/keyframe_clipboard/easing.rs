use super::*;
use motion_core::{Curve, CurveShape, CurveSpace, Easing};
use serde_json::json;

fn three_keys(mut engine: Engine, targets: &[KeyframeTarget]) -> Engine {
    let source = KeyframeClipboard::capture(
        engine.project(),
        &targets
            .iter()
            .cloned()
            .map(|target| selection(target, &[10]))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    for frame in [20, 30] {
        engine
            .apply(Command::PasteKeyframes {
                clipboard: Box::new(source.clone()),
                targets: targets.to_vec(),
                at_frame: Some(frame),
            })
            .unwrap();
    }
    Engine::new(engine.snapshot()).unwrap()
}
fn select(targets: &[KeyframeTarget], frames: &[u32]) -> Vec<KeyframeSelection> {
    targets
        .iter()
        .cloned()
        .map(|target| selection(target, frames))
        .collect()
}
fn quick(selections: Vec<KeyframeSelection>, incoming: bool, outgoing: bool) -> Command {
    Command::EaseKeyframes {
        selections,
        incoming,
        outgoing,
    }
}
fn raw(engine: &Engine, targets: &[KeyframeTarget]) -> serde_json::Value {
    serde_json::to_value(
        KeyframeClipboard::capture(engine.project(), &select(targets, &[10, 20, 30])).unwrap(),
    )
    .unwrap()
}
fn control_keys(engine: &Engine, object: u64) -> &[Keyframe<[f32; 3]>] {
    &engine
        .project()
        .layers
        .iter()
        .find(|l| l.id == object)
        .unwrap()
        .transform
        .position
        .keys
}
fn cubic(a: [f64; 2], b: [f64; 2]) -> Curve {
    Curve {
        space: CurveSpace::Progress,
        shape: CurveShape::Cubic {
            control1: a,
            control2: b,
            start: 0.,
            end: 1.,
        },
    }
}

#[test]
fn quick_ease_unions_adjacent_keys_and_preserves_unselected_handles_and_values() {
    let mut p = fixture().snapshot();
    p.layers[0].transform.position.keys = [3, 13, 23, 33]
        .into_iter()
        .map(|frame| Keyframe {
            auto_bezier: false,
            frame,
            value: [frame as f32 * 10., 20., 0.],
            ease: Ease::Linear,
            curve: Some(cubic([0.22, 0.11], [0.71, 0.84])),
            spatial: None,
        })
        .collect();
    let target = property(7, Property::Position, None);
    let mut engine = Engine::new(p).unwrap();
    let before = engine.snapshot();
    engine
        .apply(quick(
            vec![selection(target.clone(), &[20, 30])],
            true,
            true,
        ))
        .unwrap();
    let keys = control_keys(&engine, 7);
    assert_eq!(keys[0].curve, Some(cubic([0.22, 0.11], [0.71, 1.])));
    assert_eq!(
        keys[1].curve,
        Some(cubic([0.22, 0.], [0.71, 1.])),
        "both incident selections must survive in their shared interval"
    );
    assert_eq!(keys[2].curve, Some(cubic([0.22, 0.], [0.71, 0.84])));
    assert_eq!(keys[3], before.layers[0].transform.position.keys[3]);
    for (actual, old) in keys.iter().zip(&before.layers[0].transform.position.keys) {
        assert_eq!(actual.frame, old.frame);
        assert_eq!(actual.value, old.value);
        assert_eq!(actual.spatial, old.spatial);
    }
    for key in [&keys[0], &keys[1]] {
        assert!(
            Easing {
                ease: key.ease,
                curve: key.curve
            }
            .sample(1.)
            .velocity
            .abs()
                < 1e-9
        );
    }
    for key in [&keys[1], &keys[2]] {
        assert!(
            Easing {
                ease: key.ease,
                curve: key.curve
            }
            .sample(0.)
            .velocity
            .abs()
                < 1e-9
        );
    }
    assert_eq!(engine.project().layers[1], before.layers[1]);
    let after = engine.snapshot();
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    assert!(!engine.can_undo(), "multi-key quick ease is one undo entry");
    engine.redo().unwrap();
    assert_eq!(engine.project(), &after);
    let root = tempfile::tempdir().unwrap();
    motion_core::storage::save(root.path(), engine.project()).unwrap();
    assert_eq!(motion_core::storage::load(root.path()).unwrap(), after);
    // A stale mixed-channel selection cannot change the first valid channel.
    rejected(
        &mut engine,
        quick(
            vec![
                selection(target, &[20]),
                selection(property(9, Property::Position, None), &[20]),
            ],
            true,
            true,
        ),
        "does not exist",
    );
}

#[test]
fn quick_ease_edits_interval_start_before_composition_and_keeps_one_sided_policy() {
    for (incoming, outgoing) in [(true, false), (false, true), (true, true)] {
        let mut p = fixture().snapshot();
        p.layers[0].timeline.as_mut().unwrap().offset_frame = -7;
        p.layers[0].timeline.as_mut().unwrap().in_frame = 0;
        let mut engine = Engine::new(p).unwrap();
        let before = engine.snapshot();
        engine
            .apply(quick(
                vec![selection(property(7, Property::Position, None), &[6])],
                incoming,
                outgoing,
            ))
            .unwrap();
        let actual = &control_keys(&engine, 7)[0];
        if incoming {
            let e = Easing {
                ease: actual.ease,
                curve: actual.curve,
            };
            assert!(e.sample(1.).velocity.abs() < 1e-9);
            assert_eq!(
                actual.frame, 3,
                "negative composition start retains its signed local address"
            );
        } else {
            assert_eq!(*actual, before.layers[0].transform.position.keys[0]);
        }
    }
}

fn check_channels(engine: Engine, targets: Vec<KeyframeTarget>) {
    let mut engine = three_keys(engine, &targets);
    let before = engine.snapshot();
    let mut expected = raw(&engine, &targets);
    engine
        .apply(quick(select(&targets, &[10, 20]), true, true))
        .unwrap();
    // Values, signatures, source IDs, authored times and spatial metadata must
    // remain byte-for-byte equal apart from the two incident temporal curves.
    for track in expected["tracks"].as_array_mut().unwrap() {
        let keys = track["data"]["keys"].as_array_mut().unwrap();
        for (index, key) in keys.iter_mut().enumerate().take(2) {
            let old: Easing = serde_json::from_value(json!({"ease":key["ease"],"curve":key.get("curve").cloned().unwrap_or(serde_json::Value::Null)})).unwrap();
            let new = old.ease_sides(index == 0, true);
            key["ease"] = serde_json::to_value(new.ease).unwrap();
            key["curve"] = serde_json::to_value(new.curve).unwrap();
        }
    }
    assert_eq!(raw(&engine, &targets), expected);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    assert!(!engine.can_undo());
    engine
        .apply(Command::DeleteKeyframes {
            selections: select(&targets, &[10, 30]),
        })
        .unwrap();
    for track in KeyframeClipboard::capture(engine.project(), &select(&targets, &[20]))
        .unwrap()
        .tracks
    {
        let data = serde_json::to_value(track.data).unwrap();
        assert_eq!(data["keys"].as_array().unwrap().len(), 1);
    }
    assert_eq!(engine.project().layers.len(), before.layers.len());
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    assert!(!engine.can_undo());
}

#[test]
fn batch_selection_edits_cover_vector_scalar_color_geometry_and_color_curve_channels() {
    for grouped in [false, true] {
        let (engine, targets) = vector_fixture(grouped);
        check_channels(engine, targets);
    }
    for name in ["posterize", "curves"] {
        let (engine, target) = effects_fixture(name);
        check_channels(engine, vec![target]);
    }
    let mut p = fixture().snapshot();
    let mut mask = LayerMask::new(1, path());
    seed(&mut mask.opacity);
    seed(&mut mask.feather);
    seed(&mut mask.expansion);
    p.layers[0].masks.push(mask);
    let targets = [
        MaskProperty::Opacity,
        MaskProperty::Feather,
        MaskProperty::Expansion,
        MaskProperty::Node(11),
    ]
    .into_iter()
    .map(|property| KeyframeTarget::Mask {
        object: 7,
        mask: 1,
        property,
    })
    .collect();
    check_channels(Engine::new(p).unwrap(), targets);
}

#[test]
fn batch_edit_preflight_rejects_every_invalid_member_without_partial_model_or_history_changes() {
    let mut p = fixture().snapshot();
    for key in &mut p.layers[0].transform.position.keys {
        key.spatial = None;
    }
    p.layers[0].transform.position.separate().unwrap();
    p.layers[1].locked = true;
    let valid = selection(property(7, Property::Position, Some(Axis::X)), &[10]);
    for selections in [
        vec![],
        vec![valid.clone(), valid.clone()],
        vec![
            valid.clone(),
            selection(property(7, Property::Position, Some(Axis::Y)), &[11]),
        ],
        vec![
            valid.clone(),
            selection(property(9, Property::Position, None), &[43]),
        ],
        vec![
            valid.clone(),
            selection(property(7, Property::Position, None), &[10]),
        ],
        vec![selection(
            property(7, Property::Position, Some(Axis::X)),
            &[20, 10],
        )],
    ] {
        for delete in [false, true] {
            let mut model = p.clone();
            let result = if delete {
                motion_model::keyframe_clipboard::delete_keys(&mut model, &selections)
            } else {
                motion_model::keyframe_clipboard::ease_keys(&mut model, &selections, true, true)
            };
            assert!(result.is_err());
            assert_eq!(model, p, "direct model preflight cannot partially mutate");
            let mut engine = Engine::new(p.clone()).unwrap();
            let command = if delete {
                Command::DeleteKeyframes {
                    selections: selections.clone(),
                }
            } else {
                quick(selections.clone(), true, true)
            };
            assert!(engine.apply(command).is_err());
            assert_eq!(engine.project(), &p);
            assert!(!engine.can_undo());
        }
    }
    let mut engine = Engine::new(p).unwrap();
    rejected(
        &mut engine,
        quick(vec![valid], false, false),
        "at least one",
    );
}

#[test]
fn batch_edit_registry_exposes_explicit_policy_and_routes_composition_transactions() {
    let mut engine = fixture();
    engine
        .apply(Command::Composition {
            action: motion_core::CompositionAction::Precompose {
                objects: vec![7],
                name: "Child".into(),
                range: "composition".into(),
            },
        })
        .unwrap();
    let before = engine.snapshot();
    let child = &before.compositions[0].id;
    for operation in ["ease_keyframes", "delete_keyframes"] {
        let mut request = json!({"op":operation,"composition":child,
            "selections":[{"target":{"kind":"property","object":7,"property":"position","axis":null},"frames":[10]}]});
        if operation == "ease_keyframes" {
            request["incoming"] = json!(true);
            request["outgoing"] = json!(true);
        }
        engine
            .apply_batch(motion_core::CommandRegistry::parse(&request.to_string()).unwrap())
            .unwrap();
        assert_eq!(engine.project().layers, before.layers);
        assert_ne!(engine.project().compositions, before.compositions);
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
        if operation == "ease_keyframes" {
            request.as_object_mut().unwrap().remove("incoming");
            assert!(motion_core::CommandRegistry::parse(&request.to_string()).is_err());
        }
    }
}
