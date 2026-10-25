use super::*;
use serde_json::json;

fn check_edit(engine: Engine, selected: Vec<KeyframeSelection>) {
    let before = engine.snapshot();
    let untouched = before.layers[1].clone();
    let keys = KeyframeClipboard::capture(&before, &selected).unwrap();
    for ease in [Ease::Linear, Ease::Hold, Ease::InOut] {
        let mut engine = Engine::new(before.clone()).unwrap();
        engine
            .apply(Command::KeyframeInterpolation {
                selections: selected.clone(),
                ease,
            })
            .unwrap();
        let actual = KeyframeClipboard::capture(engine.project(), &selected).unwrap();
        let mut expected = serde_json::to_value(&keys).unwrap();
        for track in expected["tracks"].as_array_mut().unwrap() {
            for key in track["data"]["keys"].as_array_mut().unwrap() {
                key["ease"] = serde_json::to_value(ease).unwrap();
                key.as_object_mut().unwrap().remove("curve");
            }
        }
        // Compare typed values, composition times, tangents, IDs and signatures.
        assert_eq!(serde_json::to_value(actual).unwrap(), expected);
        assert_eq!(engine.project().layers[1], untouched);
        let after = engine.snapshot();
        if after == before {
            assert!(!engine.can_undo(), "a no-op must not publish history");
            continue;
        }
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
        engine.redo().unwrap();
        assert_eq!(engine.project(), &after);
        let root = tempfile::tempdir().unwrap();
        motion_core::storage::save(root.path(), engine.project()).unwrap();
        assert_eq!(motion_core::storage::load(root.path()).unwrap(), after);
        engine.undo().unwrap();
    }
}

#[test]
fn interpolation_preserves_flat_and_nested_vector_channels_and_one_step_history() {
    for grouped in [false, true] {
        let (engine, targets) = vector_fixture(grouped);
        check_edit(
            engine,
            targets
                .into_iter()
                .map(|target| selection(target, &[10]))
                .collect(),
        );
    }
}

#[test]
fn interpolation_preserves_effect_color_curve_and_mask_geometry_values() {
    for effect in ["posterize", "curves"] {
        let (engine, target) = effects_fixture(effect);
        check_edit(engine, vec![selection(target, &[10])]);
    }
    let mut p = fixture().snapshot();
    let mut mask = LayerMask::new(1, path());
    seed(&mut mask.opacity);
    seed(&mut mask.feather);
    seed(&mut mask.expansion);
    for layer in &mut p.layers {
        layer.masks.push(mask.clone());
    }
    let selected = [
        MaskProperty::Opacity,
        MaskProperty::Feather,
        MaskProperty::Expansion,
        MaskProperty::Node(11),
    ]
    .into_iter()
    .map(|property| {
        selection(
            KeyframeTarget::Mask {
                object: 7,
                mask: 1,
                property,
            },
            &[10],
        )
    })
    .collect();
    check_edit(Engine::new(p).unwrap(), selected);
}

#[test]
fn interpolation_preflight_rejects_missing_duplicate_locked_and_ambiguous_axes_atomically() {
    check_edit(
        fixture(),
        vec![selection(property(7, Property::Position, None), &[10])],
    );
    let mut p = fixture().snapshot();
    for key in &mut p.layers[0].transform.position.keys {
        key.spatial = None;
    }
    p.layers[0].transform.position.separate().unwrap();
    let valid = selection(property(7, Property::Position, Some(Axis::X)), &[10]);
    let mut cases = vec![
        vec![],
        vec![valid.clone(), valid.clone()],
        vec![
            valid.clone(),
            selection(property(999, Property::Opacity, None), &[10]),
        ],
        vec![
            valid.clone(),
            selection(property(7, Property::Position, Some(Axis::Y)), &[11]),
        ],
        vec![
            valid.clone(),
            selection(property(7, Property::Opacity, None), &[120]),
        ],
        vec![selection(property(7, Property::Position, None), &[10])],
        vec![selection(
            property(7, Property::Position, Some(Axis::X)),
            &[10, 10],
        )],
    ];
    let mut locked = p.clone();
    locked.layers[1].locked = true;
    cases.push(vec![
        valid.clone(),
        selection(property(9, Property::Position, None), &[43]),
    ]);
    for (index, selections) in cases.into_iter().enumerate() {
        let before = if index == 7 {
            locked.clone()
        } else {
            p.clone()
        };
        let mut model = before.clone();
        assert!(motion_model::keyframe_clipboard::set_interpolation(
            &mut model,
            &selections,
            Ease::Hold
        )
        .is_err());
        assert_eq!(model, before, "model preflight case {index} changed data");
        let mut engine = Engine::new(before.clone()).unwrap();
        assert!(engine
            .apply(Command::KeyframeInterpolation {
                selections,
                ease: Ease::Hold
            })
            .is_err());
        assert_eq!(engine.project(), &before);
        assert!(!engine.can_undo());
    }
    check_edit(Engine::new(p).unwrap(), vec![valid]);
}

#[test]
fn interpolation_registry_routes_the_selected_composition_and_requires_a_policy() {
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
    let id = before.compositions[0].id.clone();
    let command = json!({"op":"keyframe_interpolation", "composition":id,
        "selections":[{"target":{"kind":"property","object":7,"property":"position","axis":null},"frames":[10]}],"ease":"hold"});
    let parsed = motion_core::CommandRegistry::parse(&command.to_string()).unwrap();
    engine.apply_batch(parsed).unwrap();
    assert_eq!(engine.project().layers, before.layers);
    assert_ne!(engine.project().compositions, before.compositions);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    let mut invalid = command.clone();
    invalid.as_object_mut().unwrap().remove("ease");
    assert!(motion_core::CommandRegistry::parse(&invalid.to_string()).is_err());
    let catalogue = motion_core::CommandRegistry::catalogue();
    let schema = &catalogue["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "keyframe_interpolation")
        .unwrap()["inputSchema"];
    assert_eq!(schema["properties"]["selections"]["type"], "array");
    assert!(schema["required"]
        .as_array()
        .unwrap()
        .contains(&json!("ease")));
}
