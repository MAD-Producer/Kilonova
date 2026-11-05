use motion_ui::{
    paint::{PaintList, Shape},
    style::{State, States},
    text::TextAtlas,
    widgets::TextEditor,
    Rect,
};

fn drawn_caret(editor: &TextEditor, atlas: &mut TextAtlas) -> f32 {
    let mut list = PaintList::default();
    editor.draw(&mut list, atlas, Rect::new(0., 0., 400., 30.));
    match list.shapes.last().expect("active editor needs a caret") {
        Shape::Line { points, .. } => points[0][0],
        shape => panic!("expected final caret line, got {shape:?}"),
    }
}

#[test]
fn rtl_logical_start_and_end_use_the_full_visual_line() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let text = "שלום";
    let layout = atlas.layout_line(text).unwrap();
    assert_eq!(layout.missing_glyphs, 0, "fixture must render Hebrew");
    assert!(layout.advance > 0.);
    assert!(layout
        .glyphs
        .windows(2)
        .all(|g| g[0].cluster > g[1].cluster));
    let style = motion_ui::style::default_stylesheet()
        .resolve("field", States::default().with(State::Focused))
        .unwrap();
    let mut editor = TextEditor::default();
    editor.begin_at_end(text);
    editor.select_bytes(0, 0);
    assert!(
        (drawn_caret(&editor, &mut atlas) - (style.padding + layout.advance * style.font_size))
            .abs()
            < 0.001
    );
    editor.select_bytes(text.len(), text.len());
    assert!((drawn_caret(&editor, &mut atlas) - style.padding).abs() < 0.001);
}

#[test]
fn kerned_caret_uses_the_advance_of_the_full_shaped_pair() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let layout = atlas.layout_line("AV").unwrap();
    assert_eq!(layout.missing_glyphs, 0);
    assert_eq!(layout.glyphs.len(), 2);
    assert_eq!(layout.glyphs[0].cluster, 0);
    assert_eq!(layout.glyphs[1].cluster, 1);
    let full_advance = layout.glyphs[0].advance[0];
    let isolated = atlas.layout_line("A").unwrap().advance;
    assert!(
        (full_advance - isolated).abs() > 0.001,
        "fixture must visibly kern AV"
    );
    let style = motion_ui::style::default_stylesheet()
        .resolve("field", States::default().with(State::Focused))
        .unwrap();
    let mut editor = TextEditor::default();
    editor.begin_at_end("AV");
    editor.select_bytes(1, 1);
    assert!(
        (drawn_caret(&editor, &mut atlas) - (style.padding + full_advance * style.font_size)).abs()
            < 0.001
    );
}

#[test]
fn caret_queries_and_hit_tests_never_split_marks_or_sequence_controls() {
    use unicode_segmentation::UnicodeSegmentation;
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for text in [
        "e\u{301}x",
        "A\u{e0100}B",
        "A\u{200c}\u{200d}B",
        "A👩🏽\u{200d}🚀B",
        "A🇯🇵B",
    ] {
        let layout = atlas.layout_line(text).unwrap();
        let boundaries = text
            .grapheme_indices(true)
            .map(|p| p.0)
            .chain([text.len()])
            .collect::<Vec<_>>();
        assert_eq!(layout.grapheme_boundaries, boundaries);
        for byte in 0..=text.len() + 1 {
            assert_eq!(layout.caret_x(byte).is_some(), boundaries.contains(&byte));
        }
        for x in [
            -100.,
            0.,
            layout.advance * 0.5,
            layout.advance,
            layout.advance + 100.,
        ] {
            assert!(boundaries.contains(&layout.hit_test(x).unwrap()));
        }
        assert_eq!(layout.hit_test(f32::NAN), None);
        assert_eq!(layout.hit_test(f32::INFINITY), None);
    }
}

#[test]
fn logical_bidi_selection_does_not_fill_unselected_visual_runs() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let text = "abc אבג xyz";
    let layout = atlas.layout_line(text).unwrap();
    assert_eq!(layout.missing_glyphs, 0);
    assert_eq!(layout.glyphs.len(), text.chars().count());
    let range = 2.."abc א".len();
    let mut boxes = Vec::new();
    layout.selection_spans(range.clone(), |span| boxes.push(span));
    assert_eq!(boxes.len(), 2);
    let mut pen = 0.;
    for glyph in &layout.glyphs {
        let center = pen + glyph.advance[0] * 0.5;
        if glyph.advance[0] > 0. {
            let selected = boxes
                .iter()
                .any(|span| center > span[0] && center < span[1]);
            assert_eq!(
                selected,
                range.contains(&glyph.cluster),
                "byte={}",
                glyph.cluster
            );
        }
        pen += glyph.advance[0];
    }
}

#[test]
fn actual_ligatures_keep_grapheme_carets_inside_the_original_advance() {
    let mut database = fontdb::Database::new();
    database.load_system_fonts();
    let mut selected = None;
    for face in database.faces() {
        let Some(font) = database
            .with_face_data(face.id, |data, index| {
                ab_glyph::FontVec::try_from_vec_and_index(data.to_vec(), index).ok()
            })
            .flatten()
        else {
            continue;
        };
        let Ok(mut atlas) =
            TextAtlas::new(ab_glyph::FontArc::new(font), face.index, 12., Vec::new())
        else {
            continue;
        };
        let layout = atlas.layout_line("ffi").unwrap();
        if layout.missing_glyphs == 0 && layout.glyphs.len() < 3 {
            selected = Some(layout);
            break;
        }
    }
    let layout = selected.expect("installed fixture needs a real ffi ligature");
    assert_eq!(layout.grapheme_boundaries, [0, 1, 2, 3]);
    let positions = (0..=3)
        .map(|byte| layout.caret_x(byte).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(positions[0], 0.);
    assert_eq!(positions[3], layout.advance);
    assert!(positions.windows(2).all(|p| p[0] < p[1]));
    for byte in 0..=3 {
        assert_eq!(layout.hit_test(positions[byte]), Some(byte));
    }
}
