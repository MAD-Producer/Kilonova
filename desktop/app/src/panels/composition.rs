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
    let v = viewer(area);
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
    let sheet = list.styles();
    let colors = sheet.colors();
    let controls = Rect::new(area.min[0], area.max[1] - 28., area.width(), 28.);
    paint::rect(list, controls, colors.CHROME);
    let pw = state["project"]["width"].as_f64().unwrap_or(1920.) as f32;
    let ph = state["project"]["height"].as_f64().unwrap_or(1080.) as f32;
    let scale = (v.width() / pw).min(v.height() / ph) * 100.;
    label(
        list,
        atlas,
        Rect::new(controls.min[0] + 8., controls.min[1], 60., 28.),
        &format!("{scale:.1}%"),
        true,
    );
    let fps = state["project"]["fps"].as_u64().unwrap_or(30) as u32;
    motion_ui::style::text_style(
        list,
        atlas,
        Rect::new(controls.min[0] + 72., controls.min[1], 116., 28.),
        &timecode(state["frame"].as_f64().unwrap_or(0.), fps),
        sheet.resolve("label.accent", Default::default()).unwrap(),
        Align::Left,
    );
    let (quality, next) = match state["preview"]["mode"].as_str().unwrap_or("auto") {
        "high" => (T::Full, 2),
        "balanced" => (T::Smooth, 3),
        "economy" => (T::Economy, 0),
        _ => (T::Auto, 1),
    };
    button(
        list,
        atlas,
        Rect::new(controls.max[0] - 250., controls.min[1] + 3., 68., 22.),
        panels.locale.t(quality),
        input,
        true,
        Action::PreviewMode(next),
        out,
    );
    label(
        list,
        atlas,
        Rect::new(controls.max[0] - 178., controls.min[1], 98., 28.),
        if panels.locale == Locale::Zh {
            "活动摄影机"
        } else {
            "Active camera"
        },
        true,
    );
    label(
        list,
        atlas,
        Rect::new(controls.max[0] - 78., controls.min[1], 74., 28.),
        if panels.locale == Locale::Zh {
            "1 个视图"
        } else {
            "1 view"
        },
        true,
    );
    let consumed = panels.path_controls.draw(
        list,
        atlas,
        v,
        state,
        panels.selected,
        input,
        panels.locale,
        out,
    );
    let consumed = if panels.path_controls.tool == super::path_controls::Tool::Select {
        panels
            .transform_controls
            .draw(list, v, state, panels.selected, input, out)
            || consumed
    } else {
        consumed
    };
    if let Some(cursor) = panels.transform_controls.cursor {
        panels.cursor = Some(cursor);
    }
    if input
        .pressed
        .is_some_and(|(b, p)| b == MouseButton::Left && v.contains(p))
        && !consumed
    {
        let point = input.pressed.unwrap().1;
        let w = state["project"]["width"].as_f64().unwrap_or(1.0);
        let h = state["project"]["height"].as_f64().unwrap_or(1.0);
        let scale = (v.width() as f64 / w).min(v.height() as f64 / h);
        let ox = v.min[0] as f64 + (v.width() as f64 - w * scale) * 0.5;
        let oy = v.min[1] as f64 + (v.height() as f64 - h * scale) * 0.5;
        out.push(Action::SelectAt([
            (point[0] as f64 - ox) / scale,
            (point[1] as f64 - oy) / scale,
        ]));
    }
}
