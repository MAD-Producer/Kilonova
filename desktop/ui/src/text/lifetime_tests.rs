use super::*;

fn small_atlas() -> TextAtlas {
    let mut atlas =
        TextAtlas::from_system(14.).expect("installed font required for atlas regression");
    atlas.config = AtlasConfig {
        width: 32,
        height: 32,
        padding: 1,
    };
    atlas.pixels = vec![0; 32 * 32 * 4];
    atlas.clear();
    atlas
}

fn bitmap(atlas: &TextAtlas, entry: GlyphEntry) -> Vec<u8> {
    let width = atlas.config.width;
    let height = atlas.config.height;
    let [x, y, right, bottom] = entry.uv;
    let (x, y, right, bottom) = (
        (x * width as f32).round() as usize,
        (y * height as f32).round() as usize,
        (right * width as f32).round() as usize,
        (bottom * height as f32).round() as usize,
    );
    (y..bottom)
        .flat_map(|row| {
            atlas.pixels[(row * width as usize + x) * 4..(row * width as usize + right) * 4]
                .iter()
                .copied()
        })
        .collect()
}

#[test]
fn growth_preserves_cached_glyph_pixels_and_metrics() {
    let mut atlas = small_atlas();
    let first = atlas.glyph('W');
    let pixels = bitmap(&atlas, first);
    assert!(pixels.iter().any(|b| *b != 0));
    let old = atlas.config;
    for size in 15..80 {
        atlas.glyph_at('g', size as f32);
        if atlas.config.width > old.width {
            break;
        }
    }
    assert!(atlas.config.width > old.width);
    let cached = atlas.glyph('W');
    assert_eq!(cached.advance, first.advance);
    assert_eq!(cached.offset, first.offset);
    assert_eq!(bitmap(&atlas, cached), pixels);
    assert!(atlas.take_dirty());
    atlas.glyph('W');
    assert!(!atlas.take_dirty());
}

#[test]
fn a_glyph_wider_and_taller_than_the_initial_atlas_grows_before_writing() {
    let mut atlas = small_atlas();
    let entry = atlas.glyph_at('W', 120.);
    assert!(entry.uv[2] > entry.uv[0] && entry.uv[3] > entry.uv[1]);
    assert!(atlas.config.width > 32 && atlas.config.height > 32);
    assert!(entry.uv.iter().all(|v| (0. ..=1.).contains(v)));
    assert!(bitmap(&atlas, entry).iter().any(|b| *b != 0));
}

#[test]
fn capacity_refusal_preserves_existing_texels_and_does_not_cache_failures() {
    let mut atlas = small_atlas();
    atlas.max_dimension = 32;
    let first = atlas.glyph('W');
    for size in 15..40 {
        atlas.glyph_at('W', size as f32);
    }
    let before = atlas.pixels.clone();
    let entries = atlas.entries.len();
    let cursor = atlas.cursor;
    for _ in 0..120 {
        assert!(!atlas.reserve(31, 31));
        assert_eq!(atlas.glyph_at('W', 120.).uv, [0.; 4]);
    }
    assert_eq!(atlas.pixels, before);
    assert_eq!(atlas.entries.len(), entries);
    assert_eq!(atlas.cursor, cursor);
    assert_eq!(atlas.glyph('W').uv, first.uv);
    atlas.clear();
    assert!(atlas.glyph('W').uv[2] > 0.);
}

#[test]
fn invalid_and_extreme_sizes_never_reach_the_rasterizer_or_atlas_writer() {
    let mut atlas = small_atlas();
    let before = atlas.pixels.clone();
    for size in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1., 0., 1e20] {
        let entry = atlas.glyph_at('W', size);
        assert_eq!(entry.uv, [0.; 4]);
        assert!(entry.advance.is_finite());
    }
    assert!(!atlas.reserve(u32::MAX, 1));
    assert!(!atlas.reserve(1, u32::MAX));
    assert_eq!(atlas.pixels, before);
    assert!(atlas.entries.is_empty());
}
