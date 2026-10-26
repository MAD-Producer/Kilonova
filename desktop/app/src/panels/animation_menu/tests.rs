use super::*;
use motion_core::{Engine, Keyframe, Layer, LayerTimeline, Project};
use motion_ui::input::Modifiers;

fn fixture() -> (Engine, Value) {
    let mut p = Project::new(640, 360, 30, 120).unwrap();
    for (id, start) in [(7, 12), (8, 5), (9, 20)] {
        let mut layer = Layer::solid(id, "Animated", [64.; 2], [0.; 3], [1.; 4]);
        layer.timeline = Some(LayerTimeline {
            in_frame: start,
            out_frame: start + 20,
            offset_frame: start,
        });
        layer.transform.position.keys = [0, 10, 19]
            .into_iter()
            .map(|frame| Keyframe {
                frame,
                value: [frame as f32, 0., 0.],
                ease: Ease::Linear,
                curve: None,
                spatial: None,
            })
            .collect();
        p.layers.push(layer);
    }
    let engine = Engine::new(p).unwrap();
    let state = snapshot(&engine);
    (engine, state)
}
fn snapshot(engine: &Engine) -> Value {
    serde_json::json!({"root":"project-a","composition":engine.project().composition_id,
        "document_generation":4,"revision":engine.revision(),"frame":22.,"project":engine.project(),
        "timeline_layers":engine.project().timeline_layers(22.).unwrap()})
}
fn context(state: &Value, locale: Locale) -> Panels {
    let mut panels = Panels {
        selected: Some(7),
        focused: Some(Panel::Timeline),
        locale,
        ..Default::default()
    };
    panels
        .animation_controls
        .select_key_in(state, 7, Property::Position, None, 22);
    panels
}
fn commands(actions: Vec<Action>) -> Vec<Command> {
    actions
        .into_iter()
        .flat_map(|action| match action {
            Action::Edit(command) => vec![command],
            Action::EditBatch(commands) => commands,
            Action::EditorAction { commands, .. } => commands,
            other => panic!("unexpected action {other:?}"),
        })
        .collect()
}

#[test]
fn timing_menu_and_shortcut_freeze_the_same_selection_without_editing_or_changing_view_mode() {
    let (engine,state)=fixture();
    for locale in [Locale::Zh,Locale::En] {for owner in [None,Some(Panel::Timeline)] {
        let mut panels=context(&state,locale);let mut menu=Vec::new();
        apply(&mut panels,&state,Operation::TimingSettings,&mut menu);
        let mut keys=Vec::new();
        super::super::keyboard::route(&Input{keys:vec![(Key::K,Modifiers{control:true,alt:true,shift:false})],..Default::default()},&mut panels,&state,false,owner,&mut keys);
        assert_eq!(menu.len(),1);assert_eq!(keys.len(),1);
        let frozen=|action:&Action|match action {Action::KeyframeTimingSettings{source,selections}=>(source.clone(),selections.clone()),other=>panic!("unexpected {other:?}")};
        assert_eq!(frozen(&menu[0]),frozen(&keys[0]));
        assert!(!panels.timeline.graph_mode);assert!(!engine.can_undo());
        let mut blocked=state.clone();blocked["observing"]=true.into();
        assert!(!enabled(&panels,&blocked,Operation::TimingSettings));
    }}
}

#[test]
fn menu_and_keyboard_share_current_property_commands_and_single_undo() {
    let (engine, state) = fixture();
    for locale in [Locale::En, Locale::Zh] {
        for owner in [None, Some(Panel::Timeline)] {
            for (command, key, modifiers) in [
                (
                    Shortcut::ToggleKey(Property::Position),
                    Key::P,
                    Modifiers {
                        alt: true,
                        shift: true,
                        ..Default::default()
                    },
                ),
                (Shortcut::Delete, Key::Delete, Modifiers::default()),
                (Shortcut::EaseBoth, Key::F9, Modifiers::default()),
                (
                    Shortcut::EaseIncoming,
                    Key::F9,
                    Modifiers {
                        shift: true,
                        ..Default::default()
                    },
                ),
                (
                    Shortcut::EaseOutgoing,
                    Key::F9,
                    Modifiers {
                        control: true,
                        shift: true,
                        ..Default::default()
                    },
                ),
            ] {
                let mut menu_panels = context(&state, locale);
                let mut key_panels = context(&state, locale);
                assert!(enabled(&menu_panels, &state, Operation::Shortcut(command)));
                let mut menu = vec![];
                apply(
                    &mut menu_panels,
                    &state,
                    Operation::Shortcut(command),
                    &mut menu,
                );
                let mut keys = vec![];
                super::super::keyboard::route(
                    &Input {
                        keys: vec![(key, modifiers)],
                        ..Default::default()
                    },
                    &mut key_panels,
                    &state,
                    false,
                    owner,
                    &mut keys,
                );
                let menu = commands(menu);
                let keys = commands(keys);
                assert!(!menu.is_empty());
                assert_eq!(
                    serde_json::to_value(&menu).unwrap(),
                    serde_json::to_value(keys).unwrap(),
                    "locale={locale:?} owner={owner:?} command={command:?}"
                );
                let before = engine.snapshot();
                let mut actual = Engine::new(before.clone()).unwrap();
                actual.apply_batch(menu).unwrap();
                assert_ne!(actual.project(), &before);
                actual.undo().unwrap();
                assert_eq!(actual.project(), &before);
            }
        }
    }
}

#[test]
fn interpolation_targets_only_the_explicit_property_without_moving_key_times() {
    let (mut engine, state) = fixture();
    let mut panels = context(&state, Locale::Zh);
    panels
        .animation_controls
        .choose_track_in(&state, 7, Property::Position, None);
    panels.animation_controls.clear_key();
    let before = engine.snapshot();
    let mut actions = vec![];
    apply(
        &mut panels,
        &state,
        Operation::Interpolation(Ease::Hold),
        &mut actions,
    );
    let edits = commands(actions);
    assert_eq!(edits.len(), 1);
    engine.apply_batch(edits).unwrap();
    let position = &engine.project().layers[0].transform.position;
    assert_eq!(
        position.keys.iter().map(|k| k.frame).collect::<Vec<_>>(),
        vec![0, 10, 19]
    );
    assert!(position
        .keys
        .iter()
        .all(|k| k.ease == Ease::Hold && k.curve.is_none()));
    assert_eq!(position.sample(5.).unwrap(), [0., 0., 0.]);
    assert_eq!(engine.project().layers[1..], before.layers[1..]);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
}

#[test]
fn sequence_follows_visible_stack_and_preserves_sources_and_one_step_history() {
    let (mut engine, state) = fixture();
    let mut panels = context(&state, Locale::En);
    panels.selected_layers = vec![7, 8];
    let before = engine.snapshot();
    let mut actions = vec![];
    assert!(enabled(&panels, &state, Operation::Sequence));
    apply(&mut panels, &state, Operation::Sequence, &mut actions);
    assert_eq!(actions.len(), 1);
    engine.apply_batch(commands(actions)).unwrap();
    assert_eq!(engine.project().layers[1], before.layers[1]);
    assert_eq!(engine.project().layers[2], before.layers[2]);
    assert_eq!(
        engine.project().layers[0].timeline.unwrap(),
        LayerTimeline {
            in_frame: 25,
            out_frame: 45,
            offset_frame: 25
        }
    );
    assert_eq!(
        engine.project().layers[0].transform,
        before.layers[0].transform
    );
    let after = engine.snapshot();
    let root = tempfile::tempdir().unwrap();
    motion_core::storage::save(root.path(), &after).unwrap();
    assert_eq!(motion_core::storage::load(root.path()).unwrap(), after);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    engine.redo().unwrap();
    assert_eq!(engine.project(), &after);
}

#[test]
fn invalid_readonly_and_stale_contexts_disable_edits_and_reverse_time_is_explicitly_unavailable() {
    let (_, state) = fixture();
    for locale in [Locale::En, Locale::Zh] {
        for fault in ["locked", "readonly", "foreign", "stale"] {
            let mut state = state.clone();
            let mut panels = context(&state, locale);
            match fault {
                "locked" => state["project"]["layers"][0]["locked"] = true.into(),
                "readonly" => state["observing"] = true.into(),
                "foreign" => panels.focused = Some(Panel::Project),
                _ => state["composition"] = "other".into(),
            }
            for op in [
                Operation::Interpolation(Ease::Hold),
                Operation::Shortcut(Shortcut::EaseBoth),
                Operation::Shortcut(Shortcut::Delete),
            ] {
                assert!(!enabled(&panels, &state, op), "fault={fault}");
                let mut actions = vec![];
                apply(&mut panels, &state, op, &mut actions);
                assert!(actions.is_empty());
            }
        }
        let panels = context(&state, locale);
        assert!(!enabled(&panels, &state, Operation::ReverseTime));
        assert!(entries(&panels, &state)
            .iter()
            .any(|(text, op, enabled)| matches!(op, Operation::ReverseTime)
                && !enabled
                && !text.is_empty()));
    }
}

#[test]
fn sequence_rejects_the_whole_selection_for_a_locked_missing_or_fractionally_shifted_layer() {
    let (_, state) = fixture();
    for fault in ["locked", "missing", "fractional"] {
        let mut state = state.clone();
        let mut panels = context(&state, Locale::En);
        panels.selected_layers = vec![7, 8];
        match fault {
            "locked" => state["project"]["layers"][1]["locked"] = true.into(),
            "missing" => panels.selected_layers.push(999),
            _ => {
                state["timeline_layers"][0]["subframe_range"] =
                    serde_json::json!({"in_frame":12.5,"out_frame":32.5});
            }
        }
        assert!(
            !enabled(&panels, &state, Operation::Sequence),
            "fault={fault}"
        );
        let mut actions = vec![];
        apply(&mut panels, &state, Operation::Sequence, &mut actions);
        assert!(actions.is_empty());
    }
}

#[test]
fn open_menu_rejects_changed_document_revision_selection_property_and_focus_before_dispatch() {
    let (_, original) = fixture();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        let origin = super::super::menus::regions(&mut atlas, locale)
            .into_iter()
            .find(|(label, _)| matches!(label, T::Animation))
            .unwrap()
            .1
            .min[0];
        let point = [
            origin + 12.,
            metrics::TITLE_BAR + metrics::MENU_BAR + 4. + 32. + 16.,
        ];
        let click = Input {
            mouse: point,
            pressed: Some((motion_ui::input::MouseButton::Left, point)),
            ..Default::default()
        };
        for change in [
            "unchanged",
            "root",
            "generation",
            "revision",
            "composition",
            "selection",
            "property",
            "key",
            "focus",
        ] {
            let mut state = original.clone();
            let mut panels = context(&state, locale);
            panels.menu = Some(5);
            let mut actions = vec![];
            super::super::menus::render(
                &mut PaintList::default(),
                &mut atlas,
                [1280., 850.],
                &mut panels,
                &Input::default(),
                &state,
                &mut actions,
            );
            assert!(actions.is_empty());
            assert!(panels.animation_menu_scope.is_some());
            match change {
                "root" => state["root"] = "other-project".into(),
                "generation" => state["document_generation"] = 5.into(),
                "revision" => state["revision"] = 1.into(),
                "composition" => state["composition"] = "other-composition".into(),
                "selection" => panels.select_only(Some(8)),
                "property" => {
                    panels
                        .animation_controls
                        .choose_track_in(&state, 7, Property::Opacity, None);
                    panels.animation_controls.clear_key();
                }
                "key" => {
                    panels
                        .animation_controls
                        .select_key_in(&state, 7, Property::Position, None, 12)
                }
                "focus" => panels.focused = Some(Panel::Properties),
                _ => {}
            }
            super::super::menus::render(
                &mut PaintList::default(),
                &mut atlas,
                [1280., 850.],
                &mut panels,
                &click,
                &state,
                &mut actions,
            );
            assert_eq!(panels.menu, None);
            if change == "unchanged" {
                assert!(!commands(actions).is_empty(), "positive menu hit control");
            } else {
                assert!(actions.is_empty(), "locale={locale:?} change={change}");
                assert!(panels.animation_menu_scope.is_none());
            }
        }
    }
}

#[test]
fn disabled_menu_entry_focus_loss_and_outside_click_never_edit() {
    let (_, state) = fixture();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for cancel in ["focus", "outside"] {
            let mut panels = context(&state, locale);
            panels.menu = Some(5);
            let origin = super::super::menus::regions(&mut atlas, locale)
                .into_iter()
                .find(|(label, _)| matches!(label, T::Animation))
                .unwrap()
                .1
                .min[0];
            let disabled = entries(&panels, &state)
                .iter()
                .position(|(_, operation, _)| matches!(operation, Operation::ReverseTime))
                .unwrap();
            let point = [
                origin + 12.,
                metrics::TITLE_BAR + metrics::MENU_BAR + 4. + disabled as f32 * 32. + 16.,
            ];
            let mut actions = vec![];
            super::super::menus::render(
                &mut PaintList::default(),
                &mut atlas,
                [1280., 850.],
                &mut panels,
                &Input {
                    mouse: point,
                    pressed: Some((motion_ui::input::MouseButton::Left, point)),
                    ..Default::default()
                },
                &state,
                &mut actions,
            );
            assert_eq!(panels.menu, Some(5), "disabled entry must retain the popup");
            assert!(actions.is_empty());
            let point = [1200., 800.];
            let input = if cancel == "focus" {
                Input {
                    focus_lost: true,
                    ..Default::default()
                }
            } else {
                Input {
                    mouse: point,
                    pressed: Some((motion_ui::input::MouseButton::Left, point)),
                    ..Default::default()
                }
            };
            super::super::menus::render(
                &mut PaintList::default(),
                &mut atlas,
                [1280., 850.],
                &mut panels,
                &input,
                &state,
                &mut actions,
            );
            assert_eq!(panels.menu, None);
            assert!(actions.is_empty());
        }
    }
}
