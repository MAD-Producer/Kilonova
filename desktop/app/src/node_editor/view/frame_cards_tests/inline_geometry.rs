use super::super::super::inline::Control;
use super::*;

#[derive(Clone, Copy, Debug)]
enum Fixture {
    Constant,
    DefaultInput,
    Parameter,
}
#[derive(Clone, Copy, Debug)]
enum End {
    Commit,
    Escape,
    Focus,
}

fn prepare(fixture: Fixture, zoom: f32) -> (NodeEditor, String) {
    let mut editor = simple();
    let id = match fixture {
        Fixture::Constant => "number".to_owned(),
        Fixture::DefaultInput => {
            let id = editor
                .add_node(
                    NodeOp::Math {
                        operation: motion_graph::MathOp::Add,
                        value_type: ValueType::Float,
                    },
                    [80., 60.],
                )
                .unwrap();
            assert!(matches!(
                editor.input_default(&id, "a").unwrap(),
                InputValue::Float(_)
            ));
            id
        }
        Fixture::Parameter => {
            let id = editor
                .add_public_parameter(ValueType::Float, [80., 60.])
                .unwrap();
            let parameter = editor.document.public_parameters.last_mut().unwrap();
            parameter.min = -2.;
            parameter.max = 2.;
            parameter.step = 0.25;
            parameter.default[0] = 0.;
            id
        }
    };
    editor
        .document
        .nodes
        .iter_mut()
        .find(|n| n.id == id)
        .unwrap()
        .position = [80., 60.];
    editor.undo.clear();
    editor.redo.clear();
    editor.view.zoom = zoom;
    editor.inspector_open = false;
    editor.focus_panel();
    editor.select(Some("color"), false);
    editor.select(Some(&id), true);
    (editor, id)
}

fn input_rect(editor: &NodeEditor, id: &str, slider: bool) -> Rect {
    let card = editor
        .cards(editor.last_canvas)
        .into_iter()
        .find(|c| c.id == id)
        .unwrap();
    let node = editor.document.nodes.iter().find(|n| n.id == id).unwrap();
    let (row, control) =
        controls::control_rows(node, &editor.document, Language::English).remove(0);
    let local = match control {
        Control::InputDefault { .. } => {
            controls::input_value_rect(row, &PaintList::default().styles())
        }
        Control::Number {
            slider: bounded, ..
        } => {
            if slider {
                controls::slider_rect(row)
            } else {
                controls::number_rect(row, bounded)
            }
        }
        _ => panic!("numeric fixture required"),
    };
    controls::screen_rect(local, &card, editor.view.zoom)
}
fn held(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        held: Some(MouseButton::Left),
        ..Default::default()
    }
}
fn key(key: Key) -> Input {
    Input {
        keys: vec![(key, Default::default())],
        ..Default::default()
    }
}

fn drag_case(
    editor: &mut NodeEditor,
    id: &str,
    fixture: Fixture,
    end: End,
    mut step: impl FnMut(&mut NodeEditor, Input, &str),
) {
    step(editor, Input::default(), "initial");
    let before = editor.snapshot();
    let selection = editor.selected.clone();
    let rect = input_rect(editor, id, matches!(fixture, Fixture::Parameter));
    let start = center(rect);
    assert!(editor.last_canvas.contains(start));
    step(editor, press(start, MouseButton::Left), "press");
    assert!(editor.inline.number.is_some());
    for (label, delta) in [("move1", 10.), ("move2", 20.)] {
        let point = [start[0] + delta * editor.view.zoom, start[1]];
        step(editor, held(point), label);
        assert_ne!(editor.document, before.document);
        assert!(editor.undo.is_empty());
        assert_eq!(editor.selected, selection);
    }
    let finish = match end {
        End::Commit => release(
            [start[0] + 20. * editor.view.zoom, start[1]],
            MouseButton::Left,
        ),
        End::Escape => key(Key::Escape),
        End::Focus => Input {
            focus_lost: true,
            ..Default::default()
        },
    };
    step(editor, finish, "finish");
    assert!(editor.inline.number.is_none());
    assert!(editor
        .document
        .nodes
        .iter()
        .zip(&before.document.nodes)
        .all(|(a, b)| a.position == b.position));
    if matches!(end, End::Commit) {
        assert_eq!(editor.undo.len(), 1);
        let changed = editor.document.clone();
        assert!(editor.undo());
        step(editor, Input::default(), "undo");
        assert_eq!(editor.document, before.document);
        assert!(editor.redo());
        step(editor, Input::default(), "redo");
        assert_eq!(editor.document, changed);
    } else {
        assert_eq!(editor.document, before.document);
        assert!(editor.undo.is_empty());
        step(editor, Input::default(), "cancelled1");
        step(editor, Input::default(), "cancelled2");
    }
}

fn text_case(
    editor: &mut NodeEditor,
    id: &str,
    mut step: impl FnMut(&mut NodeEditor, Input, &str),
) {
    step(editor, Input::default(), "initial");
    let before = editor.snapshot();
    let point = center(input_rect(editor, id, false));
    assert!(editor.last_canvas.contains(point));
    step(editor, press(point, MouseButton::Left), "press");
    step(editor, release(point, MouseButton::Left), "release");
    assert!(editor.inline.number.as_ref().unwrap().editor.text.active());
    step(
        editor,
        Input {
            text: "invalid".into(),
            ..Default::default()
        },
        "invalid",
    );
    let save = Input {
        keys: vec![(
            Key::S,
            Modifiers {
                control: true,
                ..Default::default()
            },
        )],
        ..Default::default()
    };
    step(editor, save.clone(), "rejected_save");
    assert_eq!(editor.document, before.document);
    assert!(editor
        .inline
        .number
        .as_ref()
        .unwrap()
        .editor
        .text
        .error
        .is_some());
    step(
        editor,
        Input {
            keys: vec![(
                Key::A,
                Modifiers {
                    control: true,
                    ..Default::default()
                },
            )],
            text: "1.5".into(),
            ..Default::default()
        },
        "valid",
    );
    step(
        editor,
        Input {
            composing: true,
            keys: vec![(Key::Enter, Default::default())],
            ..Default::default()
        },
        "composing",
    );
    assert_eq!(editor.document, before.document);
    step(editor, save, "save");
    assert!(editor.inline.number.is_none());
    assert_eq!(editor.undo.len(), 1);
    assert_ne!(editor.document, before.document);
    let changed = editor.document.clone();
    assert!(editor.undo());
    step(editor, Input::default(), "undo");
    assert_eq!(editor.document, before.document);
    assert!(editor.redo());
    step(editor, Input::default(), "redo");
    assert_eq!(editor.document, changed);
}

#[test]
fn inline_value_edits_build_final_cards_once_without_changing_transactions() {
    let mut counts = vec![];
    for language in [Language::English, Language::Chinese] {
        let mut atlas =
            TextAtlas::from_system_language(12., language == Language::Chinese).unwrap();
        for size in [[1200., 800.], [1600., 1000.]] {
            for zoom in [0.55, 1.35] {
                for fixture in [Fixture::Constant, Fixture::DefaultInput, Fixture::Parameter] {
                    let area = Rect::new(13., 17., size[0], size[1]);
                    for end in [End::Commit, End::Escape, End::Focus] {
                        let (mut editor, id) = prepare(fixture, zoom);
                        drag_case(&mut editor, &id, fixture, end, |editor, input, phase| {
                            let (builds, resolutions) =
                                draw_counts(editor, &mut atlas, area, input, language);
                            counts.push((
                                format!("{language:?}-{size:?}-{zoom}-{fixture:?}-{end:?}-{phase}"),
                                builds,
                                resolutions,
                                editor.document.nodes.len(),
                            ));
                        });
                    }
                    let (mut editor, id) = prepare(fixture, zoom);
                    text_case(&mut editor, &id, |editor, input, phase| {
                        let (builds, resolutions) =
                            draw_counts(editor, &mut atlas, area, input, language);
                        counts.push((
                            format!("{language:?}-{size:?}-{zoom}-{fixture:?}-Text-{phase}"),
                            builds,
                            resolutions,
                            editor.document.nodes.len(),
                        ));
                    });
                }
            }
        }
    }
    assert_eq!(counts.len(), 744);
    assert!(
        counts
            .iter()
            .all(|(_, builds, resolutions, nodes)| *builds == 1 && resolutions == nodes),
        "one final card build for inline edits: {counts:?}"
    );
}

fn record(
    editor: &mut NodeEditor,
    atlas: &mut TextAtlas,
    area: Rect,
    input: Input,
    language: Language,
) -> String {
    let mut paint = PaintList::default();
    let response = editor.draw(&mut paint, atlas, area, &input, language);
    assert!(!paint.shapes.is_empty() && !paint.glyphs.is_empty());
    let edit = editor.inline.number.as_ref().map(|edit| {
        (
            &edit.owner,
            &edit.binding,
            edit.slider,
            edit.in_popup,
            edit.editor.interacting(),
            edit.editor.drag_values(),
            edit.editor.text.active(),
            &edit.editor.text.text,
            &edit.editor.text.error,
            edit.editor.text.caret_byte_index(),
            edit.editor.text.is_all_selected(),
            edit.before.as_ref().map(|before| {
                (
                    &before.document,
                    &before.options,
                    before.semantic,
                    &before.selection,
                    &before.primary,
                )
            }),
        )
    });
    format!(
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
            (
                editor.snapshot(),
                editor.undo.len(),
                editor.redo.len(),
                edit,
                editor.focus,
                editor.search_focus,
                &editor.notice
            )
        )
    )
}

#[test]
fn record_complete_inline_transactions_for_original_path_comparison() {
    let mut evidence = std::collections::BTreeMap::new();
    for language in [Language::English, Language::Chinese] {
        let mut atlas =
            TextAtlas::from_system_language(12., language == Language::Chinese).unwrap();
        for size in [[1200., 800.], [1600., 1000.]] {
            for zoom in [0.55, 1.35] {
                for fixture in [Fixture::Constant, Fixture::DefaultInput, Fixture::Parameter] {
                    let area = Rect::new(13., 17., size[0], size[1]);
                    for end in [End::Commit, End::Escape, End::Focus] {
                        let (mut editor, id) = prepare(fixture, zoom);
                        let mut frames = vec![];
                        drag_case(&mut editor, &id, fixture, end, |editor, input, phase| {
                            frames.push((
                                phase.to_owned(),
                                record(editor, &mut atlas, area, input, language),
                            ))
                        });
                        assert_eq!(frames.len(), 7);
                        evidence.insert(
                            format!("{language:?}-{size:?}-{zoom}-{fixture:?}-{end:?}"),
                            frames,
                        );
                    }
                    let (mut editor, id) = prepare(fixture, zoom);
                    let mut frames = vec![];
                    text_case(&mut editor, &id, |editor, input, phase| {
                        frames.push((
                            phase.to_owned(),
                            record(editor, &mut atlas, area, input, language),
                        ))
                    });
                    assert_eq!(frames.len(), 10);
                    evidence.insert(
                        format!("{language:?}-{size:?}-{zoom}-{fixture:?}-Text"),
                        frames,
                    );
                }
            }
        }
    }
    assert_eq!(evidence.len(), 96);
    if let Some(path) = std::env::var_os("MOTION_NODE_INLINE_EVIDENCE") {
        std::fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    }
    println!("complete inline frames=744; transaction configurations=96");
}

#[test]
fn inline_hit_rectangles_match_original_cards_for_builtins_overlap_and_invalid_nodes() {
    let mut documents: Vec<_> = motion_graph::builtin_examples()
        .iter()
        .map(|example| example.load().unwrap())
        .collect();
    let mut duplicate = simple().document;
    duplicate.nodes[1].id = duplicate.nodes[0].id.clone();
    documents.push(duplicate);
    let mut invalid = simple().document;
    invalid.nodes[3].operation = NodeOp::Parameter {
        parameter: "missing".into(),
    };
    documents.push(invalid);
    let mut empty = simple().document;
    empty.nodes[3].operation = NodeOp::GroupInput {
        ports: Default::default(),
    };
    documents.push(empty);
    let canvas = Rect::new(17., 33., 1200., 800.);
    let mut comparisons = 0;
    let bits = |rect: Rect| {
        [
            rect.min[0].to_bits(),
            rect.min[1].to_bits(),
            rect.max[0].to_bits(),
            rect.max[1].to_bits(),
        ]
    };
    for document in documents {
        for overlap in [false, true] {
            for collapsed in [false, true] {
                let mut editor = NodeEditor::new(document.clone());
                for (i, node) in editor.document.nodes.iter_mut().enumerate() {
                    node.position = if overlap {
                        [80., 60.]
                    } else {
                        [(i % 4) as f32 * 230., (i / 4) as f32 * 210.]
                    };
                    node.collapsed = collapsed;
                }
                for zoom in [0.33, 0.85, 2.75] {
                    editor.view.zoom = zoom;
                    let cards = editor.cards(canvas);
                    let mut points = vec![
                        [f32::NAN, 0.],
                        [0., f32::NAN],
                        [f32::INFINITY, f32::INFINITY],
                        [-1e6, -1e6],
                    ];
                    for card in &cards {
                        points.extend([
                            card.rect.min,
                            card.rect.max,
                            center(card.rect),
                            [card.rect.max[0], card.rect.min[1]],
                            [card.rect.min[0], card.rect.max[1]],
                            [card.rect.max[0] - 0.001, card.rect.max[1] - 0.001],
                        ]);
                    }
                    for point in points {
                        let expected = cards
                            .iter()
                            .rev()
                            .find(|card| card.rect.contains(point))
                            .map(|card| (card.id.as_str(), bits(card.rect)));
                        let actual = editor
                            .inline_card_hit(canvas, point)
                            .map(|(id, rect)| (id, bits(rect)));
                        assert_eq!(
                            actual, expected,
                            "overlap={overlap} collapsed={collapsed} zoom={zoom} point={point:?}"
                        );
                        comparisons += 1;
                    }
                }
            }
        }
    }
    assert!(comparisons > 0);
    println!("original inline card-hit comparisons={comparisons}");
}

#[test]
fn inline_hit_height_queries_stop_at_the_front_card_and_skip_outside_columns() {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            PROBE.with(|state| *state.borrow_mut() = None);
        }
    }
    let mut editor = simple();
    let canvas = Rect::new(17., 33., 1200., 800.);
    for node in &mut editor.document.nodes {
        node.position = [80., 60.];
    }
    let origin = editor.view.world_to_screen([80., 60.], canvas);
    PROBE.with(|state| assert!(state.borrow_mut().replace(Probe::default()).is_none()));
    let reset = Reset;
    assert!(editor
        .inline_card_hit(canvas, [origin[0] - 1., origin[1] + 5.])
        .is_none());
    assert!(editor
        .inline_card_hit(
            canvas,
            [origin[0] + NODE_WIDTH * editor.view.zoom, origin[1] + 5.]
        )
        .is_none());
    assert!(editor
        .inline_card_hit(canvas, [origin[0] + 5., origin[1] - 1.])
        .is_none());
    PROBE.with(|state| assert_eq!(state.borrow().as_ref().unwrap().inline_hit_heights, 0));
    assert_eq!(
        editor
            .inline_card_hit(canvas, [origin[0] + 5., origin[1] + 5.])
            .unwrap()
            .0,
        "math"
    );
    let probe = PROBE.with(|state| state.borrow_mut().take().unwrap());
    drop(reset);
    assert_eq!(probe.inline_hit_heights, 1);
    assert_eq!(probe.builds, 0);
    assert_eq!(probe.port_resolutions, 0);
}
