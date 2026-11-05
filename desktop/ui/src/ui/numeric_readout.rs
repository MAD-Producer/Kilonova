//! Keep numeric presentation identical while reusing the atlas-owned scratch.
use super::Align;
use crate::{paint::PaintList, style::ResolvedStyle, text::TextAtlas, theme::Color, Rect};
use std::fmt;

#[derive(Clone, Copy, Debug)]
pub enum NumberFormat {
    /// Fixed precision, retaining the existing eight-place upper bound.
    Fixed(usize),
    /// Two places and a positive zero, as used by inspector readouts.
    Compact,
}

pub fn number_text(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    value: f64,
    format: NumberFormat,
    style: ResolvedStyle,
    align: Align,
) {
    let (value, precision) = match format {
        NumberFormat::Fixed(precision) => (value, precision.min(8)),
        NumberFormat::Compact => (if value == 0. { 0. } else { value }, 2),
    };
    draw_formatted(
        atlas,
        format_args!("{:.*}", precision, value),
        |atlas, text| {
            crate::style::text_style(list, atlas, area, text, style, align);
        },
    );
}

pub(super) fn scrub_text(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    value: f32,
    color: Color,
    align: Align,
) {
    let mut draw = |atlas: &mut TextAtlas, text: &str| {
        super::text_clipped(list, atlas, area, text, color, align, atlas.font_size());
    };
    if value == 0. {
        return draw(atlas, "0");
    }
    if !value.is_finite() {
        return draw(atlas, "—");
    }
    let magnitude = value.abs();
    let precision = if magnitude >= 1000. {
        1
    } else if magnitude >= 10. {
        2
    } else if magnitude >= 1. {
        3
    } else {
        4
    };
    draw_formatted(
        atlas,
        format_args!("{:.*}", precision, value),
        |atlas, text| {
            let trimmed = text.trim_end_matches('0');
            let label = if magnitude >= 1000. && trimmed.ends_with('.') {
                // Keep the one decimal place without allocating another String.
                &text[..trimmed.len() + 1]
            } else {
                trimmed.trim_end_matches('.')
            };
            draw(atlas, label);
        },
    );
}

fn draw_formatted(
    atlas: &mut TextAtlas,
    args: fmt::Arguments<'_>,
    draw: impl FnOnce(&mut TextAtlas, &str),
) {
    if let Err(cause) = atlas.with_formatted_text(args, draw) {
        // Existing atlas-lifetime counter: Warn once, Error at occurrence 60;
        // healthy draws do not reset it. A replacement atlas starts fresh.
        atlas.report_layout_failure(
            "numeric_format",
            format_args!(
                "operation=numeric_readout phase=format cause={cause} fallback=omit_readout"
            ),
        );
    }
}

#[cfg(test)]
mod tests;
