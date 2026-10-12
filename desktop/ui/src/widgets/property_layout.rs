//! Property values keep their content width. Narrow panels stack instead of squeezing.
use crate::{style::StyleSheet, Rect};

pub struct PropertyColumns {
    pub label: Rect,
    pub axis: Rect,
    pub value: Rect,
    pub height: f32,
}
impl PropertyColumns {
    pub fn new(sheet: &StyleSheet, area: Rect, y: f32, value_width: f32) -> Self {
        let padding = sheet.metric("padding.panel", 10.).min(area.width() * 0.25);
        let row = sheet.metric("row.height", 22.).max(1.);
        let field = sheet.metric("field.height", 18.).min(row).max(1.);
        let icon = sheet.metric("icon.slot", 20.);
        let available = (area.width() - padding * 2.).max(0.);
        let needed = value_width.max(sheet.metric("value.min_width", 64.));
        let label_width = sheet.metric("label.column", 98.);
        let stacked = available < label_width + icon * 2. + needed;
        let label = Rect::new(
            area.min[0] + padding + icon,
            y,
            if stacked {
                (available - icon).max(0.)
            } else {
                label_width
            },
            row,
        );
        let value_y = if stacked { y + row } else { y };
        let value_x = if stacked {
            area.min[0] + padding + icon
        } else {
            label.max[0] + icon
        };
        Self {
            label,
            axis: Rect::new(value_x - icon, value_y, icon, row),
            value: Rect::new(
                value_x,
                value_y + (row - field) * 0.5,
                (area.max[0] - padding - value_x).max(0.),
                field,
            ),
            height: if stacked { row * 2. } else { row },
        }
    }
}
