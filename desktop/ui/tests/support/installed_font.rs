use std::sync::Arc;

pub fn source(database: &fontdb::Database) -> (Arc<[u8]>, u32) {
    use ab_glyph::Font;
    let preferred = database.query(&fontdb::Query {
        families: &[fontdb::Family::SansSerif],
        ..Default::default()
    });
    // Generic family aliases are not necessarily configured by fontdb on Linux.
    // The tests need a readable face with these glyphs, not a particular family.
    let candidates = preferred
        .into_iter()
        .chain(database.faces().map(|face| face.id));
    for id in candidates {
        let source = database
            .with_face_data(id, |bytes, index| {
                let font = ab_glyph::FontVec::try_from_vec_and_index(bytes.to_vec(), index).ok()?;
                "Property office AVéÅñq\u{301} A\u{30a}"
                    .chars()
                    .all(|c| font.glyph_id(c).0 != 0)
                    .then(|| (Arc::from(bytes), index))
            })
            .flatten();
        if let Some(source) = source {
            return source;
        }
    }
    panic!("no readable installed font covers the Latin/combining test fixture");
}

#[test]
fn installed_font_remains_usable_without_generic_sans_alias() {
    use ab_glyph::Font;
    let mut database = fontdb::Database::new();
    database.load_system_fonts();
    database.set_sans_serif_family("__motion_test_uninstalled_sans_alias__");
    assert!(
        database
            .query(&fontdb::Query {
                families: &[fontdb::Family::SansSerif],
                ..Default::default()
            })
            .is_none(),
        "control must reproduce the missing generic mapping"
    );
    let (bytes, index) = source(&database);
    let font = ab_glyph::FontVec::try_from_vec_and_index(bytes.to_vec(), index).unwrap();
    assert!("Property office AVéÅñq\u{301} A\u{30a}"
        .chars()
        .all(|c| font.glyph_id(c).0 != 0));
    let mut shaper = motion_ui::text::shaping::RunShaper::new(bytes, index).unwrap();
    let run = shaper
        .shape(
            "office AV",
            motion_ui::text::shaping::Direction::LeftToRight,
        )
        .unwrap();
    assert!(!run.glyphs.is_empty());
    assert!(run.glyphs.iter().all(|g| g.glyph_id != 0));
}
