use super::*;
use motion_core::KeyframeValueOffset;

fn transform(
    selected: Vec<KeyframeSelection>,
    delta: i32,
    value: Option<KeyframeValueOffset>,
    copy: bool,
) -> Command {
    Command::TransformKeyframes {
        selections: selected,
        delta_frame: delta,
        value_offset: value,
        copy,
    }
}

#[test]
fn overlapping_selected_sources_move_as_one_edit_without_changing_fallback_or_metadata() {
    let mut engine = fixture();
    let before = engine.snapshot();
    let target = property(7, Property::Position, None);
    engine
        .apply(transform(
            vec![selection(target, &[10, 20])],
            10,
            None,
            false,
        ))
        .unwrap();
    let mut expected = before.clone();
    for key in &mut expected.layers[0].transform.position.keys {
        key.frame += 10;
    }
    assert_eq!(engine.project(), &expected);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    assert!(!engine.can_undo());
    engine.redo().unwrap();
    assert_eq!(engine.project(), &expected);
    let dir = tempfile::tempdir().unwrap();
    motion_core::storage::save(dir.path(), engine.project()).unwrap();
    assert_eq!(motion_core::storage::load(dir.path()).unwrap(), expected);
}

#[test]
fn copying_preserves_all_source_keys_and_offsets_only_the_selected_numeric_component() {
    let mut engine = fixture();
    let before = engine.snapshot();
    let target = property(7, Property::Position, None);
    engine
        .apply(transform(
            vec![selection(target.clone(), &[10, 20])],
            1,
            Some(KeyframeValueOffset {
                target,
                component: 1,
                delta: 7.,
            }),
            true,
        ))
        .unwrap();
    let mut expected = before.clone();
    for mut key in before.layers[0].transform.position.keys.clone() {
        key.frame += 1;
        key.value[1] += 7.;
        expected.layers[0].transform.position.keys.push(key);
    }
    expected.layers[0]
        .transform
        .position
        .keys
        .sort_by_key(|k| k.frame);
    assert_eq!(engine.project(), &expected);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    assert!(!engine.can_undo());
}

#[test]
fn scalar_vector_geometry_and_color_curve_channels_translate_without_value_or_easing_changes() {
    let mut fixtures = vec![];
    for grouped in [false, true] {
        let (engine, targets) = vector_fixture(grouped);
        fixtures.push((engine, targets));
    }
    for name in ["posterize", "curves"] {
        let (engine, target) = effects_fixture(name);
        fixtures.push((engine, vec![target]));
    }
    let mut project = fixture().snapshot();
    let mut mask = LayerMask::new(1, path());
    seed(&mut mask.opacity);
    project.layers[0].masks.push(mask);
    fixtures.push((
        Engine::new(project).unwrap(),
        vec![
            KeyframeTarget::Mask {
                object: 7,
                mask: 1,
                property: MaskProperty::Opacity,
            },
            KeyframeTarget::Mask {
                object: 7,
                mask: 1,
                property: MaskProperty::Node(11),
            },
        ],
    ));
    for (mut engine, targets) in fixtures {
        let before = engine.snapshot();
        let selected = targets
            .iter()
            .cloned()
            .map(|t| selection(t, &[10]))
            .collect::<Vec<_>>();
        let captured = KeyframeClipboard::capture(&before, &selected).unwrap();
        engine.apply(transform(selected, 4, None, false)).unwrap();
        let moved = targets
            .iter()
            .cloned()
            .map(|t| selection(t, &[14]))
            .collect::<Vec<_>>();
        assert_transfer(
            &captured,
            &KeyframeClipboard::capture(engine.project(), &moved).unwrap(),
            4,
        );
        assert_eq!(engine.project().layers[1], before.layers[1]);
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
        assert!(!engine.can_undo());
    }
}

#[test]
fn collision_bounds_copy_overlap_and_late_component_failure_preserve_the_entire_transaction() {
    let target = property(7, Property::Position, None);
    let opacity = property(7, Property::Opacity, None);
    for (command, cause) in [
        (
            transform(vec![selection(target.clone(), &[10])], 10, None, false),
            "destination is occupied",
        ),
        (
            transform(vec![selection(target.clone(), &[10, 20])], 10, None, true),
            "destination is occupied",
        ),
        (
            transform(vec![selection(target.clone(), &[10, 20])], -11, None, false),
            "outside composition",
        ),
        (
            transform(vec![selection(target.clone(), &[10, 20])], 100, None, false),
            "outside composition",
        ),
        (
            transform(
                vec![
                    selection(target.clone(), &[10]),
                    selection(opacity.clone(), &[15]),
                ],
                1,
                Some(KeyframeValueOffset {
                    target: opacity.clone(),
                    component: 1,
                    delta: 2.,
                }),
                false,
            ),
            "no such component",
        ),
        (
            transform(
                vec![selection(target.clone(), &[10])],
                1,
                Some(KeyframeValueOffset {
                    target: opacity,
                    component: 0,
                    delta: 1.,
                }),
                false,
            ),
            "channel is not selected",
        ),
        (
            transform(
                vec![selection(target.clone(), &[10])],
                1,
                Some(KeyframeValueOffset {
                    target: target.clone(),
                    component: 0,
                    delta: f32::NAN,
                }),
                false,
            ),
            "must be finite",
        ),
    ] {
        rejected(&mut fixture(), command, cause);
    }
    let mut project = fixture().snapshot();
    project.layers[1].locked = true;
    rejected(
        &mut Engine::new(project).unwrap(),
        transform(
            vec![
                selection(target, &[10]),
                selection(property(9, Property::Position, None), &[43]),
            ],
            1,
            None,
            false,
        ),
        "locked",
    );
}

#[test]
fn direct_model_preflight_does_not_partially_write_when_a_later_numeric_component_is_invalid() {
    let mut project = fixture().snapshot();
    let before = project.clone();
    let target = property(7, Property::Position, None);
    let opacity = property(7, Property::Opacity, None);
    let error = motion_model::keyframe_clipboard::transform_keys(
        &mut project,
        &[selection(target, &[10]), selection(opacity.clone(), &[15])],
        1,
        Some(&KeyframeValueOffset {
            target: opacity,
            component: 2,
            delta: 1.,
        }),
        false,
    )
    .unwrap_err();
    assert!(error.to_string().contains("no such component"));
    assert_eq!(project, before);
}

#[test]
fn separated_axes_and_different_layer_offsets_keep_one_composition_delta() {
    let mut engine = fixture();
    for frame in [10, 20] {
        engine
            .apply(Command::Spatial {
                object: 7,
                property: Property::Position,
                frame,
                tangents: None,
            })
            .unwrap();
    }
    engine
        .apply(Command::SeparateDimensions {
            object: 7,
            property: Property::Position,
        })
        .unwrap();
    let clip = KeyframeClipboard::capture(
        engine.project(),
        &[selection(
            property(7, Property::Position, Some(Axis::X)),
            &[10, 20],
        )],
    )
    .unwrap();
    engine
        .apply(Command::SeparateDimensions {
            object: 9,
            property: Property::Position,
        })
        .unwrap();
    engine
        .apply(paste(
            clip,
            vec![property(9, Property::Position, Some(Axis::Y))],
            Some(43),
        ))
        .unwrap();
    let mut engine = Engine::new(engine.snapshot()).unwrap();
    let before = engine.snapshot();
    let selected = vec![
        selection(property(7, Property::Position, Some(Axis::X)), &[10, 20]),
        selection(property(9, Property::Position, Some(Axis::Y)), &[43, 53]),
    ];
    engine
        .apply(transform(selected.clone(), -3, None, false))
        .unwrap();
    let moved = vec![
        selection(selected[0].target.clone(), &[7, 17]),
        selection(selected[1].target.clone(), &[40, 50]),
    ];
    assert_transfer(
        &KeyframeClipboard::capture(&before, &selected).unwrap(),
        &KeyframeClipboard::capture(engine.project(), &moved).unwrap(),
        -3,
    );
    let a = engine.project().layers[0]
        .transform
        .position
        .axes
        .as_ref()
        .unwrap();
    let b = before.layers[0].transform.position.axes.as_ref().unwrap();
    assert_eq!(a.y, b.y);
    assert_eq!(a.z, b.z);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    assert!(!engine.can_undo());
}

#[test]
fn typed_transform_command_routes_only_to_the_requested_inactive_composition() {
    let mut engine = fixture();
    engine
        .apply(Command::Composition {
            action: motion_core::CompositionAction::Create {
                settings: motion_core::CompositionSettings {
                    name: "Child".into(),
                    width: 640,
                    height: 360,
                    fps: 30,
                    frames: 120,
                    timing: "preserve_seconds".into(),
                    shorten: "reject".into(),
                },
            },
        })
        .unwrap();
    let child = engine.project().compositions[0].id.clone();
    for layer in fixture().project().layers.clone() {
        engine
            .apply(Command::InComposition {
                composition: child.clone(),
                command: Box::new(Command::Add { layer }),
            })
            .unwrap();
    }
    let before = engine.snapshot();
    let command = Command::InComposition {
        composition: child,
        command: Box::new(transform(
            vec![selection(property(7, Property::Position, None), &[10, 20])],
            1,
            None,
            false,
        )),
    };
    let parsed =
        motion_core::CommandRegistry::parse(&serde_json::to_string(&command).unwrap()).unwrap();
    motion_core::CommandRegistry::run(&mut engine, parsed).unwrap();
    assert_eq!(engine.project().layers, before.layers);
    assert_ne!(engine.project().compositions, before.compositions);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    assert!(motion_core::CommandRegistry::catalogue()["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["id"] == "transform_keyframes"));
}

#[test]
fn duplicate_aliases_unsorted_frames_and_missing_keys_are_rejected_before_any_move() {
    let target = property(7, Property::Position, None);
    for (selected, cause) in [
        (
            vec![selection(target.clone(), &[20, 10])],
            "sorted and unique",
        ),
        (
            vec![
                selection(target.clone(), &[10]),
                selection(target.clone(), &[20]),
            ],
            "duplicate keyframe channel",
        ),
        (vec![selection(target.clone(), &[11])], "does not exist"),
    ] {
        rejected(&mut fixture(), transform(selected, 1, None, false), cause);
    }
    let (mut engine, targets) = vector_fixture(false);
    let dash = targets
        .iter()
        .find(|t| matches!(t,KeyframeTarget::Vector {parameter,..} if parameter=="dash_0"))
        .unwrap()
        .clone();
    let mut alias = dash.clone();
    if let KeyframeTarget::Vector { parameter, .. } = &mut alias {
        *parameter = "dash_00".into();
    }
    rejected(
        &mut engine,
        transform(
            vec![selection(dash, &[10]), selection(alias, &[10])],
            1,
            None,
            false,
        ),
        "duplicate keyframe channel",
    );
}
