//! Glyph atlas for shell chrome. Windows uses grid-fitted grayscale glyphs
//! from the selected installed face. Rasterization is per glyph; complex-script
//! shaping and bidirectional layout are not yet supported.

use ab_glyph::{Font, FontArc, PxScale, PxScaleFont, ScaleFont};
#[cfg(target_os = "windows")]
mod windows;

/// One rendered glyph in the atlas.
#[derive(Clone, Copy)]
pub struct GlyphEntry {
    /// Atlas rectangle in normalised coordinates.
    pub uv: [f32; 4],
    /// Offset from the pen position to the glyph's top-left, in pixels.
    pub offset: [f32; 2],
    pub advance: f32,
}

/// Atlas geometry.
#[derive(Clone, Copy)]
pub struct AtlasConfig {
    pub width: u32,
    pub height: u32,
    pub padding: u32,
}

impl Default for AtlasConfig {
    fn default() -> Self {
        Self {
            width: 1024,
            height: 1024,
            padding: 1,
        }
    }
}

/// CPU-side glyph cache plus the coverage bitmap uploaded to the GPU.
///
/// The bitmap holds white premultiplied coverage in every channel; the shader
/// multiplies it by the per-glyph colour, so a glyph only ever costs one texel
/// read regardless of how many times it is drawn.
pub struct TextAtlas {
    font: PxScaleFont<FontArc>,
    /// Font size in pixels per em, as used by the design and stylesheet.
    font_size: f32,
    fallback: Option<PxScaleFont<FontArc>>,
    config: AtlasConfig,
    pixels: Vec<u8>,
    entries: std::collections::HashMap<(char, u32, u32), GlyphEntry>,
    pixel_ratio: f32,
    pub selected_family: String,
    #[cfg(target_os = "windows")]
    hinted: Option<windows::Rasterizer>,
    /// Pen position of the next glyph in the atlas, in texels.
    cursor: [f32; 2],
    row_height: u32,
    pub line_height: f32,
    pub ascent: f32,
    dirty: bool,
    /// Families discovered on this machine, for the diagnostics panel.
    pub families: Vec<String>,
}

impl TextAtlas {
    /// Build an atlas from the system UI font at `scale` logical pixels.
    pub fn from_system(scale: f32) -> Result<Self, String> {
        let (font, families, family) = load_system_font(false)?;
        let mut atlas = Self::new(font, scale, families);
        atlas.configure_family(family);
        Ok(atlas)
    }

    pub fn from_system_language(scale: f32, chinese: bool) -> Result<Self, String> {
        Self::from_family("system-ui", scale, chinese)
    }
    pub fn from_family(family: &str, scale: f32, chinese: bool) -> Result<Self, String> {
        let (font, families, selected) = load_preferred_font(chinese, Some(family))?;
        let mut atlas = Self::new(font, scale, families);
        atlas.configure_family(selected);
        if atlas.font.font.glyph_id('中').0 == 0 {
            if let Ok((fallback, _, _)) = load_system_font(true) {
                atlas.fallback = Some(PxScaleFont {
                    scale: em_scale(&fallback, scale),
                    font: fallback,
                });
            }
        }
        Ok(atlas)
    }
    pub fn font_size(&self) -> f32 {
        self.font_size
    }
    fn configure_family(&mut self, family: String) {
        #[cfg(target_os = "windows")]
        {
            self.hinted = windows::Rasterizer::new(&family);
        }
        self.selected_family = family;
    }
    pub fn pixel_ratio(&self) -> f32 {
        self.pixel_ratio
    }
    pub fn set_pixel_ratio(&mut self, ratio: f32) {
        if ratio.is_finite() && ratio > 0. {
            self.pixel_ratio = ratio.clamp(0.5, 4.);
        }
    }
    pub fn supports(&self, character: char) -> bool {
        self.face(character).glyph_id(character).0 != 0
    }
    pub fn new(font: FontArc, scale: f32, families: Vec<String>) -> Self {
        let font = PxScaleFont {
            scale: em_scale(&font, scale),
            font,
        };
        let line_height = font.height().max(scale);
        let ascent = font.ascent();
        let config = AtlasConfig::default();
        let pixels = vec![0u8; (config.width * config.height * 4) as usize];
        Self {
            font,
            font_size: scale,
            fallback: None,
            config,
            pixels,
            entries: std::collections::HashMap::new(),
            pixel_ratio: 1.,
            selected_family: "custom".into(),
            #[cfg(target_os = "windows")]
            hinted: None,
            cursor: [0.0, 0.0],
            row_height: 0,
            line_height,
            ascent,
            dirty: true,
            families,
        }
    }

    /// Scale the rasterisation size, for example after a display scale change.
    pub fn set_scale(&mut self, scale: f32) {
        let current = self.font_size;
        if (scale - current).abs() < f32::EPSILON {
            return;
        }
        self.font_size = scale;
        self.font.scale = em_scale(&self.font.font, scale);
        if let Some(fallback) = &mut self.fallback {
            fallback.scale = em_scale(&fallback.font, scale);
        }
        self.ascent = self.font.ascent();
        self.line_height = self.font.height().max(scale);
        self.clear();
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.pixels.fill(0);
        self.cursor = [0.0, 0.0];
        self.row_height = 0;
        self.dirty = true;
    }

    pub fn pixel_data(&self) -> &[u8] {
        &self.pixels
    }

    pub fn config(&self) -> AtlasConfig {
        self.config
    }

    /// True when new glyphs were rasterised since the last upload.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// Advance width of a character, without rasterising it.
    ///
    /// Unmapped code points take a fixed width so a layout never collapses when
    /// a label contains a character the system font lacks.
    fn face(&self, character: char) -> &PxScaleFont<FontArc> {
        if self.font.glyph_id(character).0 == 0 {
            if let Some(fallback) = &self.fallback {
                if fallback.glyph_id(character).0 != 0 {
                    return fallback;
                }
            }
        }
        &self.font
    }
    fn advance(&self, character: char) -> f32 {
        let font = self.face(character);
        let id = font.glyph_id(character);
        if id.0 == 0 {
            return font.scale.x * 0.5;
        }
        font.h_advance(id)
    }

    /// Rasterise a character if it is not already cached.
    pub fn glyph(&mut self, character: char) -> GlyphEntry {
        self.glyph_at(character, self.font_size)
    }
    /// Rasterize at the requested em size rather than enlarging a small bitmap.
    pub fn glyph_at(&mut self, character: char, size: f32) -> GlyphEntry {
        let key = (character, size.to_bits(), self.pixel_ratio.to_bits());
        if let Some(entry) = self.entries.get(&key) {
            return *entry;
        }
        let ratio = size / self.font_size;
        let advance = self.advance(character) * ratio;
        let blank = GlyphEntry {
            uv: [0.0; 4],
            offset: [0.0, 0.0],
            advance,
        };
        let font = self.face(character);
        let id = font.glyph_id(character);
        if id.0 == 0 {
            self.entries.insert(key, blank);
            return blank;
        }
        let mut glyph = font.scaled_glyph(character);
        glyph.scale = PxScale::from(font.scale.y * ratio * self.pixel_ratio);
        let Some(outlined) = font.outline_glyph(glyph) else {
            self.entries.insert(key, blank);
            return blank;
        };
        // Pixel bounds are integers and exactly match what `draw` reports, so
        // the atlas rectangle and the draw coordinates agree without rounding.
        let bounds = outlined.px_bounds();
        #[cfg(target_os = "windows")]
        let hinted = if self.font.glyph_id(character).0 != 0 {
            self.hinted
                .as_mut()
                .and_then(|r| r.glyph(id.0, size * self.pixel_ratio))
        } else {
            None
        };
        #[cfg(target_os = "windows")]
        let (width, height, offset) = hinted.as_ref().map_or(
            (
                (bounds.max.x - bounds.min.x) as u32,
                (bounds.max.y - bounds.min.y) as u32,
                [bounds.min.x, bounds.min.y],
            ),
            |b| (b.width, b.height, b.offset),
        );
        #[cfg(not(target_os = "windows"))]
        let (width, height, offset) = (
            (bounds.max.x - bounds.min.x) as u32,
            (bounds.max.y - bounds.min.y) as u32,
            [bounds.min.x, bounds.min.y],
        );
        if width == 0 || height == 0 {
            // Whitespace advances without a bitmap.
            self.entries.insert(key, blank);
            return blank;
        }
        let padding = self.config.padding;
        if self.cursor[0] + width as f32 + padding as f32 > self.config.width as f32 {
            self.cursor[0] = 0.0;
            self.cursor[1] += self.row_height as f32;
            self.row_height = 0;
        }
        if self.cursor[1] + height as f32 + padding as f32 > self.config.height as f32 {
            // Atlas full. Panel text is short, so this only happens with an
            // unusual font or a very large UI scale; repacking is cheaper than
            // growing the atlas at runtime.
            self.clear();
        }
        self.row_height = self.row_height.max(height + padding);
        let x = self.cursor[0] as u32;
        let y = self.cursor[1] as u32;
        let stride = self.config.width as usize;
        let pixels = &mut self.pixels;
        let mut write = |column: u32, row: u32, coverage: f32| {
            let local_column = column;
            let local_row = row;
            if local_column >= width || local_row >= height {
                return;
            }
            let index =
                ((y + local_row) as usize * stride + x as usize + local_column as usize) * 4;
            pixels[index..index + 4].fill((coverage.clamp(0.0, 1.0) * 255.0).round() as u8);
        };
        #[cfg(target_os = "windows")]
        if let Some(bitmap) = hinted {
            for (i, c) in bitmap.coverage.into_iter().enumerate() {
                write(i as u32 % width, i as u32 / width, c as f32 / 255.);
            }
        } else {
            outlined.draw(&mut write);
        }
        #[cfg(not(target_os = "windows"))]
        outlined.draw(&mut write);
        self.cursor[0] += width as f32 + padding as f32;
        let entry = GlyphEntry {
            uv: [
                x as f32 / self.config.width as f32,
                y as f32 / self.config.height as f32,
                (x + width) as f32 / self.config.width as f32,
                (y + height) as f32 / self.config.height as f32,
            ],
            // Atlas rows advance downward; glyph space advances upward.
            offset: offset.map(|v| v / self.pixel_ratio),
            advance,
        };
        self.entries.insert(key, entry);
        self.dirty = true;
        entry
    }

    /// Total advance width of a string in logical pixels.
    pub fn measure(&mut self, text: &str) -> f32 {
        text.chars().map(|c| self.advance(c)).sum()
    }

    /// Truncate to fit `width`, appending an ellipsis when characters are lost.
    pub fn ellipsize(&mut self, text: &str, width: f32) -> String {
        if self.measure(text) <= width {
            return text.to_owned();
        }
        let ellipsis_width = self.advance('\u{2026}');
        let mut result = String::new();
        let mut used = 0.0;
        for character in text.chars() {
            let advance = self.advance(character);
            if used + advance + ellipsis_width > width {
                break;
            }
            result.push(character);
            used += advance;
        }
        result.push('\u{2026}');
        result
    }
}

// ab_glyph's PxScale is ascender-to-descender height, not pixels per em.
// Design tools and CSS use em size, so convert once for each selected face.
fn em_scale(font: &FontArc, pixels: f32) -> PxScale {
    PxScale::from(
        pixels * font.height_unscaled() / font.units_per_em().unwrap_or(font.height_unscaled()),
    )
}

/// Prefer a platform UI face, then fall back to any installed font.
fn load_system_font(chinese: bool) -> Result<(FontArc, Vec<String>, String), String> {
    load_preferred_font(chinese, None)
}
fn load_preferred_font(
    chinese: bool,
    family: Option<&str>,
) -> Result<(FontArc, Vec<String>, String), String> {
    let mut database = fontdb::Database::new();
    database.load_system_fonts();
    let families: Vec<String> = database
        .faces()
        .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
        .filter(|name| !name.is_empty())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut preferred = Vec::new();
    if let Some(family) = family.filter(|f| *f != "system-ui") {
        preferred.push(family);
    }
    if chinese {
        preferred.extend([
            "Microsoft YaHei UI",
            "Microsoft YaHei",
            "PingFang SC",
            "Noto Sans CJK SC",
            "WenQuanYi Micro Hei",
        ]);
    }
    preferred.extend([
        "Segoe UI",
        "SF Pro Text",
        "Helvetica Neue",
        "Inter",
        "Roboto",
        "Noto Sans",
        "DejaVu Sans",
        "Arial",
    ]);
    let mut candidates: Vec<fontdb::ID> = Vec::new();
    for wanted in preferred {
        if let Some(id) = database.query(&fontdb::Query {
            families: &[fontdb::Family::Name(wanted)],
            ..fontdb::Query::default()
        }) {
            candidates.push(id);
        }
    }
    // A monospaced face keeps numeric columns aligned if it is all we have.
    candidates.extend(
        database
            .faces()
            .filter(|face| face.monospaced)
            .map(|face| face.id),
    );
    candidates.extend(database.faces().map(|face| face.id));
    for id in candidates {
        let font = database.with_face_data(id, |data, index| {
            ab_glyph::FontVec::try_from_vec_and_index(data.to_vec(), index).map(FontArc::new)
        });
        if let Some(Ok(font)) = font {
            let family = database
                .face(id)
                .and_then(|f| f.families.first())
                .map(|f| f.0.clone())
                .unwrap_or_else(|| "unknown".into());
            return Ok((font, families, family));
        }
    }
    Err("no usable system font was found".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The atlas needs a real font, so these tests use whichever system face the
    /// host provides and skip when none is installed.
    fn fixture() -> Option<TextAtlas> {
        TextAtlas::from_system(13.0).ok()
    }

    #[test]
    fn every_glyph_lands_inside_the_atlas() {
        let Some(mut atlas) = fixture() else {
            return;
        };
        let (width, height) = (atlas.config().width, atlas.config().height);
        for character in "Motion Studio 0123456789 /.".chars() {
            let entry = atlas.glyph(character);
            assert!(entry.uv[0] >= 0.0 && entry.uv[2] <= 1.0, "{character}");
            assert!(entry.uv[1] >= 0.0 && entry.uv[3] <= 1.0, "{character}");
            assert!(
                (entry.uv[2] - entry.uv[0]) * width as f32 <= width as f32,
                "{character} overflows"
            );
            assert!(
                (entry.uv[3] - entry.uv[1]) * height as f32 <= height as f32,
                "{character} overflows"
            );
        }
    }

    #[test]
    fn whitespace_advances_without_consuming_atlas_texels() {
        let Some(mut atlas) = fixture() else {
            return;
        };
        let space = atlas.glyph(' ');
        assert_eq!(space.uv, [0.0; 4]);
        assert!(space.advance > 0.0);
    }

    #[test]
    fn measuring_does_not_rasterise() {
        let Some(mut atlas) = fixture() else {
            return;
        };
        atlas.take_dirty();
        let width = atlas.measure("Motion Studio");
        assert!(width > 0.0);
        assert!(!atlas.take_dirty(), "measure must not dirty the atlas");
    }

    #[test]
    fn ellipsize_fits_within_the_requested_width() {
        let Some(mut atlas) = fixture() else {
            return;
        };
        let limit = 40.0;
        let shown = atlas.ellipsize("Effect Controls", limit);
        assert!(shown.ends_with('\u{2026}'));
        assert!(atlas.measure(&shown) <= limit, "{shown:?}");
        assert_eq!(atlas.ellipsize("ab", 10_000.0), "ab");
    }

    #[test]
    fn the_atlas_reports_dirty_only_after_new_glyphs() {
        let Some(mut atlas) = fixture() else {
            return;
        };
        assert!(atlas.take_dirty());
        atlas.glyph('A');
        assert!(atlas.take_dirty());
        atlas.glyph('A');
        assert!(!atlas.take_dirty());
    }

    #[test]
    fn changing_scale_repacks_the_atlas() {
        let Some(mut atlas) = fixture() else {
            return;
        };
        atlas.glyph('A');
        atlas.take_dirty();
        atlas.set_scale(26.0);
        assert!(atlas.take_dirty());
        assert_eq!(atlas.entries.len(), 0);
        assert!(atlas.line_height >= 26.0);
    }

    #[test]
    fn dpi_rasterization_preserves_logical_metrics_and_existing_uvs() {
        let Some(mut atlas) = fixture() else { return; };
        let width = atlas.measure("Preview 123");
        let first = atlas.glyph('A');
        atlas.set_pixel_ratio(2.0);
        let second = atlas.glyph('A');
        assert_eq!(atlas.measure("Preview 123"), width);
        assert_eq!(first.advance, second.advance);
        assert!(second.uv[2] - second.uv[0] > first.uv[2] - first.uv[0]);
        atlas.set_pixel_ratio(1.0);
        assert_eq!(atlas.glyph('A').uv, first.uv);
    }
}
