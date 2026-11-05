//! Retain complete shaped prefixes, including the ellipsis in the same layout.
//! Widths are in em units so font-size and DPI do not multiply cache entries.
use super::{
    layout::{LayoutError, LineLayout, ARC_HEADER_BYTES},
    TextAtlas,
};
use std::{
    collections::{hash_map::RandomState, HashMap},
    hash::BuildHasher,
    sync::Arc,
};

const MAX_BYTES: usize = 1024 * 1024;
const ENTRY_BYTES: usize = size_of::<(u64, Entry)>() + ARC_HEADER_BYTES;
const MAX_ENTRIES: usize = MAX_BYTES / (ENTRY_BYTES + LineLayout::FIXED_RETAINED_BYTES);

#[cfg(test)]
#[path = "fitting/budget_tests.rs"]
mod budget_tests;

#[derive(Clone)]
pub struct FittedLine {
    pub layout: Arc<LineLayout>,
    pub prefix_end: usize,
    pub ellipsis: bool,
}
struct Entry {
    text: Arc<str>,
    width: u32,
    value: FittedLine,
    bytes: usize,
    age: u64,
}
#[derive(Default)]
pub(super) struct FitCache {
    entries: HashMap<u64, Entry>,
    hash: RandomState,
    clock: u64,
    bytes: usize,
}
impl FitCache {
    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.clock = 0;
        self.bytes = 0;
    }
    fn key(&self, text: &str, width: f32) -> u64 {
        self.hash.hash_one((text, width.to_bits()))
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
    fn get(&mut self, text: &str, width: f32) -> Option<FittedLine> {
        let key = self.key(text, width);
        let age = self.tick();
        self.entries
            .get_mut(&key)
            .filter(|e| e.text.as_ref() == text && e.width == width.to_bits())
            .map(|e| {
                e.age = age;
                e.value.clone()
            })
    }
    fn insert(&mut self, text: &str, width: f32, value: FittedLine) {
        let bytes = ENTRY_BYTES + text.len() + value.layout.retained_bytes();
        if bytes > MAX_BYTES {
            return;
        }
        let key = self.key(text, width);
        // Hash collisions are a cache miss. Never return another label's data.
        if let Some(old) = self.entries.remove(&key) {
            self.bytes -= old.bytes;
        }
        while self.entries.len() >= MAX_ENTRIES || self.bytes + bytes > MAX_BYTES {
            let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.age)
                .map(|(key, _)| *key)
            else {
                break;
            };
            if let Some(old) = self.entries.remove(&oldest) {
                self.bytes -= old.bytes;
            }
        }
        let age = self.tick();
        self.bytes += bytes;
        self.entries.insert(
            key,
            Entry {
                text: Arc::from(text),
                width: width.to_bits(),
                value,
                bytes,
                age,
            },
        );
    }
}

impl TextAtlas {
    /// Return a verified fitting grapheme prefix. The suffix is shaped with the
    /// prefix, preserving contextual positioning and RTL ellipsis placement.
    pub fn fit_line(&mut self, text: &str, width_em: f32) -> Result<FittedLine, LayoutError> {
        if width_em.is_nan() || width_em < 0. {
            return Ok(FittedLine {
                layout: self.layout_line("")?,
                prefix_end: 0,
                ellipsis: false,
            });
        }
        if let Some(fit) = self.fitted_layouts.get(text, width_em) {
            return Ok(fit);
        }
        let full = self.layout_label(text)?;
        let fit = if full.advance <= width_em {
            FittedLine {
                layout: full,
                prefix_end: text.len(),
                ellipsis: false,
            }
        } else {
            let ellipsis = self.layout_line("…")?;
            if ellipsis.advance > width_em {
                FittedLine {
                    layout: self.layout_line("")?,
                    prefix_end: 0,
                    ellipsis: false,
                }
            } else {
                let boundaries = &full.grapheme_boundaries;
                let mut low = 0;
                let mut high = boundaries.len().saturating_sub(2);
                let mut chosen = FittedLine {
                    layout: ellipsis,
                    prefix_end: 0,
                    ellipsis: true,
                };
                let mut candidate = String::with_capacity(text.len() + 3);
                // Probe complete layouts logarithmically instead of shaping
                // every prefix of a potentially 16 KiB label on the UI thread.
                while low < high {
                    let middle = (low + high + 1) / 2;
                    let end = boundaries[middle];
                    if end + 3 > super::shaping::MAX_RUN_BYTES {
                        high = middle - 1;
                        continue;
                    }
                    candidate.clear();
                    candidate.push_str(&text[..end]);
                    candidate.push('…');
                    let layout = self.layout_label(&candidate)?;
                    if layout.advance <= width_em {
                        low = middle;
                        chosen = FittedLine {
                            layout,
                            prefix_end: end,
                            ellipsis: true,
                        };
                    } else {
                        high = middle - 1;
                    }
                }
                chosen
            }
        };
        self.fitted_layouts.insert(text, width_em, fit.clone());
        Ok(fit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_segmentation::UnicodeSegmentation;
    #[test]
    fn shaped_prefix_and_ellipsis_fit_as_one_line_at_grapheme_boundaries() {
        let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
        for text in [
            "cafe\u{301} / 中文 long label",
            "ABC سلام 123",
            "A🇯🇵👩\u{200d}💻 long label",
            "AV fi ffi more",
        ] {
            let full = atlas.layout_line(text).unwrap();
            let boundaries = text
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .chain([text.len()])
                .collect::<Vec<_>>();
            for step in 0..=100 {
                let width = full.advance * step as f32 / 100.;
                let fit = atlas.fit_line(text, width).unwrap();
                assert!(boundaries.contains(&fit.prefix_end));
                assert!(fit.layout.advance <= width + 0.00001);
                let actual = if fit.ellipsis {
                    format!("{}…", &text[..fit.prefix_end])
                } else {
                    text[..fit.prefix_end].to_owned()
                };
                let expected = atlas.layout_line(&actual).unwrap();
                assert_eq!(fit.layout.advance, expected.advance);
                assert_eq!(
                    fit.layout
                        .glyphs
                        .iter()
                        .map(|g| (g.id, g.face, g.position, g.advance))
                        .collect::<Vec<_>>(),
                    expected
                        .glyphs
                        .iter()
                        .map(|g| (g.id, g.face, g.position, g.advance))
                        .collect::<Vec<_>>()
                );
            }
        }
    }
    #[test]
    fn fitted_cache_is_bounded_and_invalid_widths_remain_empty() {
        let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
        let initial = atlas.fit_line("a long label", 2.).unwrap();
        assert!(Arc::ptr_eq(
            &initial.layout,
            &atlas.fit_line("a long label", 2.).unwrap().layout
        ));
        for n in 0..MAX_ENTRIES * 2 {
            atlas.fit_line(&format!("label {n}"), 2.).unwrap();
        }
        assert!(atlas.fitted_layouts.entries.len() <= MAX_ENTRIES);
        assert!(atlas.fitted_layouts.bytes <= MAX_BYTES);
        for width in [f32::NAN, -1., f32::NEG_INFINITY] {
            assert!(atlas
                .fit_line("label", width)
                .unwrap()
                .layout
                .glyphs
                .is_empty());
        }
        let full = atlas.fit_line("label", f32::INFINITY).unwrap();
        assert_eq!(full.prefix_end, 5);
        assert!(!full.ellipsis);
        atlas.set_scale(13.);
        assert!(atlas.fitted_layouts.entries.is_empty());
    }
}
