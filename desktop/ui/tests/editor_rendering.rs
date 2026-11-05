use motion_ui::{
    input::{Modifiers, MouseButton},
    paint::PaintList,
    style::{State, States},
    text::TextAtlas,
    widgets::{TextCommand, TextEditor, TextViewport},
    Color, Input, Key, Rect,
};

#[test]
fn syntax_spans_preserve_complete_line_kerning_marks_and_joining() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let text = "AV e\u{301} سلام";
    let mut plain = TextEditor::default();
    plain.begin_multiline(text, 1024);
    plain.select_bytes(0, 0);
    let mut colored = plain.clone();
    let mut expected = PaintList::default();
    let mut actual = PaintList::default();
    let area = Rect::new(0., 0., 400., 80.);
    plain.multiline_view(
        &mut TextViewport::default(),
        &mut expected,
        &mut atlas,
        area,
        &Input::default(),
    );
    colored.multiline_view_colored(
        &mut TextViewport::default(),
        &mut actual,
        &mut atlas,
        area,
        &Input::default(),
        &|_, line| {
            line.chars()
                .enumerate()
                .map(|(i, c)| {
                    (
                        c.to_string(),
                        if i % 2 == 0 {
                            Color::rgb(250, 30, 30)
                        } else {
                            Color::rgb(30, 120, 250)
                        },
                    )
                })
                .collect()
        },
    );
    let geometry = |list: &PaintList| {
        list.glyphs
            .iter()
            .map(|g| (g.position, g.size, g.uv))
            .collect::<Vec<_>>()
    };
    assert!(expected.glyphs.len() > 4);
    assert_eq!(geometry(&actual), geometry(&expected));
    assert!(actual
        .glyphs
        .iter()
        .any(|g| g.color == Color::rgb(250, 30, 30)));
    assert!(actual
        .glyphs
        .iter()
        .any(|g| g.color == Color::rgb(30, 120, 250)));
}

#[test]
fn multiline_rtl_pointer_uses_visual_edges_instead_of_prefix_widths() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let text = "שלום";
    let layout = atlas.layout_line(text).unwrap();
    assert_eq!(layout.missing_glyphs, 0);
    let sheet = motion_ui::style::default_stylesheet();
    let style = sheet
        .resolve("field", States::default().with(State::Focused))
        .unwrap();
    let gutter = sheet.metric("text.editor.gutter.width", 44.);
    let mut editor = TextEditor::default();
    editor.begin_multiline(text, 1024);
    editor.select_bytes(0, 0);
    let mut view = TextViewport::default();
    for (x, byte) in [
        (gutter + 0.01, text.len()),
        (gutter + layout.advance * style.font_size, 0),
    ] {
        let point = [x, style.padding + 2.];
        editor.multiline_view(
            &mut view,
            &mut PaintList::default(),
            &mut atlas,
            Rect::new(0., 0., 400., 80.),
            &Input {
                pressed: Some((MouseButton::Left, point)),
                mouse: point,
                held: Some(MouseButton::Left),
                ..Default::default()
            },
        );
        assert_eq!(editor.caret_byte_index(), byte);
    }
}

#[test]
fn single_line_partial_grapheme_selection_replaces_and_undo_restores_it() {
    let mut editor = TextEditor::default();
    let text = "A👩🏽\u{200d}🚀e\u{301}Z";
    editor.begin_at_end(text);
    let end = "A👩🏽\u{200d}🚀".len();
    editor.select_bytes(1, end);
    editor.input(&Input {
        text: "X".into(),
        ..Default::default()
    });
    assert_eq!(editor.text, "AXe\u{301}Z");
    editor.command(TextCommand::Undo, None);
    assert_eq!(editor.text, text);
    assert_eq!(editor.selected_text(), Some("👩🏽\u{200d}🚀"));
    editor.input(&Input {
        keys: vec![(Key::Home, Modifiers::default())],
        ..Default::default()
    });
    editor.input(&Input {
        keys: vec![(
            Key::Right,
            Modifiers {
                shift: true,
                ..Default::default()
            },
        )],
        ..Default::default()
    });
    assert_eq!(editor.selected_text(), Some("A"));
}
