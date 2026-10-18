use super::*;

fn render_window(
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    floating: bool,
    atlas: &mut TextAtlas,
) -> Vec<Action> {
    let mut list = PaintList::default();
    if floating {
        floating_chrome(
            &mut list,
            atlas,
            Panel::Timeline,
            [900., 600.],
            panels,
            state,
            input,
        )
    } else {
        chrome(
            &mut list,
            atlas,
            [900., 600.],
            &dock::Node::dock(Panel::Timeline.dock()),
            panels,
            state,
            input,
        )
    }
}
fn edit(actions: Vec<Action>) -> Command {
    let mut edits = actions.into_iter().filter_map(|a| {
        if let Action::Edit(c) = a {
            Some(c)
        } else {
            None
        }
    });
    let command = edits.next().expect("one expression edit");
    assert!(edits.next().is_none());
    command
}
#[test]
fn expression_toggle_preserves_source_seed_target_history_and_save_in_both_windows() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        for locale in [Locale::Zh, Locale::En] {
            let root = tempfile::tempdir().unwrap();
            let mut project = motion_core::Project::new(640, 480, 30, 120).unwrap();
            project.layers.push(motion_core::Layer::solid(
                7,
                "Expression",
                [100.; 2],
                [0.; 3],
                [1.; 4],
            ));
            let original = motion_core::PropertyExpression {
                target: motion_core::ExpressionTarget::Property {
                    object: 7,
                    property: Property::Position,
                    axis: None,
                },
                source: "[10,20,30]".into(),
                enabled: true,
                seed: 987654,
                profile: motion_core::EXPRESSION_PROFILE.into(),
            };
            project.expressions.push(original.clone());
            motion_core::storage::save(root.path(), &project).unwrap();
            let host = crate::engine::Engine::start(
                root.path().into(),
                motion_desktop_media::platform(),
                8 << 30,
            )
            .unwrap();
            let initial = host.state().unwrap();
            let mut panels = Panels {
                selected: Some(7),
                focused: Some(Panel::Timeline),
                locale,
                ..Default::default()
            };
            panels.timeline.expanded.insert(7);
            render_window(
                &mut panels,
                &initial,
                &Input::default(),
                floating,
                &mut atlas,
            );
            assert!(
                panels.expressions.rects.is_empty(),
                "saved expressions start collapsed"
            );
            let property_rect = panels.expressions.triggers[0].1;
            let fields = super::super::timeline::property_layout(
                &PaintList::default().styles(),
                property_rect,
                property_rect.min[1],
                property_rect.height(),
                0,
                2,
            );
            let point = [
                fields.watch.min[0] + fields.watch.width() * 0.5,
                fields.watch.min[1] + fields.watch.height() * 0.5,
            ];
            // Alt+stopwatch opens the editor without creating an animation/history edit.
            assert!(render_window(
                &mut panels,
                &initial,
                &Input {
                    mouse: point,
                    pressed: Some((MouseButton::Left, point)),
                    modifiers: motion_ui::input::Modifiers {
                        alt: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                floating,
                &mut atlas
            )
            .is_empty());
            render_window(
                &mut panels,
                &initial,
                &Input::default(),
                floating,
                &mut atlas,
            );
            let rect = panels.expressions.rects[0].1;
            let point = [
                rect.min[0] + rect.width() * 0.5,
                rect.min[1] + rect.height() * 0.5,
            ];
            let click = Input {
                mouse: point,
                pressed: Some((MouseButton::Left, point)),
                ..Default::default()
            };
            let command = edit(render_window(
                &mut panels,
                &initial,
                &click,
                floating,
                &mut atlas,
            ));
            let disabled = host.edit(serde_json::to_string(&command).unwrap()).unwrap();
            let mut expected = original.clone();
            expected.enabled = false;
            assert_eq!(
                disabled["project"]["expressions"][0],
                serde_json::to_value(&expected).unwrap()
            );
            assert_eq!(
                disabled["sampledLayers"][0]["position"],
                serde_json::json!([0., 0., 0.])
            );
            host.save().unwrap();
            assert_eq!(
                motion_core::storage::load(root.path()).unwrap().expressions,
                vec![expected]
            );
            assert_eq!(host.history(0).unwrap()["project"], initial["project"]);
            assert_eq!(host.history(1).unwrap()["project"], disabled["project"]);
            let command = edit(render_window(
                &mut panels,
                &disabled,
                &click,
                floating,
                &mut atlas,
            ));
            let enabled = host.edit(serde_json::to_string(&command).unwrap()).unwrap();
            assert_eq!(
                enabled["project"]["expressions"][0],
                serde_json::to_value(&original).unwrap()
            );
            assert_eq!(
                enabled["sampledLayers"][0]["position"],
                serde_json::json!([10., 20., 30.])
            );
            let locked = host
                .edit(
                    serde_json::to_string(&Command::Flags {
                        object: 7,
                        visible: true,
                        locked: true,
                    })
                    .unwrap(),
                )
                .unwrap();
            assert!(
                render_window(&mut panels, &locked, &click, floating, &mut atlas)
                    .iter()
                    .all(|a| !matches!(a, Action::Edit(_)))
            );
        }
    }
}
#[test]
fn invalid_disabled_expression_stays_editable_and_apply_preserves_disabled_metadata() {
    let mut project = motion_core::Project::new(640, 480, 30, 120).unwrap();
    project.layers.push(motion_core::Layer::solid(
        7,
        "Broken expression",
        [100.; 2],
        [0.; 3],
        [1.; 4],
    ));
    let original = motion_core::PropertyExpression {
        target: motion_core::ExpressionTarget::Property {
            object: 7,
            property: Property::Position,
            axis: None,
        },
        source: "value +".into(),
        enabled: false,
        seed: 42,
        profile: motion_core::EXPRESSION_PROFILE.into(),
    };
    project.expressions.push(original.clone());
    let mut engine = motion_core::Engine::new(project).unwrap();
    let state = serde_json::json!({"frame":0,"project":engine.project()});
    let mut panels = Panels {
        selected: Some(7),
        focused: Some(Panel::Timeline),
        ..Default::default()
    };
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let caption = Rect::new(0., 0., 100., 22.);
    let source = Rect::new(100., 0., 300., 22.);
    panels
        .expressions
        .open(&state, original.target.clone(), Panel::Timeline);
    panels.expressions.row(
        &mut PaintList::default(),
        &mut atlas,
        caption,
        source,
        &state,
        original.target.clone(),
        &Input::default(),
        Locale::En,
        true,
        &mut vec![],
    );
    let rect = panels.expressions.rects[0].1;
    let point = [
        rect.min[0] + rect.width() * 0.5,
        rect.min[1] + rect.height() * 0.5,
    ];
    let mut toggle_actions = vec![];
    panels.expressions.row(
        &mut PaintList::default(),
        &mut atlas,
        caption,
        source,
        &state,
        original.target.clone(),
        &Input {
            mouse: point,
            pressed: Some((MouseButton::Left, point)),
            ..Default::default()
        },
        Locale::En,
        true,
        &mut toggle_actions,
    );
    assert!(engine.apply(edit(toggle_actions)).is_err());
    assert_eq!(engine.project().expressions, vec![original.clone()]);
    panels.expressions.editor.begin(&original.source);
    panels.expressions.toggle_focus = false;
    let mut actions = vec![];
    panels.expressions.row(
        &mut PaintList::default(),
        &mut atlas,
        caption,
        source,
        &state,
        original.target.clone(),
        &Input {
            text: "[1,2,3]".into(),
            ..Default::default()
        },
        Locale::En,
        true,
        &mut actions,
    );
    panels.expressions.row(
        &mut PaintList::default(),
        &mut atlas,
        caption,
        source,
        &state,
        original.target.clone(),
        &Input {
            keys: vec![(Key::Enter, Default::default())],
            ..Default::default()
        },
        Locale::En,
        true,
        &mut actions,
    );
    engine.apply(edit(actions)).unwrap();
    let expression = &engine.project().expressions[0];
    assert_eq!(expression.source, "[1,2,3]");
    assert!(!expression.enabled);
    assert_eq!(expression.seed, 42);
    assert_eq!(expression.target, original.target);
    engine.undo().unwrap();
    assert_eq!(engine.project().expressions, vec![original]);
}

#[test]
fn right_click_opens_inline_draft_and_escape_does_not_edit_in_either_window() {
    let root = tempfile::tempdir().unwrap();
    let mut project = motion_core::Project::new(640, 480, 30, 120).unwrap();
    project.layers.push(motion_core::Layer::solid(
        7, "Layer", [100.; 2], [0.; 3], [1.; 4],
    ));
    motion_core::storage::save(root.path(), &project).unwrap();
    let host = crate::engine::Engine::start(
        root.path().into(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let initial = host.state().unwrap();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        for locale in [Locale::En, Locale::Zh] {
            let mut panels = Panels {
                focused: Some(Panel::Timeline),
                selected: Some(7),
                locale,
                ..Default::default()
            };
            panels.timeline.expanded.insert(7);
            render_window(
                &mut panels,
                &initial,
                &Input::default(),
                floating,
                &mut atlas,
            );
            assert!(panels.expressions.rects.is_empty());
            let (target, rect) = panels.expressions.triggers[0].clone();
            let point = [rect.max[0] - 10., rect.min[1] + rect.height() * 0.5];
            assert!(render_window(
                &mut panels,
                &initial,
                &Input {
                    mouse: point,
                    pressed: Some((MouseButton::Right, point)),
                    ..Default::default()
                },
                floating,
                &mut atlas
            )
            .is_empty());
            assert!(panels.expressions.menu_open());
            assert!(panels.expressions.draft.is_none());
            assert!(
                !panels.layer_actions.editing(),
                "property menu owns this right click"
            );
            assert!(render_window(
                &mut panels,
                &initial,
                &Input {
                    keys: vec![(Key::Enter, Default::default())],
                    ..Default::default()
                },
                floating,
                &mut atlas
            )
            .is_empty());
            assert!(!panels.expressions.menu_open());
            assert!(
                !panels.layer_actions.editing(),
                "menu Enter must not start layer rename"
            );
            assert_eq!(panels.expressions.draft, Some(target));
            render_window(
                &mut panels,
                &initial,
                &Input::default(),
                floating,
                &mut atlas,
            );
            assert_eq!(panels.expressions.rects.len(), 1);
            assert!(render_window(
                &mut panels,
                &initial,
                &Input {
                    text: "[1,2,3]".into(),
                    ..Default::default()
                },
                floating,
                &mut atlas
            )
            .is_empty());
            assert!(render_window(
                &mut panels,
                &initial,
                &Input {
                    keys: vec![(Key::Escape, Default::default())],
                    ..Default::default()
                },
                floating,
                &mut atlas
            )
            .is_empty());
            assert!(panels.expressions.draft.is_none());
            assert_eq!(host.state().unwrap()["project"], initial["project"]);
            assert_eq!(host.state().unwrap()["revision"], initial["revision"]);
        }
    }
}

#[test]
fn effect_parameter_context_menu_and_checkbox_preserve_the_exact_target() {
    let root = tempfile::tempdir().unwrap();
    let mut project = motion_core::Project::new(640, 480, 30, 120).unwrap();
    project.layers.push(motion_core::Layer::solid(
        7, "Effects", [100.; 2], [0.; 3], [1.; 4],
    ));
    motion_core::storage::save(root.path(), &project).unwrap();
    let host = crate::engine::Engine::start(
        root.path().into(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let catalogue = host
        .plugin(serde_json::json!({"op":"catalogue"}).to_string())
        .unwrap();
    let package = catalogue["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| {
            p["manifest"]["effects"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["id"] == "brightness_contrast")
        })
        .unwrap();
    host.plugin(serde_json::json!({"op":"add","object":7,"plugin":package["manifest"]["id"],"version":package["manifest"]["version"],"hash":package["hash"],"effect":"brightness_contrast"}).to_string()).unwrap();
    let state = host.state().unwrap();
    let effect = state["project"]["layers"][0]["effects"][0]["id"]
        .as_u64()
        .unwrap();
    let original = motion_core::PropertyExpression {
        target: motion_core::ExpressionTarget::Effect {
            object: 7,
            effect,
            param: "p0001".into(),
        },
        source: "value".into(),
        enabled: true,
        seed: 9182,
        profile: motion_core::EXPRESSION_PROFILE.into(),
    };
    let state = host
        .edit(
            serde_json::to_string(&Command::SetExpression {
                expression: original.clone(),
                frame: 0,
            })
            .unwrap(),
        )
        .unwrap();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        let mut panels = Panels {
            selected: Some(7),
            focused: Some(Panel::EffectControls),
            ..Default::default()
        };
        panels.effect_controls.catalogue = catalogue.clone().into();
        let mut draw = |panels: &mut Panels, input: &Input| {
            if floating {
                floating_chrome(
                    &mut PaintList::default(),
                    &mut atlas,
                    Panel::EffectControls,
                    [900., 800.],
                    panels,
                    &state,
                    input,
                )
            } else {
                chrome(
                    &mut PaintList::default(),
                    &mut atlas,
                    [900., 800.],
                    &dock::Node::dock(Panel::EffectControls.dock()),
                    panels,
                    &state,
                    input,
                )
            }
        };
        draw(&mut panels, &Input::default());
        assert!(panels.effect_controls.expressions.rects.is_empty());
        let rect = panels.effect_controls.test_rects[&(7, effect, "p0001".into(), -7)];
        let point = [
            rect.min[0] + rect.width() * 0.5,
            rect.min[1] + rect.height() * 0.5,
        ];
        assert!(draw(
            &mut panels,
            &Input {
                mouse: point,
                pressed: Some((MouseButton::Right, point)),
                ..Default::default()
            }
        )
        .is_empty());
        let menu = panels.effect_controls.test_rects[&(7, effect, "__menu".into(), 6)];
        let point = [
            menu.min[0] + menu.width() * 0.5,
            menu.min[1] + menu.height() * 0.5,
        ];
        assert!(draw(
            &mut panels,
            &Input {
                mouse: point,
                pressed: Some((MouseButton::Left, point)),
                ..Default::default()
            }
        )
        .is_empty());
        draw(&mut panels, &Input::default());
        let (target, rect, _) = panels.effect_controls.expressions.rects[0].clone();
        assert_eq!(target, original.target);
        let point = [
            rect.min[0] + rect.width() * 0.5,
            rect.min[1] + rect.height() * 0.5,
        ];
        let command = edit(draw(
            &mut panels,
            &Input {
                mouse: point,
                pressed: Some((MouseButton::Left, point)),
                ..Default::default()
            },
        ));
        let disabled = host.edit(serde_json::to_string(&command).unwrap()).unwrap();
        let mut expected = original.clone();
        expected.enabled = false;
        assert_eq!(
            disabled["project"]["expressions"][0],
            serde_json::to_value(&expected).unwrap()
        );
        host.save().unwrap();
        assert_eq!(
            motion_core::storage::load(root.path()).unwrap().expressions,
            vec![expected]
        );
        assert_eq!(host.history(0).unwrap()["project"], state["project"]);
    }
}

#[test]
fn inline_enter_commits_one_expression_and_never_renames_the_layer() {
    for floating in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut project = motion_core::Project::new(640, 480, 30, 120).unwrap();
        project.layers.push(motion_core::Layer::solid(
            7,
            "Original name",
            [100.; 2],
            [0.; 3],
            [1.; 4],
        ));
        motion_core::storage::save(root.path(), &project).unwrap();
        let host = crate::engine::Engine::start(
            root.path().into(),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let initial = host.state().unwrap();
        let target = motion_core::ExpressionTarget::Property {
            object: 7,
            property: Property::Position,
            axis: None,
        };
        let mut panels = Panels {
            focused: Some(Panel::Timeline),
            selected: Some(7),
            ..Default::default()
        };
        panels.timeline.expanded.insert(7);
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        panels
            .expressions
            .open(&initial, target.clone(), Panel::Timeline);
        render_window(
            &mut panels,
            &initial,
            &Input::default(),
            floating,
            &mut atlas,
        );
        assert!(render_window(
            &mut panels,
            &initial,
            &Input {
                text: "[30,40,50]".into(),
                ..Default::default()
            },
            floating,
            &mut atlas
        )
        .is_empty());
        assert!(render_window(
            &mut panels,
            &initial,
            &Input {
                keys: vec![(Key::Enter, Default::default())],
                composing: true,
                ..Default::default()
            },
            floating,
            &mut atlas
        )
        .is_empty());
        assert_eq!(host.state().unwrap()["revision"], initial["revision"]);
        let command = edit(render_window(
            &mut panels,
            &initial,
            &Input {
                keys: vec![(Key::Enter, Default::default())],
                ..Default::default()
            },
            floating,
            &mut atlas,
        ));
        assert!(!panels.layer_actions.editing());
        let committed = host.edit(serde_json::to_string(&command).unwrap()).unwrap();
        assert_eq!(committed["project"]["layers"][0]["name"], "Original name");
        assert_eq!(
            committed["project"]["expressions"][0]["target"],
            serde_json::to_value(&target).unwrap()
        );
        assert_eq!(
            committed["sampledLayers"][0]["position"],
            serde_json::json!([30., 40., 50.])
        );
        host.save().unwrap();
        assert_eq!(
            motion_core::storage::load(root.path())
                .unwrap()
                .expressions
                .len(),
            1
        );
        assert_eq!(host.history(0).unwrap()["project"], initial["project"]);
    }
}
