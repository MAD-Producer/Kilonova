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
    let plan = super::timeline_columns::ColumnRects::styled(
        styles, area.min[0], gutter, Default::default(),
    );
    Columns {
        name: plan.name.rect(area.min[1], area.height()),
        three_d: plan.three_d.map_or_else(
            || Rect::new(area.min[0], area.min[1], 0., area.height()),
            |slot| slot.rect(area.min[1], area.height()),
        ),
        three_d_visible: plan.three_d.is_some(),
        blend_visible: plan.blend.is_some(),
        parent_visible: plan.parent.is_some(),
    }
}
