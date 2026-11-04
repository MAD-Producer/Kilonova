use motion_ui::text::shaping::{Direction, RunShaper, ShapeError, MAX_RUN_BYTES};
use std::sync::Arc;

fn fixture() -> RunShaper {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    let id = db
        .query(&fontdb::Query {
            families: &[fontdb::Family::SansSerif],
            ..Default::default()
        })
        .expect("installed sans-serif UI font");
    db.with_face_data(id, |bytes, index| RunShaper::new(Arc::from(bytes), index))
        .expect("font bytes")
        .expect("valid system font")
}

fn signature(shaper: &mut RunShaper, text: &str) -> (Vec<(u32, [i32; 4])>, [i64; 2]) {
    let run = shaper.shape(text, Direction::LeftToRight).unwrap();
    assert_eq!(run.glyphs.len(), run.positions.len());
    assert!(run.units_per_em > 0);
    let glyphs = run
        .glyphs
        .iter()
        .zip(run.positions)
        .map(|(g, p)| {
            (
                g.glyph_id,
                [p.x_advance, p.y_advance, p.x_offset, p.y_offset],
            )
        })
        .collect();
    (glyphs, run.advance())
}

#[test]
fn canonical_combining_sequences_have_the_same_positioned_glyphs() {
    let mut shaper = fixture();
    for (composed, decomposed) in [("é", "e\u{301}"), ("Å", "A\u{30a}"), ("ñ", "n\u{303}")] {
        let actual = signature(&mut shaper, decomposed);
        let expected = signature(&mut shaper, composed);
        assert_eq!(actual, expected);
        assert!(
            actual.0.iter().any(|(id, _)| *id != 0),
            "fixture must map real glyphs"
        );
        assert!(actual.1[0] > 0);
    }
}

#[test]
fn combining_cluster_ids_are_source_boundaries_and_reusing_the_buffer_is_stable() {
    let mut shaper = fixture();
    let text = "q\u{301} A\u{30a}";
    let expected = signature(&mut shaper, text);
    for _ in 0..100 {
        assert_eq!(signature(&mut shaper, text), expected);
    }
    let run = shaper.shape(text, Direction::LeftToRight).unwrap();
    for glyph in run.glyphs {
        assert!(text.is_char_boundary(glyph.cluster as usize));
        assert!(
            [0, 3, 4].contains(&glyph.cluster),
            "marks remain with the preceding base"
        );
    }
}

#[test]
fn invalid_font_direction_and_long_runs_return_context_without_echoing_text() {
    let error = match RunShaper::new(Arc::from(&b"invalid"[..]), 7) {
        Ok(_) => panic!("invalid font accepted"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        ShapeError::InvalidFont {
            face_index: 7,
            bytes: 7
        }
    ));
    let mut shaper = fixture();
    let before = signature(&mut shaper, "Stable");
    let secret = "s".repeat(MAX_RUN_BYTES + 1);
    let error = match shaper.shape(&secret, Direction::LeftToRight) {
        Ok(_) => panic!("unbounded text accepted"),
        Err(error) => error,
    };
    assert!(matches!(error, ShapeError::RunTooLong { .. }));
    assert!(!error.to_string().contains(&secret));
    assert!(matches!(
        shaper.shape("x", Direction::Invalid),
        Err(ShapeError::InvalidDirection)
    ));
    assert_eq!(signature(&mut shaper, "Stable"), before);
    let empty = shaper.shape("", Direction::LeftToRight).unwrap();
    assert!(empty.glyphs.is_empty());
    assert_eq!(empty.advance(), [0, 0]);
}
