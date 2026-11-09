use super::*;

fn fixtures() -> (fontdb::Database, fontdb::ID, fontdb::ID) {
    let mut database = fontdb::Database::new();
    database.load_system_fonts();
    let find = |families: &[&str], chinese: bool| {
        families.iter().find_map(|family| {
            let id = database.query(&fontdb::Query {
                families: &[fontdb::Family::Name(family)],
                ..Default::default()
            })?;
            database.with_face_data(id, |data, index| {
                let font = ab_glyph::FontVec::try_from_vec_and_index(data.to_vec(), index).ok()?;
                (font.glyph_id('A').0 != 0 && (font.glyph_id('中').0 != 0) == chinese).then_some(())
            })??;
            Some(id)
        })
    };
    let latin = find(
        &["Arial", "Helvetica Neue", "Segoe UI", "DejaVu Sans"],
        false,
    )
    .expect("an installed Latin-only fixture font is required");
    let chinese = find(
        &[
            "Microsoft YaHei",
            "Microsoft YaHei UI",
            "PingFang SC",
            "Heiti SC",
            "Hiragino Sans GB",
            "Songti SC",
            "Noto Sans CJK SC",
            "WenQuanYi Micro Hei",
        ],
        true,
    )
    .expect("an installed CJK fixture font is required");
    (database, latin, chinese)
}

fn assert_rendered(atlas: &mut TextAtlas) {
    let label = "此版本暂不支持视频导出 所选关键帧已失效，请重新选择";
    for character in label.chars().filter(|c| !c.is_whitespace()) {
        assert!(
            atlas.supports(character),
            "family={:?} character={character}",
            atlas.selected_family
        );
        let glyph = atlas.glyph_at(character, 11.);
        assert!(
            glyph.uv[2] > glyph.uv[0] && glyph.uv[3] > glyph.uv[1],
            "family={:?} character={character} has no bitmap",
            atlas.selected_family
        );
    }
    let mut paint = crate::paint::PaintList::default();
    let style = paint
        .styles()
        .resolve("label.small.muted", Default::default())
        .unwrap();
    crate::style::text_style(
        &mut paint,
        atlas,
        crate::Rect::new(0., 0., 900., 22.),
        label,
        style,
        crate::ui::Align::Left,
    );
    assert_eq!(
        paint.glyphs.len(),
        label.chars().filter(|c| !c.is_whitespace()).count()
    );
}

#[test]
fn automatic_chinese_selection_searches_past_a_usable_latin_candidate() {
    let (database, latin, chinese) = fixtures();
    let (font, family, index) = select_candidate(&database, [latin, chinese], true).selected.unwrap();
    assert_ne!(
        font.glyph_id('中').0,
        0,
        "selected {family:?} despite an installed CJK candidate"
    );
    let expected = database.face(chinese).unwrap().families[0].0.clone();
    assert_eq!(family, expected);
    let mut atlas = TextAtlas::new(font, index, 12., Vec::new()).expect("indexed fixture font");
    atlas.configure_family(family);
    assert_rendered(&mut atlas);
}

#[test]
fn latin_preference_and_missing_coverage_keep_the_existing_fallback() {
    let (database, latin, chinese) = fixtures();
    let expected = database.face(latin).unwrap().families[0].0.clone();
    let (_, family, _) = select_candidate(&database, [latin, chinese], false).selected.unwrap();
    assert_eq!(family, expected);
    let (font, family, _) = select_candidate(&database, [latin], true).selected.unwrap();
    assert_eq!(family, expected);
    assert_eq!(font.glyph_id('中').0, 0);
    assert!(select_candidate(&database, [], true).selected.is_none());
}

#[test]
fn actual_system_chinese_and_explicit_latin_atlases_render_complete_notices() {
    let (database, latin, _) = fixtures();
    let family = database.face(latin).unwrap().families[0].0.clone();
    let mut automatic = TextAtlas::from_system_language(12., true).unwrap();
    assert_rendered(&mut automatic);
    let mut explicit = TextAtlas::from_family(&family, 12., true).unwrap();
    assert_eq!(explicit.selected_family, family);
    assert_rendered(&mut explicit);
}

#[test]
fn collection_face_index_is_shared_by_rasterization_and_shaping() {
    let mut database = fontdb::Database::new();
    database.load_system_fonts();
    let mut checked = 0;
    for face in database.faces().filter(|face| face.index > 0) {
        let Some((font, family, index)) = select_candidate(&database, [face.id], false).selected else {
            continue;
        };
        assert_eq!(index, face.index);
        let atlas = TextAtlas::new(font, index, 12., vec![]).expect("indexed collection font");
        for character in ['A', '0', 'é', '中'] {
            assert_eq!(
                atlas.shaper.nominal_glyph(character).unwrap_or(0),
                u32::from(atlas.font.glyph_id(character).0),
                "family={family} index={index} character={character}"
            );
        }
        checked += 1;
    }
    assert!(
        checked > 0,
        "installed CJK fonts must supply a nonzero collection face"
    );
    println!("matched_collection_faces={checked}");
}

#[test]
fn primary_and_fallback_coverage_use_the_same_font_as_their_rasterizer() {
    let (database, latin, _) = fixtures();
    let family = database.face(latin).unwrap().families[0].0.clone();
    let atlas = TextAtlas::from_family(&family, 12., true).unwrap();
    assert!(atlas.fallback_shaper.is_some());
    for character in "Aé中很长的属性名称".chars() {
        assert_eq!(
            atlas
                .shaping_face(character)
                .nominal_glyph(character)
                .unwrap_or(0),
            u32::from(atlas.face(character).glyph_id(character).0)
        );
        assert!(atlas.supports(character));
    }
}
