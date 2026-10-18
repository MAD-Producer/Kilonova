use super::*;
use crate::panels::animation_data as data;
use motion_core::{Axis, Engine, Keyframe, Layer, Project};

fn key<T>(frame: i32, value: T) -> Keyframe<T> {
    Keyframe {
        frame,
        value,
        ease: motion_core::Ease::Linear,
        curve: None,
        spatial: None,
    }
}

fn fixture() -> Engine {
    let mut project = Project::new(640, 480, 30, 120).unwrap();
    project.version = 20;
    let mut layer = Layer::solid(7, "Channel", [100.; 2], [10., 20., 30.], [1.; 4]);
    layer.three_d = true;
    layer.transform.anchor.keys = vec![key(-12, [0.2, 0.8]), key(18, [0.8, 0.2])];
    layer.transform.position.keys = vec![key(-12, [10., 20., 30.]), key(18, [40., 80., 60.])];
    project.layers.push(layer);
    let mut engine = Engine::new(project).unwrap();
    engine
        .apply(Command::MoveLayerClip {
            object: 7,
            in_frame: 12,
        })
        .unwrap();
    engine
}

fn state(engine: &Engine, frame: f64) -> Value {
    serde_json::json!({"frame": frame, "project": engine.project(), "timeline_layers": engine.project().timeline_layers(frame)})
}

#[test]
fn candidates_obey_composition_bounds_without_rounding_subframe_navigation() {
    let times = [60., -12., 12., 60., 120., f64::NAN, 0., 30.];
    assert_eq!(
        neighbours(times, 12., 119.),
        Neighbours {
            previous: Some(0.),
            current: true,
            next: Some(30.)
        }
    );
    assert_eq!(
        neighbours(times, 12.5, 119.),
        Neighbours {
            previous: Some(12.),
            current: false,
            next: Some(30.)
        }
    );
    assert_eq!(neighbours(times, 0., 119.).previous, None);
    assert_eq!(neighbours(times, 60., 119.).next, None);
    assert_eq!(editable_frame(12., 119.), Some(12));
    for frame in [-1., 12.5, 120., f64::NAN, f64::INFINITY] {
        assert_eq!(editable_frame(frame, 119.), None);
    }
}

#[test]
fn adding_an_anchor_key_uses_two_components_and_preserves_local_time_storage_and_undo() {
    let mut engine = fixture();
    let before = engine.project().clone();
    let root = tempfile::tempdir().unwrap();
    let commands =
        data::key_commands(&state(&engine, 12.), 7, Property::Anchor, None, 12, false).unwrap();
    assert!(matches!(
        commands.as_slice(),
        [
            Command::Animate {
                object: 7,
                property: Property::Anchor,
                axis: None,
                frame: 12,
                enabled: true
            },
            Command::SetVector {
                object: 7,
                property: Property::Anchor,
                frame: 12,
                ..
            }
        ]
    ));
    engine.apply_batch(commands).unwrap();
    let inserted = &engine.project().layers[0].transform.anchor.keys;
    assert_eq!(
        inserted.iter().map(|key| key.frame).collect::<Vec<_>>(),
        vec![-12, 0, 18]
    );
    assert!((inserted[1].value[0] - 0.44).abs() < 1e-6);
    assert!((inserted[1].value[1] - 0.56).abs() < 1e-6);
    assert_eq!(
        engine.project().layers[0].transform.position,
        before.layers[0].transform.position
    );
    motion_core::storage::save(root.path(), engine.project()).unwrap();
    assert_eq!(
        motion_core::storage::load(root.path()).unwrap(),
        *engine.project()
    );
    let added = engine.project().clone();
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    engine.redo().unwrap();
    assert_eq!(engine.project(), &added);
    let remove =
        data::key_commands(&state(&engine, 12.), 7, Property::Anchor, None, 12, true).unwrap();
    assert!(matches!(
        remove.as_slice(),
        [Command::DeleteKey {
            property: Property::Anchor,
            frame: 12,
            ..
        }]
    ));
    engine.apply_batch(remove).unwrap();
    assert_eq!(
        engine.project().layers[0]
            .transform
            .anchor
            .keys
            .iter()
            .map(|key| key.frame)
            .collect::<Vec<_>>(),
        vec![-12, 18]
    );
    engine.undo().unwrap();
    assert_eq!(engine.project(), &added);
}

#[test]
fn one_axis_key_edits_preserve_other_axes_and_readonly_or_missing_targets_do_not_edit() {
    let mut engine = fixture();
    engine
        .apply(Command::SeparateDimensions {
            object: 7,
            property: Property::Position,
        })
        .unwrap();
    let before = state(&engine, 12.);
    let before_project = engine.project().clone();
    let commands =
        data::key_commands(&before, 7, Property::Position, Some(Axis::X), 12, false).unwrap();
    assert!(matches!(
        commands.as_slice(),
        [
            Command::Animate {
                axis: Some(Axis::X),
                ..
            },
            Command::SetComponent { axis: Axis::X, .. }
        ]
    ));
    engine.apply_batch(commands).unwrap();
    let after = state(&engine, 12.);
    let channel = |s: &Value, axis| {
        data::channel(s, 7, Property::Position, Some(axis))
            .unwrap()
            .clone()
    };
    for axis in [Axis::Y, Axis::Z] {
        assert_eq!(channel(&after, axis), channel(&before, axis));
    }
    let root = tempfile::tempdir().unwrap();
    motion_core::storage::save(root.path(), engine.project()).unwrap();
    assert_eq!(
        motion_core::storage::load(root.path()).unwrap(),
        *engine.project()
    );
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before_project);
    assert!(
        data::key_commands(&before, 99, Property::Position, Some(Axis::X), 12, false).is_none()
    );
    assert!(data::key_commands(&before, 7, Property::Position, None, 12, false).is_none());
    let mut readonly = before.clone();
    readonly["observing"] = true.into();
    assert!(
        data::key_commands(&readonly, 7, Property::Position, Some(Axis::X), 12, false).is_none()
    );
    engine
        .apply(Command::Flags {
            object: 7,
            visible: true,
            locked: true,
        })
        .unwrap();
    let locked = engine.project().clone();
    assert!(data::key_commands(
        &state(&engine, 12.),
        7,
        Property::Position,
        Some(Axis::X),
        12,
        false
    )
    .is_none());
    assert_eq!(engine.project(), &locked);
}

fn draw_host(
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    panel: Panel,
    floating: bool,
    atlas: &mut TextAtlas,
) -> Vec<Action> {
    let mut list = PaintList::default();
    if floating {
        super::super::floating_chrome(&mut list, atlas, panel, [1000., 800.], panels, state, input)
    } else {
        super::super::chrome(
            &mut list,
            atlas,
            [1000., 800.],
            &dock::Node::dock(panel.dock()),
            panels,
            state,
            input,
        )
    }
}

fn click_slot(row: Rect, slot: usize) -> Input {
    let styles = PaintList::default().styles();
    let rect = rail(&styles, row);
    let point = [
        rect.min[0] + (slot as f32 + 0.5) * styles.metric("icon.slot", 20.),
        (rect.min[1] + rect.max[1]) * 0.5,
    ];
    Input {
        mouse: point,
        pressed: Some((MouseButton::Left, point)),
        ..Default::default()
    }
}

fn mutations(actions: &[Action]) -> usize {
    actions
        .iter()
        .filter(|action| {
            matches!(
                action,
                Action::Edit(_) | Action::EditBatch(_) | Action::History(_)
            )
        })
        .count()
}

fn assert_row_cancellation_and_readonly(
    panels: &mut Panels,
    state: &Value,
    row: Rect,
    panel: Panel,
    floating: bool,
    atlas: &mut TextAtlas,
) {
    for escape in [false, true] {
        let mut input = click_slot(row, 1);
        if escape {
            input.keys.push((Key::Escape, Default::default()));
        } else {
            input.focus_lost = true;
        }
        assert_eq!(
            mutations(&draw_host(panels, state, &input, panel, floating, atlas)),
            0
        );
    }
    let mut readonly = state.clone();
    readonly["observing"] = true.into();
    let mut locked = state.clone();
    locked["project"]["layers"][0]["locked"] = true.into();
    for snapshot in [&readonly, &locked] {
        assert_eq!(
            mutations(&draw_host(
                panels,
                snapshot,
                &click_slot(row, 1),
                panel,
                floating,
                atlas
            )),
            0
        );
    }
}

#[test]
fn actual_property_hosts_add_remove_and_navigate_one_separated_axis_without_cross_channel_edits() {
    let mut authored = fixture();
    authored
        .apply(Command::SeparateDimensions {
            object: 7,
            property: Property::Position,
        })
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    motion_core::storage::save(root.path(), authored.project()).unwrap();
    let host = crate::engine::Engine::start(
        root.path().into(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::Zh, Locale::En] {
        for panel in [Panel::Properties, Panel::Timeline] {
            for floating in [false, true] {
                host.seek(12.).unwrap();
                let before = host.state().unwrap();
                let mut panels = Panels {
                    focused: Some(panel),
                    locale,
                    ..Default::default()
                };
                panels.select_only(Some(7));
                panels.timeline.expanded.insert(7);
                draw_host(
                    &mut panels,
                    &before,
                    &Input::default(),
                    panel,
                    floating,
                    &mut atlas,
                );
                let target = motion_core::ExpressionTarget::Property {
                    object: 7,
                    property: Property::Position,
                    axis: Some(Axis::X),
                };
                let row = panels
                    .expressions
                    .triggers
                    .iter()
                    .find(|(candidate, _)| candidate == &target)
                    .expect("the actual separated row is visible")
                    .1;
                let row = Rect::new(0., row.min[1], 1000., row.height());

                let added_actions = draw_host(
                    &mut panels,
                    &before,
                    &click_slot(row, 1),
                    panel,
                    floating,
                    &mut atlas,
                );
                assert_eq!(
                    mutations(&added_actions),
                    1,
                    "{panel:?} {floating} {locale:?}: {added_actions:?}"
                );
                let commands = added_actions
                    .into_iter()
                    .find_map(|action| {
                        if let Action::EditBatch(commands) = action {
                            Some(commands)
                        } else {
                            None
                        }
                    })
                    .expect("one atomic add from the real buttons");
                assert!(commands.iter().all(|command| matches!(
                    command,
                    Command::Animate {
                        property: Property::Position,
                        axis: Some(Axis::X),
                        ..
                    } | Command::SetComponent {
                        property: Property::Position,
                        axis: Axis::X,
                        ..
                    }
                )));
                let added = host
                    .edit(serde_json::to_string(&commands).unwrap())
                    .unwrap();
                for axis in ["y", "z"] {
                    assert_eq!(
                        added["project"]["layers"][0]["transform"]["position"]["axes"][axis],
                        before["project"]["layers"][0]["transform"]["position"]["axes"][axis]
                    );
                }
                assert_eq!(host.history(0).unwrap()["project"], before["project"]);
                assert_eq!(host.history(1).unwrap()["project"], added["project"]);
                let removed = draw_host(
                    &mut panels,
                    &added,
                    &click_slot(row, 1),
                    panel,
                    floating,
                    &mut atlas,
                );
                let commands = removed
                    .into_iter()
                    .find_map(|action| {
                        if let Action::EditBatch(commands) = action {
                            Some(commands)
                        } else {
                            None
                        }
                    })
                    .expect("current key removes instead of replacing");
                assert!(matches!(
                    commands.as_slice(),
                    [Command::DeleteKey {
                        property: Property::Position,
                        axis: Some(Axis::X),
                        frame: 12,
                        ..
                    }]
                ));
                let after = host
                    .edit(serde_json::to_string(&commands).unwrap())
                    .unwrap();
                assert_eq!(after["project"], before["project"]);

                for (slot, expected) in [(0, 0.), (2, 30.)] {
                    let actions = draw_host(
                        &mut panels,
                        &after,
                        &click_slot(row, slot),
                        panel,
                        floating,
                        &mut atlas,
                    );
                    assert_eq!(mutations(&actions), 0);
                    assert_eq!(
                        actions
                            .iter()
                            .filter(
                                |action| matches!(action, Action::Seek(frame) if *frame == expected)
                            )
                            .count(),
                        1,
                        "{actions:?}"
                    );
                    assert_eq!(
                        panels.animation_controls.user_key().unwrap().axis,
                        Some(Axis::X)
                    );
                }
                host.seek(0.).unwrap();
                let first = host.state().unwrap();
                let actions = draw_host(
                    &mut panels,
                    &first,
                    &click_slot(row, 0),
                    panel,
                    floating,
                    &mut atlas,
                );
                assert!(
                    !actions.iter().any(|action| matches!(
                        action,
                        Action::Seek(_)
                            | Action::Edit(_)
                            | Action::EditBatch(_)
                            | Action::History(_)
                    )),
                    "endpoint: {actions:?}"
                );
                for cancel in [true, false] {
                    let mut input = click_slot(row, 1);
                    if cancel {
                        input.keys.push((Key::Escape, Default::default()));
                    } else {
                        input.focus_lost = true;
                    }
                    assert_eq!(
                        mutations(&draw_host(
                            &mut panels,
                            &first,
                            &input,
                            panel,
                            floating,
                            &mut atlas
                        )),
                        0
                    );
                }
                let mut readonly = first.clone();
                readonly["observing"] = true.into();
                assert_eq!(
                    mutations(&draw_host(
                        &mut panels,
                        &readonly,
                        &click_slot(row, 1),
                        panel,
                        floating,
                        &mut atlas
                    )),
                    0
                );
                assert_eq!(host.state().unwrap()["project"], before["project"]);
            }
        }
    }
}

#[test]
fn camera_rows_keep_axis_targets_and_cancellation_in_both_native_hosts() {
    let mut engine = Engine::new(Project::new(640, 480, 30, 120).unwrap()).unwrap();
    let object = match engine
        .apply_batch(vec![Command::CreateCamera])
        .unwrap()
        .as_slice()
    {
        [motion_core::EditResult::CreateCamera { object }] => *object,
        other => panic!("unexpected camera creation: {other:?}"),
    };
    engine
        .apply(Command::Animate {
            object,
            property: Property::Position,
            axis: None,
            frame: 0,
            enabled: true,
        })
        .unwrap();
    engine
        .apply(Command::SetVector {
            object,
            property: Property::Position,
            frame: 30,
            value: [420., 340., -1000.],
        })
        .unwrap();
    engine
        .apply(Command::SeparateDimensions {
            object,
            property: Property::Position,
        })
        .unwrap();
    engine
        .apply(Command::MoveLayerClip {
            object,
            in_frame: 12,
        })
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    motion_core::storage::save(root.path(), engine.project()).unwrap();
    let host = crate::engine::Engine::start(
        root.path().into(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::Zh, Locale::En] {
        for panel in [Panel::Properties, Panel::Timeline] {
            for floating in [false, true] {
                host.seek(24.).unwrap();
                let state = host.state().unwrap();
                let mut panels = Panels {
                    focused: Some(panel),
                    locale,
                    ..Default::default()
                };
                panels.select_only(Some(object));
                panels.timeline.expanded.insert(object);
                draw_host(
                    &mut panels,
                    &state,
                    &Input::default(),
                    panel,
                    floating,
                    &mut atlas,
                );
                let top = if floating {
                    30.
                } else {
                    super::super::frames(&dock::Node::dock(panel.dock()), [1000., 800.])[0]
                        .content
                        .min[1]
                };
                let styles = PaintList::default().styles();
                let row = if panel == Panel::Properties {
                    let height = styles.metric("camera.row.height", 28.);
                    Rect::new(
                        0.,
                        top + styles.metric("padding.panel", 8.) + height,
                        1000.,
                        height,
                    )
                } else {
                    let rows = super::super::timeline::rows(&state);
                    let row = rows.iter().find(|row| row.id == object).unwrap();
                    let plan = super::super::timeline_layout::LayerRows::new(
                        row,
                        &panels.timeline,
                        24.,
                        locale,
                    );
                    let property = row
                        .properties
                        .iter()
                        .position(|track| {
                            track.property == Property::Position && track.axis == Some(0)
                        })
                        .unwrap();
                    let index = plan
                        .properties
                        .iter()
                        .position(|index| *index == property)
                        .unwrap();
                    let height = styles.metric("row.height", 22.);
                    Rect::new(
                        0.,
                        top + 64. + plan.property_offsets[index] as f32 * height,
                        1000.,
                        height,
                    )
                };
                let actions = draw_host(
                    &mut panels,
                    &state,
                    &click_slot(row, 1),
                    panel,
                    floating,
                    &mut atlas,
                );
                let commands = actions
                    .into_iter()
                    .find_map(|action| {
                        if let Action::EditBatch(commands) = action {
                            Some(commands)
                        } else {
                            None
                        }
                    })
                    .expect("camera row emits one channel edit");
                assert_row_cancellation_and_readonly(
                    &mut panels,
                    &state,
                    row,
                    panel,
                    floating,
                    &mut atlas,
                );
                assert!(
                    matches!(commands.as_slice(), [Command::Animate { object: target, property: Property::Position, axis: Some(Axis::X), frame: 24, enabled: true }, Command::SetComponent { object: target2, property: Property::Position, axis: Axis::X, frame: 24, .. }] if *target == object && *target2 == object),
                    "{commands:?}"
                );
                let after = host
                    .edit(serde_json::to_string(&commands).unwrap())
                    .unwrap();
                for axis in ["y", "z"] {
                    assert_eq!(
                        after["project"]["layers"][0]["content"]["camera"]["position"]["axes"]
                            [axis],
                        state["project"]["layers"][0]["content"]["camera"]["position"]["axes"]
                            [axis]
                    );
                }
                assert_eq!(host.history(0).unwrap()["project"], state["project"]);
                let next = draw_host(
                    &mut panels,
                    &state,
                    &click_slot(row, 2),
                    panel,
                    floating,
                    &mut atlas,
                );
                assert_eq!(mutations(&next), 0);
                assert_eq!(
                    next.iter()
                        .filter(|action| matches!(action, Action::Seek(frame) if *frame == 42.))
                        .count(),
                    1,
                    "{panel:?} {floating} {locale:?}: {next:?}"
                );
            }
        }
    }
}

#[test]
fn effect_navigation_uses_the_actual_instance_and_preserves_curve_samples_in_both_hosts() {
    use motion_core::{EffectAction, EffectParam};
    let authored = fixture();
    let root = tempfile::tempdir().unwrap();
    motion_core::storage::save(root.path(), authored.project()).unwrap();
    let host = crate::engine::Engine::start(
        root.path().into(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let catalogue = host.plugin("{\"op\":\"catalogue\"}".into()).unwrap();
    for name in ["drop_shadow", "curves"] {
        let package = catalogue["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|package| {
                package["manifest"]["effects"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|effect| effect["id"] == name)
            })
            .unwrap();
        host.plugin(serde_json::json!({"op":"add", "object":7, "plugin":package["manifest"]["id"], "version":package["manifest"]["version"], "hash":package["hash"], "effect":name}).to_string()).unwrap();
    }
    let state = host.state().unwrap();
    let effects = state["project"]["layers"][0]["effects"].as_array().unwrap();
    let shadow = effects
        .iter()
        .find(|effect| effect["effect"] == "drop_shadow")
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    let curves = effects
        .iter()
        .find(|effect| effect["effect"] == "curves")
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    let targets = [(shadow, "offset"), (curves, "p0001")];
    for (effect, param) in targets {
        for frame in [0, 30] {
            let value = if effect == curves {
                let mut value = motion_core::CurveObject::default();
                if frame == 30 {
                    value.channels[0] = vec![[0., 0.], [0.5, 0.85], [1., 1.]];
                }
                EffectAction::SetCurveObject {
                    effect,
                    param: param.into(),
                    frame,
                    value,
                }
            } else {
                EffectAction::Set {
                    effect,
                    param: param.into(),
                    frame,
                    value: [frame as f32, 10., 0., 0.],
                }
            };
            host.edit(
                serde_json::to_string(&vec![
                    Command::Effect {
                        object: 7,
                        action: EffectAction::Animate {
                            effect,
                            param: param.into(),
                            frame,
                            enabled: true,
                        },
                    },
                    Command::Effect {
                        object: 7,
                        action: value,
                    },
                ])
                .unwrap(),
            )
            .unwrap();
        }
    }
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::Zh, Locale::En] {
        for panel in [Panel::Properties, Panel::EffectControls] {
            for floating in [false, true] {
                for (effect, param) in targets {
                    host.seek(12.).unwrap();
                    let before = host.state().unwrap();
                    let mut panels = Panels {
                        locale,
                        focused: Some(panel),
                        ..Default::default()
                    };
                    panels.select_only(Some(7));
                    panels.effect_controls.catalogue = catalogue.clone().into();
                    panels.properties.effects.catalogue = catalogue.clone().into();
                    panels.properties.effects_collapsed = false;
                    panels.properties.transform_collapsed = true;
                    draw_host(
                        &mut panels,
                        &before,
                        &Input::default(),
                        panel,
                        floating,
                        &mut atlas,
                    );
                    let controls = if panel == Panel::Properties {
                        &panels.properties.effects
                    } else {
                        &panels.effect_controls
                    };
                    let value = controls.test_rects[&(7, effect, param.into(), -4)];
                    let origin = if panel == Panel::Properties {
                        PaintList::default().styles().metric("tree.indent", 14.)
                    } else {
                        0.
                    };
                    let row = Rect::new(origin, value.min[1], 1000. - origin, value.height());
                    assert_row_cancellation_and_readonly(
                        &mut panels,
                        &before,
                        row,
                        panel,
                        floating,
                        &mut atlas,
                    );
                    let actions = draw_host(
                        &mut panels,
                        &before,
                        &click_slot(row, 1),
                        panel,
                        floating,
                        &mut atlas,
                    );
                    assert_eq!(
                        mutations(&actions),
                        1,
                        "{panel:?}/{effect}/{param}: {actions:?}"
                    );
                    let commands = actions
                        .into_iter()
                        .find_map(|action| {
                            if let Action::EditBatch(commands) = action {
                                Some(commands)
                            } else {
                                None
                            }
                        })
                        .unwrap();
                    assert!(commands
                        .iter()
                        .all(|command| matches!(command, Command::Effect { object: 7, .. })));
                    let added = host
                        .edit(serde_json::to_string(&commands).unwrap())
                        .unwrap();
                    let find = |state: &Value| {
                        state["project"]["layers"][0]["effects"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|entry| entry["id"] == effect)
                            .unwrap()["params"][param]
                            .clone()
                    };
                    let old: EffectParam = serde_json::from_value(find(&before)).unwrap();
                    let new: EffectParam = serde_json::from_value(find(&added)).unwrap();
                    if effect == curves {
                        assert_eq!(
                            new.curve.as_ref().unwrap().sample(0.),
                            old.curve.as_ref().unwrap().sample(0.)
                        );
                        assert_eq!(
                            new.curve
                                .as_ref()
                                .unwrap()
                                .keys
                                .iter()
                                .map(|key| key.frame)
                                .collect::<Vec<_>>(),
                            vec![-12, 0, 18]
                        );
                    } else {
                        assert_eq!(new.sample(0.), old.sample(0.));
                        assert_eq!(
                            new.track
                                .keys
                                .iter()
                                .map(|key| key.frame)
                                .collect::<Vec<_>>(),
                            vec![-12, 0, 18]
                        );
                    }
                    for other in before["project"]["layers"][0]["effects"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|other| other["id"] != effect)
                    {
                        assert!(added["project"]["layers"][0]["effects"]
                            .as_array()
                            .unwrap()
                            .contains(other));
                    }
                    assert_eq!(host.history(0).unwrap()["project"], before["project"]);
                    assert_eq!(host.history(1).unwrap()["project"], added["project"]);
                    host.save().unwrap();
                    assert_eq!(
                        serde_json::to_value(motion_core::storage::load(root.path()).unwrap())
                            .unwrap(),
                        added["project"]
                    );
                    for (slot, expected, key) in [(0, 0., Key::J), (2, 30., Key::K)] {
                        let actions = draw_host(
                            &mut panels,
                            &added,
                            &click_slot(row, slot),
                            panel,
                            floating,
                            &mut atlas,
                        );
                        assert_eq!(mutations(&actions), 0);
                        assert_eq!(actions.iter().filter(|action| matches!(action,Action::Seek(frame) if *frame == expected)).count(),1,"{actions:?}");
                        let actions = draw_host(
                            &mut panels,
                            &added,
                            &Input {
                                keys: vec![(key, Default::default())],
                                ..Default::default()
                            },
                            panel,
                            floating,
                            &mut atlas,
                        );
                        assert_eq!(mutations(&actions), 0);
                        assert_eq!(actions.iter().filter(|action| matches!(action,Action::Seek(frame) if *frame == expected)).count(),1,"{actions:?}");
                    }
                    let actions = draw_host(
                        &mut panels,
                        &added,
                        &click_slot(row, 1),
                        panel,
                        floating,
                        &mut atlas,
                    );
                    let command = actions
                        .into_iter()
                        .find_map(|action| {
                            if let Action::Edit(command) = action {
                                Some(command)
                            } else {
                                None
                            }
                        })
                        .unwrap();
                    host.edit(serde_json::to_string(&command).unwrap()).unwrap();
                    assert_eq!(host.state().unwrap()["project"], before["project"]);
                }
            }
        }
    }
}
