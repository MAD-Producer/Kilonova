use super::*;

#[cfg(test)]
#[path = "composition_tests.rs"]
mod tests;

pub(super) fn render(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    let sheet = list.styles();
    let v = styled_viewer(area, &sheet);
    let surround = list.styles().color("field");
    // Paint the viewer surround, leaving its contents to the shared GPU renderer.
    for r in [
        Rect::new(
            area.min[0],
            area.min[1],
            area.width(),
            v.min[1] - area.min[1],
        ),
        Rect::new(area.min[0], v.min[1], v.min[0] - area.min[0], v.height()),
        Rect::new(v.max[0], v.min[1], area.max[0] - v.max[0], v.height()),
        Rect::new(area.min[0], v.max[1], area.width(), area.max[1] - v.max[1]),
    ] {
        paint::rect(list, r, surround);
    }
    let colors = sheet.colors();
    let height = sheet.metric("viewer.controls.height", 28.);
    let gap = sheet.metric("viewer.controls.gap", 4.);
    let padding = sheet.metric("padding.panel", 8.);
    let field_height = sheet.metric("viewer.field.height", 26.);
    let button_height = sheet.metric("viewer.button.height", 22.);
    let zoom_width = sheet.metric("viewer.zoom.width", 64.);
    let percent_width = sheet.metric("viewer.percent.width", 14.);
    let fit_width = sheet.metric("viewer.fit.width", 40.);
    let time_width = sheet.metric("viewer.time.width", 116.);
    let quality_width = sheet.metric("viewer.quality.width", 68.);
    let view_width = sheet.metric("viewer.view.width", 98.);
    let count_width = sheet.metric("viewer.count.width", 74.);
    let controls = Rect::new(area.min[0], area.max[1] - height, area.width(), height);
    paint::rect(list, controls, colors.CHROME);
    let canvas = [
        state["project"]["width"].as_f64().unwrap_or(1920.),
        state["project"]["height"].as_f64().unwrap_or(1080.),
    ];
    // A bounded identity key resets ephemeral navigation on project/composition changes.
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    state["root"].as_str().hash(&mut hasher);
    state["project"]["composition_id"]
        .as_str()
        .hash(&mut hasher);
    let context = hasher.finish();
    if panels.composition.context != Some(context) {
        panels.composition = Default::default();
        panels.composition.context = Some(context);
    }
    if panels.composition.navigation.mapping(v, canvas).is_none() {
        // Empty/minimized views are normal; malformed canvas state is degradation.
        if v.width() <= 0. || v.height() <= 0. {
            panels.composition.mapping_failures = 0;
            return;
        }
        panels.composition.mapping_failures = panels.composition.mapping_failures.saturating_add(1).min(61);
        let count = panels.composition.mapping_failures;
        if count == 1 {
            log::debug!("viewer mapping unavailable operation=canvas_mapping phase=draw context={context:016x} canvas={canvas:?} viewport={v:?} count=1");
        } else if count == 60 {
            // Saturate at 61 after reporting; recover/reset on the next usable mapping.
            log::error!("viewer mapping unavailable operation=canvas_mapping phase=draw context={context:016x} canvas={canvas:?} viewport={v:?} count=60");
            panels.composition.mapping_failures = 61;
        }
        return;
    }
    panels.composition.mapping_failures = 0;
    let center = [(v.min[0] + v.max[0]) * 0.5, (v.min[1] + v.max[1]) * 0.5];
    let observing = state["observing"].as_bool().unwrap_or(false);
    let cancel_navigation = input.focus_lost || input.keys.iter().any(|(k, _)| *k == Key::Escape);
    if cancel_navigation {
        if let Some((_, before)) = panels.composition.pan.take() {
            panels.composition.navigation = before;
        }
        panels.composition.orbit = None;
    } else if !panels.path_controls.editing() && !panels.transform_controls.gesture_active() {
        if input
            .pressed
            .is_some_and(|(b, p)| (b == MouseButton::Middle || b == MouseButton::Left && super::tools::active(panels) == super::tools::Active::Hand) && v.contains(p))
        {
            panels.composition.pan = Some((input.mouse, panels.composition.navigation));
        }
        if let Some((last, before)) = panels.composition.pan {
            panels.composition.navigation.pan(
                v,
                canvas,
                [input.mouse[0] - last[0], input.mouse[1] - last[1]],
            );
            if input.held == Some(MouseButton::Middle) || input.held == Some(MouseButton::Left) && super::tools::active(panels) == super::tools::Active::Hand {
                panels.composition.pan = Some((input.mouse, before));
                panels.cursor = Some(motion_ui::cursor::Cursor::Grabbing);
            } else {
                panels.composition.pan = None;
            }
        }
        if input.wheel[1] != 0. && v.contains(input.mouse) && panels.composition.pan.is_none() {
            if let Some(mapping) = panels.composition.navigation.mapping(v, canvas) {
                let ratio = if input.wheel[1] > 0. { 1.1 } else { 1. / 1.1 };
                if observing && input.modifiers.alt {
                    out.push(Action::ViewZoom {
                        ratio,
                        size: [v.width() as i32, v.height() as i32],
                    });
                } else {
                    panels.composition.navigation.zoom_at(
                        v,
                        canvas,
                        input.mouse,
                        mapping.scale * ratio,
                    );
                }
            }
        }
    }
    let original = panels.composition.navigation;
    let scale = original.mapping(v, canvas).map_or(1., |m| m.scale);
    let zoom_rect = Rect::new(
        controls.min[0] + padding,
        controls.min[1] + (height - field_height) * 0.5,
        zoom_width,
        field_height,
    );
    let options = motion_ui::widgets::NumberOptions {
        min: 1.,
        max: 6400.,
        speed: 1.,
        decimals: 1,
    };
    panels.composition.zoom.drag_threshold = Some(sheet.metric("motion.drag_threshold", 3.));
    for event in panels
        .composition
        .zoom
        .update(scale * 100., options, input, zoom_rect)
    {
        use motion_ui::widgets::NumberEvent;
        match event {
            NumberEvent::Begin { .. } => panels.composition.zoom_before = Some(original),
            NumberEvent::Change { value } => {
                panels
                    .composition
                    .navigation
                    .zoom_at(v, canvas, center, value / 100.);
            }
            NumberEvent::Commit { .. } => {
                panels.composition.zoom_before = None;
            }
            NumberEvent::Cancel { .. } => {
                if let Some(before) = panels.composition.zoom_before.take() {
                    panels.composition.navigation = before;
                }
            }
        }
    }
    if panels.locale == Locale::Zh && panels.composition.zoom.text.error.is_some() {
        panels.composition.zoom.text.error = Some("请输入 1–6400 范围内的有限数值".into());
    }
    let shown_scale = panels
        .composition
        .navigation
        .mapping(v, canvas)
        .map_or(scale, |m| m.scale);
    panels
        .composition
        .zoom
        .draw(list, atlas, zoom_rect, shown_scale * 100., options);
    label(
        list,
        atlas,
        Rect::new(zoom_rect.max[0], controls.min[1], percent_width, height),
        "%",
        true,
    );
    let fit_rect = Rect::new(
        zoom_rect.max[0] + percent_width + gap,
        controls.min[1]+(height-button_height)*0.5,
        fit_width,
        button_height,
    );
    let fit_state = if panels.composition.navigation.is_fit() {
        motion_ui::style::States::default().with(motion_ui::style::State::Selected)
    } else {
        Default::default()
    };
    if motion_ui::ui::button_with_states(
        list,
        atlas,
        fit_rect,
        if panels.locale == Locale::Zh {
            "适合"
        } else {
            "Fit"
        },
        input,
        true,
        &sheet,
        "button",
        fit_state,
    )
    .clicked
    {
        panels.composition.navigation.fit();
        panels.composition.pan = None;
        panels.composition.zoom.cancel();
        panels.composition.zoom_before = None;
    }
    let right_width = quality_width + view_width + gap * 2.;
    let show_count = controls.width()
        >= padding * 2.
            + zoom_width
            + percent_width
            + fit_width
            + time_width
            + right_width
            + count_width
            + gap * 4.;
    let count = if show_count { count_width + gap } else { 0. };
    let right = controls.max[0] - padding;
    let view_rect = Rect::new(
        right - count - view_width,
        controls.min[1]+(height-button_height)*0.5,
        view_width,
        button_height,
    );
    let quality_rect = Rect::new(
        view_rect.min[0] - gap - quality_width,
        controls.min[1]+(height-button_height)*0.5,
        quality_width,
        button_height,
    );
    if quality_rect.min[0] - gap - fit_rect.max[0] >= time_width {
        let fps = state["project"]["fps"].as_u64().unwrap_or(30) as u32;
        motion_ui::style::text_style(
            list,
            atlas,
            Rect::new(fit_rect.max[0] + gap, controls.min[1], time_width, height),
            &timecode(state["frame"].as_f64().unwrap_or(0.), fps),
            sheet.resolve("label.accent", Default::default()).unwrap(),
            Align::Left,
        );
    }
    let (quality, next) = match state["preview"]["mode"].as_str().unwrap_or("auto") {
        "high" => (T::Full, 2),
        "balanced" => (T::Smooth, 3),
        "economy" => (T::Economy, 0),
        _ => (T::Auto, 1),
    };
    if quality_rect.min[0] > fit_rect.max[0] + gap {
        button(
            list,
            atlas,
            quality_rect,
            panels.locale.t(quality),
            input,
            true,
            Action::PreviewMode(next),
            out,
        );
    }
    // View switcher reflects the live observation state and cycles
    // Composition → Free → Top → Side → Composition.
    let (view_label, next_view) = if !observing {
        (T::ViewComposition, 1)
    } else {
        match state["observationView"].as_str().unwrap_or("free") {
            "top" => (T::ViewTop, 3),
            "side" => (T::ViewSide, 0),
            _ => (T::ViewFree, 2),
        }
    };
    if view_rect.min[0] > fit_rect.max[0] + gap {
        button(
            list,
            atlas,
            view_rect,
            panels.locale.t(view_label),
            input,
            true,
            Action::ViewKind(next_view),
            out,
        );
    }
    if show_count {
        label(
            list,
            atlas,
            Rect::new(right - count_width, controls.min[1], count_width, height),
            if panels.locale == Locale::Zh {
                "1 个视图"
            } else {
                "1 view"
            },
            true,
        );
    }
    if !observing { panels.effect_drag.target(v, None); }
    let Some(mapping) = panels.composition.navigation.mapping(v, canvas) else {
        return;
    };
    let mut handles = list.child();
    let tool_owned = super::tools::viewport(panels, &mut handles, v, mapping, state, input, out);
    let consumed = tool_owned || panels.path_controls.draw_in_viewport(
        &mut handles,
        atlas,
        v,
        mapping.image,
        state,
        panels.selected,
        input,
        panels.locale,
        out,
    );
    let consumed = if !tool_owned && panels.path_controls.tool == super::path_controls::Tool::Select {
        panels.transform_controls.draw_in_viewport(
            &mut handles,
            v,
            mapping.image,
            state,
            panels.selected,
            input,
            out,
        ) || consumed
    } else {
        consumed
    };
    handles.clip_to(v);
    list.append(handles);
    if let Some(cursor) = panels
        .transform_controls
        .cursor
        .filter(|_| !tool_owned && panels.composition.pan.is_none())
    {
        panels.cursor = Some(cursor);
    }
    // Observation gestures: right-drag orbits (free view) or pans
    // (orthogonal views). Wheel navigation belongs to the independent canvas.
    if input
        .pressed
        .is_some_and(|(b, p)| (b == MouseButton::Right || b == MouseButton::Left && super::tools::active(panels) == super::tools::Active::Camera && observing) && v.contains(p))
    {
        panels.composition.orbit = input.pressed.map(|(_, p)| p);
    }
    if let Some(last) = panels.composition.orbit {
        if input.held == Some(MouseButton::Right) || input.held == Some(MouseButton::Left) && super::tools::active(panels) == super::tools::Active::Camera && observing {
            let delta = [
                (input.mouse[0] - last[0]) * 0.2,
                (input.mouse[1] - last[1]) * 0.2,
            ];
            if delta != [0., 0.] {
                out.push(Action::ViewOrbit { delta });
            }
            panels.composition.orbit = Some(input.mouse);
            panels.cursor = Some(motion_ui::cursor::Cursor::Grabbing);
        } else {
            panels.composition.orbit = None;
        }
    }
    if input
        .pressed
        .is_some_and(|(b, p)| b == MouseButton::Left && v.contains(p))
        && !consumed
    {
        let point = input.pressed.unwrap().1;
        out.push(Action::SelectAt(mapping.local(point)));
    }
}
