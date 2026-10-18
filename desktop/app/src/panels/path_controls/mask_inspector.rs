//! Compact mask properties; keyframe geometry belongs to the shared timeline.
use super::*;
use motion_ui::{
    dock::Panel,
    icons::{self, Icon},
    widgets::{Checkbox, Dropdown, DropdownItem},
};

pub(super) fn spec(index: u8, locale: Locale) -> (MaskProperty, &'static str, [f32; 2]) {
    match (index, locale == Locale::Zh) {
        (mask_path::CHANNEL, zh) => (
            MaskProperty::Expansion,
            if zh { "蒙版路径" } else { "Mask path" },
            [0., 0.],
        ),
        (0, zh) => (
            MaskProperty::Opacity,
            if zh { "不透明度 %" } else { "Opacity %" },
            [0., 100.],
        ),
        (1, zh) => (
            MaskProperty::Feather,
            if zh { "羽化 X / Y" } else { "Feather X / Y" },
            [0., 32000.],
        ),
        (_, zh) => (
            MaskProperty::Expansion,
            if zh { "扩展 px" } else { "Expansion px" },
            [-32000., 32000.],
        ),
    }
}
/// One inline row contract for the inspector and shared timeline mask rows.
pub(super) fn property_row(
    styles: &motion_ui::style::StyleSheet,
    area: Rect,
    y: f32,
    axis: usize,
    axes: usize,
    host: Panel,
) -> crate::panels::timeline::PropertyLayout {
    let row = styles.metric("row.height", 22.);
    let indent = if host == Panel::Properties {
        styles.metric("padding.panel", 8.)
    } else {
        styles.metric("tree.indent", 14.)
    };
    if matches!(host, Panel::Timeline | Panel::Curves) {
        let mut layout = crate::panels::timeline::property_layout(styles, area, y, row, axis, axes);
        layout.watch.min[0] += indent * 2.;
        layout.watch.max[0] += indent * 2.;
        layout.caption.min[0] += indent * 2.;
        layout.caption.max[0] = layout.caption.max[0].max(layout.caption.min[0]);
        return layout;
    }
    let padding = styles.metric("padding.panel", 8.);
    let slot = styles.metric("icon.slot", 20.);
    let field = styles.metric("field.height", 18.).min(row);
    let start = area.min[0] + padding + indent * 2.;
    let value = if host == Panel::Properties {
        area.min[0]
            + (padding + styles.metric("label.column", 72.) + slot * 2.).min(area.width() * 0.52)
    } else {
        area.min[0] + area.width() * 0.52
    };
    let cell = ((area.max[0] - padding - value) / axes.max(1) as f32)
        .min(styles.metric("value.min_width", 64.));
    crate::panels::timeline::PropertyLayout {
        watch: Rect::new(start, y, slot, row),
        caption: Rect::new(
            start + slot,
            y,
            (value - start - slot - padding).max(0.),
            row,
        ),
        number: Rect::new(
            value + axis as f32 * cell,
            y + (row - field) * 0.5,
            (cell - styles.metric("border", 1.) * 2.).max(0.),
            field,
        ),
    }
}
fn icon(
    list: &mut PaintList,
    input: &Input,
    rect: Rect,
    kind: Icon,
    active: bool,
    enabled: bool,
) -> bool {
    let mut states = motion_ui::style::States::default();
    if active {
        states = states.with(motion_ui::style::State::Selected);
    }
    if !enabled {
        states = states.with(motion_ui::style::State::Disabled);
    }
    let styles = list.styles();
    icons::button(list, rect, kind, input, styles.as_ref(), states).clicked
}
struct ModeRow {
    caption: Rect,
    mode: Rect,
    invert: Rect,
    invert_label: bool,
}
fn mode_row(
    styles: &motion_ui::style::StyleSheet,
    area: Rect,
    y: f32,
    invert_text: f32,
    host: Panel,
) -> ModeRow {
    let row = styles.metric("row.height", 22.);
    let field = styles.metric("field.height", 18.).min(row);
    let padding = styles.metric("padding.panel", 8.);
    let slot = styles.metric("icon.slot", 20.);
    let gap = styles.metric("padding", 5.);
    let fields = property_row(styles, area, y, 0, 1, host);
    let name = fields.caption.min[0];
    let right = area.max[0] - padding;
    let available = (right - name).max(0.);
    let minimum = styles.metric("value.min_width", 64.);
    let full_invert = styles.metric("checkbox.size", 14.) + gap + invert_text;
    let invert_label = available >= minimum + gap + full_invert;
    let invert_width = if invert_label { full_invert } else { slot }.min(available);
    let max_mode = (available - gap - invert_width).max(0.);
    let preferred = fields.number.min[0];
    let width = (right - preferred - gap - invert_width)
        .max(minimum.min(max_mode))
        .min(max_mode)
        .min(styles.metric("dropdown.menu_min_width", 160.));
    let x = preferred.min(right - width - gap - invert_width).max(name);
    ModeRow {
        caption: Rect::new(name, y, (x - gap - name).max(0.), row),
        mode: Rect::new(x, y + (row - field) * 0.5, width, field),
        invert: Rect::new(x + width + gap, y, invert_width, row),
        invert_label,
    }
}
impl Controls {
    pub(super) fn path_value(
        &mut self,
        list: &mut PaintList,
        rect: Rect,
        owner: mask_keys::Owner,
        mask: u64,
        input: &Input,
        out: &mut Vec<Action>,
    ) {
        if icon(
            list,
            input,
            rect,
            Icon::Pen,
            self.tool == Tool::Mask && self.mask == Some(mask),
            !owner.locked,
        ) {
            self.edit_mask_path(mask, out);
        }
    }
    pub(super) fn edit_mask_path(&mut self, mask: u64, out: &mut Vec<Action>) {
        if self.cancel() {
            out.push(Action::History(4));
        }
        self.tool = Tool::Mask;
        self.mask = Some(mask);
    }
    pub(super) fn mask_number(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        rect: Rect,
        state: &Value,
        owner: mask_keys::Owner,
        mask: u64,
        property: MaskProperty,
        axis: usize,
        values: &[f32],
        range: [f32; 2],
        host: Panel,
        input: &Input,
        out: &mut Vec<Action>,
    ) {
        let value = values[axis];
        let enabled = !owner.locked;
        let active = self.parameter.as_ref().is_some_and(|p| {
            p.host == host
                && p.object == owner.object
                && p.mask == mask
                && p.axis == axis
                && std::mem::discriminant(&p.property) == std::mem::discriminant(&property)
        });
        if active {
            let p = self.parameter.as_mut().unwrap();
            p.rect = rect;
            let styles = list.styles();
            let style = styles
                .resolve(
                    "field.number",
                    motion_ui::style::States::default().with(motion_ui::style::State::Focused),
                )
                .unwrap();
            motion_ui::style::box_style(list, rect, style);
            motion_ui::style::text_style(
                list,
                atlas,
                rect.shrink(style.padding),
                &p.text,
                style,
                Align::Left,
            );
        } else {
            crate::panels::numeric::passive_with_precision(
                list,
                atlas,
                rect,
                f64::from(value),
                input,
                enabled,
                2,
            );
        }
        if click(input, rect) && enabled && !active {
            if self.parameter.take().is_some_and(|p| p.begun) {
                out.push(Action::History(3));
            }
            self.parameter = Some(Parameter {
                host,
                object: owner.object,
                mask,
                property,
                axis,
                values: values.to_vec(),
                text: format!("{value:.2}"),
                replace: true,
                rect,
                start: Some(input.pressed.unwrap().1[0]),
                begun: false,
                last_value: value,
                range,
            });
            self.parameter_context = Some((
                state["root"].as_str().unwrap_or("").into(),
                state["composition"].as_str().unwrap_or("").into(),
                state["frame"].as_f64().unwrap_or(0.).floor() as u32,
            ));
            out.push(Action::Seek(state["frame"].as_f64().unwrap_or(0.)));
        }
    }
    fn add_rectangle_mask(&mut self, layer: &Layer, locale: Locale, out: &mut Vec<Action>) {
        if self.cancel() {
            out.push(Action::History(4));
        }
        let id = layer.masks.iter().map(|m| m.id).max().unwrap_or(0) + 1;
        let [w, h] = layer.size;
        let path = VectorPath {
            id: 1,
            closed: true,
            nodes: [
                [w * 0.1, h * 0.1],
                [w * 0.9, h * 0.1],
                [w * 0.9, h * 0.9],
                [w * 0.1, h * 0.9],
            ]
            .into_iter()
            .enumerate()
            .map(|(i, p)| PathNode {
                id: i as u64 + 1,
                geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
            })
            .collect(),
        };
        let mut mask = LayerMask::new(id, path);
        mask.name = format!("{} {id}", locale.t(T::Mask));
        out.push(Action::Edit(Command::Mask {
            object: layer.id,
            action: MaskAction::Add { mask },
        }));
        self.tool = Tool::Mask;
        self.mask = Some(id);
    }
    pub(super) fn compact_masks(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: &mut f32,
        layer: &Layer,
        state: &Value,
        input: &Input,
        locale: Locale,
        host: Panel,
        out: &mut Vec<Action>,
    ) {
        let context = (
            state["root"].as_str().unwrap_or("").into(),
            state["composition"].as_str().unwrap_or("").into(),
            layer.id,
            host,
        );
        if self.inspector_context.as_ref() != Some(&context) {
            if self.cancel() {
                out.push(Action::History(4));
            }
            self.mode_dropdowns.clear();
            self.inspector_context = Some(context);
        }
        self.keys.prepare(state, layer, input, out);
        let mode_owner = self
            .mode_dropdowns
            .iter()
            .find(|(_, d)| d.open)
            .map(|(key, _)| *key);
        let neutral = Input::default();
        let original = input;
        let input = if self.add_mask_menu.open || mode_owner.is_some() || self.keys.modal() {
            &neutral
        } else {
            input
        };
        let styles = list.styles();
        let colors = styles.colors();
        let row = styles.metric("row.height", 22.);
        let slot = styles.metric("icon.slot", 20.);
        let padding = styles.metric("padding.panel", 8.);
        let indent = if host == Panel::Properties {
            styles.metric("padding.panel", 8.)
        } else {
            styles.metric("icon.slot", 20.)
        };
        let gap = styles.metric("padding", 5.);
        let mut overlays = list.child();
        let allowed = !layer.locked
            && !matches!(
                layer.content,
                Content::Adjustment | Content::Null | Content::Camera { .. } | Content::Audio { .. }
            )
            && state["observing"] != true;
        let heading = Rect::new(
            area.min[0] + padding,
            *y,
            (area.width() - padding * 2. - slot).max(0.),
            row,
        );
        icons::draw(
            list,
            Rect::new(heading.min[0], *y, slot, row),
            if self.masks_open {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            },
            slot,
            colors.TEXT_MUTED,
        );
        label(
            list,
            atlas,
            Rect::new(
                heading.min[0] + slot,
                *y,
                (heading.width() - slot).max(0.),
                row,
            ),
            &format!("{} ({})", locale.t(T::Mask), layer.masks.len()),
            colors.TEXT,
        );
        if click(input, heading) {
            if self.cancel() {
                out.push(Action::History(4));
            }
            self.masks_open = !self.masks_open;
        }
        let add = Rect::new(area.max[0] - padding - slot, *y, slot, row);
        self.inspector_add = Some((host, add));
        if ui::button_icon(
            list,
            add,
            Icon::Plus,
            if mode_owner.is_none() {
                original
            } else {
                input
            },
            allowed,
        )
        .clicked
        {
            let open = !self.add_mask_menu.open;
            if self.cancel() {
                out.push(Action::History(4));
            }
            self.add_mask_menu.open = open;
            self.masks_open = true;
        }
        let add_items: Vec<_> = if locale == Locale::Zh {
            ["钢笔蒙版", "矩形蒙版"]
        } else {
            ["Pen mask", "Rectangle mask"]
        }
        .into_iter()
        .map(|label| DropdownItem {
            label,
            enabled: allowed,
        })
        .collect();
        if let Some(index) = self.add_mask_menu.draw_menu(
            &mut overlays,
            atlas,
            add,
            area,
            &add_items,
            original,
            allowed,
        ) {
            if index == 0 {
                self.choose(Tool::Mask, out);
            } else {
                self.add_rectangle_mask(layer, locale, out);
            }
        }
        *y += row;
        if !self.masks_open {
            return;
        }
        let local = layer.local_frame(state["frame"].as_f64().unwrap_or(0.));
        let owner = mask_keys::Owner {
            locked: !allowed,
            ..mask_keys::Owner::from(layer)
        };
        for mask in &layer.masks {
            let closed = self.collapsed_mask_cards.contains(&(layer.id, mask.id));
            let heading = Rect::new(
                area.min[0] + padding + indent,
                *y,
                (area.width() - padding * 2. - indent - slot * 4.).max(0.),
                row,
            );
            icons::draw(
                list,
                Rect::new(heading.min[0], *y, slot, row),
                if closed {
                    Icon::ChevronRight
                } else {
                    Icon::ChevronDown
                },
                slot,
                colors.TEXT_MUTED,
            );
            label(
                list,
                atlas,
                Rect::new(
                    heading.min[0] + slot,
                    *y,
                    (heading.width() - slot).max(0.),
                    row,
                ),
                &mask.name,
                colors.TEXT,
            );
            if click(input, heading) {
                if self.cancel() {
                    out.push(Action::History(4));
                }
                if !self.collapsed_mask_cards.remove(&(layer.id, mask.id)) {
                    self.collapsed_mask_cards.insert((layer.id, mask.id));
                }
            }
            for (index, kind) in [Icon::Eye, Icon::Pen, Icon::Key, Icon::Close]
                .into_iter()
                .enumerate()
            {
                let rect = Rect::new(
                    area.max[0] - padding - slot * (4 - index) as f32,
                    *y,
                    slot,
                    row,
                );
                let pressed = if index == 0 {
                    Checkbox::draw(
                        list,
                        atlas,
                        rect,
                        "",
                        mask.enabled,
                        input,
                        allowed,
                        false,
                        None,
                    )
                    .clicked
                } else {
                    icon(list, input, rect, kind, false, allowed)
                };
                if pressed {
                    match index {
                        0 => out.push(Action::Edit(Command::Mask {
                            object: layer.id,
                            action: MaskAction::Options {
                                mask: mask.id,
                                mode: None,
                                inverted: None,
                                enabled: Some(!mask.enabled),
                            },
                        })),
                        1 => {
                            if self.cancel() {
                                out.push(Action::History(4));
                            }
                            self.tool = Tool::Mask;
                            self.mask = Some(mask.id);
                        }
                        2 => out.push(Action::MaskTimeline {
                            object: layer.id,
                            mask: mask.id,
                        }),
                        _ => out.push(Action::Edit(Command::Mask {
                            object: layer.id,
                            action: MaskAction::Remove { mask: mask.id },
                        })),
                    }
                }
            }
            *y += row;
            if closed {
                *y += gap;
                continue;
            }
            let modes = [
                MaskMode::None,
                MaskMode::Add,
                MaskMode::Subtract,
                MaskMode::Intersect,
                MaskMode::Lighten,
                MaskMode::Darken,
                MaskMode::Difference,
            ];
            let labels = if locale == Locale::Zh {
                ["无", "相加", "相减", "交集", "变亮", "变暗", "差值"]
            } else {
                [
                    "None",
                    "Add",
                    "Subtract",
                    "Intersect",
                    "Lighten",
                    "Darken",
                    "Difference",
                ]
            };
            let invert = if locale == Locale::Zh {
                "反转"
            } else {
                "Invert"
            };
            let mode_layout = mode_row(styles.as_ref(), area, *y, atlas.measure(invert), host);
            label(
                list,
                atlas,
                mode_layout.caption,
                if locale == Locale::Zh {
                    "模式"
                } else {
                    "Mode"
                },
                colors.TEXT_MUTED,
            );
            let cell = mode_layout.mode;
            let items: Vec<_> = labels
                .iter()
                .map(|label| DropdownItem {
                    label,
                    enabled: allowed,
                })
                .collect();
            let dropdown = self
                .mode_dropdowns
                .entry((layer.id, mask.id))
                .or_insert_with(Dropdown::default);
            if let Some(choice) = dropdown.draw_inline(
                &mut overlays,
                atlas,
                cell,
                area,
                &items,
                modes.iter().position(|m| *m == mask.mode).unwrap_or(1),
                if mode_owner == Some((layer.id, mask.id)) {
                    original
                } else {
                    input
                },
                allowed,
                mode_owner == Some((layer.id, mask.id)),
            ) {
                if modes[choice] != mask.mode {
                    out.push(Action::Edit(Command::Mask {
                        object: layer.id,
                        action: MaskAction::Options {
                            mask: mask.id,
                            mode: Some(modes[choice]),
                            inverted: None,
                            enabled: None,
                        },
                    }));
                }
            }
            if Checkbox::draw(
                list,
                atlas,
                mode_layout.invert,
                if mode_layout.invert_label { invert } else { "" },
                mask.inverted,
                input,
                allowed,
                false,
                None,
            )
            .clicked
            {
                out.push(Action::Edit(Command::Mask {
                    object: layer.id,
                    action: MaskAction::Options {
                        mask: mask.id,
                        mode: None,
                        inverted: Some(!mask.inverted),
                        enabled: None,
                    },
                }));
            }
            *y += row;
            for index in mask_path::CHANNELS {
                let (property, title, range) = spec(index, locale);
                let values = if index == mask_path::CHANNEL {
                    vec![]
                } else {
                    mask_keys::values(mask, index, local)
                };
                let fields = property_row(styles.as_ref(), area, *y, 0, values.len(), host);
                self.keys
                    .watch(list, fields.watch, state, owner, mask, index, input, out);
                label(list, atlas, fields.caption, title, colors.TEXT_MUTED);
                if index == mask_path::CHANNEL {
                    let rect = Rect::new(fields.number.min[0], *y, slot, row);
                    self.path_value(list, rect, owner, mask.id, input, out);
                }
                for axis in 0..values.len() {
                    let fields = property_row(styles.as_ref(), area, *y, axis, values.len(), host);
                    self.mask_number(
                        list,
                        atlas,
                        fields.number,
                        state,
                        owner,
                        mask.id,
                        property.clone(),
                        axis,
                        &values,
                        range,
                        host,
                        input,
                        out,
                    );
                }
                *y += row;
            }
            *y += gap;
        }
        list.append(overlays);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compact_mask_controls_stay_inside_narrow_panels_and_do_not_overlap() {
        let styles = PaintList::default().styles();
        for width in [160., 180., 220., 286., 400., 720.] {
            let area = Rect::new(17., 31., width, 600.);
            for text in [24., 40.] {
                let layout = mode_row(&styles, area, 90., text, Panel::Properties);
                assert!(layout.caption.max[0] <= layout.mode.min[0]);
                assert!(layout.mode.max[0] < layout.invert.min[0]);
                assert!(layout.invert.max[0] <= area.max[0]);
                assert!(layout.mode.width() >= styles.metric("icon.slot", 20.));
            }
            let x = property_row(&styles, area, 120., 0, 2, Panel::Properties);
            let y = property_row(&styles, area, 120., 1, 2, Panel::Properties);
            assert!(x.watch.max[0] <= x.caption.min[0]);
            assert!(x.caption.max[0] <= x.number.min[0]);
            assert!(x.number.max[0] <= y.number.min[0]);
            assert!(y.number.max[0] <= area.max[0]);
        }
    }
}
