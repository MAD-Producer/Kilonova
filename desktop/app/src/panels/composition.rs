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
    let outer = styled_viewer(area, &sheet);
    let gap = sheet.metric("viewer.multiview.gap", 2.);
    let tiles = match observe!(
        "viewer.tiles",
        panels.viewer_views.settings.layout().tiles(outer, gap),
        "window={} phase=layout",
        panels.viewer_views.window
    ) {
        Ok(tiles) => tiles,
        Err(_) => return,
    };
    if tiles.is_empty() {
        return;
    }
    if panels.composition.pan.is_none()
        && !panels.tools.editing()
        && !panels.path_controls.editing()
        && !panels.transform_controls.gesture_active()
        && !panels.viewer_guides.dragging()
        && !panels.viewer_inspection.selecting()
        && !panels.viewer_views.popup()
    {
        let point = input
            .pressed
            .map(|(_, p)| p)
            .or_else(|| (input.wheel != [0., 0.]).then_some(input.mouse));
        if let Some(index) = point.and_then(|p| tiles.iter().position(|tile| tile.contains(p))) {
            panels.viewer_views.settings.activate(index);
        }
    }
    let active = panels.viewer_views.settings.active();
    let owner = (active, panels.viewer_views.window);
    if panels.viewer_views.loaded != Some(owner) {
        if panels.viewer_views.loaded.is_some() {
            out.extend(super::cancel_for_focus_loss(panels));
        }
        panels.composition.pan = None;
        panels.composition.zoom = Default::default();
        panels.composition.zoom_before = None;
        panels.composition.navigation = panels
            .viewer_views
            .settings
            .view(active)
            .unwrap()
            .navigation;
        panels.viewer_views.loaded = Some(owner);
    }
    panels.viewer_inspection.select_cell(owner.1 != 0, active);
    panels.viewer_guides.select_cell(owner.1 != 0, active);
    let key = panels.viewer_views.key(state);
    if let Some((source, tile, navigation)) = &panels.viewer_views.navigation_capture {
        if !source.same_context(&key) || *tile != tiles[active] {
            panels.composition.navigation = *navigation;
            panels.composition.pan = None;
            panels.viewer_views.navigation_capture = None;
        }
    }
    let projected = panels
        .viewer_views
        .cache
        .as_ref()
        .filter(|(cached, _)| {
            *cached == key || panels.tools.camera.editing() && cached.same_context(&key)
        })
        .and_then(|(_, views)| views.get(active))
        .cloned();
    // Ordinary composition geometry is already the live host state. Independent
    // cameras must wait for their exact projection rather than use another view.
    let ordinary = panels.viewer_views.settings.view(active).unwrap().mode
        == crate::viewer_views::Mode::Composition
        && state["observing"] != true;
    let ready = projected.is_some() || ordinary || panels.tools.camera.editing();
    let displayed = projected.as_ref().unwrap_or(state);
    let start = out.len();
    render_active(
        list,
        atlas,
        area,
        tiles[active],
        panels,
        displayed,
        input,
        ready,
        out,
    );
    if let Some((_, navigation)) = panels.composition.pan {
        if panels.viewer_views.navigation_capture.is_none() {
            panels.viewer_views.navigation_capture = Some((key.clone(), tiles[active], navigation));
        }
    } else {
        panels.viewer_views.navigation_capture = None;
    }
    panels
        .viewer_views
        .settings
        .view_mut(active)
        .unwrap()
        .navigation = panels.composition.navigation;
    for action in &mut out[start..] {
        if let Action::SelectAt(point) = action {
            *action = Action::ViewerPick {
                key: key.clone(),
                cell: active,
                point: *point,
            };
        } else if let Action::CameraGesture(op) = action {
            *action = Action::ViewerCamera {
                key: key.clone(),
                cell: active,
                action: op.clone(),
            };
        } else if let Action::PreviewDrag {
            object,
            delta,
            size,
        } = action
        {
            *action = Action::ViewerDrag {
                key: key.clone(),
                cell: active,
                object: *object,
                delta: *delta,
                size: *size,
            };
        }
    }
    if tiles.len() > 1 {
        let caption_height = sheet.metric("row.height", 22.);
        let padding = sheet.metric("padding.panel", 8.);
        let width = sheet.metric("viewer.view.width", 98.);
        let modes = super::composition_views::modes(state, panels.locale);
        for (i, tile) in tiles.iter().enumerate() {
            paint::stroke(
                list,
                *tile,
                sheet.color(if i == active { "focus" } else { "border" }),
                sheet.metric("border", 1.),
            );
            let rect = Rect::new(tile.min[0] + padding, tile.min[1], width, caption_height);
            paint::rect(list, rect, sheet.color("header"));
            let mode = panels.viewer_views.settings.view(i).unwrap().mode;
            let name = modes
                .iter()
                .find(|(value, _)| *value == mode)
                .map(|(_, name)| name.as_str())
                .unwrap_or("Camera");
            label(list, atlas, rect, name, i != active);
        }
    }
}


fn render_active(list:&mut PaintList,atlas:&mut TextAtlas,area:Rect,v:Rect,panels:&mut Panels,
    state:&Value,input:&Input,geometry_ready:bool,out:&mut Vec<Action>) {
    let sheet = list.styles();
    let outer = styled_viewer(area, &sheet);
    let inspection_input = panels.viewer_inspection.selection_input(input).clone();
    let surround = list.styles().color("field");
    // Paint the viewer surround, leaving its contents to the shared GPU renderer.
    for r in [
        Rect::new(
            area.min[0],
            area.min[1],
            area.width(),
            outer.min[1] - area.min[1],
        ),
        Rect::new(area.min[0], outer.min[1], outer.min[0] - area.min[0], outer.height()),
        Rect::new(outer.max[0], outer.min[1], area.max[0] - outer.max[0], outer.height()),
        Rect::new(area.min[0], outer.max[1], area.width(), area.max[1] - outer.max[1]),
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
        let restored=panels.composition.context.is_none().then_some(panels.composition.navigation);
        panels.composition = Default::default();
        panels.composition.context = Some(context);
        if let Some(navigation)=restored{panels.composition.navigation=navigation;}
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
    // A new pointer-down supersedes a lost release before footer controls can
    // change the view or enqueue another navigation operation.
    if panels.tools.editing() && input.pressed.is_some() {
        panels.tools.cancel();
        super::tools::camera::flush(panels,out);
    }
    let center = [(v.min[0] + v.max[0]) * 0.5, (v.min[1] + v.max[1]) * 0.5];
    let observing = state["observing"].as_bool().unwrap_or(false);
    let cancel_navigation = input.focus_lost || input.keys.iter().any(|(k, _)| *k == Key::Escape);
    if cancel_navigation {
        if let Some((_, before)) = panels.composition.pan.take() {
            panels.composition.navigation = before;
        }
    } else if !panels.path_controls.editing() && !panels.transform_controls.gesture_active() && !panels.viewer_guides.dragging()
        && !panels.viewer_inspection.selecting() {
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
        if input.wheel[1] != 0. && v.contains(input.mouse) && panels.composition.pan.is_none() && !panels.tools.editing() {
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
    if cancel_navigation { super::tools::camera::cancel(panels,out); }
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
    let overlay_width = sheet.metric("viewer.overlay.button.width", 64.);
    let overlay_rect = Rect::new(fit_rect.max[0] + gap, fit_rect.min[1], overlay_width, button_height);
    let overlay_fits = overlay_rect.max[0] <= controls.max[0] - padding;
    let overlay_label = match (panels.locale == Locale::Zh, panels.viewer_guides.current_channel()) {
        (true, motion_render::ViewChannel::Color) => "辅助显示",
        (false, motion_render::ViewChannel::Color) => "Overlays",
        (true, motion_render::ViewChannel::Red) => "辅助 R",
        (true, motion_render::ViewChannel::Green) => "辅助 G",
        (true, motion_render::ViewChannel::Blue) => "辅助 B",
        (true, motion_render::ViewChannel::Alpha) => "辅助 A",
        (false, motion_render::ViewChannel::Red) => "View R",
        (false, motion_render::ViewChannel::Green) => "View G",
        (false, motion_render::ViewChannel::Blue) => "View B",
        (false, motion_render::ViewChannel::Alpha) => "View A",
    };
    if overlay_fits && motion_ui::ui::button(list, atlas, overlay_rect,
        overlay_label, input, true).clicked {
        panels.viewer_guides.open(overlay_rect.min, state);
    }
    let left_end = if overlay_fits { overlay_rect.max[0] } else { fit_rect.max[0] };
    let count = count_width + gap;
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
    let camera_width = sheet.metric("viewer.camera.width", 24.);
    let camera_rect = Rect::new(quality_rect.min[0] - gap - camera_width, quality_rect.min[1], camera_width, button_height);
    if camera_rect.min[0] > left_end + gap {
        super::tools::camera::button(list, camera_rect, panels, state, input, out);
    }
    if camera_rect.min[0] - gap - left_end >= time_width {
        let fps = state["project"]["fps"].as_u64().unwrap_or(30) as u32;
        motion_ui::style::text_style(
            list,
            atlas,
            Rect::new(left_end + gap, controls.min[1], time_width, height),
            &timecode(state["frame"].as_f64().unwrap_or(0.), fps),
            sheet.resolve("label.accent", Default::default()).unwrap(),
            Align::Left,
        );
    }
    panels.viewer_views.fields(list,atlas,area,
        Rect::new(right-count_width,controls.min[1]+(height-button_height)*0.5,count_width,button_height),
        view_rect, quality_rect, state,panels.locale,input);
    let neutral_geometry=Input::default();
    let input=if geometry_ready {input} else {&neutral_geometry};
    let canvas_clip = panels.viewer_guides.canvas(v,&sheet);
    if !observing { panels.effect_drag.target(canvas_clip, None); }
    let Some(mapping) = panels.composition.navigation.mapping(v, canvas) else {
        return;
    };
    let mut inspection = list.child();
    let inspection_owned = panels.viewer_inspection.viewport(&mut inspection, canvas_clip, mapping, canvas, &inspection_input);
    let guide_neutral = Input::default();
    let guide_input = if inspection_owned { &guide_neutral } else { input };
    let mut handles = list.child();
    let handles_visible = panels.viewer_guides.visibility.handles;
    let handle_hit = handles_visible && input.pressed.is_some_and(|(button,point)| button == MouseButton::Left && canvas_clip.contains(point)
        && panels.transform_controls.handle_hit(state,panels.selected,mapping.image,point,&sheet));
    let guide_tool = super::tools::active(panels) == super::tools::Active::Select
        || input.pressed.is_some_and(|(button,point)| button == MouseButton::Left && panels.viewer_guides.ruler_hit(v,point,&sheet));
    let guide_owned = panels.viewer_guides.viewport(&mut handles, atlas, v, mapping, canvas, state, guide_input,
        panels.locale, !panels.path_controls.editing() && !panels.transform_controls.gesture_active()
            && !panels.tools.editing() && panels.composition.pan.is_none() && !handle_hit && guide_tool, out);
    if let Some(notice) = panels.viewer_guides.take_notice(panels.locale) { panels.message = Some(notice); }
    if guide_owned || inspection_owned { handles.clip_to(v); list.append(handles); list.append(inspection); return; }
    let mut editing = handles.child();
    let tool_owned = super::tools::viewport(panels, &mut editing, v, mapping, state, input, out);
    let consumed = tool_owned || panels.path_controls.draw_with_handles(
        &mut editing,
        atlas,
        canvas_clip,
        mapping.image,
        state,
        panels.selected,
        input,
        panels.locale,
        handles_visible,
        out,
    );
    let consumed = if handles_visible && !tool_owned && panels.path_controls.tool == super::path_controls::Tool::Select {
        // Build automatic geometry targets only while an input gesture needs
        // them; idle paints do not traverse other layers for transform snapping.
        let snapping = if panels.transform_controls.gesture_active() || input.pressed.is_some() {
            panels.viewer_guides.transform_snapping(state, mapping, canvas, panels.selected, &sheet)
        } else { None };
        panels.transform_controls.draw_with_guides(
            &mut editing,
            canvas_clip,
            mapping.image,
            state,
            panels.selected,
            input,
            snapping.as_ref(),
            out,
        ) || consumed
    } else {
        consumed
    };
    if handles_visible && !tool_owned && panels.path_controls.tool == super::path_controls::Tool::Select {
        panels.transform_controls.paint_spatial_labels(&mut editing, atlas);
    }
    if let Some(notice) = panels.transform_controls.take_notice(panels.locale) { panels.message = Some(notice); }
    editing.clip_to(canvas_clip);
    handles.append(editing);
    handles.clip_to(v);
    list.append(handles);
    list.append(inspection);
    if let Some(cursor) = panels
        .transform_controls
        .cursor
        .filter(|_| !tool_owned && panels.composition.pan.is_none())
    {
        panels.cursor = Some(cursor);
    }
    if input
        .pressed
        .is_some_and(|(b, p)| b == MouseButton::Left && canvas_clip.contains(p))
        && !consumed
    {
        let point = input.pressed.unwrap().1;
        out.push(Action::SelectAt(mapping.local(point)));
    }
}
