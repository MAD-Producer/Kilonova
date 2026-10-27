use super::*;
pub(super) fn render(
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
    let Some(layer) = auxiliary::selected(panels, state).cloned() else {
        label(
            list,
            atlas,
            area.shrink(10.),
            panels.locale.t(T::SelectLayer),
            true,
        );
        return;
    };
    let object = layer["id"].as_u64();
    let sample = state["sampledLayers"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|l| l["id"].as_u64() == object);
    let timeline = state["timeline_layers"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|l| l["object"].as_u64() == object);
    let neutral_scroll = Input::default();
    let scroll_input = if panels.animation_controls.mini_owns_wheel(input) { &neutral_scroll } else { input };
    let mut y = auxiliary::body_y(&styles, Panel::Curves, area, 850., panels, scroll_input);
    let _ = observe!(
        "curves.snapshot",
        (sample.is_some() && timeline.is_some())
            .then_some(())
            .ok_or("selected layer missing graph snapshot"),
        "object={object:?} sampled={} timeline={}",
        sample.is_some(),
        timeline.is_some()
    );
    if let (Some(sample), Some(timeline)) = (sample, timeline) {
        panels.animation_controls.clipboard_busy = panels.clipboard_busy;
        panels.animation_controls.expanded = true;
        panels.animation_controls.render(
            list,
            atlas,
            area,
            &mut y,
            &layer,
            sample,
            timeline,
            state,
            input,
            panels.locale,
            out,
            Some(animation_controls::mini::Context {
                ranges: &mut panels.timeline.graph_ranges,
                selection: &panels.selected_keys,
                effect_key: panels.effect_timeline.range_key(),
                focused: panels.focused,
            }),
        );
    }
}
