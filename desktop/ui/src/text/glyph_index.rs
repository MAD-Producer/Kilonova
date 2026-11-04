//! Atlas insertion for glyph IDs produced by the shaping engine.
use super::{is_sequence_control, GlyphEntry, TextAtlas};
use ab_glyph::{Font, ScaleFont};
use std::fmt;
// Bounds retained keys even when a glyph does not occupy any atlas texels.
const MAX_INDEXED_ENTRIES: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FontSlot {
    Primary,
    Fallback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RasterFailure {
    InvalidSize,
    MissingFace,
    InvalidGlyph,
    MissingOutline,
    AtlasFull,
    CacheFull,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RasterError {
    pub face: FontSlot,
    pub glyph: u32,
    pub failure: RasterFailure,
}
impl fmt::Display for RasterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "UI glyph rasterization failed: face={:?} glyph={} cause={:?}",
            self.face, self.glyph, self.failure
        )
    }
}
impl std::error::Error for RasterError {}

impl TextAtlas {
    /// Rasterize an already shaped ID, never map it back to a Unicode scalar.
    /// `source` is a single source code point for bounded diagnostic context.
    /// Positioned run advances and offsets must still be applied by the caller.
    pub fn glyph_by_id(
        &mut self,
        face: FontSlot,
        glyph: u32,
        size: f32,
        source: char,
    ) -> Result<GlyphEntry, RasterError> {
        let error = |failure| RasterError {
            face,
            glyph,
            failure,
        };
        if !size.is_finite()
            || size <= 0.
            || !self.font_size.is_finite()
            || self.font_size <= 0.
            || size * self.pixel_ratio > self.max_dimension as f32
        {
            return Err(error(RasterFailure::InvalidSize));
        }
        let font = match face {
            FontSlot::Primary => &self.font,
            FontSlot::Fallback => self
                .fallback
                .as_ref()
                .ok_or_else(|| error(RasterFailure::MissingFace))?,
        };
        let id = u16::try_from(glyph)
            .ok()
            .filter(|id| *id != 0 && usize::from(*id) < font.glyph_count())
            .ok_or_else(|| error(RasterFailure::InvalidGlyph))?;
        let key = (face, id, size.to_bits(), self.pixel_ratio.to_bits());
        if let Some(entry) = self.indexed_entries.get(&key) {
            return Ok(*entry);
        }
        if self.indexed_entries.len() >= MAX_INDEXED_ENTRIES {
            return Err(error(RasterFailure::CacheFull));
        }
        let ratio = size / self.font_size;
        let blank = GlyphEntry {
            uv: [0.; 4],
            offset: [0.; 2],
            advance: font.h_advance(ab_glyph::GlyphId(id)) * ratio,
        };
        let outline = ab_glyph::Glyph {
            id: ab_glyph::GlyphId(id),
            scale: ab_glyph::PxScale::from(font.scale.y * ratio * self.pixel_ratio),
            position: ab_glyph::point(0., 0.),
        };
        if font.outline_glyph(outline).is_none()
            && !source.is_whitespace()
            && !source.is_control()
            && !is_sequence_control(source)
            && font.glyph_id(' ').0 != id
        {
            return Err(error(RasterFailure::MissingOutline));
        }
        let font = font.clone();
        let entry = self
            .rasterize_glyph(font, id, size, blank, face == FontSlot::Primary, source)
            .ok_or_else(|| error(RasterFailure::AtlasFull))?;
        self.indexed_entries.insert(key, entry);
        Ok(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texels(atlas: &TextAtlas, entry: GlyphEntry) -> Vec<u8> {
        let c = atlas.config();
        let x0 = (entry.uv[0] * c.width as f32).round() as usize;
        let x1 = (entry.uv[2] * c.width as f32).round() as usize;
        let y0 = (entry.uv[1] * c.height as f32).round() as usize;
        let y1 = (entry.uv[3] * c.height as f32).round() as usize;
        (y0..y1)
            .flat_map(|y| {
                atlas.pixel_data()[(y * c.width as usize + x0) * 4..(y * c.width as usize + x1) * 4]
                    .to_vec()
            })
            .collect()
    }

    #[test]
    fn shaped_ids_and_scalar_rasterization_have_identical_texels_and_metrics() {
        let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
        for ratio in [1., 1.25, 1.5, 1.75, 2.] {
            atlas.set_pixel_ratio(ratio);
            for character in ['W', 'é', 'g'] {
                let id = atlas.shaper.nominal_glyph(character).expect("mapped glyph");
                let scalar = atlas.glyph_at(character, 18.);
                let indexed = atlas
                    .glyph_by_id(FontSlot::Primary, id, 18., character)
                    .unwrap();
                assert_eq!(scalar.advance, indexed.advance);
                assert_eq!(scalar.offset, indexed.offset);
                let expected = texels(&atlas, scalar);
                assert!(!expected.is_empty());
                assert!(expected.iter().any(|byte| *byte > 0));
                assert_eq!(texels(&atlas, indexed), expected);
                let retained = atlas
                    .glyph_by_id(FontSlot::Primary, id, 18., character)
                    .unwrap();
                assert_eq!(indexed.uv, retained.uv);
            }
        }
    }

    #[test]
    fn both_caches_keep_pixel_coordinates_and_texels_after_growth() {
        let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
        atlas.config.width = 32;
        atlas.config.height = 32;
        atlas.max_dimension = 256;
        atlas.pixels = vec![0; 32 * 32 * 4];
        let id = atlas.shaper.nominal_glyph('W').unwrap();
        let indexed = atlas.glyph_by_id(FontSlot::Primary, id, 12., 'W').unwrap();
        let scalar = atlas.glyph_at('g', 12.);
        let before = atlas.config();
        let indexed_pixels = texels(&atlas, indexed);
        let scalar_pixels = texels(&atlas, scalar);
        atlas.glyph_by_id(FontSlot::Primary, id, 110., 'W').unwrap();
        assert!(atlas.config().width > before.width);
        let after_indexed = atlas.glyph_by_id(FontSlot::Primary, id, 12., 'W').unwrap();
        let after_scalar = atlas.glyph_at('g', 12.);
        for (old, new) in [(indexed, after_indexed), (scalar, after_scalar)] {
            for i in 0..4 {
                let old_extent = if i % 2 == 0 {
                    before.width
                } else {
                    before.height
                };
                let c = atlas.config();
                let new_extent = if i % 2 == 0 { c.width } else { c.height };
                assert_eq!(old.uv[i] * old_extent as f32, new.uv[i] * new_extent as f32);
            }
        }
        assert_eq!(texels(&atlas, after_indexed), indexed_pixels);
        assert_eq!(texels(&atlas, after_scalar), scalar_pixels);
    }

    #[test]
    fn invalid_requests_return_typed_causes_without_mutating_the_atlas() {
        let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
        // The Latin entry point normally installs a CJK fallback. Construct
        // the absent-face state explicitly; keep the original assertion.
        atlas.fallback = None;
        atlas.fallback_shaper = None;
        let id = atlas.shaper.nominal_glyph('W').unwrap();
        let valid = atlas.glyph_by_id(FontSlot::Primary, id, 12., 'W').unwrap();
        let before = atlas.pixel_data().to_vec();
        for size in [0., -1., f32::NAN, f32::INFINITY, f32::MAX] {
            assert!(matches!(
                atlas.glyph_by_id(FontSlot::Primary, id, size, 'W'),
                Err(RasterError {
                    failure: RasterFailure::InvalidSize,
                    ..
                })
            ));
        }
        for glyph in [0, u32::MAX] {
            assert!(matches!(
                atlas.glyph_by_id(FontSlot::Primary, glyph, 12., 'W'),
                Err(RasterError {
                    failure: RasterFailure::InvalidGlyph,
                    ..
                })
            ));
        }
        assert!(matches!(
            atlas.glyph_by_id(FontSlot::Fallback, id, 12., 'W'),
            Err(RasterError {
                failure: RasterFailure::MissingFace,
                ..
            })
        ));
        assert_eq!(atlas.pixel_data(), before);
        assert_eq!(
            atlas
                .glyph_by_id(FontSlot::Primary, id, 12., 'W')
                .unwrap()
                .uv,
            valid.uv
        );
    }
    #[test]
    fn font_slots_have_distinct_cache_entries_even_for_the_same_glyph_number() {
        let mut database = fontdb::Database::new();
        database.load_system_fonts();
        let family = ["Arial", "Helvetica Neue", "Segoe UI", "DejaVu Sans"]
            .into_iter()
            .find(|family| {
                let Some(id) = database.query(&fontdb::Query {
                    families: &[fontdb::Family::Name(family)],
                    ..Default::default()
                }) else {
                    return false;
                };
                database
                    .with_face_data(id, |bytes, index| {
                        ab_glyph::FontVec::try_from_vec_and_index(bytes.to_vec(), index).is_ok_and(
                            |font| font.glyph_id('W').0 != 0 && font.glyph_id('中').0 == 0,
                        )
                    })
                    .unwrap_or(false)
            })
            .expect("installed Latin fixture font");
        let mut atlas = TextAtlas::from_family(family, 12., true).unwrap();
        let id = atlas.shaper.nominal_glyph('W').unwrap();
        assert!(atlas.fallback.is_some());
        atlas.glyph_by_id(FontSlot::Primary, id, 18., 'W').unwrap();
        atlas.glyph_by_id(FontSlot::Fallback, id, 18., 'W').unwrap();
        assert_eq!(atlas.indexed_entries.len(), 2);
        assert!(atlas.indexed_entries.contains_key(&(
            FontSlot::Primary,
            id as u16,
            18_f32.to_bits(),
            1_f32.to_bits()
        )));
        assert!(atlas.indexed_entries.contains_key(&(
            FontSlot::Fallback,
            id as u16,
            18_f32.to_bits(),
            1_f32.to_bits()
        )));
    }

    #[test]
    fn full_atlas_and_cache_refuse_before_writing_and_keep_cached_glyphs_usable() {
        let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
        atlas.config.width = 32;
        atlas.config.height = 32;
        atlas.max_dimension = 32;
        atlas.pixels = vec![0; 32 * 32 * 4];
        let id = atlas.shaper.nominal_glyph('W').unwrap();
        let retained = atlas.glyph_by_id(FontSlot::Primary, id, 12., 'W').unwrap();
        let before = atlas.pixel_data().to_vec();
        atlas.cursor = [0., 32.];
        assert!(matches!(
            atlas.glyph_by_id(FontSlot::Primary, id, 13., 'W'),
            Err(RasterError {
                failure: RasterFailure::AtlasFull,
                ..
            })
        ));
        assert_eq!(atlas.pixel_data(), before);
        for n in 0..MAX_INDEXED_ENTRIES {
            atlas
                .indexed_entries
                .entry((FontSlot::Primary, id as u16, n as u32, 0))
                .or_insert(retained);
        }
        assert!(matches!(
            atlas.glyph_by_id(FontSlot::Primary, id, 14., 'W'),
            Err(RasterError {
                failure: RasterFailure::CacheFull,
                ..
            })
        ));
        assert_eq!(atlas.pixel_data(), before);
        assert_eq!(
            atlas
                .glyph_by_id(FontSlot::Primary, id, 12., 'W')
                .unwrap()
                .uv,
            retained.uv
        );
        atlas.set_scale(13.);
        assert!(atlas.indexed_entries.is_empty());
        assert!(atlas.glyph_by_id(FontSlot::Primary, id, 13., 'W').is_ok());
    }

    #[test]
    fn space_is_a_normal_empty_glyph_and_other_missing_outlines_return_a_cause() {
        let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
        let space = atlas.shaper.nominal_glyph(' ').unwrap();
        let entry = atlas
            .glyph_by_id(FontSlot::Primary, space, 12., ' ')
            .unwrap();
        assert_eq!(entry.uv, [0.; 4]);
        assert!(entry.advance > 0.);
        let id = (1..atlas.font.glyph_count())
            .find(|id| {
                *id != space as usize
                    && atlas
                        .font
                        .outline_glyph(ab_glyph::Glyph {
                            id: ab_glyph::GlyphId(*id as u16),
                            scale: atlas.font.scale,
                            position: ab_glyph::point(0., 0.),
                        })
                        .is_none()
            })
            .expect("installed UI font contains a non-space empty glyph");
        assert!(matches!(
            atlas.glyph_by_id(FontSlot::Primary, id as u32, 12., 'W'),
            Err(RasterError {
                failure: RasterFailure::MissingOutline,
                ..
            })
        ));
    }
}
