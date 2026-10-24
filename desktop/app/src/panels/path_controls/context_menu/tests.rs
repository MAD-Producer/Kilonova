use super::*;

fn fixture(closed: bool, locked: bool) -> (Controls, Value, Projection, Vec<Value>) {
    let projection = Projection::new(
        DMat4::IDENTITY.to_cols_array(),
        Rect::new(0., 0., 200., 200.),
        [200.; 2],
    )
    .unwrap();
    let state = serde_json::json!({"root":"path-menu", "composition":"main", "document_generation":1, "revision":0,
        "project":{"layers":[{"id":7,"locked":locked}]}});
    let paths = vec![serde_json::json!({"id":3,"closed":closed,"nodes":[
        {"id":10,"geometry":[-0.5,-0.5,0.,0.,0.,0.]},
        {"id":11,"geometry":[0.5,-0.5,0.,0.,0.,0.]},
        {"id":12,"geometry":[0.,0.5,0.,0.,0.,0.]}]})];
    let mut c = Controls::default();
    c.tool = Tool::Pen;
    c.point_selection
        .sync(&state, Some(7), false, &paths, &mut vec![]);
    (c, state, projection, paths)
}
fn open(c: &mut Controls, state: &Value, p: &Projection, paths: &[Value]) {
    c.prepare_context_menu(None, state, Some(7), &Input::default());
    let point = p.screen([-0.5, -0.5]).unwrap();
    assert!(c.open_context_menu(
        p.image,
        paths,
        Some(p),
        [0.; 2],
        state,
        Some(7),
        &Input {
            mouse: point,
            pressed: Some((MouseButton::Right, point)),
            ..Default::default()
        }
    ));
}
fn paint(
    c: &mut Controls,
    state: &Value,
    input: Input,
    locale: Locale,
    height: f32,
) -> Vec<Action> {
    let mut out = vec![];
    c.finish_context_menu(
        &mut PaintList::default(),
        &mut TextAtlas::from_system(12.).unwrap(),
        Rect::new(0., 0., 280., height),
        state,
        &input,
        locale,
        true,
        false,
        &mut out,
    );
    out
}
#[test]
fn context_menu_retains_source_labels_closedness_lock_and_single_choice_semantics() {
    for locale in [Locale::Zh, Locale::En] {
        for (closed, locked) in [(true, false), (false, false), (true, true)] {
            let (mut c, state, p, paths) = fixture(closed, locked);
            open(&mut c, &state, &p, &paths);
            assert!(paint(&mut c, &state, Input::default(), locale, 240.).is_empty());
            assert_eq!(c.context_rows[0].2, closed && !locked);
            assert_eq!(c.context_rows[1].2, !locked);
            assert_eq!(
                c.context_rows[1].0,
                if locale == Locale::Zh {
                    "复制为蒙版路径"
                } else {
                    "Copy as Mask Path"
                }
            );
            let r = c.context_rows[1].1;
            let point = [r.min[0] + 1., r.min[1] + 1.];
            let out = paint(
                &mut c,
                &state,
                Input {
                    mouse: point,
                    pressed: Some((MouseButton::Left, point)),
                    ..Default::default()
                },
                locale,
                240.,
            );
            if locked {
                assert!(out.is_empty());
            } else {
                assert!(matches!(
                    out.as_slice(),
                    [Action::PathOperation {
                        operation: PathOperation::CopyAs {
                            target: PathTarget::Vector {
                                object: 7,
                                item: None
                            },
                            path: 3
                        },
                        ..
                    }]
                ));
                assert!(c.context_menu.is_none());
            }
        }
    }
}
#[test]
fn context_changes_focus_escape_and_foreign_input_cancel_without_dispatch_or_repaint_loss() {
    for reason in [
        "root",
        "composition",
        "document_generation",
        "revision",
        "selection",
        "focus",
        "escape",
        "foreign",
    ] {
        let (mut c, mut state, p, paths) = fixture(true, false);
        open(&mut c, &state, &p, &paths);
        assert!(paint(&mut c, &state, Input::default(), Locale::En, 240.).is_empty());
        assert!(!c.prepare_context_menu(
            Some(Panel::Composition),
            &state,
            Some(7),
            &Input::default()
        ));
        assert!(c.context_menu.is_some(), "foreign repaint");
        let mut input = Input::default();
        let mut selected = Some(7);
        let mut owner = None;
        match reason {
            "root" | "composition" => state[reason] = "replacement".into(),
            "document_generation" | "revision" => state[reason] = 2.into(),
            "selection" => selected = Some(9),
            "focus" => input.focus_lost = true,
            "escape" => input.keys.push((Key::Escape, Default::default())),
            "foreign" => {
                owner = Some(Panel::Composition);
                input.keys.push((Key::C, Default::default()));
            }
            _ => unreachable!(),
        }
        assert!(c.prepare_context_menu(owner, &state, selected, &input));
        assert!(paint(&mut c, &state, input, Locale::En, 240.).is_empty());
        assert!(c.context_menu.is_none(), "{reason}");
    }
}
#[test]
fn short_context_menu_scrolls_to_paste_without_clipped_invisible_hit_regions() {
    let (mut c, state, p, paths) = fixture(true, false);
    open(&mut c, &state, &p, &paths);
    assert!(paint(&mut c, &state, Input::default(), Locale::En, 88.).is_empty());
    let row = c.context_rows[0].1;
    let mouse = [
        (row.min[0] + row.max[0]) * 0.5,
        (row.min[1] + row.max[1]) * 0.5,
    ];
    for _ in 0..6 {
        assert!(paint(
            &mut c,
            &state,
            Input {
                mouse,
                wheel: [0., -1.],
                ..Default::default()
            },
            Locale::En,
            88.
        )
        .is_empty());
    }
    let (_, r, enabled) = c
        .context_rows
        .iter()
        .find(|(s, _, _)| s == "Paste Paths in Place")
        .unwrap();
    assert!(*enabled);
    assert!(r.min[1] >= 0. && r.max[1] <= 88.);
    let point = [r.min[0] + 1., r.min[1] + 1.];
    assert!(matches!(
        paint(
            &mut c,
            &state,
            Input {
                mouse: point,
                pressed: Some((MouseButton::Left, point)),
                ..Default::default()
            },
            Locale::En,
            88.
        )
        .as_slice(),
        [Action::PathClipboard {
            intent: Intent::Paste { in_place: true },
            ..
        }]
    ));
}
