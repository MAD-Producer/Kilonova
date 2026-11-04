use super::super::super::inline::{Control, Popup};
use super::*;

#[derive(Clone, Copy, Debug)]
enum End {
    Escape,
    Outside,
    Apply,
}

fn prepare(zoom: f32) -> NodeEditor {
    let mut editor = simple();
    editor.inspector_open = false;
    editor.view.zoom = zoom;
    editor.focus_panel();
    editor.document.nodes.iter_mut().find(|n| n.id == "math").unwrap().position = [80., 60.];
    editor
}

fn field_rect(editor: &NodeEditor, row: usize, language: Language) -> Rect {
    let card = editor.cards(editor.last_canvas).into_iter().find(|c| c.id == "math").unwrap();
    let node = editor.document.nodes.iter().find(|n| n.id == "math").unwrap();
    let (row, _) = controls::control_rows(node, &editor.document, language)
        .into_iter().filter(|(_, c)| matches!(c, Control::Choice { .. })).nth(row).unwrap();
    controls::screen_rect(controls::number_rect(row, false), &card, editor.view.zoom)
}

fn menu_point(editor: &NodeEditor, row: usize, language: Language) -> [f32; 2] {
    let Some(Popup::Choice { items, .. }) = &editor.inline.popup else { panic!("choice popup required") };
    let anchor = field_rect(editor, row, language);
    let styles = PaintList::default().styles();
    let padding = styles.metric("padding", 5.) * editor.view.zoom;
    let height = styles.metric("node.dropdown.row.height", 28.) * editor.view.zoom;
    let rect = super::super::control_popup::choice_popup_rect(anchor, editor.last_canvas,
        [anchor.width(), items.len() as f32 * height + padding * 2.]);
    [rect.min[0] + padding * 2., rect.min[1] + padding + height * 0.5]
}

fn key(key: Key) -> Input {
    Input { keys: vec![(key, Default::default())], ..Default::default() }
}

fn scenario(editor: &mut NodeEditor, row: usize, end: End, language: Language,
    mut step: impl FnMut(&mut NodeEditor, Input, &str)) {
    step(editor, Input::default(), "initial");
    let before = editor.snapshot();
    let point = center(field_rect(editor, row, language));
    assert!(editor.last_canvas.contains(point));
    step(editor, press(point, MouseButton::Left), "open");
    assert!(matches!(editor.inline.popup, Some(Popup::Choice { .. })));
    step(editor, release(point, MouseButton::Left), "release");
    assert!(editor.inline.popup.is_some());
    // The popup must follow the current view, rather than a retained Card.
    editor.view.pan[0] += 7.;
    editor.view.pan[1] += 11.;
    editor.view.zoom *= 0.93;
    step(editor, Input::default(), "view");
    assert!(editor.inline.popup.is_some());
    let mouse = menu_point(editor, row, language);
    step(editor, Input { mouse, wheel: [0., 240.], ..Default::default() }, "scroll");
    assert!(editor.inline.popup.is_some());
    step(editor, key(Key::Down), "navigate");
    assert!(matches!(editor.inline.popup, Some(Popup::Choice { selected: 1, .. })));
    let close = match end {
        End::Escape => key(Key::Escape),
        End::Outside => press([editor.last_canvas.max[0] - 7., editor.last_canvas.min[1] + 7.], MouseButton::Left),
        End::Apply => key(Key::Enter),
    };
    step(editor, close, "close");
    assert!(editor.inline.popup.is_none());
    assert!(editor.drag.is_none(), "closing a popup must consume the canvas press");
    step(editor, Input::default(), "settled");
    if matches!(end, End::Apply) {
        assert_eq!(row, 0);
        assert_ne!(editor.document, before.document);
        assert_eq!(editor.revision, before.revision.wrapping_add(1));
        assert_eq!(editor.undo.len(), 1);
        assert!(matches!(editor.document.nodes.iter().find(|n| n.id == "math").unwrap().operation,
            NodeOp::Math { operation: motion_graph::MathOp::Subtract, value_type: ValueType::Float }));
        let after = editor.document.clone();
        assert!(editor.undo());
        assert_eq!(editor.document, before.document);
        assert!(editor.redo());
        assert_eq!(editor.document, after);
    } else {
        assert_eq!(editor.document, before.document);
        assert_eq!(editor.revision, before.revision);
        assert!(editor.undo.is_empty() && editor.redo.is_empty());
    }
}

fn matrix(mut run: impl FnMut(NodeEditor, Rect, Language, usize, End, &mut TextAtlas)) {
    for language in [Language::English, Language::Chinese] {
        let mut atlas = TextAtlas::from_system_language(12., language == Language::Chinese).unwrap();
        for size in [[900., 420.], [1600., 1000.]] {
            for zoom in [0.55, 1.25] {
                for (row, end) in [(0, End::Escape), (0, End::Outside), (0, End::Apply),
                    (1, End::Escape), (1, End::Outside)] {
                    run(prepare(zoom), Rect::new(13., 17., size[0], size[1]), language, row, end, &mut atlas);
                }
            }
        }
    }
}

#[test]
fn choice_popups_build_geometry_once_while_preserving_navigation_and_history() {
    let mut counts = vec![];
    matrix(|mut editor, area, language, row, end, atlas| {
        let zoom = editor.view.zoom;
        scenario(&mut editor, row, end, language, |editor, input, phase| {
            let (builds, ports) = draw_counts(editor, atlas, area, input, language);
            counts.push((format!("{language:?}-{}-{zoom}-{row}-{end:?}-{phase}", area.width()),
                builds, ports, editor.document.nodes.len()));
        });
    });
    assert_eq!(counts.len(), 320);
    assert!(counts.iter().all(|(_, builds, ports, nodes)| *builds == 1 && ports == nodes),
        "one card build for choice popup frames: {counts:?}");
}

fn popup_state(editor: &NodeEditor) -> String {
    match &editor.inline.popup {
        Some(Popup::Choice { node, field, items, selected, scroll }) => format!("{:?}",
            (node, field, items.iter().map(|i| (&i.label, &i.operation)).collect::<Vec<_>>(), selected, scroll)),
        None => "none".into(),
        _ => panic!("unexpected popup in choice fixture"),
    }
}

fn history_state(history: &[History]) -> String {
    history.iter().map(|h| format!("{:?}", (&h.document, &h.options, h.semantic,
        h.view, &h.selection, &h.primary))).collect::<Vec<_>>().join("\n")
}

#[test]
fn record_complete_choice_popup_frames_for_original_path_comparison() {
    let mut evidence = std::collections::BTreeMap::new();
    matrix(|mut editor, area, language, row, end, atlas| {
        let zoom = editor.view.zoom;
        scenario(&mut editor, row, end, language, |editor, input, phase| {
            let mut paint = PaintList::default();
            let response = editor.draw(&mut paint, atlas, area, &input, language);
            let record = format!("{:?}", ((paint.shapes, paint.colors, paint.glyphs, paint.order),
                (response.actions, response.wants_pointer, response.wants_keyboard, response.changed, response.semantic_changed),
                editor.snapshot(), (&editor.selected, &editor.primary, editor.view, &editor.notice, &editor.diagnostic),
                popup_state(editor), history_state(&editor.undo), history_state(&editor.redo)));
            assert!(evidence.insert(format!("{language:?}-{}-{zoom}-{row}-{end:?}-{phase}", area.width()), record).is_none());
        });
    });
    assert_eq!(evidence.len(), 320);
    if let Some(path) = std::env::var_os("MOTION_NODE_POPUP_EVIDENCE") {
        std::fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    }
}
