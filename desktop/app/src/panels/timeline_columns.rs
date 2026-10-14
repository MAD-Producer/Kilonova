//! Editable timeline cells with one modal dropdown and typed core commands.
use super::*;
use motion_core::compositing::BlendMode;
use motion_ui::widgets::{Dropdown, DropdownItem};
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Column {
    Blend,
    Parent,
}
struct Popup {
    object: u64,
    column: Column,
    rect: Rect,
    dropdown: Dropdown,
    window: Option<Panel>,
}
#[derive(Default)]
pub(crate) struct Controls {
    popup: Option<Popup>,
    focused: Option<(u64, Column)>,
    seen: bool,
    opened: bool,
    context: Option<(String, String)>,
    window: Option<Panel>,
    deferred: bool,
}
impl Controls {
    pub fn editing(&self) -> bool {
        self.popup.is_some()
    }
    pub fn editing_in(&self, window: Option<Panel>) -> bool {
        self.popup.as_ref().is_some_and(|p| p.window == window)
    }
    pub(super) fn prepare_window(&mut self, window: Option<Panel>, input: &Input) {
        if self.editing() && !self.editing_in(window) && input.pressed.is_some() {
            self.cancel();
        }
        self.window = window;
        self.deferred = true;
        if self.editing_in(window) {
            self.seen = false;
        }
    }
    pub(super) fn finish_window(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        bounds: Rect,
        state: &Value,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        self.deferred = false;
        if self.editing_in(self.window) {
            self.overlay(list, atlas, bounds, state, input, locale, out);
        }
    }
    pub fn cancel(&mut self) {
        self.popup = None;
        self.focused = None;
    }
    pub(super) fn begin(&mut self, state: &Value) {
        let context = (
            state["root"].as_str().unwrap_or("").to_string(),
            state["composition"].as_str().unwrap_or("").to_string(),
        );
        if self.context.as_ref() != Some(&context) {
            self.cancel();
            self.context = Some(context);
        }
        self.seen = false;
        self.opened = false;
    }
    pub(super) fn cell(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        rect: Rect,
        object: u64,
        column: Column,
        caption: &str,
        input: &Input,
        enabled: bool,
    ) -> bool {
        let sheet = list.styles();
        let colors = sheet.colors();
        if self
            .popup
            .as_ref()
            .is_some_and(|p| p.object == object && p.column == column)
        {
            self.seen = true;
            if let Some(p) = &mut self.popup {
                p.rect = rect;
            }
            if !enabled {
                self.cancel();
            }
        }
        if enabled && rect.contains(input.mouse) {
            paint::rect(list, rect, colors.TEXT.with_alpha(0.06));
        }
        let focused = self.focused == Some((object, column));
        if focused {
            paint::stroke(list, rect, colors.ACCENT.with_alpha(0.65), 0.5);
        }
        ui::text(
            list,
            atlas,
            Rect::new(
                rect.min[0] + 4.,
                rect.min[1],
                (rect.width() - 22.).max(1.),
                rect.height(),
            ),
            caption,
            if enabled {
                colors.TEXT_MUTED
            } else {
                sheet.color("disabled").with_alpha(0.6)
            },
            Align::Left,
        );
        let x = rect.max[0] - 11.;
        let y = rect.min[1] + rect.height() * 0.5;
        paint::line(
            list,
            vec![[x - 3., y - 1.], [x, y + 2.], [x + 3., y - 1.]],
            if enabled {
                colors.TEXT_MUTED
            } else {
                sheet.color("disabled").with_alpha(0.4)
            },
        );
        if enabled && clicked(input, rect) {
            self.focused = Some((object, column));
            self.popup = Some(Popup {
                object,
                column,
                rect,
                dropdown: Dropdown::default(),
                window: self.window,
            });
            self.seen = true;
            self.opened = true;
            return true;
        }
        false
    }
    pub(super) fn overlay(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        bounds: Rect,
        state: &Value,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        if self.deferred || self.popup.as_ref().is_some_and(|p| p.window != self.window) {
            return;
        }
        if !self.seen || input.focus_lost {
            self.cancel();
            return;
        }
        let Some(popup) = &mut self.popup else {
            return;
        };
        let Some(owner) = owner(state, popup.object) else {
            self.cancel();
            return;
        };
        let (labels, selected, parents) = match popup.column {
            Column::Blend => {
                let current = blend(owner);
                let modes = blend_modes(locale);
                (
                    modes
                        .iter()
                        .map(|(_, label)| label.to_string())
                        .collect::<Vec<_>>(),
                    modes
                        .iter()
                        .position(|(mode, _)| *mode == current)
                        .unwrap_or(0),
                    vec![],
                )
            }
            Column::Parent => {
                let choices = super::parent_controls::menu_entries(state, popup.object, locale);
                let current = owner["parent"]["object"].as_u64();
                let selected = choices
                    .iter()
                    .position(|(id, _, _)| *id == current)
                    .unwrap_or(0);
                let labels = choices.iter().map(|(_, name, _)| name.clone()).collect();
                let parents = choices.iter().map(|(id, _, _)| *id).collect();
                (labels, selected, parents)
            }
        };
        let allowed = if popup.column == Column::Parent {
            super::parent_controls::menu_entries(state, popup.object, locale)
                .into_iter()
                .map(|(_, _, enabled)| enabled)
                .collect::<Vec<_>>()
        } else {
            vec![true; labels.len()]
        };
        let items: Vec<_> = labels
            .iter()
            .zip(&allowed)
            .map(|(label, enabled)| DropdownItem {
                label,
                enabled: *enabled,
            })
            .collect();

        // Let the shared widget initialize its keyboard highlight on the opening frame.
        let first = Input {
            pressed: Some((
                MouseButton::Left,
                [popup.rect.min[0] + 1., popup.rect.min[1] + 1.],
            )),
            mouse: input.mouse,
            ..Default::default()
        };
        let choice = popup.dropdown.draw_grouped(
            list,
            atlas,
            popup.rect,
            bounds,
            &items,
            selected,
            if self.opened { &first } else { input },
            true,
            true,
            if popup.column == Column::Blend {
                &[1, 4, 8, 11]
            } else {
                &[1]
            },
            true,
        );
        if let Some(index) = choice {
            let command = match popup.column {
                Column::Blend => {
                    let mode = blend_modes(locale)[index].0;
                    if mode == blend(owner) {
                        None
                    } else {
                        Some(Command::SetLayerBlend {
                            object: popup.object,
                            mode: Some(mode),
                            space: None,
                        })
                    }
                }
                Column::Parent => {
                    let parent = parents[index];
                    if parent == owner["parent"]["object"].as_u64() {
                        None
                    } else {
                        Some(Command::Parent {
                            object: popup.object,
                            parent,
                            frame: state["frame"].as_f64().unwrap_or(0.).floor().max(0.) as u32,
                        })
                    }
                }
            };
            if let Some(command) = command {
                out.push(Action::Edit(command));
            }
        }
        if !popup.dropdown.open {
            self.popup = None;
            self.focused = None;
        }
    }
}
pub(super) fn owner(state: &Value, object: u64) -> Option<&Value> {
    if object == 0 {
        Some(&state["project"]["camera"])
    } else {
        state["project"]["layers"]
            .as_array()?
            .iter()
            .find(|l| l["id"].as_u64() == Some(object))
    }
}
pub(super) fn blend(layer: &Value) -> BlendMode {
    serde_json::from_value(layer["blend"]["mode"].clone()).unwrap_or_default()
}
pub(super) fn blend_name(layer: &Value, locale: Locale) -> &'static str {
    blend_modes(locale)
        .into_iter()
        .find(|(mode, _)| *mode == blend(layer))
        .map(|(_, label)| label)
        .unwrap_or("Normal")
}
pub(super) fn parent_name(layer: &Value, state: &Value, locale: Locale) -> String {
    let Some(id) = layer["parent"]["object"].as_u64() else {
        return locale.t(T::None).to_string();
    };
    let camera = state["project"]["camera"]["created"] == true;
    if id == 0 {
        return format!(
            "1. {}",
            if locale == Locale::Zh {
                "摄像机"
            } else {
                "Camera"
            }
        );
    }
    state["project"]["layers"]
        .as_array()
        .into_iter()
        .flatten()
        .rev()
        .enumerate()
        .find(|(_, l)| l["id"].as_u64() == Some(id))
        .map_or_else(
            || locale.t(T::None).to_string(),
            |(index, l)| {
                format!(
                    "{}. {}",
                    index + 1 + usize::from(camera),
                    l["name"].as_str().unwrap_or("Layer")
                )
            },
        )
}
fn blend_modes(locale: Locale) -> [(BlendMode, &'static str); 15] {
    use BlendMode::*;
    let zh = locale == Locale::Zh;
    [
        (Normal, if zh { "正常" } else { "Normal" }),
        (Darken, if zh { "变暗" } else { "Darken" }),
        (Multiply, if zh { "相乘" } else { "Multiply" }),
        (ColorBurn, if zh { "颜色加深" } else { "Color Burn" }),
        (Add, if zh { "相加" } else { "Add" }),
        (Lighten, if zh { "变亮" } else { "Lighten" }),
        (Screen, if zh { "屏幕" } else { "Screen" }),
        (ColorDodge, if zh { "颜色减淡" } else { "Color Dodge" }),
        (Overlay, if zh { "叠加" } else { "Overlay" }),
        (SoftLight, if zh { "柔光" } else { "Soft Light" }),
        (HardLight, if zh { "强光" } else { "Hard Light" }),
        (Difference, if zh { "差值" } else { "Difference" }),
        (Exclusion, if zh { "排除" } else { "Exclusion" }),
        (Subtract, if zh { "相减" } else { "Subtract" }),
        (Divide, if zh { "相除" } else { "Divide" }),
    ]
}
