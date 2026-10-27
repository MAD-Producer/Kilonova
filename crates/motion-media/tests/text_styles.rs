use motion_media::fonts::{FontStore, Raster};
use motion_model::{TextAlign, TextStyle};

fn bounds(raster: &Raster) -> [u32; 4] {
    let mut result = [raster.width, raster.height, 0, 0];
    for y in 0..raster.height {
        for x in 0..raster.width {
            if raster.rgba[((y * raster.width + x) * 4 + 3) as usize] != 0 {
                result[0] = result[0].min(x);
                result[1] = result[1].min(y);
                result[2] = result[2].max(x);
                result[3] = result[3].max(y);
            }
        }
    }
    result
}
#[test]
fn authored_style_changes_actual_pixels_and_reopens_deterministically() {
    let root = tempfile::tempdir().unwrap();
    let mut fonts = FontStore::new(root.path()).unwrap();
    let id = FontStore::builtin_id();
    let original = fonts.raster_text(&id, "Text\nAB", 24., 320).unwrap();
    let mut style = TextStyle::new(24., 320);
    assert_eq!(
        fonts
            .raster_styled_text(&id, "Text\nAB", &style)
            .unwrap()
            .rgba,
        original.rgba
    );
    style.line_height = Some(64.);
    style.tracking = 6.;
    style.indent_left = 12.;
    style.indent_right = 20.;
    style.indent_first = 8.;
    style.alignment = TextAlign::Center;
    style.stroke_width = 1.5;
    let styled = fonts.raster_styled_text(&id, "Text\nAB", &style).unwrap();
    assert_ne!(styled.rgba, original.rgba);
    assert!(styled.height > original.height);
    assert!(bounds(&styled)[0] > bounds(&original)[0]);
    let encoded = serde_json::to_vec(&style).unwrap();
    let decoded: TextStyle = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded, style);
    drop(fonts);
    let mut reopened = FontStore::new(root.path()).unwrap();
    let again = reopened
        .raster_styled_text(&id, "Text\nAB", &decoded)
        .unwrap();
    assert_eq!((again.width, again.height), (styled.width, styled.height));
    assert_eq!(again.rgba, styled.rgba);
}
#[test]
fn alignments_translate_glyphs_and_tracking_indentation_and_outline_affect_extent() {
    let root = tempfile::tempdir().unwrap();
    let mut fonts = FontStore::new(root.path()).unwrap();
    let id = FontStore::builtin_id();
    let mut style = TextStyle::new(24., 320);
    let left = fonts.raster_styled_text(&id, "ABC", &style).unwrap();
    style.alignment = TextAlign::Center;
    let center = fonts.raster_styled_text(&id, "ABC", &style).unwrap();
    style.alignment = TextAlign::Right;
    let right = fonts.raster_styled_text(&id, "ABC", &style).unwrap();
    let (l, c, r) = (bounds(&left), bounds(&center), bounds(&right));
    assert!(l[0] < c[0] && c[0] < r[0]);
    assert_eq!(l[2] - l[0], c[2] - c[0]);
    assert_eq!(l[2] - l[0], r[2] - r[0]);
    style.alignment = TextAlign::Left;
    style.tracking = 8.;
    let tracked = fonts.raster_styled_text(&id, "ABC", &style).unwrap();
    assert!(bounds(&tracked)[2] >= l[2] + 16);
    style.indent_left = 12.;
    style.indent_first = 8.;
    let indented = fonts.raster_styled_text(&id, "ABC", &style).unwrap();
    assert!(bounds(&indented)[0] >= l[0] + 20);
    style.stroke_width = 2.;
    let outlined = fonts.raster_styled_text(&id, "ABC", &style).unwrap();
    let ink = |r: &Raster| r.rgba.chunks_exact(4).map(|p| u64::from(p[3])).sum::<u64>();
    assert!(ink(&outlined) > ink(&indented));
    assert!(outlined
        .rgba
        .chunks_exact(4)
        .all(|p| p[3] == 0 || p[..3] == [255; 3]));
}
#[test]
fn invalid_styles_missing_fonts_glyphs_and_oversize_layouts_return_original_causes() {
    let root = tempfile::tempdir().unwrap();
    let mut fonts = FontStore::new(root.path()).unwrap();
    let id = FontStore::builtin_id();
    let style = TextStyle::new(24., 320);
    for mutate in [
        |s: &mut TextStyle| s.font_size = f32::NAN,
        |s: &mut TextStyle| s.line_height = Some(f32::INFINITY),
        |s: &mut TextStyle| s.tracking = f32::NAN,
        |s: &mut TextStyle| s.indent_right = 320.,
        |s: &mut TextStyle| s.stroke_width = 33.,
    ] {
        let mut invalid = style.clone();
        mutate(&mut invalid);
        assert!(fonts.raster_styled_text(&id, "A", &invalid).is_err());
    }
    let cause = fonts
        .raster_styled_text(&id, "中文", &style)
        .err()
        .expect("missing glyph")
        .to_string();
    assert!(cause.contains("U+4E2D"), "{cause}");
    assert!(fonts.raster_styled_text("missing", "A", &style).is_err());
    let mut tall = style.clone();
    tall.line_height = Some(1024.);
    assert!(fonts
        .raster_styled_text(&id, &"A\n".repeat(20), &tall)
        .err()
        .expect("oversize raster")
        .to_string()
        .contains("limits"));
    assert!(fonts
        .raster_styled_text(&id, &"A".repeat(4097), &style)
        .is_err());
    // Failure cannot poison the font/cache or alter a subsequent healthy result.
    assert_eq!(
        fonts.raster_styled_text(&id, "ABC", &style).unwrap().rgba,
        fonts.raster_text(&id, "ABC", 24., 320).unwrap().rgba
    );
}
#[test]
fn crlf_tabs_empty_paragraphs_and_negative_tracking_are_explicit_layout_inputs() {
    let root = tempfile::tempdir().unwrap();
    let mut fonts = FontStore::new(root.path()).unwrap();
    let id = FontStore::builtin_id();
    let mut style = TextStyle::new(24., 160);
    style.line_height = Some(40.);
    style.tracking = -2.;
    let lf = fonts.raster_styled_text(&id, "A\tB\n\nC", &style).unwrap();
    let crlf = fonts
        .raster_styled_text(&id, "A\tB\r\n\r\nC", &style)
        .unwrap();
    assert_eq!(
        (lf.width, lf.height, lf.rgba),
        (crlf.width, crlf.height, crlf.rgba)
    );
}
