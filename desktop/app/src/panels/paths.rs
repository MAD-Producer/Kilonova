use super::*;
pub(super) fn render(
    panel: Panel,
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    let styles = list.styles();
    let colors = styles.colors();

    paint::rect(list, area, colors.PANEL);
    let Some(layer) = auxiliary::selected(panels, state)
        .and_then(|l| serde_json::from_value::<motion_core::Layer>(l.clone()).ok())
    else {
        label(
            list,
            atlas,
            area.shrink(10.),
            panels.locale.t(T::SelectLayer),
            true,
        );
        return;
    };
    let height = if panel == Panel::Vector {
        panels.path_controls.groups.height(&layer, styles.as_ref())
            + match &layer.content {
                motion_core::Content::Vector { vector } => match vector.source {
                    motion_core::vector::VectorSource::Group { .. } => 0.,
                    _ => styles.metric("row.height", 22.),
                },
                _ => 0.,
            }
    } else {
        panels.path_controls.height(&layer, styles.as_ref())
    };
    let mut y = auxiliary::body_y(
        &styles,
        panel,
        area,
        height + styles.metric("padding.panel", 8.) * 2.,
        panels,
        input,
    );
    panels.path_controls.inspector_section(
        panel == Panel::Vector,
        list,
        atlas,
        area,
        &mut y,
        &layer,
        state,
        input,
        panels.locale,
        out,
    );
    let numeric = if panel == Panel::Vector {
        panels.path_controls.groups.cursor
    } else {
        panels.path_controls.numeric_cursor()
    };
    if let Some((rect, dragging)) = numeric {
        if dragging || rect.contains(input.mouse) {
            panels.cursor = Some(motion_ui::widgets::numeric_behavior::cursor(
                input, dragging,
            ));
        }
    }
}
