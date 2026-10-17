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
    let mut y = auxiliary::body_y(&styles, Panel::Curves, area, 850., panels, input);
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
        );
    }
}
