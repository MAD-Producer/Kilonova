//! Carets use the original shaped run, never a separately shaped prefix.
use super::{
    layout::{LayoutError, LineLayout},
    shaping::{Direction, ShapedRun},
};
use std::ops::Range;

pub(super) fn place_run(
    boundaries: &[usize],
    spans: &mut [[f32; 2]],
    source: Range<usize>,
    run: &ShapedRun<'_>,
    mut pen: f32,
) -> Result<(), LayoutError> {
    let scale = 1. / f32::from(run.units_per_em);
    let rtl = run.direction == Direction::RightToLeft;
    let invalid = |offset| LayoutError::InvalidCluster {
        offset,
        bytes: *boundaries.last().unwrap_or(&0),
    };
    let cluster_start = |index: usize| -> Result<usize, LayoutError> {
        let byte = source.start + run.glyphs[index].cluster as usize;
        if byte >= source.end {
            return Err(invalid(byte));
        }
        let index = boundaries.partition_point(|b| *b <= byte).saturating_sub(1);
        let start = boundaries[index];
        if start < source.start {
            return Err(invalid(byte));
        }
        Ok(start)
    };
    let mut first = 0;
    while first < run.glyphs.len() {
        let source_start = cluster_start(first)?;
        let mut end = first + 1;
        while end < run.glyphs.len() && cluster_start(end)? == source_start {
            end += 1;
        }
        let source_end = if rtl {
            if first == 0 {
                source.end
            } else {
                cluster_start(first - 1)?
            }
        } else if end == run.glyphs.len() {
            source.end
        } else {
            cluster_start(end)?
        };
        let low = boundaries
            .binary_search(&source_start)
            .map_err(|_| invalid(source_start))?;
        let high = boundaries
            .binary_search(&source_end)
            .map_err(|_| invalid(source_end))?;
        if high <= low || high > spans.len() {
            return Err(invalid(source_end));
        }
        let left = pen;
        for position in &run.positions[first..end] {
            pen += position.x_advance as f32 * scale;
        }
        let (leading, trailing) = if rtl { (pen, left) } else { (left, pen) };
        // Ligature interiors divide the existing advance at grapheme stops.
        // This is not a new shape operation or a stop inside an emoji sequence.
        let at = |index: usize| {
            if index == high {
                trailing
            } else if index == low {
                leading
            } else {
                leading + (trailing - leading) * (index - low) as f32 / (high - low) as f32
            }
        };
        for index in low..high {
            spans[index] = [at(index), at(index + 1)];
        }
        first = end;
    }
    Ok(())
}

impl LineLayout {
    /// Visual X in em units at an exact extended-grapheme byte boundary.
    /// Bidi junctions use the following grapheme's leading edge; EOF uses the
    /// previous trailing edge. None denotes a byte that is not a caret stop.
    pub fn caret_x(&self, byte: usize) -> Option<f32> {
        let index = self.grapheme_boundaries.binary_search(&byte).ok()?;
        Some(
            self.grapheme_spans
                .get(index)
                .map(|span| span[0])
                .or_else(|| self.grapheme_spans.last().map(|span| span[1]))
                .unwrap_or(0.),
        )
    }
    /// Closest canonical caret. Non-finite coordinates are invalid hit tests.
    pub fn hit_test(&self, x: f32) -> Option<usize> {
        if !x.is_finite() {
            return None;
        }
        self.grapheme_boundaries.iter().copied().min_by(|a, b| {
            let distance = |byte| (self.caret_x(byte).expect("stored grapheme boundary") - x).abs();
            distance(*a).total_cmp(&distance(*b))
        })
    }
    /// Visit a logical selection's separate visual boxes without allocation.
    pub fn selection_spans(&self, range: Range<usize>, mut visit: impl FnMut([f32; 2])) {
        let mut current: Option<[f32; 2]> = None;
        for (bounds, span) in self
            .grapheme_boundaries
            .windows(2)
            .zip(&self.grapheme_spans)
        {
            if bounds[0] < range.start || bounds[1] > range.end {
                continue;
            }
            let next = [span[0].min(span[1]), span[0].max(span[1])];
            if next[0] == next[1] {
                continue;
            }
            match current {
                Some(old) if next[0] <= old[1] && next[1] >= old[0] => {
                    current = Some([old[0].min(next[0]), old[1].max(next[1])])
                }
                Some(old) => {
                    visit(old);
                    current = Some(next);
                }
                None => current = Some(next),
            }
        }
        if let Some(span) = current {
            visit(span);
        }
    }
}

impl super::TextAtlas {
    pub(crate) fn editor_layout(&mut self, text: &str) -> Option<std::sync::Arc<LineLayout>> {
        match self.layout_line(text) {
            Ok(layout) => Some(layout),
            Err(cause) => {
                self.report_layout_failure(
                    "editor",
                    format_args!("bytes={} cause={cause}", text.len()),
                );
                None
            }
        }
    }
    pub(crate) fn editor_caret(&self, layout: &LineLayout, byte: usize) -> f32 {
        match layout.caret_x(byte) {
            Some(x) => x,
            None => {
                self.report_layout_failure(
                    "caret_boundary",
                    format_args!(
                        "byte={byte} bytes={}",
                        layout.grapheme_boundaries.last().copied().unwrap_or(0)
                    ),
                );
                0.
            }
        }
    }
}

#[cfg(test)]
mod tests;
