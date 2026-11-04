use super::*;

#[derive(Clone, Copy, Debug)]
enum Case {
    Connect,
    Rewire,
    Disconnect,
    Cut,
    Preview,
    RestorePreview,
    TypeReject,
    CycleReject,
}

const CASES: [Case; 8] = [
    Case::Connect,
    Case::Rewire,
    Case::Disconnect,
    Case::Cut,
    Case::Preview,
    Case::RestorePreview,
    Case::TypeReject,
    Case::CycleReject,
];

fn prepare(case: Case, zoom: f32) -> NodeEditor {
    let mut editor = simple();
    editor.inspector_open = false;
    editor.view.zoom = zoom;
    editor.focus_panel();
    if matches!(case, Case::Rewire | Case::Disconnect | Case::Cut) {
        editor
            .connect_ports(PortRef::new("number", "value"), PortRef::new("math", "a"))
            .unwrap();
    }
    if matches!(case, Case::RestorePreview) {
        editor
            .toggle_preview(PortRef::new("number", "value"))
            .unwrap();
    }
    editor.undo.clear();
    editor.redo.clear();
    editor
}

fn socket(editor: &NodeEditor, node: &str, port: &str, output: bool) -> [f32; 2] {
    editor
        .cards(editor.last_canvas)
        .into_iter()
        .flat_map(|card| card.sockets)
        .find(|s| s.port == PortRef::new(node, port) && s.output == output)
        .unwrap()
        .point
}

fn scenario(
    editor: &mut NodeEditor,
    case: Case,
    mut step: impl FnMut(&mut NodeEditor, Input, &str),
) {
    step(editor, Input::default(), "initial");
    let before = editor.snapshot();
    let number = socket(editor, "number", "value", true);
    let a = socket(editor, "math", "a", false);
    let b = socket(editor, "math", "b", false);
    let blank = [
        editor.last_canvas.max[0] - 35.,
        editor.last_canvas.max[1] - 35.,
    ];
    let (from, to, button, modifiers) = match case {
        Case::Connect => (number, a, MouseButton::Left, Modifiers::default()),
        Case::Rewire => (a, b, MouseButton::Left, Modifiers::default()),
        Case::Disconnect => (a, blank, MouseButton::Left, Modifiers::default()),
        Case::Cut => {
            let mid = [(number[0] + a[0]) * 0.5, (number[1] + a[1]) * 0.5];
            (
                [mid[0], mid[1] - 35.],
                [mid[0], mid[1] + 35.],
                MouseButton::Right,
                Modifiers {
                    alt: true,
                    ..Default::default()
                },
            )
        }
        Case::Preview => (
            number,
            number,
            MouseButton::Left,
            Modifiers {
                control: true,
                shift: true,
                ..Default::default()
            },
        ),
        Case::RestorePreview => {
            let card = editor
                .cards(editor.last_canvas)
                .into_iter()
                .find(|card| card.id == "output")
                .unwrap();
            let point = [
                (card.rect.min[0] + card.rect.max[0]) * 0.5,
                card.rect.min[1] + HEADER * editor.view.zoom * 0.5,
            ];
            (
                point,
                point,
                MouseButton::Left,
                Modifiers {
                    control: true,
                    shift: true,
                    ..Default::default()
                },
            )
        }
        Case::TypeReject => (
            number,
            socket(editor, "output", "color", false),
            MouseButton::Left,
            Modifiers::default(),
        ),
        Case::CycleReject => (
            socket(editor, "math", "value", true),
            a,
            MouseButton::Left,
            Modifiers::default(),
        ),
    };
    assert!(editor.last_canvas.contains(from) && editor.last_canvas.contains(to));
    let mut start = press(from, button);
    start.modifiers = modifiers;
    step(editor, start, "press");
    step(
        editor,
        Input {
            mouse: to,
            held: Some(button),
            modifiers,
            ..Default::default()
        },
        "move",
    );
    let mut finish = release(to, button);
    finish.modifiers = modifiers;
    step(editor, finish, "release");
    assert!(editor.drag.is_none() && !editor.inline.busy());
    let mut expected = before.document.clone();
    match case {
        Case::Connect => expected
            .connections
            .push(Connection::new("number", "value", "math", "a")),
        Case::Rewire => {
            expected
                .connections
                .retain(|edge| edge.to != PortRef::new("math", "a"));
            expected
                .connections
                .push(Connection::new("number", "value", "math", "b"));
        }
        Case::Disconnect | Case::Cut => expected
            .connections
            .retain(|edge| edge.to != PortRef::new("math", "a")),
        _ => {}
    }
    assert_eq!(editor.document, expected, "document after {case:?}");
    match case {
        Case::Preview => assert_eq!(editor.preview_target, Some(PortRef::new("number", "value"))),
        Case::RestorePreview => assert!(editor.preview_target.is_none()),
        Case::TypeReject | Case::CycleReject => {
            assert!(editor.notice.is_some());
            assert_eq!(editor.preview_revision, before.preview_revision);
        }
        _ => assert_eq!(editor.preview_target, before.preview_target),
    }
    step(editor, Input::default(), "settled");
    if matches!(
        case,
        Case::Connect | Case::Rewire | Case::Disconnect | Case::Cut
    ) {
        assert_eq!(editor.undo.len(), 1);
        assert_eq!(
            editor.semantic_revision,
            before.semantic_revision.wrapping_add(1)
        );
        assert!(editor.undo());
        step(editor, Input::default(), "undo");
        assert_eq!(editor.document, before.document);
        assert!(editor.redo());
        step(editor, Input::default(), "redo");
        assert_eq!(editor.document, expected);
    } else {
        assert!(editor.undo.is_empty() && editor.redo.is_empty());
        assert_eq!(editor.semantic_revision, before.semantic_revision);
        step(editor, Input::default(), "idle1");
        step(editor, Input::default(), "idle2");
    }
}

#[test]
fn connection_and_preview_frames_resolve_card_ports_once_without_changing_transactions() {
    let mut counts = vec![];
    for language in [Language::English, Language::Chinese] {
        let mut atlas =
            TextAtlas::from_system_language(12., language == Language::Chinese).unwrap();
        for size in [[1200., 800.], [1600., 1000.]] {
            let area = Rect::new(13., 17., size[0], size[1]);
            for zoom in [0.55, 1.35] {
                for case in CASES {
                    let mut editor = prepare(case, zoom);
                    scenario(&mut editor, case, |editor, input, phase| {
                        let (builds, ports) =
                            draw_counts(editor, &mut atlas, area, input, language);
                        counts.push((
                            format!("{language:?}-{size:?}-{zoom}-{case:?}-{phase}"),
                            builds,
                            ports,
                            editor.document.nodes.len(),
                        ));
                    });
                }
            }
        }
    }
    assert_eq!(counts.len(), 448);
    assert!(
        counts
            .iter()
            .all(|(_, builds, ports, nodes)| *builds == 1 && ports == nodes),
        "one final card build for connection and preview edits: {counts:?}"
    );
}

fn drag_state(editor: &NodeEditor) -> String {
    match &editor.drag {
        Some(Drag::Wire { source, old_input }) => format!("wire {source:?} {old_input:?}"),
        Some(Drag::WireInput { target }) => format!("input {target:?}"),
        Some(Drag::Cut {
            points,
            last,
            inputs,
        }) => format!("cut {points:?} {last:?} {inputs:?}"),
        None => "none".into(),
        _ => panic!("unexpected canvas transaction"),
    }
}

fn history_state(history: &[History]) -> String {
    history
        .iter()
        .map(|h| {
            format!(
                "{:?}",
                (
                    &h.document,
                    &h.options,
                    h.semantic,
                    h.view,
                    &h.selection,
                    &h.primary
                )
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn record_complete_canvas_transactions_for_original_path_comparison() {
    let mut evidence = std::collections::BTreeMap::new();
    for language in [Language::English, Language::Chinese] {
        let mut atlas =
            TextAtlas::from_system_language(12., language == Language::Chinese).unwrap();
        for size in [[1200., 800.], [1600., 1000.]] {
            let area = Rect::new(13., 17., size[0], size[1]);
            for zoom in [0.55, 1.35] {
                for case in CASES {
                    let mut editor = prepare(case, zoom);
                    scenario(&mut editor, case, |editor, input, phase| {
                        let mut paint = PaintList::default();
                        let response = editor.draw(&mut paint, &mut atlas, area, &input, language);
                        let record = format!(
                            "{:?}",
                            (
                                (paint.shapes, paint.colors, paint.glyphs, paint.order),
                                (
                                    response.actions,
                                    response.wants_pointer,
                                    response.wants_keyboard,
                                    response.changed,
                                    response.semantic_changed
                                ),
                                editor.snapshot(),
                                (
                                    &editor.selected,
                                    &editor.primary,
                                    editor.view,
                                    &editor.notice,
                                    &editor.diagnostic
                                ),
                                drag_state(editor),
                                history_state(&editor.undo),
                                history_state(&editor.redo)
                            )
                        );
                        assert!(evidence
                            .insert(
                                format!("{language:?}-{size:?}-{zoom}-{case:?}-{phase}"),
                                record
                            )
                            .is_none());
                    });
                }
            }
        }
    }
    assert_eq!(evidence.len(), 448);
    if let Some(path) = std::env::var_os("MOTION_NODE_CANVAS_EVIDENCE") {
        std::fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    }
}

#[test]
fn canvas_field_commits_rejections_and_group_navigation_reject_port_reuse() {
    let canvas = Rect::new(13., 55., 1200., 700.);
    let point = [1100., 700.];
    for text in ["2", "invalid"] {
        let mut editor = prepare(Case::Connect, 1.);
        let before = editor.snapshot();
        editor.last_canvas = canvas;
        editor.field = Some(Field {
            binding: Binding::Constant("number".into(), 0),
            text: text.into(),
            cursor: text.len(),
            replace: false,
            editor: Default::default(),
            error: None,
        });
        let cards = editor.cards(canvas);
        assert!(!editor.canvas_input(
            &cards,
            canvas,
            &press(point, MouseButton::Left),
            Language::English
        ));
        if text == "invalid" {
            assert_eq!(editor.document, before.document);
            assert!(editor.field.as_ref().unwrap().error.is_some());
            assert!(editor.notice.is_some() && editor.undo.is_empty());
        } else {
            assert!(editor.field.is_none());
            assert_ne!(editor.document, before.document);
            assert_eq!(editor.undo.len(), 1);
        }
    }
    let mut editor = prepare(Case::Connect, 1.);
    editor.select(Some("number"), false);
    editor.select(Some("math"), true);
    assert!(editor.group_selected().unwrap());
    editor.last_canvas = canvas;
    let id = editor.primary.clone().unwrap();
    let mut cards = editor.cards(canvas);
    let card = cards.iter().find(|c| c.id == id).unwrap();
    let point = [
        (card.rect.min[0] + card.rect.max[0]) * 0.5,
        card.rect.min[1] + HEADER * editor.view.zoom * 0.5,
    ];
    editor.last_group_click = Some((id, std::time::Instant::now(), point));
    assert!(!editor.canvas_input(
        &cards,
        canvas,
        &press(point, MouseButton::Left),
        Language::English
    ));
    assert_eq!(editor.parents.len(), 1);
    assert!(!editor.refresh_card_positions(&mut cards, canvas));
}
