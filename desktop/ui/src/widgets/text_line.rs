//! Preserve source byte positions while displaying tabs as editor columns.
use crate::text::{layout::LineLayout, TextAtlas};
use std::borrow::Cow;
use unicode_segmentation::UnicodeSegmentation;

pub(super) struct ExpandedLine<'a> {
    pub text: Cow<'a, str>,
    boundaries: Vec<(usize, usize)>,
}
impl<'a> ExpandedLine<'a> {
    pub fn new(text: &'a str, tab: usize) -> Self {
        if !text.contains('\t') {
            return Self {
                text: Cow::Borrowed(text),
                boundaries: Vec::new(),
            };
        }
        let mut column = 0;
        let mut expanded = String::new();
        let mut boundaries = Vec::new();
        for (byte, cluster) in text.grapheme_indices(true) {
            boundaries.push((byte, expanded.len()));
            if cluster == "\t" {
                let count = tab - column % tab;
                expanded.extend(std::iter::repeat_n(' ', count));
                column += count;
            } else {
                expanded.push_str(cluster);
                column += 1;
            }
        }
        boundaries.push((text.len(), expanded.len()));
        Self {
            text: Cow::Owned(expanded),
            boundaries,
        }
    }
    pub fn mapped_byte(&self, layout: &LineLayout, byte: usize) -> Option<usize> {
        if self.boundaries.is_empty() {
            return layout
                .grapheme_boundaries
                .binary_search(&byte)
                .ok()
                .map(|i| layout.grapheme_boundaries[i]);
        }
        let index = self.boundaries.binary_search_by_key(&byte, |p| p.0).ok()?;
        let expanded = self.boundaries[index].1;
        // Spaces replacing a tab may join a following combining mark.
        layout
            .grapheme_boundaries
            .get(
                layout
                    .grapheme_boundaries
                    .partition_point(|b| *b < expanded),
            )
            .copied()
    }
    pub fn caret(&self, atlas: &TextAtlas, layout: &LineLayout, byte: usize) -> f32 {
        match self.mapped_byte(layout, byte) {
            Some(byte) => atlas.editor_caret(layout, byte),
            None => {
                atlas.report_layout_failure(
                    "expanded_boundary",
                    format_args!("byte={byte} display_bytes={}", self.text.len()),
                );
                0.
            }
        }
    }
    pub fn hit_test(&self, atlas: &TextAtlas, layout: &LineLayout, x: f32) -> Option<usize> {
        if !x.is_finite() {
            return None;
        }
        if self.boundaries.is_empty() {
            return layout.hit_test(x);
        }
        self.boundaries
            .iter()
            .min_by(|a, b| {
                (self.caret(atlas, layout, a.0) - x)
                    .abs()
                    .total_cmp(&(self.caret(atlas, layout, b.0) - x).abs())
            })
            .map(|p| p.0)
    }
}

#[cfg(test)]
mod tests;
