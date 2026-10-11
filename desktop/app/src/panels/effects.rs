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
    paint::rect(list, area, palette::PANEL);
    let selected = auxiliary::selected(panels, state).cloned();
    let fallback = serde_json::json!({"id":0,"locked":true,"effects":[]});
    let layer = selected.as_ref().unwrap_or(&fallback);
    if panel == Panel::Effects {
        panels.browser.catalogue = panels.effect_controls.catalogue.clone();
    }
    let controls = if panel == Panel::Effects {
        &mut panels.browser
    } else {
        &mut panels.effect_controls
    };
    let height = controls.height(layer).max(500.);
    let mut y = auxiliary::body_y(panel, area, height, panels, input);
    if panel == Panel::Effects {
        panels.browser.render_library(
            list,
            atlas,
            area,
            &mut y,
            layer,
            state,
            input,
            panels.locale,
            out,
        );
    } else if selected.is_some() {
        panels.effect_controls.render(
            list,
            atlas,
            area,
            &mut y,
            layer,
            state,
            input,
            panels.locale,
            out,
        );
    } else {
        label(
            list,
            atlas,
            area.shrink(10.),
            panels.locale.t(T::SelectLayer),
            true,
        );
    }
}
