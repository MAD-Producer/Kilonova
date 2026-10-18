//! Layer-row switches share their geometry with the column header and hit tests.
use super::*;

pub(super) struct Columns {
    pub name: Rect,
    pub three_d: Rect,
    pub three_d_visible: bool,
    pub blend_visible: bool,
    pub parent_visible: bool,
}

pub(super) fn columns(styles: &motion_ui::style::StyleSheet, area: Rect, gutter: f32) -> Columns {
    let slot = styles.metric("icon.slot", 20.);
    let offset = styles.metric("timeline.layer.name_offset", 112.);
    let wide = gutter >= styles.metric("timeline.layer.wide_threshold", 500.);
    let reserved = styles.metric(
        if wide {
            "timeline.layer.wide_columns_width"
        } else {
            "timeline.layer.compact_columns_width"
        },
        if wide { 398. } else { 320. },
    );
    let width = gutter - reserved - slot;
    let blend_visible = wide || width >= styles.metric("value.min_width", 64.);
    let mut width = (width
        + if blend_visible {
            0.
        } else {
            styles.metric("timeline.layer.blend_width", 76.)
        })
    .max(0.);
    let parent_visible = wide || width >= styles.metric("value.min_width", 64.);
    let padding = styles.metric("padding.panel", 8.);
    let three_d_visible = gutter >= offset + slot + padding;
    if !parent_visible {
        // Optional columns must leave room for the fixed leading controls and
        // the entire switch target; zero-width names cannot absorb overlap.
        width = (gutter - offset - padding - if three_d_visible { slot } else { 0. }).max(0.);
    }
    Columns {
        name: Rect::new(area.min[0] + offset, area.min[1], width, area.height()),
        three_d: Rect::new(
            area.min[0] + offset + width,
            area.min[1],
            slot,
            area.height(),
        ),
        three_d_visible,
        blend_visible,
        parent_visible,
    }
}
