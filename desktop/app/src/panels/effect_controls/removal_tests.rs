use super::super::{menus, Panels};
use super::*;
use motion_core::{EffectInstance, Engine, Layer, Project};

fn fixture() -> Engine {
    let graph = motion_graph::builtin_examples()
        .iter()
        .find(|example| example.id == "soft_sphere_motion")
        .unwrap()
        .load()
        .unwrap();
    let compiled = motion_graph::compile(&graph, &Default::default()).unwrap();
    let package = motion_effects::EffectPackage::from_bytes(
        motion_graph::compiled_package_bytes(&compiled.manifest, &compiled.source).unwrap(),
    )
    .unwrap();
    let mut project = Project::new(640, 360, 30, 180).unwrap();
    for (object, effects) in [
        (7, vec![100, 101]),
        (8, vec![102]),
        (9, vec![103]),
        (10, vec![]),
    ] {
        let mut layer = Layer::solid(
            object,
            "Removal fixture",
            [100.; 2],
            [100., 100., 0.],
            [1.; 4],
        );
        for id in effects {
            layer.effects.push(EffectInstance::new(
                id,
                &package.manifest.id,
                &package.manifest.version,
                &package.hash,
                &package.manifest.effects[0],
                [100.; 2],
            ));
        }
        project.layers.push(layer);
    }
    project.rebuild_plugin_dependencies();
    Engine::new(project).unwrap()
}

fn state(engine: &Engine) -> Value {
    serde_json::json!({"root":"remove-effects", "composition":motion_core::composition::MAIN_COMPOSITION,
        "frame":20, "revision":engine.revision(), "project":engine.project()})
}

fn panels(locale: Locale) -> Panels {
    Panels {
        locale,
        selected: Some(7),
        selected_layers: vec![7, 8],
        menu: Some(4),
        ..Default::default()
    }
}

fn click(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        pressed: Some((MouseButton::Left, point)),
        held: Some(MouseButton::Left),
        ..Default::default()
    }
}

fn menu_point(atlas: &mut TextAtlas, locale: Locale) -> [f32; 2] {
    let x = 8.
        + [
            super::super::T::File,
            super::super::T::Edit,
            super::super::T::Composition,
            super::super::T::Layer,
        ]
        .iter()
        .map(|key| atlas.measure(locale.t(*key)) + 22.)
        .sum::<f32>();
    [
        x + 50.,
        motion_ui::theme::metrics::TITLE_BAR + motion_ui::theme::metrics::MENU_BAR + 4. + 32. + 15.,
    ]
}

fn draw_menu(p: &mut Panels, s: &Value, input: &Input, atlas: &mut TextAtlas) -> Vec<Action> {
    let mut out = Vec::new();
    menus::render(
        &mut PaintList::default(),
        atlas,
        [900., 600.],
        p,
        input,
        s,
        &mut out,
    );
    out
}

fn batch(actions: Vec<Action>) -> Vec<Command> {
    assert_eq!(
        actions.len(),
        1,
        "expected one atomic removal action: {actions:?}"
    );
    match actions.into_iter().next().unwrap() {
        Action::EditBatch(commands) => commands,
        action => panic!("unexpected action: {action:?}"),
    }
}

#[test]
fn effect_menu_removes_selected_layers_atomically_and_one_undo_restores_all() {
    for locale in [Locale::En, Locale::Zh] {
        let mut engine = fixture();
        let original = serde_json::to_value(engine.project()).unwrap();
        let mut p = panels(locale);
        let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
        let point = menu_point(&mut atlas, locale);
        let commands = batch(draw_menu(
            &mut p,
            &state(&engine),
            &click(point),
            &mut atlas,
        ));
        assert_eq!(commands.len(), 3);
        assert_eq!(
            commands
                .iter()
                .map(|c| match c {
                    Command::Effect {
                        object,
                        action: EffectAction::Remove { effect },
                    } => (*object, *effect),
                    other => panic!("unexpected command: {other:?}"),
                })
                .collect::<Vec<_>>(),
            vec![(7, 100), (7, 101), (8, 102)]
        );
        engine.apply_batch(commands).unwrap();
        assert!(engine.project().layers[0].effects.is_empty());
        assert!(engine.project().layers[1].effects.is_empty());
        assert_eq!(engine.project().layers[2].effects[0].id, 103);
        let saved = tempfile::tempdir().unwrap();
        motion_core::storage::save(saved.path(), engine.project()).unwrap();
        let reopened = Engine::new(motion_core::storage::load(saved.path()).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(reopened.project()).unwrap(),
            serde_json::to_value(engine.project()).unwrap()
        );
        engine.undo().unwrap();
        assert_eq!(serde_json::to_value(engine.project()).unwrap(), original);
    }
}

fn context(controls: &mut Controls) {
    controls.context_menu = Some(EffectContextMenu {
        owner: controls.context_window,
        anchor: [100., 400.],
        object: 7,
        control: "Effect controls".into(),
        effect: "Selected effect".into(),
        category: None,
        instance: Some((100, 0)),
        catalogue: false,
        expression: None,
        expression_supported: false,
        opened: false,
    });
}

fn draw_context(
    controls: &mut Controls,
    s: &Value,
    input: &Input,
    locale: Locale,
    size: [f32; 2],
    atlas: &mut TextAtlas,
) -> Vec<Action> {
    let mut out = Vec::new();
    controls.finish_context_menu(
        &mut PaintList::default(),
        atlas,
        Rect::new(0., 0., size[0], size[1]),
        s,
        input,
        locale,
        &mut out,
    );
    out
}

fn clear_point(controls: &Controls) -> [f32; 2] {
    let rect = controls.test_rects[&(7, 100, "__menu".into(), 5)];
    [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]
}

#[test]
fn context_removal_keeps_its_single_layer_scope_at_both_host_sizes() {
    for locale in [Locale::En, Locale::Zh] {
        let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
        for size in [[900., 600.], [1600., 1000.]] {
            let mut engine = fixture();
            let original = serde_json::to_value(engine.project()).unwrap();
            let mut controls = Controls::default();
            context(&mut controls);
            let mut s = state(&engine);
            // A locked different layer must not disable this single-layer operation.
            s["project"]["layers"][1]["locked"] = true.into();
            assert!(draw_context(
                &mut controls,
                &s,
                &Input::default(),
                locale,
                size,
                &mut atlas
            )
            .is_empty());
            let point = clear_point(&controls);
            let commands = batch(draw_context(
                &mut controls,
                &s,
                &click(point),
                locale,
                size,
                &mut atlas,
            ));
            assert_eq!(commands.len(), 2);
            engine.apply_batch(commands).unwrap();
            assert!(engine.project().layers[0].effects.is_empty());
            assert_eq!(engine.project().layers[1].effects[0].id, 102);
            assert_eq!(engine.project().layers[2].effects[0].id, 103);
            assert!(controls.context_menu.is_none());
            assert!(engine.undo().unwrap());
            assert_eq!(serde_json::to_value(engine.project()).unwrap(), original);
        }
    }
}

#[test]
fn unavailable_targets_disable_whole_selection_and_empty_contexts() {
    let engine = fixture();
    for locale in [Locale::En, Locale::Zh] {
        let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
        for case in 0..8 {
            let mut s = state(&engine);
            let mut p = panels(locale);
            match case {
                0 => {
                    p.selected = None;
                    p.selected_layers.clear();
                }
                1 => s["observing"] = true.into(),
                2 => s["project"]["layers"][0]["locked"] = true.into(),
                3 => s["project"]["layers"][1]["locked"] = true.into(),
                4 => {
                    s["project"]["layers"].as_array_mut().unwrap().remove(1);
                }
                5 => {
                    p.selected = Some(10);
                    p.selected_layers = vec![10];
                }
                6 => s["project"]["layers"][1]["effects"][0]["id"] = Value::Null,
                7 => s["project"]["layers"][1]["effects"] = false.into(),
                _ => unreachable!(),
            }
            let point = menu_point(&mut atlas, locale);
            assert!(
                draw_menu(&mut p, &s, &click(point), &mut atlas).is_empty(),
                "case {case}"
            );
        }
        for case in 0..4 {
            let mut s = state(&engine);
            match case {
                0 => s["observing"] = true.into(),
                1 => s["project"]["layers"][0]["locked"] = true.into(),
                2 => {
                    s["project"]["layers"].as_array_mut().unwrap().remove(0);
                }
                3 => s["project"]["layers"][0]["effects"] = serde_json::json!([]),
                _ => unreachable!(),
            }
            let mut controls = Controls::default();
            context(&mut controls);
            draw_context(
                &mut controls,
                &s,
                &Input::default(),
                locale,
                [900., 600.],
                &mut atlas,
            );
            let point = clear_point(&controls);
            assert!(draw_context(
                &mut controls,
                &s,
                &click(point),
                locale,
                [900., 600.],
                &mut atlas
            )
            .is_empty());
            assert!(
                controls.context_menu.is_some(),
                "disabled row closed menu: case {case}"
            );
        }
    }
}

#[test]
fn cancellation_closes_both_menus_without_editing() {
    let engine = fixture();
    let original = serde_json::to_value(engine.project()).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
        for input in [
            Input {
                focus_lost: true,
                ..Default::default()
            },
            click([850., 550.]),
        ] {
            let mut p = panels(locale);
            assert!(draw_menu(&mut p, &state(&engine), &input, &mut atlas).is_empty());
            assert!(p.menu.is_none());
            for size in [[900., 600.], [1600., 1000.]] {
                let mut controls = Controls::default();
                context(&mut controls);
                assert!(draw_context(
                    &mut controls,
                    &state(&engine),
                    &input,
                    locale,
                    size,
                    &mut atlas
                )
                .is_empty());
                assert!(controls.context_menu.is_none());
            }
        }
    }
    assert_eq!(serde_json::to_value(engine.project()).unwrap(), original);
    assert!(!engine.can_undo());
}

fn draw_host(
    p: &mut Panels,
    s: &Value,
    input: &Input,
    floating: bool,
    size: [f32; 2],
    atlas: &mut TextAtlas,
) -> Vec<Action> {
    if floating {
        super::super::floating_chrome(
            &mut PaintList::default(),
            atlas,
            motion_ui::dock::Panel::EffectControls,
            size,
            p,
            s,
            input,
        )
    } else {
        super::super::chrome(
            &mut PaintList::default(),
            atlas,
            size,
            &motion_ui::dock::Node::dock(motion_ui::dock::Panel::EffectControls.dock()),
            p,
            s,
            input,
        )
    }
}

#[test]
fn real_main_and_floating_input_owners_preserve_removal_and_cancellation() {
    let engine = fixture();
    let mut s = state(&engine);
    s["timeline_layers"] =
        serde_json::to_value(engine.project().timeline_layers(20.).unwrap()).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
        for floating in [false, true] {
            for size in [[900., 600.], [1600., 1000.]] {
                let mut p = panels(locale);
                p.menu = None;
                p.focused = Some(motion_ui::dock::Panel::EffectControls);
                assert!(
                    draw_host(&mut p, &s, &Input::default(), floating, size, &mut atlas).is_empty()
                );
                context(&mut p.effect_controls);
                assert!(
                    draw_host(&mut p, &s, &Input::default(), floating, size, &mut atlas).is_empty()
                );
                let point = clear_point(&p.effect_controls);
                let commands = batch(draw_host(
                    &mut p,
                    &s,
                    &click(point),
                    floating,
                    size,
                    &mut atlas,
                ));
                assert_eq!(commands.len(), 2);
                assert!(commands.iter().all(|command| matches!(
                    command,
                    Command::Effect {
                        object: 7,
                        action: EffectAction::Remove { .. }
                    }
                )));
                assert!(p.effect_controls.context_menu.is_none());
                for input in [
                    Input {
                        focus_lost: true,
                        ..Default::default()
                    },
                    Input {
                        keys: vec![(Key::Escape, Default::default())],
                        ..Default::default()
                    },
                    click([850., 550.]),
                ] {
                    context(&mut p.effect_controls);
                    assert!(draw_host(&mut p, &s, &input, floating, size, &mut atlas).is_empty());
                    assert!(p.effect_controls.context_menu.is_none());
                }
                if !floating {
                    p.menu = Some(4);
                    assert!(draw_host(
                        &mut p,
                        &s,
                        &Input {
                            keys: vec![(Key::Escape, Default::default())],
                            ..Default::default()
                        },
                        false,
                        size,
                        &mut atlas
                    )
                    .is_empty());
                    assert!(p.menu.is_none());
                }
            }
        }
    }
    assert!(!engine.can_undo());
}

#[test]
fn invalidated_batch_returns_the_engine_error_without_partial_removal_or_history() {
    let mut engine = fixture();
    let original = serde_json::to_value(engine.project()).unwrap();
    let mut p = panels(Locale::En);
    let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
    let point = menu_point(&mut atlas, Locale::En);
    let commands = batch(draw_menu(
        &mut p,
        &state(&engine),
        &click(point),
        &mut atlas,
    ));
    engine
        .apply(Command::Effect {
            object: 7,
            action: EffectAction::Remove { effect: 101 },
        })
        .unwrap();
    let before = serde_json::to_value(engine.project()).unwrap();
    let revision = engine.revision();
    let history = (engine.can_undo(), engine.can_redo());
    let error = engine.apply_batch(commands).unwrap_err();
    assert!(!error.to_string().is_empty());
    assert_eq!(serde_json::to_value(engine.project()).unwrap(), before);
    assert_eq!(engine.revision(), revision);
    assert_eq!((engine.can_undo(), engine.can_redo()), history);
    assert!(engine.undo().unwrap());
    assert_eq!(serde_json::to_value(engine.project()).unwrap(), original);
    assert!(!engine.can_undo());
}

#[test]
fn menu_refreshes_live_targets_and_keeps_primary_and_empty_layer_scope() {
    let engine = fixture();
    let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
    let mut p = panels(Locale::En);
    let mut s = state(&engine);
    assert!(draw_menu(&mut p, &s, &Input::default(), &mut atlas).is_empty());
    s["project"]["layers"][1]["locked"] = true.into();
    let point = menu_point(&mut atlas, Locale::En);
    assert!(draw_menu(&mut p, &s, &click(point), &mut atlas).is_empty());
    assert_eq!(p.menu, Some(4));
    s["project"]["layers"][1]["locked"] = false.into();
    p.selected = Some(7);
    p.selected_layers = vec![10];
    let commands = batch(draw_menu(&mut p, &s, &click(point), &mut atlas));
    assert_eq!(commands.len(), 2);
    assert!(commands
        .iter()
        .all(|command| matches!(command, Command::Effect { object: 7, .. })));
    p.menu = Some(4);
    p.selected_layers.clear();
    assert_eq!(
        batch(draw_menu(&mut p, &s, &click(point), &mut atlas)).len(),
        2
    );
    // Invalid duplicate selections never dispatch a batch with repeated removals.
    p.menu = Some(4);
    p.selected_layers = vec![8, 8];
    assert!(draw_menu(&mut p, &s, &click(point), &mut atlas).is_empty());
}

#[test]
fn localized_scope_and_counts_are_fully_painted_in_the_existing_menu_row() {
    let engine = fixture();
    for locale in [Locale::En, Locale::Zh] {
        let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
        let mut p = panels(locale);
        let caption = super::super::effect_removal::menu_entry(&state(&engine), &p).0;
        assert_eq!(
            caption,
            if locale == Locale::Zh {
                "移除所选2层全部3项效果"
            } else {
                "Remove all: 2 selected layers / 3 effects"
            }
        );
        for size in [[900., 600.], [1600., 1000.]] {
            let mut list = PaintList::default();
            let mut actions = Vec::new();
            menus::render(
                &mut list,
                &mut atlas,
                size,
                &mut p,
                &Input::default(),
                &state(&engine),
                &mut actions,
            );
            let row_y = motion_ui::theme::metrics::TITLE_BAR
                + motion_ui::theme::metrics::MENU_BAR
                + 4.
                + 32.;
            let glyphs = list
                .glyphs
                .iter()
                .filter(|glyph| glyph.position[1] >= row_y && glyph.position[1] < row_y + 30.)
                .collect::<Vec<_>>();
            assert_eq!(
                glyphs.len(),
                caption
                    .chars()
                    .filter(|character| !character.is_whitespace())
                    .count(),
                "clipped scope: {caption}"
            );
            assert!(glyphs.iter().all(|glyph| glyph.size[0] > 0.
                && glyph.size[1] > 0.
                && glyph.uv[2] > glyph.uv[0]
                && glyph.uv[3] > glyph.uv[1]));
            assert!(actions.is_empty());
        }
    }
}
