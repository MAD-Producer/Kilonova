use super::super::inline::{NumericEdit, Popup};
use super::*;
use motion_ui::style::{box_style, text_style, State, States};
use motion_ui::widgets::{NumberEditor, NumberOptions};

pub(super) fn rgb_to_hsv(color: [f32; 4]) -> [f32; 3] {
    let [r, g, b, _] = color;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let h = if delta <= 1e-6 {
        0.
    } else if max == r {
        ((g - b) / delta).rem_euclid(6.) / 6.
    } else if max == g {
        ((b - r) / delta + 2.) / 6.
    } else {
        ((r - g) / delta + 4.) / 6.
    };
    [h, if max <= 1e-6 { 0. } else { delta / max }, max]
}
pub(super) fn hsv_to_rgb(hsv: [f32; 3], alpha: f32) -> [f32; 4] {
    let [h, s, v] = hsv;
    let h = h.rem_euclid(1.) * 6.;
    let c = v * s;
    let x = c * (1. - (h.rem_euclid(2.) - 1.).abs());
    let m = v - c;
    let rgb = match h as u32 {
        0 => [c, x, 0.],
        1 => [x, c, 0.],
        2 => [0., c, x],
        3 => [0., x, c],
        4 => [x, 0., c],
        _ => [c, 0., x],
    };
    [rgb[0] + m, rgb[1] + m, rgb[2] + m, alpha]
}
pub(super) fn popup_rect(position: [f32; 2], area: Rect, size: [f32; 2]) -> Rect {
    let width = size[0].min(area.width().max(1.));
    let height = size[1].min(area.height().max(1.));
    Rect::new(
        position[0].clamp(area.min[0], area.max[0] - width),
        position[1].clamp(area.min[1], area.max[1] - height),
        width,
        height,
    )
}
/// Keep choice menus attached to their field, including when the canvas is zoomed.
pub(super) fn choice_popup_rect(anchor: Rect, area: Rect, size: [f32; 2]) -> Rect {
    let below = (area.max[1] - anchor.max[1]).max(0.);
    let above = (anchor.min[1] - area.min[1]).max(0.);
    let down = below >= size[1] || below >= above;
    let height = size[1].min(if down { below } else { above });
    popup_rect(
        [
            anchor.min[0],
            if down {
                anchor.max[1]
            } else {
                anchor.min[1] - height
            },
        ],
        area,
        [size[0], height],
    )
}
fn paint_popup(list: &mut PaintList, rect: Rect) {
    let styles = list.styles();
    let colors = styles.colors();

    paint::rect(
        list,
        Rect::new(
            rect.min[0] + 4.,
            rect.min[1] + 5.,
            rect.width(),
            rect.height(),
        ),
        Color::rgba(0, 0, 0, 0.4),
    );
    paint::rect(list, rect, colors.CHROME);
    paint::stroke(list, rect, colors.BORDER, 1.);
}
impl NodeEditor {
    fn popup_color_values(&self, bindings: &[Binding; 4]) -> [f32; 4] {
        std::array::from_fn(|i| {
            self.binding_value(&bindings[i]).unwrap_or_else(|| {
                self.observe_failure(
                    "popup.color_binding",
                    format_args!("component={i} binding={:?} fallback=0", bindings[i]),
                );
                0.
            })
        })
    }
    pub(super) fn paint_control_popup(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        input: &Input,
        language: Language,
    ) {
        let styles = list.styles();
        let colors = styles.colors();

        let Some(popup) = self.inline.popup.take() else {
            return;
        };
        let escape = input.focus_lost
            || input
                .key_presses()
                .any(|(key, _, repeat)| key == Key::Escape && !repeat);
        match popup {
            Popup::Choice {
                node,
                field,
                items,
                mut selected,
                mut scroll,
            } => {
                if escape || !self.document.nodes.iter().any(|n| n.id == node) {
                    return;
                }
                let Some(card) = self
                    .cards(self.last_canvas)
                    .into_iter()
                    .find(|card| card.id == node)
                else {
                    return;
                };
                let anchor = controls::screen_rect(field, &card, self.view.zoom);
                if !intersects(anchor, area) {
                    return;
                }
                let zoom = self.view.zoom;
                let padding = styles.metric("padding", 5.) * zoom;
                let row_height = styles.metric("node.dropdown.row.height", 28.) * zoom;
                let mut row_style = styles.resolve("row", States::default()).expect("row style");
                row_style.font_size = styles.metric("font.body", 12.) * zoom;
                let width = items
                    .iter()
                    .map(|item| {
                        atlas.measure(&item.label) * row_style.font_size / atlas.font_size()
                            + padding * 4.
                    })
                    .fold(anchor.width(), f32::max)
                    .min(styles.metric("dropdown.max_width", 320.) * zoom);
                let rect = choice_popup_rect(
                    anchor,
                    area,
                    [width, items.len() as f32 * row_height + padding * 2.],
                );
                if rect.height() < row_height + padding * 2. {
                    return;
                }
                if input
                    .pressed
                    .is_some_and(|(_, point)| !rect.contains(point) && !anchor.contains(point))
                {
                    return;
                }
                let content = Rect::new(
                    rect.min[0] + padding,
                    rect.min[1] + padding,
                    (rect.width() - padding * 2.).max(0.),
                    rect.height() - padding * 2.,
                );
                let max_scroll = (items.len() as f32 * row_height - content.height()).max(0.);
                scroll = scroll.clamp(0., max_scroll);
                if rect.contains(input.mouse) {
                    scroll = (scroll + input.wheel[1]).clamp(0., max_scroll);
                }
                let mut apply = None;
                for (key, modifiers, repeat) in input.key_presses() {
                    if modifiers != Default::default() {
                        continue;
                    }
                    match key {
                        Key::Up => selected = selected.saturating_sub(1),
                        Key::Down => selected = (selected + 1).min(items.len().saturating_sub(1)),
                        Key::Enter if !repeat => apply = Some(selected),
                        _ => continue,
                    }
                    let top = selected as f32 * row_height;
                    if top < scroll {
                        scroll = top;
                    } else if top + row_height > scroll + content.height() {
                        scroll = top + row_height - content.height();
                    }
                    scroll = scroll.clamp(0., max_scroll);
                }
                let mut menu = list.child();
                let mut popup_style = styles
                    .resolve("popup", States::default())
                    .expect("popup style");
                popup_style.border_width *= zoom;
                popup_style.radius *= zoom;
                popup_style.shadow_blur *= zoom;
                popup_style.shadow_offset_y *= zoom;
                box_style(&mut menu, rect, popup_style);
                let mut rows = list.child();
                for (index, item) in items.iter().enumerate() {
                    let row = Rect::new(
                        content.min[0],
                        content.min[1] + index as f32 * row_height - scroll,
                        content.width(),
                        row_height,
                    );
                    if !intersects(row, content) {
                        continue;
                    }
                    let mut states = States::default();
                    if index == selected {
                        states = states.with(State::Selected);
                    }
                    if row.contains(input.mouse) && content.contains(input.mouse) {
                        states = states.with(State::Hovered);
                    }
                    let mut style = styles.resolve("row", states).expect("row style");
                    style.font_size = row_style.font_size;
                    box_style(&mut rows, row, style);
                    text_style(
                        &mut rows,
                        atlas,
                        Rect::new(
                            row.min[0] + padding,
                            row.min[1],
                            (row.width() - padding * 2.).max(0.),
                            row.height(),
                        ),
                        &item.label,
                        style,
                        Align::Left,
                    );
                    if hit(input, row)
                        && input
                            .pressed
                            .is_some_and(|(_, point)| content.contains(point))
                    {
                        apply = Some(index);
                    }
                }
                rows.clip_to(content);
                menu.append(rows);
                menu.clip_to(area);
                list.append(menu);
                if let Some(index) = apply {
                    if let Some(item) = items.get(index) {
                        if let Err(error) = self.set_operation(&node, item.operation.clone()) {
                            self.notice = Some(error.to_string());
                        }
                    }
                } else {
                    self.inline.popup = Some(Popup::Choice {
                        node,
                        field,
                        items,
                        selected,
                        scroll,
                    });
                }
            }
            Popup::Color {
                position,
                node,
                bindings,
                before,
                mut hue,
                mut scroll,
                mut drag,
            } => {
                if escape || !self.document.nodes.iter().any(|n| n.id == node) {
                    self.inline.number = None;
                    self.finish_inline_history(before, false);
                    return;
                }
                let show_hsv = area.height() >= 290.;
                let rect = popup_rect(position, area, [284., if show_hsv { 290. } else { 180. }]);
                let square = Rect::new(
                    rect.min[0] + 12.,
                    rect.min[1] + 32.,
                    rect.width() - 54.,
                    94.,
                );
                let hue_rect = Rect::new(rect.max[0] - 32., square.min[1], 18., square.height());
                let form_top = rect.min[1] + if show_hsv { 138. } else { 32. };
                let form = Rect::new(
                    rect.min[0] + 10.,
                    form_top,
                    rect.width() - 20.,
                    (rect.max[1] - 36. - form_top).max(1.),
                );
                if self.inline.number.is_none() && drag.is_none() && form.contains(input.mouse) {
                    scroll = (scroll + input.wheel[1]).clamp(0., (112. - form.height()).max(0.));
                }
                let row_y = form_top - scroll;
                let had_number = self.inline.number.is_some();
                let mut color = self.popup_color_values(&bindings);
                let mut panel = list.child();
                paint_popup(&mut panel, rect);
                label(
                    &mut panel,
                    atlas,
                    Rect::new(rect.min[0] + 10., rect.min[1] + 4., rect.width() - 20., 24.),
                    language.text("颜色 · RGBA", "Color · RGBA"),
                    colors.TEXT,
                    Align::Left,
                );
                if show_hsv {
                    let hsv = rgb_to_hsv(color);
                    if self.inline.number.is_none() {
                        if let Some((MouseButton::Left, point)) =
                            input.pressed.filter(|(_, point)| rect.contains(*point))
                        {
                            if square.contains(point) {
                                drag = Some(false);
                            } else if hue_rect.contains(point) {
                                drag = Some(true);
                            }
                        }
                    }
                    if let Some(hue_drag) = drag {
                        if (input.dragging() || input.released.is_some())
                            && input.mouse.iter().all(|v| v.is_finite())
                        {
                            if !square.width().is_finite()
                                || square.width() <= 0.
                                || !square.height().is_finite()
                                || square.height() <= 0.
                                || !hue_rect.height().is_finite()
                                || hue_rect.height() <= 0.
                            {
                                self.observe_failure("popup.color_geometry", format_args!("square={square:?} hue_rect={hue_rect:?} hue_drag={hue_drag}"));
                            }
                            let next = if hue_drag {
                                hue = ((input.mouse[1] - hue_rect.min[1]) / hue_rect.height())
                                    .clamp(0., 1.);
                                hsv_to_rgb([hue, hsv[1], hsv[2]], color[3])
                            } else {
                                hsv_to_rgb(
                                    [
                                        hue,
                                        ((input.mouse[0] - square.min[0]) / square.width())
                                            .clamp(0., 1.),
                                        1. - ((input.mouse[1] - square.min[1]) / square.height())
                                            .clamp(0., 1.),
                                    ],
                                    color[3],
                                )
                            };
                            for i in 0..3 {
                                let options = self.color_number_options(&bindings[i]);
                                if let Err(error) = self.preview_binding(
                                    &bindings[i],
                                    next[i].clamp(options.min as f32, options.max as f32),
                                ) {
                                    self.notice = Some(error.to_string());
                                }
                            }
                            color = self.popup_color_values(&bindings);
                        }
                        if input.held != Some(MouseButton::Left) {
                            drag = None;
                        }
                    }
                    for y in 0..24 {
                        for x in 0..40 {
                            paint::rect(
                                &mut panel,
                                Rect::new(
                                    square.min[0] + square.width() * x as f32 / 40.,
                                    square.min[1] + square.height() * y as f32 / 24.,
                                    square.width() / 40. + 0.5,
                                    square.height() / 24. + 0.5,
                                ),
                                Color(hsv_to_rgb([hue, x as f32 / 39., 1. - y as f32 / 23.], 1.)),
                            );
                        }
                    }
                    for y in 0..48 {
                        paint::rect(
                            &mut panel,
                            Rect::new(
                                hue_rect.min[0],
                                hue_rect.min[1] + hue_rect.height() * y as f32 / 48.,
                                hue_rect.width(),
                                hue_rect.height() / 48. + 0.5,
                            ),
                            Color(hsv_to_rgb([y as f32 / 47., 1., 1.], 1.)),
                        );
                    }
                    let hsv = rgb_to_hsv(color);
                    paint::diamond(
                        &mut panel,
                        [
                            square.min[0] + hsv[1] * square.width(),
                            square.min[1] + (1. - hsv[2]) * square.height(),
                        ],
                        4.,
                        colors.TEXT,
                    );
                    paint::line(
                        &mut panel,
                        vec![
                            [
                                hue_rect.min[0] - 2.,
                                hue_rect.min[1] + hue * hue_rect.height(),
                            ],
                            [
                                hue_rect.max[0] + 2.,
                                hue_rect.min[1] + hue * hue_rect.height(),
                            ],
                        ],
                        colors.TEXT,
                    );
                }
                let mut active_rect = None;
                for i in 0..4 {
                    let row = Rect::new(
                        rect.min[0] + 12.,
                        row_y + i as f32 * 28.,
                        rect.width() - 24.,
                        25.,
                    );
                    let field = controls::number_rect(row, true);
                    let slider = controls::slider_rect(row);
                    let options = self.color_number_options(&bindings[i]);
                    if self
                        .inline
                        .number
                        .as_ref()
                        .is_some_and(|edit| edit.binding == bindings[i])
                    {
                        active_rect = Some((
                            if self.inline.number.as_ref().unwrap().slider {
                                slider
                            } else {
                                field
                            },
                            i,
                            options,
                        ));
                    }
                    if self.inline.number.is_none()
                        && drag.is_none()
                        && hit(input, row)
                        && form.contains(input.pressed.unwrap().1)
                    {
                        self.inline.number = Some(NumericEdit {
                            owner: node.clone(),
                            binding: bindings[i].clone(),
                            editor: NumberEditor::default(),
                            before: None,
                            slider: slider.contains(input.pressed.unwrap().1),
                            in_popup: true,
                        });
                        active_rect = Some((
                            if self.inline.number.as_ref().unwrap().slider {
                                slider
                            } else {
                                field
                            },
                            i,
                            options,
                        ));
                    }
                }
                if let Some((field, i, options)) = active_rect {
                    self.update_inline_number(input, field, color[i], options);
                }
                color = std::array::from_fn(|i| self.binding_value(&bindings[i]).unwrap_or(0.));
                if had_number && drag.is_none() {
                    let hsv = rgb_to_hsv(color);
                    if hsv[1] > 1e-6 {
                        hue = hsv[0];
                    }
                }
                let mut fields = list.child();
                for i in 0..4 {
                    let row = Rect::new(
                        rect.min[0] + 12.,
                        row_y + i as f32 * 28.,
                        rect.width() - 24.,
                        25.,
                    );
                    label(
                        &mut fields,
                        atlas,
                        Rect::new(row.min[0], row.min[1], 30., row.height()),
                        ["R", "G", "B", "A"][i],
                        colors.TEXT_MUTED,
                        Align::Left,
                    );
                    let active = self
                        .inline
                        .number
                        .as_ref()
                        .filter(|edit| edit.binding == bindings[i]);
                    self.paint_inline_number(
                        &mut fields,
                        atlas,
                        controls::number_rect(row, true),
                        Some(controls::slider_rect(row)),
                        color[i],
                        self.color_number_options(&bindings[i]),
                        active,
                    );
                }
                fields.clip_to(form);
                panel.append(fields);
                let cancel = Rect::new(rect.max[0] - 158., rect.max[1] - 32., 70., 25.);
                let apply = Rect::new(rect.max[0] - 80., rect.max[1] - 32., 68., 25.);
                let reject = ui::button(
                    &mut panel,
                    atlas,
                    cancel,
                    language.text("取消", "Cancel"),
                    input,
                    true,
                )
                .clicked;
                let accept = ui::button(
                    &mut panel,
                    atlas,
                    apply,
                    language.text("确定", "Apply"),
                    input,
                    true,
                )
                .clicked
                    || (!had_number
                        && input.key_presses().any(|(key, modifiers, repeat)| {
                            key == Key::Enter && modifiers == Default::default() && !repeat
                        }))
                    || (input
                        .pressed
                        .is_some_and(|(_, point)| !rect.contains(point))
                        && drag.is_none());
                panel.clip_to(rect);
                list.append(panel);
                if reject {
                    self.inline.number = None;
                    self.finish_inline_history(before, false);
                } else if accept && self.inline.number.is_none() {
                    self.finish_inline_history(before, true);
                } else {
                    self.inline.popup = Some(Popup::Color {
                        position,
                        node,
                        bindings,
                        before,
                        hue,
                        scroll,
                        drag,
                    });
                }
            }
        }
    }
    fn color_number_options(&self, binding: &Binding) -> NumberOptions {
        let mut min = 0.;
        let mut max = 1.;
        let mut speed = 0.005;
        if let Binding::ParamDefault(id, _) = binding {
            if let Some(p) = self.document.public_parameters.iter().find(|p| p.id == *id) {
                min = p.min as f64;
                max = p.max as f64;
                speed = p.step.max(0.001) as f64;
            }
        }
        NumberOptions {
            min,
            max,
            speed,
            decimals: 3,
        }
    }
}
