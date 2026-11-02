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
        if !panels.animation_controls.editing()
            && panels.selected_keys.status(state) == super::key_selection::Status::Current {
            if let Some(super::key_selection::AddressedKey::Spatial(key)) = panels.selected_keys.addressed_primary() {
                if let motion_core::KeyframeTarget::Property {object: owner,property,axis} = key.target {
                    if Some(owner) == object && panels.animation_controls.mini_context()
                        != Some((motion_core::KeyframeTarget::Property {object:owner,property,axis},animation_controls::time::Address::Spatial(key.id))) {
                        panels.animation_controls.select_address(state,owner,property,axis,animation_controls::time::Address::Spatial(key.id));
                    }
                }
            }
        }
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
        if let Some(key) = panels.animation_controls.take_spatial_selection() {
            let context = key.as_ref().map(|key|
                format!("object={} key={}", key.target.object(), key.id));
            let _ = observe!("curve.controls.spatial_selection",
                panels.selected_keys.apply_addressed(state, key.map(super::key_selection::AddressedKey::Spatial),
                    super::key_selection::Mode::Replace),
                "{} phase=spatial_selection", context.as_deref().unwrap_or("selection=empty"));
        }
        if let Some(key) = panels.animation_controls.take_local_selection() {
            let _ = observe!("curve.controls.selection",
                panels.selected_keys.apply_addressed(state, key.map(super::key_selection::AddressedKey::Local),
                    super::key_selection::Mode::Replace),
                "object={object:?} phase=local_selection");
        }
    }
}
