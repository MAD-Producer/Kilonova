//! Single-line layout in em units. Font choice never splits a grapheme; shaping
//! keeps adjacent compatible graphemes together, including Arabic joining.
//! This is the shared result intended for measurement, truncation and painting.
use super::{glyph_index::FontSlot, shaping, TextAtlas};
use shaping::{Direction, Script};
use std::{collections::HashMap, fmt, ops::Range, sync::Arc};
use unicode_bidi::{BidiInfo, Level, ParagraphBidiInfo};
use unicode_segmentation::UnicodeSegmentation;

const MAX_CACHE_BYTES: usize = 1024 * 1024;
// Bound keys and values by their retained storage, rather than evicting a small
// working set solely because it contains more than 128 different labels.
const ENTRY_BYTES: usize = size_of::<(Arc<str>, CachedLayout)>() + ARC_HEADER_BYTES;
const MAX_CACHE_ENTRIES: usize =
    MAX_CACHE_BYTES / (ENTRY_BYTES + LineLayout::FIXED_RETAINED_BYTES);
pub(super) const ARC_HEADER_BYTES: usize = 2 * size_of::<usize>();

#[cfg(test)]
#[path = "layout/budget_tests.rs"]
mod budget_tests;
const MAX_LAYOUT_GLYPHS: usize = 16 * 1024;

#[derive(Debug)]
pub enum LayoutError {
    TooLong { bytes: usize, limit: usize },
    MultipleLines,
    TooManyGlyphs { limit: usize },
    MissingFace,
    InvalidCluster { offset: usize, bytes: usize },
    Shape(shaping::ShapeError),
    FontFallback(Arc<super::fallback::FallbackError>),
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
            Self::FontFallback(cause) => write!(f, "UI line font selection: {cause}"),
        }
    }
}
impl std::error::Error for LayoutError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Shape(cause) => Some(cause),
            Self::FontFallback(cause) => Some(cause.as_ref()),
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
    pub(super) grapheme_spans: Vec<[f32; 2]>,
    /// Explicit unsupported glyphs, retained as ID 0, never scalar substitution.
    /// The drawing boundary must handle and observe this degradation.
    pub missing_glyphs: usize,
}

impl LineLayout {
    pub(super) const FIXED_RETAINED_BYTES: usize = size_of::<Self>() + ARC_HEADER_BYTES;

    pub(super) fn retained_bytes(&self) -> usize {
        Self::FIXED_RETAINED_BYTES
            + self.glyphs.capacity() * size_of::<PositionedGlyph>()
            + self.grapheme_boundaries.capacity() * size_of::<usize>()
            + self.grapheme_spans.capacity() * size_of::<[f32; 2]>()
    }
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
        // Charge shared layouts in full in each cache: outstanding callers must
        // not let either cache retain unaccounted data. Hash-table spare buckets
        // and allocator overhead are separate from this retained-data budget;
        // the positive minimum entry charge also bounds table capacity.
        let bytes = ENTRY_BYTES + text.len() + value.retained_bytes();
        if bytes > MAX_CACHE_BYTES {
            return;
        }
        if let Some(old) = self.entries.remove(text) {
            self.bytes -= old.bytes;
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
    paragraph: usize,
    hard_break: bool,
}
struct Run {
    range: Range<usize>,
    script: Script,
    face: FontSlot,
    level: Level,
    paragraph: usize,
}
fn strong(script: Script) -> bool {
    !matches!(script, Script::COMMON | Script::INHERITED | Script::UNKNOWN)
}
pub(super) fn coverage(face: &shaping::RunShaper, text: &str) -> bool {
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
        self.layout_label(text)
    }

    /// Labels occupy one visual row. Paragraph separators remain whole source
    /// graphemes but occupy no glyph or advance; they still separate joining and
    /// bidi runs. Multiline editors already draw their individual source lines.
    pub(crate) fn layout_label(&mut self, text: &str) -> Result<Arc<LineLayout>, LayoutError> {
        if text.len() > shaping::MAX_RUN_BYTES {
            return Err(LayoutError::TooLong {
                bytes: text.len(),
                limit: shaping::MAX_RUN_BYTES,
            });
        }
        if let Some(layout) = self.layouts.get(text) {
            return Ok(layout);
        }
        let bidi = BidiInfo::new(text, None);
        let mut paragraph = 0;
        let mut clusters: Vec<_> = text
            .grapheme_indices(true)
            .map(|(start, grapheme)| {
                let hard_break = grapheme.chars().all(|c| {
                    matches!(
                        c,
                        '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{85}' | '\u{1c}'
                            ..='\u{1e}' | '\u{2028}' | '\u{2029}'
                    )
                });
                let segment = paragraph;
                if hard_break {
                    paragraph += 1;
                }
                let face = self
                    .font_for_grapheme(grapheme)
                    .map_err(LayoutError::FontFallback)?;
                let script = grapheme
                    .chars()
                    .map(|c| harfrust::unicode::script(c as u32))
                    .find(|s| strong(*s))
                    .unwrap_or(Script::COMMON);
                Ok(Cluster {
                    range: start..start + grapheme.len(),
                    script,
                    face,
                    paragraph: segment,
                    hard_break,
                })
            })
            .collect::<Result<_, LayoutError>>()?;
        let boundaries: Vec<usize> = clusters
            .iter()
            .map(|c| c.range.start)
            .chain([text.len()])
            .collect();
        let levels = if bidi.paragraphs.len() <= 1 {
            bidi.paragraphs
                .first()
                .map(|para| bidi.reordered_levels(para, para.range.clone()))
                .unwrap_or_default()
        } else {
            // Reordering the full BidiInfo once per paragraph would clone the
            // whole label repeatedly. Local paragraph views keep this linear
            // in the source length, even for many hard breaks.
            bidi.paragraphs
                .iter()
                .flat_map(|para| {
                    let source = &text[para.range.clone()];
                    ParagraphBidiInfo::new(source, None).reordered_levels(0..source.len())
                })
                .collect()
        };
        // Common/inherited characters take an adjacent strong script at the
        // same bidi level. Never merge strong scripts merely because they use
        // the same font, or shape each grapheme independently.
        let mut previous = None;
        for cluster in &mut clusters {
            if cluster.hard_break {
                previous = None;
                continue;
            }
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
            if cluster.hard_break {
                next = None;
                continue;
            }
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
            if cluster.hard_break {
                continue;
            }
            let level = levels[cluster.range.start];
            if let Some(last) = runs.last_mut().filter(|last| {
                last.face == cluster.face
                    && last.script == cluster.script
                    && last.level == level
                    && last.range.end == cluster.range.start
                    && last.paragraph == cluster.paragraph
            }) {
                last.range.end = cluster.range.end;
            } else {
                runs.push(Run {
                    range: cluster.range,
                    script: cluster.script,
                    face: cluster.face,
                    level,
                    paragraph: cluster.paragraph,
                });
            }
        }
        let mut order = Vec::new();
        let mut offset = 0;
        for paragraph in runs.chunk_by(|a, b| a.paragraph == b.paragraph) {
            let local = BidiInfo::reorder_visual(
                &paragraph.iter().map(|run| run.level).collect::<Vec<_>>(),
            );
            order.extend(local.into_iter().map(|index| index + offset));
            offset += paragraph.len();
        }
        let mut glyphs = Vec::new();
        let mut grapheme_spans = vec![[f32::NAN; 2]; boundaries.len().saturating_sub(1)];
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
                FontSlot::System(index) => {
                    &mut self
                        .system_fallback
                        .as_mut()
                        .and_then(|pool| pool.faces.get_mut(index as usize))
                        .ok_or(LayoutError::MissingFace)?
                        .shaper
                }
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
            super::caret::place_run(&boundaries, &mut grapheme_spans, run.range.clone(), &shaped, pen[0])?;
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
        // Removed formatting/paragraph controls have no painted extent.
        let mut previous = 0.;
        for span in &mut grapheme_spans {
            if span[0].is_nan() { *span = [previous; 2]; }
            previous = span[1];
        }
        let layout = Arc::new(LineLayout {
            glyphs,
            advance: pen[0],
            grapheme_boundaries: boundaries,
            grapheme_spans,
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

    #[test]
    fn labels_keep_crlf_source_boundaries_without_rejecting_or_reordering_paragraphs() {
        let mut atlas = arabic_atlas();
        let text = "سلام\r\nسلام";
        let a = atlas.layout_line("سلام").unwrap();
        let b = atlas.layout_label(text).unwrap();
        assert!((b.advance - 2. * a.advance).abs() < 0.00001);
        assert_eq!(b.glyphs.len(), 2 * a.glyphs.len());
        let break_offset = text.find('\r').unwrap();
        assert!(b.grapheme_boundaries.contains(&break_offset));
        assert!(!b.grapheme_boundaries.contains(&(break_offset + 1)));
        assert!(b.glyphs[..a.glyphs.len()]
            .iter()
            .all(|g| g.cluster < break_offset));
        assert!(b.glyphs[a.glyphs.len()..]
            .iter()
            .all(|g| g.cluster > break_offset + 1));
        assert_eq!(b.missing_glyphs, 0);
        assert!(matches!(
            atlas.layout_line(text),
            Err(LayoutError::MultipleLines)
        ));
    }

    #[test]
    fn default_language_fonts_render_installed_arabic_without_missing_glyphs() {
        // Establish actual installed coverage independently of the default UI
        // family. The product must discover this face without a custom font.
        let reference = arabic_atlas();
        assert!(coverage(&reference.shaper, "سلام"));
        for chinese in [false, true] {
            let mut atlas = TextAtlas::from_system_language(16., chinese).unwrap();
            let line = atlas.layout_line("cafe\u{301} · سلام 123 · 位置").unwrap();
            assert_eq!(
                line.missing_glyphs, 0,
                "chinese={chinese} family={:?}",
                atlas.selected_family
            );
            let arabic: Vec<_> = line
                .glyphs
                .iter()
                .filter(|g| ('\u{600}'..='\u{6ff}').contains(&g.source))
                .collect();
            assert!(!arabic.is_empty());
            for glyph in arabic {
                let bitmap = atlas
                    .glyph_by_id(glyph.face, glyph.id, 16., glyph.source)
                    .unwrap();
                assert!(bitmap.uv[2] > bitmap.uv[0] && bitmap.uv[3] > bitmap.uv[1]);
            }
        }
    }
}
