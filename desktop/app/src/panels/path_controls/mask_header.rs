//! Shared mask-card actions and the timeline's bounded add-menu state.
use super::*;
use motion_ui::{
    dock::Panel,
    icons::Icon,
    widgets::{Checkbox, Dropdown, DropdownItem},
};

pub(super) struct AddMenu {
    context: (String, String, u64),
    anchor: Rect,
    dropdown: Dropdown,
    visible: bool,
    opened_now: bool,
}

impl AddMenu {
    pub(super) fn belongs_to(&self, state: &Value, object: u64) -> bool {
        self.context.0 == state["root"].as_str().unwrap_or("")
            && self.context.1 == state["composition"].as_str().unwrap_or("")
            && self.context.2 == object
    }
}

pub(in crate::panels) fn action_rects(
    styles: &motion_ui::style::StyleSheet,
    area: Rect,
    y: f32,
    height: f32,
) -> [Rect; 4] {
    let slot = styles.metric("icon.slot", 20.);
    let padding = styles.metric("padding.panel", 8.);
    std::array::from_fn(|index| {
        Rect::new(
            area.max[0] - padding - slot * (4 - index) as f32,
            y,
            slot,
            height,
        )
    })
}

impl Controls {
    pub(in crate::panels) fn timeline_mask_add_open(&self) -> bool {
        self.timeline_add_mask_menu.is_some()
    }

    pub(in crate::panels) fn dismiss_timeline_mask_add(&mut self) {
        self.timeline_add_mask_menu = None;
    }

    pub(super) fn mask_header_actions(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: f32,
        height: f32,
        owner: mask_keys::Owner,
        mask: &LayerMask,
        input: &Input,
        out: &mut Vec<Action>,
    ) {
        for (index, rect) in action_rects(&list.styles(), area, y, height)
            .into_iter()
            .enumerate()
        {
            let pressed = if index == 0 {
                Checkbox::draw(
                    list,
                    atlas,
                    rect,
                    "",
                    mask.enabled,
                    input,
                    !owner.locked,
                    false,
                    None,
                )
                .clicked
            } else {
                super::mask_inspector::icon(
                    list,
                    input,
                    rect,
                    [Icon::Pen, Icon::Key, Icon::Close][index - 1],
                    false,
                    !owner.locked,
                )
            };
            if !pressed {
                continue;
            }
            match index {
                0 => out.push(Action::Edit(Command::Mask {
                    object: owner.object,
                    action: MaskAction::Options {
                        mask: mask.id,
                        mode: None,
                        inverted: None,
                        enabled: Some(!mask.enabled),
                    },
                })),
                1 => {
                    if self.mask != Some(mask.id) {
                        self.selected = None;
                    }
                    self.edit_mask_path(mask.id, out);
                }
                2 => out.push(Action::MaskTimeline {
                    object: owner.object,
                    mask: mask.id,
                }),
                _ => out.push(Action::Edit(Command::Mask {
                    object: owner.object,
                    action: MaskAction::Remove { mask: mask.id },
                })),
            }
        }
    }

    pub(in crate::panels) fn begin_timeline_mask_headers(&mut self) {
        if let Some(menu) = &mut self.timeline_add_mask_menu {
            menu.visible = false;
        }
    }

    pub(super) fn timeline_mask_add_button(
        &mut self,
        list: &mut PaintList,
        rect: Rect,
        object: u64,
        state: &Value,
        input: &Input,
        allowed: bool,
        out: &mut Vec<Action>,
    ) -> bool {
        self.inspector_add = Some((Panel::Timeline, rect));
        if let Some(menu) = &mut self.timeline_add_mask_menu {
            if menu.context.2 == object {
                menu.anchor = rect;
                menu.visible = true;
            }
        }
        if !ui::button_icon(list, rect, Icon::Plus, input, allowed).clicked {
            return false;
        }
        if self.cancel() {
            out.push(Action::History(4));
        }
        let mut dropdown = Dropdown::default();
        dropdown.open = true;
        self.timeline_add_mask_menu = Some(AddMenu {
            context: (
                state["root"].as_str().unwrap_or("").into(),
                state["composition"].as_str().unwrap_or("").into(),
                object,
            ),
            anchor: rect,
            dropdown,
            visible: true,
            opened_now: true,
        });
        true
    }

    pub(super) fn timeline_mask_add_overlay(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        bounds: Rect,
        state: &Value,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        let Some(mut menu) = self.timeline_add_mask_menu.take() else {
            return;
        };
        if !menu.visible
            || menu.context.0 != state["root"].as_str().unwrap_or("")
            || menu.context.1 != state["composition"].as_str().unwrap_or("")
            || state["observing"] == true
        {
            return;
        }
        let Some(value) = state["project"]["layers"].as_array().and_then(|layers| {
            layers
                .iter()
                .find(|l| l["id"].as_u64() == Some(menu.context.2))
        }) else {
            return;
        };
        let Some(layer) = self.inspector_layer(state, value) else {
            return;
        };
        if layer.locked
            || matches!(
                layer.content,
                Content::Adjustment | Content::Null | Content::Audio { .. }
            )
        {
            return;
        }
        if !menu.opened_now && click(input, menu.anchor) {
            return;
        }
        menu.opened_now = false;
        let labels = if locale == Locale::Zh {
            ["钢笔蒙版", "矩形蒙版"]
        } else {
            ["Pen mask", "Rectangle mask"]
        };
        let items: Vec<_> = labels
            .into_iter()
            .map(|label| DropdownItem {
                label,
                enabled: true,
            })
            .collect();
        match menu
            .dropdown
            .draw_menu(list, atlas, menu.anchor, bounds, &items, input, true)
        {
            Some(0) => self.choose(Tool::Mask, out),
            Some(_) => self.add_rectangle_mask(&layer, locale, out),
            None if menu.dropdown.open => self.timeline_add_mask_menu = Some(menu),
            None => (),
        }
    }
}
