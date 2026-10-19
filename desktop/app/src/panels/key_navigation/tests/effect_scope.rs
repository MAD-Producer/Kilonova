use super::*;

fn run(host: &crate::engine::Engine, actions: Vec<Action>) {
    for action in actions {
        match action {
            Action::Edit(command) => {
                host.edit(serde_json::to_string(&command).unwrap()).unwrap();
            }
            Action::EditBatch(commands) => {
                host.edit(serde_json::to_string(&commands).unwrap())
                    .unwrap();
            }
            Action::History(index) => {
                host.history(index).unwrap();
            }
            other => panic!("unexpected numeric action: {other:?}"),
        }
    }
}

#[test]
fn active_effect_fields_cancel_on_context_reuse_missing_targets_and_readonly_transitions() {
    let root = tempfile::tempdir().unwrap();
    let authored = fixture();
    motion_core::storage::save(root.path(), authored.project()).unwrap();
    let host = crate::engine::Engine::start(
        root.path().into(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let catalogue = host.plugin("{\"op\":\"catalogue\"}".into()).unwrap();
    let package = catalogue["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|package| {
            package["manifest"]["effects"]
                .as_array()
                .unwrap()
                .iter()
                .any(|effect| effect["id"] == "drop_shadow")
        })
        .unwrap();
    host.plugin(
        serde_json::json!({"op":"add","object":7,"plugin":package["manifest"]["id"],
        "version":package["manifest"]["version"],"hash":package["hash"],"effect":"drop_shadow"})
        .to_string(),
    )
    .unwrap();
    let before = host.seek(12.).unwrap();
    let effect = before["project"]["layers"][0]["effects"][0]["id"]
        .as_u64()
        .unwrap();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for floating in [false, true] {
            for panel in [Panel::EffectControls, Panel::Properties] {
                for case in 0..9 {
                    let mut panels = Panels {
                        locale,
                        focused: Some(panel),
                        ..Default::default()
                    };
                    panels.select_only(Some(7));
                    panels.effect_controls.catalogue = catalogue.clone().into();
                    panels.properties.effects_collapsed = false;
                    panels.properties.transform_collapsed = true;
                    assert!(draw_host(
                        &mut panels,
                        &before,
                        &Input::default(),
                        panel,
                        floating,
                        &mut atlas
                    )
                    .is_empty());
                    let controls = if panel == Panel::Properties {
                        &panels.properties.effects
                    } else {
                        &panels.effect_controls
                    };
                    let rect = controls.test_rects[&(7, effect, "offset".into(), 0)];
                    let point = [
                        (rect.min[0] + rect.max[0]) * 0.5,
                        (rect.min[1] + rect.max[1]) * 0.5,
                    ];
                    run(
                        &host,
                        draw_host(
                            &mut panels,
                            &before,
                            &Input {
                                mouse: point,
                                pressed: Some((MouseButton::Left, point)),
                                held: Some(MouseButton::Left),
                                ..Default::default()
                            },
                            panel,
                            floating,
                            &mut atlas,
                        ),
                    );
                    let actions = draw_host(
                        &mut panels,
                        &before,
                        &Input {
                            mouse: [point[0] + 30., point[1]],
                            held: Some(MouseButton::Left),
                            ..Default::default()
                        },
                        panel,
                        floating,
                        &mut atlas,
                    );
                    assert!(
                        actions.iter().any(|a| matches!(a, Action::Edit(_))),
                        "real drag required: {actions:?}"
                    );
                    run(&host, actions);
                    let mut changed = host.state().unwrap();
                    assert_ne!(changed["project"], before["project"]);
                    let mut input = Input {
                        mouse: [point[0] + 60., point[1]],
                        held: Some(MouseButton::Left),
                        ..Default::default()
                    };
                    match case {
                        0 => changed["root"] = "another-project-with-the-same-ids".into(),
                        1 => {
                            changed["composition"] = "another-composition-with-the-same-ids".into()
                        }
                        2 => changed["project"]["layers"][0]["effects"] = serde_json::json!([]),
                        3 => {
                            changed["project"]["layers"][0]["effects"][0]["params"]
                                .as_object_mut()
                                .unwrap()
                                .remove("offset");
                        }
                        4 => changed["project"]["layers"][0]["locked"] = true.into(),
                        5 => changed["observing"] = true.into(),
                        6 => input.focus_lost = true,
                        7 => input.keys.push((Key::Escape, Default::default())),
                        _ => changed["project"]["layers"] = serde_json::json!([]),
                    }
                    let actions =
                        draw_host(&mut panels, &changed, &input, panel, floating, &mut atlas);
                    assert!(matches!(actions.as_slice(),[Action::History(4)]),
                "locale={locale:?} floating={floating} panel={panel:?} case={case}: {actions:?}");
                    run(&host, actions);
                    assert_eq!(host.state().unwrap()["project"], before["project"]);
                    let actions = draw_host(
                        &mut panels,
                        &before,
                        &Input {
                            mouse: [point[0] + 90., point[1]],
                            held: Some(MouseButton::Left),
                            ..Default::default()
                        },
                        panel,
                        floating,
                        &mut atlas,
                    );
                    assert!(
                        actions.is_empty(),
                        "old capture must stay cancelled: {actions:?}"
                    );
                    host.save().unwrap();
                    assert_eq!(
                        serde_json::to_value(motion_core::storage::load(root.path()).unwrap())
                            .unwrap(),
                        before["project"]
                    );
                }
            }
        }
    }
}
