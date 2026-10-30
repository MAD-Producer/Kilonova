//! Inline inspector geometry shared by property families. No edit or history state.
use crate::{style::StyleSheet, Rect};

#[derive(Clone, Copy, Debug)]
pub struct InspectorColumns {
    pub icon: Rect,
    pub label: Rect,
    pub values: Rect,
    field_height: f32,
    cell_width: f32,
    inset: f32,
}

impl InspectorColumns {
    /// `leading` reserves the caller's existing navigation/tree slots. An icon
    /// target retains its full width; limited space is taken from the caption.
    pub fn new(
        styles: &StyleSheet,
        area: Rect,
        y: f32,
        height: f32,
        leading: f32,
        icon_width: f32,
        components: usize,
    ) -> Self {
        let padding = styles.metric("padding.panel", 8.);
        let gap = styles.metric("padding", 5.);
        let left = area.min[0] + padding + leading;
        let label_x = left + icon_width + if icon_width > 0. { gap } else { 0. };
        let right = (area.max[0] - padding).max(label_x);
        let count = components.max(1) as f32;
        let minimum = styles.metric("value.min_width", 64.);
        let preferred = styles.metric("label.column", 72.);
        let label_width = preferred
            .min((right - label_x - gap - minimum).max(preferred * 0.5))
            .min((right - label_x - gap).max(0.));
        let value_x = (label_x + label_width + gap).min(right);
        let available = (right - value_x).max(0.);
        Self {
            icon: Rect::new(left, y, icon_width, height),
            label: Rect::new(label_x, y, label_width, height),
            values: Rect::new(
                value_x,
                y,
                available.min(
                    styles
                        .metric("inspector.value.max_width", 192.)
                        .max(count * minimum),
                ),
                height,
            ),
            field_height: styles.metric("field.height", 18.).min(height),
            cell_width: (available / count).min(minimum),
            inset: styles.metric("border", 1.) * 2.,
        }
    }

    pub fn number(self, axis: usize) -> Rect {
        Rect::new(
            self.values.min[0] + axis as f32 * self.cell_width,
            self.values.min[1] + (self.values.height() - self.field_height) * 0.5,
            (self.cell_width - self.inset).max(0.),
            self.field_height,
        )
    }

    /// Preserve a caller's established target size within the shared icon slot.
    pub fn icon_target(self, width: f32) -> Rect {
        Rect::new(
            self.icon.min[0] + (self.icon.width() - width) * 0.5,
            self.icon.min[1],
            width,
            self.icon.height(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_columns_preserve_hit_slots_and_stay_inside_translated_panels() {
        let styles = crate::style::default_stylesheet();
        for width in [220., 360., 720.] {
            for height in [22., 26., 28.] {
                for count in 1..=4 {
                    let area = Rect::new(137., 49., width, 520.);
                    let columns =
                        InspectorColumns::new(&styles, area, 91., height, 74., 24., count);
                    assert_eq!(columns.icon.width(), 24.);
                    assert_eq!(columns.icon.height(), height);
                    assert!(columns.icon.max[0] <= columns.label.min[0]);
                    assert!(columns.label.max[0] <= columns.values.min[0]);
                    assert!(columns.values.max[0] <= area.max[0]);
                    let mut last = columns.values.min[0];
                    for axis in 0..count {
                        let field = columns.number(axis);
                        assert!(field.min[0] >= last);
                        assert!(field.max[0] <= area.max[0]);
                        assert!(field.max[0] <= columns.values.max[0]);
                        assert!(field.min[1] >= 91. && field.max[1] <= 91. + height);
                        last = field.max[0];
                    }
                }
            }
        }
        for (width, leading) in [(220., 14.), (360., 74.), (720., 74.)] {
            let area = Rect::new(0., 0., width, 520.);
            let scalar = InspectorColumns::new(&styles, area, 0., 22., leading, 24., 1);
            for axes in 2..=4 {
                let vector = InspectorColumns::new(&styles, area, 0., 26., leading, 24., axes);
                assert_eq!(scalar.label.min[0], vector.label.min[0]);
                assert_eq!(scalar.values.min[0], vector.values.min[0]);
            }
        }
    }
}
