use motion_ui::TextAtlas;

#[test]
fn a_glyph_taller_than_the_initial_atlas_grows_before_writing_and_keeps_old_texels() {
    let mut atlas = TextAtlas::from_system_language(12., false).expect("installed UI font");
    let first = atlas.glyph('W');
    assert!(first.uv[2] > first.uv[0]);
    let initial = atlas.config();
    let old_pixels = atlas.pixel_data().to_vec();
    let large = atlas.glyph_at('W', initial.height as f32 * 2.);
    let grown = atlas.config();
    let height = (large.uv[3] - large.uv[1]) * grown.height as f32;
    assert!(
        height > initial.height as f32,
        "fixture must exceed the original atlas height"
    );
    assert!(grown.height > initial.height);
    assert!(large
        .uv
        .into_iter()
        .all(|value| (0. ..=1.).contains(&value)));
    assert_eq!(
        atlas.pixel_data().len(),
        grown.width as usize * grown.height as usize * 4
    );

    // Existing glyph coordinates may be renormalized, but their texels and
    // location in pixel space must remain identical after the growth.
    let retained = atlas.glyph('W');
    for i in 0..4 {
        let old_extent = if i % 2 == 0 {
            initial.width
        } else {
            initial.height
        };
        let new_extent = if i % 2 == 0 {
            grown.width
        } else {
            grown.height
        };
        assert_eq!(
            first.uv[i] * old_extent as f32,
            retained.uv[i] * new_extent as f32
        );
    }
    let left = (first.uv[0] * initial.width as f32) as usize;
    let right = (first.uv[2] * initial.width as f32) as usize;
    let top = (first.uv[1] * initial.height as f32) as usize;
    let bottom = (first.uv[3] * initial.height as f32) as usize;
    for y in top..bottom {
        assert_eq!(
            &old_pixels
                [(y * initial.width as usize + left) * 4..(y * initial.width as usize + right) * 4],
            &atlas.pixel_data()
                [(y * grown.width as usize + left) * 4..(y * grown.width as usize + right) * 4]
        );
    }
    let large_left = (large.uv[0] * grown.width as f32) as usize;
    let large_right = (large.uv[2] * grown.width as f32) as usize;
    let large_top = (large.uv[1] * grown.height as f32) as usize;
    let large_bottom = (large.uv[3] * grown.height as f32) as usize;
    assert!(
        (large_top..large_bottom).any(|y| {
            atlas.pixel_data()[(y * grown.width as usize + large_left) * 4
                ..(y * grown.width as usize + large_right) * 4]
                .iter()
                .any(|byte| *byte != 0)
        }),
        "the large glyph must actually be rasterized"
    );
}

#[test]
fn invalid_and_oversized_glyph_requests_keep_existing_pixels_and_allow_recovery() {
    let mut atlas = TextAtlas::from_system_language(12., false).expect("installed UI font");
    let valid = atlas.glyph('W');
    assert!(valid.uv[2] > valid.uv[0]);
    let before = atlas.pixel_data().to_vec();
    let config = atlas.config();
    for size in [
        0.,
        -1.,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::MAX,
    ] {
        assert_eq!(atlas.glyph_at('W', size).uv, [0.; 4], "size={size}");
        assert_eq!(atlas.pixel_data(), before);
        assert_eq!(
            [atlas.config().width, atlas.config().height],
            [config.width, config.height]
        );
    }
    assert_eq!(atlas.glyph('W').uv, valid.uv);
    let recovered = atlas.glyph('Z');
    assert!(recovered.uv[2] > recovered.uv[0]);
    assert!(recovered.uv[3] > recovered.uv[1]);
}
