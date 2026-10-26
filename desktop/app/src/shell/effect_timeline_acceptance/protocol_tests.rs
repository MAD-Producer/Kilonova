//! Exercise the native capture's document stamp and action adapter without a GPU.
use super::*;

fn fixture() -> (motion_core::Engine, Value) {
    let graph = motion_graph::builtin_examples()
        .iter()
        .find(|example| example.id == "soft_sphere_motion")
        .unwrap()
        .load()
        .unwrap();
    let compiled = motion_graph::compile(&graph, &Default::default()).unwrap();
    let bytes = motion_graph::compiled_package_bytes(&compiled.manifest, &compiled.source).unwrap();
    let package = motion_effects::EffectPackage::from_bytes(bytes).unwrap();
    let catalogue =
        serde_json::json!({"packages":[{"hash":package.hash,"manifest":package.manifest}]});
    let mut project = motion_core::Project::new(640, 360, 30, 180).unwrap();
    let mut layer = Layer::solid(7, "Capture protocol", [100.; 2], [100., 100., 0.], [1.; 4]);
    let mut effect = motion_core::EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        &package.manifest.effects[0],
        [100.; 2],
    );
    let track = &mut effect.params.get_mut("speed").unwrap().track;
    track
        .upsert(20, [0.25, 0., 0., 0.], motion_core::Ease::Linear)
        .unwrap();
    track
        .upsert(100, [1.5, 0., 0., 0.], motion_core::Ease::Linear)
        .unwrap();
    layer.effects.push(effect);
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    (motion_core::Engine::new(project).unwrap(), catalogue)
}

#[test]
fn native_capture_stamp_and_adapter_apply_real_f9_and_undo_in_both_hosts_and_languages() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [i18n::Locale::En, i18n::Locale::Zh] {
        for floating in [false, true] {
            let (mut engine, catalogue) = fixture();
            let original = engine.project().clone();
            let mut panels = panels::Panels {
                locale,
                focused: Some(Panel::Timeline),
                ..Default::default()
            };
            panels.select_only(Some(7));
            panels.effect_controls.catalogue = catalogue.into();
            panels.timeline.expanded.insert(7);
            panels.timeline.collapsed_transforms.insert(7);
            panels.timeline.expanded_effects.insert(7);
            panels.effect_timeline.begin_probe(1, "speed");
            let size = if floating {
                [900., 760.]
            } else {
                [1200., 760.]
            };
            let mut draw =
                |panels: &mut panels::Panels, engine: &motion_core::Engine, input: &Input| {
                    let mut paint = PaintList::default();
                    paint.stylesheet = panels.styles.clone();
                    let state = fixture_state(engine);
                    if floating {
                        panels::floating_chrome(
                            &mut paint,
                            &mut atlas,
                            Panel::Timeline,
                            size,
                            panels,
                            &state,
                            input,
                        )
                    } else {
                        panels::chrome(
                            &mut paint,
                            &mut atlas,
                            size,
                            &dock::Node::dock(Panel::Timeline.dock()),
                            panels,
                            &state,
                            input,
                        )
                    }
                };
            let actions = draw(&mut panels, &engine, &Input::default());
            apply_fixture_actions(&mut engine, actions).unwrap();
            let caption = panels.effect_timeline.probe.rect().unwrap();
            let track = panels.timeline.zoom_track.unwrap();
            let point = [
                track.min[0] + 20. * track.width() / 180.,
                (caption.min[1] + caption.max[1]) * 0.5,
            ];
            for input in [
                Input {
                    mouse: point,
                    pressed: Some((MouseButton::Left, point)),
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
                Input {
                    mouse: point,
                    released: Some((MouseButton::Left, point)),
                    ..Default::default()
                },
            ] {
                let actions = draw(&mut panels, &engine, &input);
                apply_fixture_actions(&mut engine, actions).unwrap();
            }
            let actions = draw(
                &mut panels,
                &engine,
                &Input {
                    keys: vec![(motion_ui::Key::F9, Default::default())],
                    ..Default::default()
                },
            );
            assert!(
                matches!(actions.as_slice(), [panels::Action::EditorAction { source, commands }]
                if *source == crate::layer_clipboard::KeyframeSource::of(&fixture_state(&engine))
                    && matches!(commands.as_slice(), [motion_core::Command::EaseKeyframes { incoming: true, outgoing: true, .. }]))
            );
            apply_fixture_actions(&mut engine, actions).unwrap();
            assert!(engine.project().layers[0].effects[0].params["speed"]
                .track
                .keys[0]
                .curve
                .is_some());
            assert_eq!(
                engine.project().layers[0].transform,
                original.layers[0].transform
            );
            engine.undo().unwrap();
            assert_eq!(engine.project(), &original);
        }
    }
}

#[test]
fn native_capture_adapter_rejects_incomplete_and_stale_editor_sources_without_editing() {
    let (mut engine, _) = fixture();
    let original = engine.project().clone();
    let revision = engine.revision();
    for (field, stale) in [
        ("root", serde_json::json!("another-document")),
        ("composition", serde_json::json!("another-composition")),
        ("document_generation", serde_json::json!(2)),
        ("revision", serde_json::json!(revision + 1)),
    ] {
        for missing in [false, true] {
            let mut state = fixture_state(&engine);
            if missing {
                state.as_object_mut().unwrap().remove(field);
                if field == "composition" {
                    // KeyframeSource legitimately falls back to the project's ID.
                    state["project"]
                        .as_object_mut()
                        .unwrap()
                        .remove("composition_id");
                }
            } else {
                state[field] = stale.clone();
            }
            let error = apply_fixture_actions(
                &mut engine,
                vec![panels::Action::EditorAction {
                    source: crate::layer_clipboard::KeyframeSource::of(&state),
                    commands: vec![motion_core::Command::Rename {
                        object: 7,
                        name: "Must not apply".into(),
                    }],
                }],
            )
            .unwrap_err();
            assert_eq!(error, "effect capture editor action source is stale");
            assert_eq!(engine.project(), &original);
            assert_eq!(engine.revision(), revision);
        }
    }
}
