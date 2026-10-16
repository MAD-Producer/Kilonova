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
    let selected = if panel == Panel::Effects {
        panels.editable_selection(state).first().and_then(|id| {
            state["project"]["layers"]
                .as_array()?
                .iter()
                .find(|l| l["id"].as_u64() == Some(*id))
        })
    } else {
        auxiliary::selected(panels, state)
    };
    let fallback = serde_json::json!({"id":0,"locked":true,"effects":[]});
    let layer = selected.unwrap_or(&fallback);
    if panel == Panel::Effects {
        panels.browser.catalogue = panels.effect_controls.catalogue.clone();
        let mut y = area.min[1];
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
        return;
    }
    let controls = if panel == Panel::Effects {
        &mut panels.browser
    } else {
        &mut panels.effect_controls
    };
    let height = controls.height(layer,&styles,panels.locale, area.width()).max(area.height());
    let scroll_frame = if panel == Panel::Effects {
        Some(panels.library_scroll.begin(
            list,
            area,
            panels.library_height.max(area.height()),
            input,
            false,
        ))
    } else {
        None
    };
    let mut y = scroll_frame.as_ref().map_or_else(
        || auxiliary::body_y(&styles,panel, area, height, panels, input),
        |f| f.content.min[1] + 8.,
    );
    let input = scroll_frame.as_ref().map_or(input, |f| &f.input);
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
    if panel == Panel::Effects {
        panels.library_height =
            (y - area.min[1] + panels.library_scroll.offset + 8.).max(area.height());
    }
    let controls = if panel == Panel::Effects {
        &panels.browser
    } else {
        &panels.effect_controls
    };
    if let Some(readout) = &controls.drag_readout {
        panels.drag_readout = Some(readout.clone());
    }
    if controls.cursor {
        panels.cursor = Some(motion_ui::widgets::numeric_behavior::cursor(
            input,
            controls.drag_readout.is_some(),
        ));
    }
}
