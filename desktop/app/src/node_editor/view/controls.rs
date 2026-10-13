use super::super::inline::{Control, NumericEdit, Popup, CONTROL_ROW};
use super::*;
use motion_ui::widgets::{NumberEditor, NumberEvent, NumberOptions};

pub(super) fn control_rows(
    node: &Node,
    document: &GraphDocument,
    language: Language,
) -> Vec<(Rect, Control)> {
    if node.collapsed {
        return vec![];
    }
    let ports = node_ports(node, &document.public_parameters)
        .map(|p| p.inputs.len().max(p.outputs.len()))
        .unwrap_or(1);
    inline::controls(node, document, language)
        .into_iter()
        .enumerate()
        .map(|(index, control)| {
            (
                Rect::new(
                    10.,
                    HEADER + ports as f32 * ROW + 6. + index as f32 * CONTROL_ROW,
                    NODE_WIDTH - 20.,
                    CONTROL_ROW - 3.,
                ),
                control,
            )
        })
        .collect()
}
pub(super) fn number_rect(row: Rect, slider: bool) -> Rect {
    Rect::new(
        row.min[0] + 36.,
        row.min[1],
        row.width() - 36.,
        if slider {
            row.height() - 5.
        } else {
            row.height()
        },
    )
}
pub(super) fn slider_rect(row: Rect) -> Rect {
    Rect::new(row.min[0] + 36., row.max[1] - 5., row.width() - 36., 5.)
}
pub(super) fn screen_rect(local: Rect, card: &Card, zoom: f32) -> Rect {
    Rect::new(
        card.rect.min[0] + local.min[0] * zoom,
        card.rect.min[1] + local.min[1] * zoom,
        local.width() * zoom,
        local.height() * zoom,
    )
}

pub(super) fn transform_content(content: &mut PaintList, origin: [f32; 2], zoom: f32) {
    let point = |p: &mut [f32; 2]| {
        p[0] = origin[0] + p[0] * zoom;
        p[1] = origin[1] + p[1] * zoom;
    };
    for shape in &mut content.shapes {
        use motion_ui::paint::{Pattern, Shape};
        match shape {
            Shape::Quad { points } => points.iter_mut().for_each(&point),
            Shape::Line { points } | Shape::Triangles { points } => {
                points.iter_mut().for_each(&point)
            }
            Shape::Procedural { area, clip, kind } => {
                point(&mut area.min);
                point(&mut area.max);
                point(&mut clip.min);
                point(&mut clip.max);
                match kind {
                    Pattern::Box { width, radius, .. } => {
                        *width *= zoom;
                        *radius *= zoom;
                    }
                    Pattern::Shadow {
                        radius,
                        blur,
                        bounds,
                    } => {
                        *radius *= zoom;
                        *blur *= zoom;
                        point(&mut bounds.min);
                        point(&mut bounds.max);
                    }
                    Pattern::Checker { size, .. } => *size *= zoom,
                    Pattern::Grid {
                        origin: grid,
                        minor,
                        major_step,
                        ..
                    } => {
                        point(grid);
                        *minor *= zoom;
                        *major_step *= zoom;
                    }
                }
            }
        }
    }
    for glyph in &mut content.glyphs {
        point(&mut glyph.position);
        glyph.size[0] *= zoom;
        glyph.size[1] *= zoom;
    }
}

impl NodeEditor {
    pub(super) fn inline_input(
        &mut self,
        cards: &[Card],
        canvas: Rect,
        input: &Input,
        language: Language,
    ) -> bool {
        if self.inline.popup.is_some() {
            return true;
        }
        if let Some(edit) = &self.inline.number {
            let spec = cards.iter().find(|c| c.id == edit.owner).and_then(|card| {
                let node = self.document.nodes.iter().find(|n| n.id == card.id)?;
                control_rows(node, &self.document, Language::English)
                    .into_iter()
                    .find_map(|(row, control)| {
                        if let Control::Number {
                            binding,
                            value,
                            mut options,
                            slider,
                            ..
                        } = control
                        {
                            if binding == edit.binding {
                                if !edit.slider {
                                    options.speed /= self.view.zoom.max(1e-6) as f64;
                                }
                                let local = if edit.slider {
                                    slider_rect(row)
                                } else {
                                    number_rect(row, slider)
                                };
                                return Some((
                                    screen_rect(local, card, self.view.zoom),
                                    value,
                                    options,
                                ));
                            }
                        }
                        None
                    })
            });
            if let Some((rect, value, options)) = spec {
                self.update_inline_number(input, rect, value, options);
            } else {
                self.cancel_inline();
            }
            return true;
        }
        if self.drag.is_some() {
            return false;
        }
        let Some((MouseButton::Left, point)) =
            input.pressed.filter(|(_, point)| canvas.contains(*point))
        else {
            return false;
        };
        if input.modifiers.alt || input.modifiers.control && input.modifiers.shift {
            return false;
        }
        let Some(card) = cards.iter().rev().find(|card| card.rect.contains(point)) else {
            return false;
        };
        let node = self
            .document
            .nodes
            .iter()
            .find(|node| node.id == card.id)
            .unwrap()
            .clone();
        let collapse = Rect::new(
            card.rect.max[0] - 28. * self.view.zoom,
            card.rect.min[1] + 2. * self.view.zoom,
            25. * self.view.zoom,
            (HEADER - 4.) * self.view.zoom,
        );
        if collapse.contains(point) && input.modifiers == Default::default() {
            self.toggle_collapsed(&node.id);
            self.last_group_click = None;
            return true;
        }
        let Some((row, control)) = control_rows(&node, &self.document, language)
            .into_iter()
            .find(|(row, _)| screen_rect(*row, card, self.view.zoom).contains(point))
        else {
            return false;
        };
        if !matches!(control, Control::Number { .. })
            && (input.modifiers.control || input.modifiers.shift)
        {
            return false;
        }
        if self.commit_field().is_err() {
            return true;
        }
        self.search_focus = false;
        match control {
            Control::Number {
                binding,
                value,
                mut options,
                slider,
                ..
            } => {
                let sliding =
                    slider && screen_rect(slider_rect(row), card, self.view.zoom).contains(point);
                self.inline.number = Some(NumericEdit {
                    owner: node.id,
                    binding,
                    editor: NumberEditor::default(),
                    before: None,
                    slider: sliding,
                    in_popup: false,
                });
                if !sliding {
                    options.speed /= self.view.zoom.max(1e-6) as f64;
                }
                let rect = screen_rect(
                    if sliding {
                        slider_rect(row)
                    } else {
                        number_rect(row, slider)
                    },
                    card,
                    self.view.zoom,
                );
                self.update_inline_number(input, rect, value, options);
            }
            Control::Toggle {
                binding, checked, ..
            } => {
                let result = if let Some(Binding::ParamDefault(id, component)) = binding {
                    self.set_parameter_default(&id, component, if checked { 0. } else { 1. })
                } else {
                    self.set_operation(&node.id, NodeOp::Bool { value: !checked })
                };
                if let Err(error) = result {
                    self.notice = Some(error.to_string());
                }
            }
            Control::Color { bindings, value } => {
                self.inline.popup = Some(Popup::Color {
                    position: point,
                    node: node.id,
                    bindings,
                    before: self.state(true),
                    hue: control_popup::rgb_to_hsv(value)[0],
                    scroll: 0.,
                    drag: None,
                })
            }
            Control::Choice { items, .. } => {
                let selected = items
                    .iter()
                    .position(|item| item.operation == node.operation)
                    .unwrap_or(0);
                self.inline.popup = Some(Popup::Choice {
                    position: point,
                    node: node.id,
                    items,
                    selected,
                    scroll: 0.,
                });
            }
        }
        true
    }

    pub(super) fn update_inline_number(
        &mut self,
        input: &Input,
        rect: Rect,
        value: f32,
        options: NumberOptions,
    ) {
        let Some(mut edit) = self.inline.number.take() else {
            return;
        };
        let mut local = input.clone();
        local.keys = input
            .key_presses()
            .map(|(key, modifiers, _)| (key, modifiers))
            .collect();
        if edit.slider {
            if input.focus_lost || local.keys.iter().any(|(key, _)| *key == Key::Escape) {
                if let Some(before) = edit.before {
                    self.finish_inline_history(before, false);
                }
                return;
            }
            if (input.dragging() || input.released.is_some())
                && input.mouse.iter().all(|v| v.is_finite())
            {
                if edit.before.is_none() && !edit.in_popup {
                    edit.before = Some(self.state(true));
                }
                let ratio =
                    ((input.mouse[0] - rect.min[0]) / rect.width().max(1.)).clamp(0., 1.) as f64;
                let raw = options.min + ratio * (options.max - options.min);
                let step = options.speed.abs().max(0.000001);
                let next = (options.min + ((raw - options.min) / step).round() * step)
                    .clamp(options.min, options.max) as f32;
                if let Err(error) = self.preview_binding(&edit.binding, next) {
                    self.notice = Some(error.to_string());
                }
            }
            if input.released.is_some() || input.held != Some(MouseButton::Left) {
                if let Some(before) = edit.before {
                    self.finish_inline_history(before, true);
                }
                return;
            }
        } else {
            if edit.editor.text.active()
                && input
                    .pressed
                    .is_some_and(|(_, point)| !rect.contains(point))
            {
                local.keys.push((Key::Enter, Default::default()));
                local.pressed = None;
            }
            for event in edit.editor.update(value as f64, options, &local, rect) {
                match event {
                    NumberEvent::Begin { .. } => {
                        if !edit.in_popup {
                            edit.before = Some(self.state(true));
                        }
                    }
                    NumberEvent::Change { value } => {
                        if let Err(error) = self.preview_binding(&edit.binding, value as f32) {
                            edit.editor.text.error = Some(error.to_string());
                            self.notice = Some(error.to_string());
                        }
                    }
                    NumberEvent::Commit { .. } => {
                        if let Some(before) = edit.before.take() {
                            self.finish_inline_history(before, true);
                        }
                    }
                    NumberEvent::Cancel { .. } => {
                        if let Some(before) = edit.before.take() {
                            self.finish_inline_history(before, false);
                        }
                    }
                }
            }
            if !edit.editor.interacting() {
                return;
            }
        }
        self.inline.number = Some(edit);
    }

    pub(super) fn paint_controls(
        &self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        node: &Node,
        language: Language,
    ) {
        let styles = list.styles();
        let colors = styles.colors();

        for (row, control) in control_rows(node, &self.document, language) {
            match control {
                Control::Number {
                    label: name,
                    binding,
                    value,
                    options,
                    slider,
                } => {
                    label(
                        list,
                        atlas,
                        Rect::new(row.min[0], row.min[1], 33., row.height()),
                        &name,
                        colors.TEXT_MUTED,
                        Align::Left,
                    );
                    let number = self.inline.number.as_ref().filter(|edit| {
                        !edit.in_popup && edit.owner == node.id && edit.binding == binding
                    });
                    self.paint_inline_number(
                        list,
                        atlas,
                        number_rect(row, slider),
                        slider.then(|| slider_rect(row)),
                        value,
                        options,
                        number,
                    );
                }
                Control::Toggle {
                    label: name,
                    checked,
                    ..
                } => {
                    let box_rect = Rect::new(row.min[0], row.min[1] + 5., 14., 14.);
                    paint::rect(
                        list,
                        box_rect,
                        if checked {
                            colors.ACCENT
                        } else {
                            colors.SUNKEN
                        },
                    );
                    paint::stroke(list, box_rect, colors.BORDER, 1.);
                    if checked {
                        paint::line(
                            list,
                            vec![
                                [box_rect.min[0] + 3., box_rect.min[1] + 7.],
                                [box_rect.min[0] + 6., box_rect.min[1] + 10.],
                                [box_rect.min[0] + 11., box_rect.min[1] + 4.],
                            ],
                            colors.CHROME,
                        );
                    }
                    label(
                        list,
                        atlas,
                        Rect::new(
                            row.min[0] + 22.,
                            row.min[1],
                            row.width() - 22.,
                            row.height(),
                        ),
                        &name,
                        colors.TEXT,
                        Align::Left,
                    );
                }
                Control::Color { value, .. } => {
                    paint::checkerboard(
                        list,
                        row,
                        colors.TRANSPARENT_LIGHT,
                        colors.TRANSPARENT_DARK,
                        6.,
                    );
                    paint::rect(list, row, Color(value));
                    paint::stroke(list, row, colors.BORDER, 1.);
                }
                Control::Choice {
                    label: name, value, ..
                } => {
                    label(
                        list,
                        atlas,
                        Rect::new(row.min[0], row.min[1], 34., row.height()),
                        &name,
                        colors.TEXT_MUTED,
                        Align::Left,
                    );
                    let field = number_rect(row, false);
                    paint::rect(list, field, colors.SUNKEN);
                    paint::stroke(list, field, colors.BORDER, 1.);
                    let mut text = list.child();
                    label(
                        &mut text,
                        atlas,
                        Rect::new(
                            field.min[0] + 5.,
                            field.min[1],
                            field.width() - 20.,
                            field.height(),
                        ),
                        &value,
                        colors.TEXT,
                        Align::Left,
                    );
                    text.clip_to(field);
                    list.append(text);
                    ui::icon(
                        list,
                        Rect::new(field.max[0] - 17., field.min[1] + 5., 14., 14.),
                        ui::Icon::Chevron,
                    );
                }
            }
        }
    }
    pub(super) fn paint_inline_number(
        &self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        rect: Rect,
        slider: Option<Rect>,
        value: f32,
        options: NumberOptions,
        active: Option<&NumericEdit>,
    ) {
        let styles = list.styles();
        let colors = styles.colors();

        if let Some(edit) = active {
            edit.editor.draw(list, atlas, rect, value as f64, options);
        } else {
            NumberEditor::default().draw(list, atlas, rect, value as f64, options);
        }
        if let Some(slider) = slider {
            paint::rect(list, slider, colors.BORDER.with_alpha(0.5));
            let ratio = ((value as f64 - options.min) / (options.max - options.min).max(1e-9))
                .clamp(0., 1.) as f32;
            paint::rect(
                list,
                Rect::new(
                    slider.min[0],
                    slider.min[1] + 1.,
                    slider.width() * ratio,
                    (slider.height() - 2.).max(1.),
                ),
                colors.ACCENT.with_alpha(0.6),
            );
        }
    }
}
