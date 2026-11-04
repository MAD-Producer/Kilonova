//! Single-line layout in em units. Font choice never splits a grapheme; shaping
//! keeps adjacent compatible graphemes together, including Arabic joining.
//! This is the shared result intended for measurement, truncation and painting.
use super::{glyph_index::FontSlot, shaping, TextAtlas};
use shaping::{Direction, Script};
use std::{collections::HashMap, fmt, ops::Range, sync::Arc};
use unicode_bidi::{BidiInfo, Level};
use unicode_segmentation::UnicodeSegmentation;

const MAX_CACHE_ENTRIES: usize = 128;
const MAX_CACHE_BYTES: usize = 1024 * 1024;
const MAX_LAYOUT_GLYPHS: usize = 16 * 1024;

#[derive(Debug)]
pub enum LayoutError {
    TooLong { bytes: usize, limit: usize },
    MultipleLines,
    TooManyGlyphs { limit: usize },
    MissingFace,
    InvalidCluster { offset: usize, bytes: usize },
    Shape(shaping::ShapeError),
}
impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong { bytes, limit } => {
                write!(f, "UI line exceeds limit: bytes={bytes} limit={limit}")
            }
            Self::MultipleLines => {
                f.write_str("UI single-line layout received multiple paragraphs")
            }
            Self::TooManyGlyphs { limit } => {
                write!(f, "UI shaped glyph count exceeds limit={limit}")
            }
            Self::MissingFace => f.write_str("UI layout fallback font disappeared"),
            Self::InvalidCluster { offset, bytes } => write!(
                f,
                "UI shaping returned invalid source cluster: offset={offset} bytes={bytes}"
            ),
            Self::Shape(cause) => write!(f, "UI line shaping: {cause}"),
        }
    }
}
impl std::error::Error for LayoutError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Shape(cause) => Some(cause),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PositionedGlyph {
    pub face: FontSlot,
    pub id: u32,
    /// UTF-8 source byte offset; in visual order this need not increase.
    pub cluster: usize,
    /// Position and advance in em units, positive Y upward.
    pub position: [f32; 2],
    pub advance: [f32; 2],
    pub source: char,
}

#[derive(Debug)]
pub struct LineLayout {
    pub glyphs: Vec<PositionedGlyph>,
    pub advance: f32,
    pub grapheme_boundaries: Vec<usize>,
    /// Explicit unsupported glyphs, retained as ID 0, never scalar substitution.
    /// The drawing boundary must handle and observe this degradation.
    pub missing_glyphs: usize,
}

struct CachedLayout {
    value: Arc<LineLayout>,
    age: u64,
    bytes: usize,
}
#[derive(Default)]
pub(super) struct LayoutCache {
    entries: HashMap<Arc<str>, CachedLayout>,
    bytes: usize,
    clock: u64,
}
impl LayoutCache {
    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
        self.clock = 0;
    }
    fn tick(&mut self) -> u64 {
        if self.clock == u64::MAX {
            for entry in self.entries.values_mut() {
                entry.age = 0;
            }
            self.clock = 0;
        }
        self.clock += 1;
        self.clock
    }
    fn get(&mut self, text: &str) -> Option<Arc<LineLayout>> {
        let age = self.tick();
        self.entries.get_mut(text).map(|entry| {
            entry.age = age;
            Arc::clone(&entry.value)
        })
    }
    fn insert(&mut self, text: &str, value: Arc<LineLayout>) {
        let bytes = text.len()
            + value.glyphs.capacity() * size_of::<PositionedGlyph>()
            + value.grapheme_boundaries.capacity() * size_of::<usize>();
        if bytes > MAX_CACHE_BYTES {
            return;
        }
        while self.entries.len() >= MAX_CACHE_ENTRIES || self.bytes + bytes > MAX_CACHE_BYTES {
            let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.age)
                .map(|(key, _)| Arc::clone(key))
            else {
                break;
            };
            if let Some(old) = self.entries.remove(&oldest) {
                self.bytes -= old.bytes;
            }
        }
        let age = self.tick();
        self.entries
            .insert(Arc::from(text), CachedLayout { value, age, bytes });
        self.bytes += bytes;
    }
}

struct Cluster {
    range: Range<usize>,
    script: Script,
    face: FontSlot,
}
struct Run {
    range: Range<usize>,
    script: Script,
    face: FontSlot,
    level: Level,
}
fn strong(script: Script) -> bool {
    !matches!(script, Script::COMMON | Script::INHERITED | Script::UNKNOWN)
}
fn coverage(face: &shaping::RunShaper, text: &str) -> bool {
    text.chars()
        .filter(|c| !c.is_control() && !super::is_sequence_control(*c))
        .all(|c| face.nominal_glyph(c).is_some())
}

impl TextAtlas {
    /// Shape a bounded single line without rasterizing or dirtying the atlas.
    /// Cache hits share the retained layout and allocate no owned label string.
    pub fn layout_line(&mut self, text: &str) -> Result<Arc<LineLayout>, LayoutError> {
        if text.len() > shaping::MAX_RUN_BYTES {
            return Err(LayoutError::TooLong {
                bytes: text.len(),
                limit: shaping::MAX_RUN_BYTES,
            });
        }
        if text.contains(['\n', '\r', '\u{2028}', '\u{2029}']) {
            return Err(LayoutError::MultipleLines);
        }
        if let Some(layout) = self.layouts.get(text) {
            return Ok(layout);
        }
        let bidi = BidiInfo::new(text, None);
        if bidi.paragraphs.len() > 1 {
            return Err(LayoutError::MultipleLines);
        }
        let mut clusters: Vec<_> = text
            .grapheme_indices(true)
            .map(|(start, grapheme)| {
                let face = if coverage(&self.shaper, grapheme) {
                    FontSlot::Primary
                } else if self
                    .fallback_shaper
                    .as_ref()
                    .is_some_and(|face| coverage(face, grapheme))
                {
                    FontSlot::Fallback
                } else {
                    FontSlot::Primary
                };
                let script = grapheme
                    .chars()
                    .map(|c| harfrust::unicode::script(c as u32))
                    .find(|s| strong(*s))
                    .unwrap_or(Script::COMMON);
                Cluster {
                    range: start..start + grapheme.len(),
                    script,
                    face,
                }
            })
            .collect();
        let boundaries = clusters
            .iter()
            .map(|c| c.range.start)
            .chain([text.len()])
            .collect();
        let levels = bidi
            .paragraphs
            .first()
            .map(|para| bidi.reordered_levels(para, para.range.clone()))
            .unwrap_or_default();
        // Common/inherited characters take an adjacent strong script at the
        // same bidi level. Never merge strong scripts merely because they use
        // the same font, or shape each grapheme independently.
        let mut previous = None;
        for cluster in &mut clusters {
            let level = levels[cluster.range.start];
            if strong(cluster.script) {
                previous = Some((cluster.script, level));
            } else if let Some((script, prior_level)) = previous {
                if prior_level == level {
                    cluster.script = script;
                }
            }
        }
        let mut next = None;
        for cluster in clusters.iter_mut().rev() {
            let level = levels[cluster.range.start];
            if strong(cluster.script) {
                next = Some((cluster.script, level));
            } else if let Some((script, next_level)) = next {
                if next_level == level {
                    cluster.script = script;
                }
            }
        }
        let mut runs: Vec<Run> = Vec::new();
        for cluster in clusters {
            let level = levels[cluster.range.start];
            if let Some(last) = runs.last_mut().filter(|last| {
                last.face == cluster.face && last.script == cluster.script && last.level == level
            }) {
                last.range.end = cluster.range.end;
            } else {
                runs.push(Run {
                    range: cluster.range,
                    script: cluster.script,
                    face: cluster.face,
                    level,
                });
            }
        }
        let order = BidiInfo::reorder_visual(&runs.iter().map(|run| run.level).collect::<Vec<_>>());
        let mut glyphs = Vec::new();
        let mut pen = [0., 0.];
        let mut missing = 0;
        for index in order {
            let run = &runs[index];
            let shaper = match run.face {
                FontSlot::Primary => &mut self.shaper,
                // Fallback slots are only constructed after a coverage query
                // on this same atlas. No scalar fallback on shaping failure.
                FontSlot::Fallback => self
                    .fallback_shaper
                    .as_mut()
                    .ok_or(LayoutError::MissingFace)?,
            };
            let direction = if run.level.is_rtl() {
                Direction::RightToLeft
            } else {
                Direction::LeftToRight
            };
            let shaped = shaper
                .shape_script(&text[run.range.clone()], direction, Some(run.script))
                .map_err(LayoutError::Shape)?;
            if glyphs.len() + shaped.glyphs.len() > MAX_LAYOUT_GLYPHS {
                return Err(LayoutError::TooManyGlyphs {
                    limit: MAX_LAYOUT_GLYPHS,
                });
            }
            let scale = 1. / f32::from(shaped.units_per_em);
            for (info, position) in shaped.glyphs.iter().zip(shaped.positions) {
                let cluster = run.range.start + info.cluster as usize;
                let source = text
                    .get(cluster..run.range.end)
                    .and_then(|s| s.chars().next())
                    .ok_or(LayoutError::InvalidCluster {
                        offset: cluster,
                        bytes: text.len(),
                    })?;
                let advance = [
                    position.x_advance as f32 * scale,
                    position.y_advance as f32 * scale,
                ];
                glyphs.push(PositionedGlyph {
                    face: run.face,
                    id: info.glyph_id,
                    cluster,
                    position: [
                        pen[0] + position.x_offset as f32 * scale,
                        pen[1] + position.y_offset as f32 * scale,
                    ],
                    advance,
                    source,
                });
                pen[0] += advance[0];
                pen[1] += advance[1];
                missing += usize::from(info.glyph_id == 0);
            }
        }
        let layout = Arc::new(LineLayout {
            glyphs,
            advance: pen[0],
            grapheme_boundaries: boundaries,
            missing_glyphs: missing,
        });
        self.layouts.insert(text, Arc::clone(&layout));
        Ok(layout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ab_glyph::{Font, FontArc, FontVec};

    fn arabic_atlas() -> TextAtlas {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        for face in db.faces() {
            let Some(font) = db
                .with_face_data(face.id, |data, index| {
                    FontVec::try_from_vec_and_index(data.to_vec(), index).ok()
                })
                .flatten()
            else {
                continue;
            };
            if "سلامABC".chars().all(|c| font.glyph_id(c).0 != 0) {
                return TextAtlas::new(FontArc::new(font), face.index, 12., Vec::new()).unwrap();
            }
        }
        panic!("installed fixture needs an Arabic outline font");
    }
    #[test]
    fn equivalent_composed_marks_keep_glyphs_positions_and_width_without_rasterization() {
        let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
        atlas.take_dirty();
        for (composed, decomposed) in [("café", "cafe\u{301}"), ("Åñ", "A\u{30a}n\u{303}")] {
            let a = atlas.layout_line(composed).unwrap();
            let b = atlas.layout_line(decomposed).unwrap();
            assert_eq!(a.advance, b.advance);
            assert!(a.advance > 0.);
            assert_eq!(a.missing_glyphs, 0);
            assert_eq!(b.missing_glyphs, 0);
            assert_eq!(
                a.glyphs
                    .iter()
                    .map(|g| (g.id, g.face, g.position, g.advance))
                    .collect::<Vec<_>>(),
                b.glyphs
                    .iter()
                    .map(|g| (g.id, g.face, g.position, g.advance))
                    .collect::<Vec<_>>()
            );
            for g in &b.glyphs {
                assert!(b.grapheme_boundaries.contains(&g.cluster));
            }
        }
        assert!(
            !atlas.take_dirty(),
            "layout may not rasterize or dirty the atlas"
        );
    }
    #[test]
    fn arabic_is_shaped_as_a_joining_run_and_visual_order_matches_the_engine() {
        let mut atlas = arabic_atlas();
        let text = "سلام";
        let expected = {
            let run = atlas
                .shaper
                .shape_script(text, Direction::RightToLeft, Some(Script::ARABIC))
                .unwrap();
            (
                run.glyphs.iter().map(|g| g.glyph_id).collect::<Vec<_>>(),
                run.advance()[0] as f32 / f32::from(run.units_per_em),
            )
        };
        let layout = atlas.layout_line(text).unwrap();
        assert_eq!(layout.missing_glyphs, 0);
        assert_eq!(
            layout.glyphs.iter().map(|g| g.id).collect::<Vec<_>>(),
            expected.0
        );
        assert!((layout.advance - expected.1).abs() < 0.00001);
        let scalar_ids = text
            .chars()
            .map(|c| atlas.shaper.nominal_glyph(c).unwrap())
            .collect::<Vec<_>>();
        assert_ne!(
            expected.0, scalar_ids,
            "negative control must detect unjoined scalar layout"
        );
        assert!(layout
            .glyphs
            .windows(2)
            .all(|pair| pair[0].cluster >= pair[1].cluster));
        assert!(layout
            .glyphs
            .windows(2)
            .all(|pair| pair[0].position[0] <= pair[1].position[0]));
    }
    #[test]
    fn mixed_bidi_orders_visual_runs_without_reversing_latin_digits_or_clusters() {
        let mut atlas = arabic_atlas();
        let text = "ABC سلام 123";
        let layout = atlas.layout_line(text).unwrap();
        assert_eq!(layout.missing_glyphs, 0);
        let offsets = layout.glyphs.iter().map(|g| g.cluster).collect::<Vec<_>>();
        assert_eq!(&offsets[..3], &[0, 1, 2]);
        let digits = text.find("123").unwrap();
        let ordered = offsets
            .iter()
            .copied()
            .filter(|i| *i >= digits)
            .collect::<Vec<_>>();
        assert_eq!(ordered, [digits, digits + 1, digits + 2]);
        assert!(
            offsets.windows(2).any(|p| p[0] > p[1]),
            "RTL run must be in visual order"
        );
        assert!(layout
            .glyphs
            .iter()
            .all(|g| layout.grapheme_boundaries.contains(&g.cluster)));
    }
    #[test]
    fn fallback_uses_a_whole_grapheme_and_missing_sequences_remain_explicit() {
        let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
        let text = "A中e\u{301}🇯🇵👩\u{200d}💻";
        let layout = atlas.layout_line(text).unwrap();
        assert!(layout.advance > 0.);
        assert_eq!(
            layout.missing_glyphs,
            layout.glyphs.iter().filter(|g| g.id == 0).count()
        );
        assert!(layout
            .glyphs
            .iter()
            .all(|g| layout.grapheme_boundaries.contains(&g.cluster)));
        if atlas.shaper.nominal_glyph('中').is_none()
            && atlas
                .fallback_shaper
                .as_ref()
                .is_some_and(|f| f.nominal_glyph('中').is_some())
        {
            assert_eq!(
                layout
                    .glyphs
                    .iter()
                    .find(|g| g.source == '中')
                    .unwrap()
                    .face,
                FontSlot::Fallback
            );
        }
        assert!(layout
            .grapheme_boundaries
            .contains(&text.find('👩').unwrap()));
        assert!(!layout
            .grapheme_boundaries
            .contains(&text.find('💻').unwrap()));
    }
    #[test]
    fn bounded_cache_reuses_hits_evicts_old_entries_and_recovers_after_rejection() {
        let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
        let initial = atlas.layout_line("Position").unwrap();
        assert!(Arc::ptr_eq(
            &initial,
            &atlas.layout_line("Position").unwrap()
        ));
        for n in 0..MAX_CACHE_ENTRIES * 2 {
            atlas.layout_line(&format!("row {n}")).unwrap();
        }
        assert!(atlas.layouts.entries.len() <= MAX_CACHE_ENTRIES);
        assert!(atlas.layouts.bytes <= MAX_CACHE_BYTES);
        assert!(!Arc::ptr_eq(
            &initial,
            &atlas.layout_line("Position").unwrap()
        ));
        let before = (atlas.layouts.entries.len(), atlas.layouts.bytes);
        let long = "x".repeat(shaping::MAX_RUN_BYTES + 1);
        let error = atlas.layout_line(&long).unwrap_err();
        assert!(matches!(error, LayoutError::TooLong { .. }));
        assert!(!error.to_string().contains(&long));
        assert_eq!((atlas.layouts.entries.len(), atlas.layouts.bytes), before);
        assert!(matches!(
            atlas.layout_line("a\nb"),
            Err(LayoutError::MultipleLines)
        ));
        let empty = atlas.layout_line("").unwrap();
        assert!(empty.glyphs.is_empty());
        assert_eq!(empty.advance, 0.);
        assert!(atlas.layout_line("Healthy").unwrap().advance > 0.);
        atlas.set_scale(13.);
        assert!(atlas.layouts.entries.is_empty());
    }

    #[test]
    fn byte_budget_evicts_large_layouts_before_the_entry_limit() {
        let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
        for n in 0..8 {
            let text = format!("row {n} {}", "W".repeat(4096));
            let shaped = atlas.layout_line(&text).unwrap();
            assert!(shaped.glyphs.len() >= 4096);
            assert!(atlas.layouts.bytes <= MAX_CACHE_BYTES);
        }
        assert!(atlas.layouts.entries.len() < 8);
        assert!(atlas.layouts.entries.len() < MAX_CACHE_ENTRIES);
        assert!(atlas.layout_line("Healthy").unwrap().advance > 0.);
    }
}
