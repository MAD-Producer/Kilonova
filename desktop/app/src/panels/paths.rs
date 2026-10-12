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
    let mut y = auxiliary::body_y(
        panel,
        area,
        panels.path_controls.height(&layer) + 140.,
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
    if let Some((rect, dragging)) = panels.path_controls.numeric_cursor() {
        if dragging || rect.contains(input.mouse) {
            panels.cursor = Some(motion_ui::widgets::numeric_behavior::cursor(
                input, dragging,
            ));
        }
    }
}
