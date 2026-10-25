use super::*;
use motion_ui::input::Key;

fn state() -> Value {
    serde_json::json!({"root":"project-a", "composition":"main", "document_generation":3,
        "revision":8, "frame":21.5, "project":{"composition_id":"main", "layers":[
        {"id":7,"name":"Child","content":{"kind":"solid"}},
        {"id":8,"name":"Second","content":{"kind":"solid"}},
        {"id":42,"name":"控制器","content":{"kind":"null"}},
        {"id":99,"name":"Descendant","content":{"kind":"solid"},"parent":{"object":7}},
        {"id":100,"name":"Other descendant","content":{"kind":"solid"},"parent":{"object":8}},
        {"id":101,"name":"Audio","content":{"kind":"audio"}}
    ]}})
}
fn draw(
    chooser: &mut Chooser,
    state: &Value,
    selected: &[u64],
    mode: &mut ParentMode,
    input: Input,
    locale: Locale,
    bounds: Rect,
) -> Decision {
    chooser.draw(
        &mut PaintList::default(),
        &mut TextAtlas::from_system(12.).unwrap(),
        Rect::new(20., 20., 86., 18.),
        bounds,
        state,
        selected,
        mode,
        &input,
        locale,
        false,
    )
}
fn click(rect: Rect) -> Input {
    let point = [
        rect.min[0] + rect.width() * 0.5,
        rect.min[1] + rect.height() * 0.5,
    ];
    Input {
        mouse: point,
        pressed: Some((MouseButton::Left, point)),
        ..Default::default()
    }
}
fn bounds() -> Rect {
    Rect::new(0., 0., 640., 400.)
}

#[test]
fn searchable_choices_exclude_both_sources_descendants_and_audio_and_commit_one_batch() {
    for locale in [Locale::En, Locale::Zh] {
        let state = state();
        let mut chooser = Chooser::new(&state, 7, &[8, 7]).unwrap();
        let mut mode = ParentMode::PreserveWorld;
        assert!(matches!(
            draw(
                &mut chooser,
                &state,
                &[7, 8],
                &mut mode,
                Input::default(),
                locale,
                bounds()
            ),
            Decision::Open
        ));
        assert_eq!(
            chooser.rows.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            vec![None, Some(42)]
        );
        assert!(matches!(
            draw(
                &mut chooser,
                &state,
                &[7, 8],
                &mut mode,
                Input {
                    text: "控制".into(),
                    ..Default::default()
                },
                locale,
                bounds()
            ),
            Decision::Open
        ));
        assert_eq!(
            chooser.rows.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            vec![Some(42)]
        );
        let rect = chooser.rows[0].1;
        let Decision::Apply(commands) = draw(
            &mut chooser,
            &state,
            &[7, 8],
            &mut mode,
            click(rect),
            locale,
            bounds(),
        ) else {
            panic!("expected a parent edit");
        };
        assert_eq!(commands.len(), 2);
        for (object, command) in [7, 8].into_iter().zip(&commands) {
            assert!(
                matches!(command,Command::InComposition {composition,command}
                if composition=="main" && matches!(command.as_ref(),Command::ParentWithMode {
                    object:id,parent:Some(42),frame:21,mode:ParentMode::PreserveWorld } if *id==object))
            );
        }
        let mut out = vec![];
        dispatch(commands, &mut out);
        assert!(matches!(out.as_slice(),[Action::EditBatch(commands)] if commands.len()==2));
    }
}

#[test]
fn checkbox_selects_local_policy_and_id_search_reaches_a_renamed_target() {
    let mut state = state();
    state["project"]["layers"][2]["name"] = Value::String("Same label".into());
    let mut chooser = Chooser::new(&state, 7, &[7]).unwrap();
    let mut mode = ParentMode::PreserveWorld;
    draw(
        &mut chooser,
        &state,
        &[7],
        &mut mode,
        Input::default(),
        Locale::En,
        bounds(),
    );
    let option = chooser.option.unwrap();
    assert!(matches!(
        draw(
            &mut chooser,
            &state,
            &[7],
            &mut mode,
            click(option),
            Locale::En,
            bounds()
        ),
        Decision::Open
    ));
    assert_eq!(mode, ParentMode::PreserveLocal);
    draw(
        &mut chooser,
        &state,
        &[7],
        &mut mode,
        Input {
            text: "42".into(),
            ..Default::default()
        },
        Locale::En,
        bounds(),
    );
    assert_eq!(
        chooser.rows.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        vec![Some(42)]
    );
    let Decision::Apply(commands) = draw(
        &mut chooser,
        &state,
        &[7],
        &mut mode,
        Input {
            keys: vec![(Key::Enter, Default::default())],
            ..Default::default()
        },
        Locale::En,
        bounds(),
    ) else {
        panic!("expected one edit");
    };
    assert!(
        matches!(commands.as_slice(),[Command::InComposition {command,..}]
        if matches!(command.as_ref(),Command::ParentWithMode {mode:ParentMode::PreserveLocal,..}))
    );
}

#[test]
fn stale_identity_scope_and_real_cancellation_never_dispatch() {
    let original = state();
    for key in ["root", "composition", "document_generation", "revision"] {
        let mut chooser = Chooser::new(&original, 7, &[7]).unwrap();
        let mut mode = ParentMode::PreserveWorld;
        draw(
            &mut chooser,
            &original,
            &[7],
            &mut mode,
            Input::default(),
            Locale::En,
            bounds(),
        );
        let mut changed = original.clone();
        changed[key] = serde_json::json!("different");
        assert!(matches!(
            draw(
                &mut chooser,
                &changed,
                &[7],
                &mut mode,
                Input::default(),
                Locale::En,
                bounds()
            ),
            Decision::Close
        ));
    }
    for (selected, input) in [
        (vec![7, 8], Input::default()),
        (
            vec![7],
            Input {
                focus_lost: true,
                ..Default::default()
            },
        ),
        (
            vec![7],
            Input {
                keys: vec![(Key::Escape, Default::default())],
                ..Default::default()
            },
        ),
        (vec![7], click(Rect::new(600., 350., 20., 20.))),
    ] {
        let mut chooser = Chooser::new(&original, 7, &[7]).unwrap();
        let mut mode = ParentMode::PreserveWorld;
        draw(
            &mut chooser,
            &original,
            &[7],
            &mut mode,
            Input::default(),
            Locale::En,
            bounds(),
        );
        assert!(matches!(
            draw(
                &mut chooser,
                &original,
                &selected,
                &mut mode,
                input,
                Locale::En,
                bounds()
            ),
            Decision::Close
        ));
    }
    let mut locked = original.clone();
    locked["project"]["layers"][1]["locked"] = serde_json::json!(true);
    assert!(Chooser::new(&locked, 7, &[7, 8]).is_none());
    assert!(Chooser::new(&original, 101, &[101]).is_none());
    locked["observing"] = serde_json::json!(true);
    assert!(Chooser::new(&locked, 7, &[7]).is_none());
}

#[test]
fn no_match_and_short_window_keep_visible_targets_inside_bounds() {
    let state = state();
    let small = Rect::new(0., 0., 220., 88.);
    let mut chooser = Chooser::new(&state, 7, &[7]).unwrap();
    let mut mode = ParentMode::PreserveWorld;
    assert!(matches!(
        draw(
            &mut chooser,
            &state,
            &[7],
            &mut mode,
            Input::default(),
            Locale::Zh,
            small
        ),
        Decision::Open
    ));
    assert_eq!(chooser.rows.len(), 1);
    assert!(chooser
        .rows
        .iter()
        .all(|(_, rect)| rect.min[0] >= small.min[0]
            && rect.min[1] >= small.min[1]
            && rect.max[0] <= small.max[0]
            && rect.max[1] <= small.max[1]));
    draw(
        &mut chooser,
        &state,
        &[7],
        &mut mode,
        Input {
            text: "不存在".repeat(128),
            ..Default::default()
        },
        Locale::Zh,
        small,
    );
    assert!(chooser.rows.is_empty());
    assert!(matches!(
        draw(
            &mut chooser,
            &state,
            &[7],
            &mut mode,
            Input {
                keys: vec![(Key::Enter, Default::default())],
                ..Default::default()
            },
            Locale::Zh,
            small
        ),
        Decision::Open
    ));
    assert!(matches!(
        draw(
            &mut chooser,
            &state,
            &[7],
            &mut mode,
            Input::default(),
            Locale::Zh,
            Rect::new(0., 0., 220., 50.)
        ),
        Decision::Close
    ));
}

fn finish_pick(
    controls: &mut Controls,
    state: &Value,
    selected: &[u64],
) -> (PaintList, Vec<Action>) {
    let mut paint = PaintList::default();
    let mut out = vec![];
    controls.finish_window(
        &mut paint,
        &mut TextAtlas::from_system(12.).unwrap(),
        bounds(),
        state,
        selected,
        &Input::default(),
        Locale::En,
        false,
        &mut out,
    );
    (paint, out)
}

#[test]
fn pick_highlights_legal_target_and_a_release_commits_once_for_the_frozen_selection() {
    let state = state();
    let mut controls = Controls::default();
    controls.prepare_window(None, &Input::default());
    controls.begin_pick(
        &state,
        7,
        &[7, 8],
        Rect::new(100., 100., 14., 14.),
        &click(Rect::new(100., 100., 14., 14.)),
    );
    let target = Rect::new(0., 150., 300., 22.);
    let held = Input {
        mouse: [200., 160.],
        held: Some(MouseButton::Left),
        ..Default::default()
    };
    assert!(controls.prepare_window(None, &held));
    controls.pick_row(42, target, None);
    let (paint, out) = finish_pick(&mut controls, &state, &[7, 8]);
    assert!(out.is_empty());
    assert!(
        paint.shapes.len() >= 2,
        "legal hover must paint a line and target outline"
    );
    let release = Input {
        mouse: [200., 160.],
        released: Some((MouseButton::Left, [200., 160.])),
        ..Default::default()
    };
    assert!(controls.prepare_window(None, &release));
    controls.pick_row(42, target, None);
    let (_, out) = finish_pick(&mut controls, &state, &[7, 8]);
    assert!(matches!(out.as_slice(),[Action::EditBatch(commands)] if commands.len()==2));
    assert!(controls.picker.is_none());
    assert!(!controls.prepare_window(None, &release));
    controls.pick_row(42, target, None);
    assert!(finish_pick(&mut controls, &state, &[7, 8]).1.is_empty());
}

#[test]
fn pick_cancellation_and_foreign_window_repaints_cannot_leak_a_later_release() {
    use motion_ui::dock::Panel;
    let state = state();
    for failure in [
        "self",
        "descendant",
        "outside",
        "stale",
        "selection",
        "lost-release",
        "escape",
        "focus",
        "foreign",
    ] {
        let mut controls = Controls::default();
        controls.prepare_window(Some(Panel::Timeline), &Input::default());
        controls.begin_pick(
            &state,
            7,
            &[7],
            Rect::new(100., 100., 14., 14.),
            &click(Rect::new(100., 100., 14., 14.)),
        );
        assert!(!controls.prepare_window(None, &Input::default()));
        assert!(finish_pick(&mut controls, &state, &[7]).1.is_empty());
        assert!(
            controls.picker.is_some(),
            "a neutral main repaint must retain the float pick"
        );
        let mut changed = state.clone();
        let mut selected = vec![7];
        let point = if failure == "outside" {
            [600., 350.]
        } else {
            [200., 160.]
        };
        let mut input = Input {
            mouse: point,
            released: Some((MouseButton::Left, point)),
            ..Default::default()
        };
        match failure {
            "stale" => changed["revision"] = serde_json::json!(9),
            "selection" => selected.push(8),
            "lost-release" => input.released = None,
            "escape" => input.keys.push((Key::Escape, Default::default())),
            "focus" => input.focus_lost = true,
            "foreign" => {
                assert!(controls.prepare_window(None, &click(Rect::new(0., 0., 20., 20.))));
                assert!(controls.picker.is_none());
            }
            _ => {}
        }
        controls.prepare_window(Some(Panel::Timeline), &input);
        let target = match failure {
            "self" => 7,
            "descendant" => 99,
            _ => 42,
        };
        controls.pick_row(target, Rect::new(0., 150., 300., 22.), None);
        assert!(
            finish_pick(&mut controls, &changed, &selected).1.is_empty(),
            "{failure}"
        );
        assert!(controls.picker.is_none(), "{failure}");
        controls.prepare_window(
            Some(Panel::Timeline),
            &Input {
                mouse: [200., 160.],
                ..Default::default()
            },
        );
        controls.pick_row(42, Rect::new(0., 150., 300., 22.), None);
        assert!(
            finish_pick(&mut controls, &state, &[7]).1.is_empty(),
            "{failure}"
        );
    }
}

#[test]
fn inspector_search_uses_the_same_policy_without_inserting_property_rows() {
    for window in [None, Some(motion_ui::dock::Panel::Properties)] {
        let state = state();
        let mut controls = Controls::default();
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let mut paint = PaintList::default();
        let mut actions = vec![];
        let styles = paint.styles();
        let row = styles.metric("parent.row.height", f32::default());
        let area = Rect::new(0., 0., 320., 480.);
        for input in [
            click(Rect::new(40., 20., 20., 20.)),
            Input {
                text: "42".into(),
                ..Default::default()
            },
            Input {
                keys: vec![(Key::Enter, Default::default())],
                ..Default::default()
            },
        ] {
            controls.prepare_window(window, &input);
            let mut y = 20.;
            controls.render(
                &mut paint,
                &mut atlas,
                area,
                &mut y,
                7,
                &state["project"]["layers"][0],
                &state,
                &[7],
                false,
                &input,
                Locale::Zh,
                &mut actions,
            );
            assert_eq!(y, 20. + row, "the popup must not add property rows");
            controls.finish_window(
                &mut paint,
                &mut atlas,
                area,
                &state,
                &[7],
                &input,
                Locale::Zh,
                false,
                &mut actions,
            );
        }
        assert!(!controls.open);
        assert!(
            matches!(actions.as_slice(),[Action::Edit(Command::InComposition {command,..})]
            if matches!(command.as_ref(),Command::ParentWithMode {object:7,parent:Some(42),mode:ParentMode::PreserveWorld,..}))
        );
    }
}
