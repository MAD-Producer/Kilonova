use crate::{
    i18n::{Locale, Text as T},
    panels::Action,
};
use motion_core::{Axis, CameraMode, Command, Property};
use motion_ui::{
    input::{Input, Key, MouseButton, Rect},
    paint::{self, PaintList},
    text::TextAtlas,
    theme::palette,
    ui::{self, Align},
};
use serde_json::Value;
#[derive(Default)]
pub struct Controls {
    field: Option<Field>,
}
struct Field {
    property: Property,
    axis: usize,
    text: String,
    replace: bool,
}
fn clicked(input: &Input, rect: Rect) -> bool {
    input
        .pressed
        .is_some_and(|(b, p)| b == MouseButton::Left && rect.contains(p))
}
fn local(locale: Locale, zh: &str, en: &str) -> String {
    if locale == Locale::Zh { zh } else { en }.into()
}
impl Controls {
    pub fn editing(&self) -> bool {
        self.field.is_some()
    }
    pub fn cancel(&mut self) {
        self.field = None;
    }
    pub fn render(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: &mut f32,
        state: &Value,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        let camera = &state["project"]["camera"];
        let sample = &state["camera"];
        let orbit = camera["mode"] == "orbit";
        let mode = Rect::new(area.min[0] + 16., *y, area.width() - 32., 28.);
        paint::rect(list, mode, palette::SUNKEN);
        ui::text(
            list,
            atlas,
            mode.shrink(5.),
            &local(
                locale,
                if orbit {
                    "摄影机模式：环绕"
                } else {
                    "摄影机模式：位置"
                },
                if orbit {
                    "Camera Mode: Orbit"
                } else {
                    "Camera Mode: Position"
                },
            ),
            palette::ACCENT,
            Align::Left,
        );
        if clicked(input, mode) {
            out.push(Action::Edit(Command::CameraMode {
                mode: if orbit {
                    CameraMode::Position
                } else {
                    CameraMode::Orbit
                },
            }));
            self.field = None;
        }
        *y += 36.;
        let mut properties = vec![
            (
                Property::Target,
                "target",
                local(locale, "目标点", "Target"),
                3,
            ),
            (Property::Roll, "roll", local(locale, "倾斜", "Roll"), 1),
            (
                Property::Fov,
                "fov",
                local(locale, "视场角", "Field of View"),
                1,
            ),
        ];
        if orbit {
            properties.extend([
                (
                    Property::Radius,
                    "radius",
                    local(locale, "半径", "Radius"),
                    1,
                ),
                (
                    Property::Azimuth,
                    "azimuth",
                    local(locale, "方位角", "Azimuth"),
                    1,
                ),
                (
                    Property::Elevation,
                    "elevation",
                    local(locale, "仰角", "Elevation"),
                    1,
                ),
            ]);
        } else {
            properties.insert(
                0,
                (
                    Property::Position,
                    "position",
                    locale.t(T::Position).into(),
                    3,
                ),
            );
        }
        for (property, key, name, dimensions) in properties {
            ui::text(
                list,
                atlas,
                Rect::new(area.min[0] + 16., *y, area.width() - 32., 22.),
                &name,
                palette::TEXT_MUTED,
                Align::Left,
            );
            *y += 22.;
            for axis in 0..dimensions {
                let value = if dimensions == 1 {
                    sample[key].as_f64().unwrap_or(0.)
                } else {
                    sample[key][axis].as_f64().unwrap_or(0.)
                };
                let rect = Rect::new(area.min[0] + 30., *y, area.width() - 50., 26.);
                let active = self
                    .field
                    .as_ref()
                    .is_some_and(|f| f.property == property && f.axis == axis);
                paint::rect(list, rect, palette::SUNKEN);
                let shown = if active {
                    self.field.as_ref().unwrap().text.clone()
                } else {
                    format!(
                        "{}{value:.2}",
                        if dimensions == 3 {
                            ["X  ", "Y  ", "Z  "][axis]
                        } else {
                            ""
                        }
                    )
                };
                ui::text(
                    list,
                    atlas,
                    rect.shrink(5.),
                    &shown,
                    palette::ACCENT,
                    Align::Left,
                );
                if active {
                    paint::stroke(list, rect, palette::ACCENT, 1.);
                }
                if clicked(input, rect) {
                    self.field = Some(Field {
                        property,
                        axis,
                        text: format!("{value:.2}"),
                        replace: true,
                    });
                }
                *y += 28.;
            }
        }
        if let Some(field) = &mut self.field {
            if !input.text.is_empty() {
                if field.replace {
                    field.text.clear();
                    field.replace = false;
                }
                field.text.push_str(&input.text);
            }
            if input.keys.iter().any(|(k, _)| *k == Key::Backspace) {
                if field.replace {
                    field.text.clear();
                    field.replace = false;
                } else {
                    field.text.pop();
                }
            }
            if input.keys.iter().any(|(k, _)| *k == Key::Enter) {
                if let Ok(value) = field.text.parse::<f32>() {
                    if value.is_finite() {
                        let property = field.property;
                        let frame = state["frame"].as_f64().unwrap_or(0.).floor() as u32;
                        let key = match property {
                            Property::Position => "position",
                            Property::Target => "target",
                            Property::Roll => "roll",
                            Property::Fov => "fov",
                            Property::Radius => "radius",
                            Property::Azimuth => "azimuth",
                            _ => "elevation",
                        };
                        let command = if matches!(property, Property::Position | Property::Target) {
                            if camera[key]["axes"].is_object() {
                                Command::SetComponent {
                                    object: 0,
                                    property,
                                    axis: [Axis::X, Axis::Y, Axis::Z][field.axis],
                                    frame,
                                    value,
                                }
                            } else {
                                let mut xyz = std::array::from_fn(|i| {
                                    sample[key][i].as_f64().unwrap_or(0.) as f32
                                });
                                xyz[field.axis] = value;
                                Command::SetVector {
                                    object: 0,
                                    property,
                                    frame,
                                    value: xyz,
                                }
                            }
                        } else {
                            Command::SetScalar {
                                object: 0,
                                property,
                                frame,
                                value,
                            }
                        };
                        out.push(Action::Edit(command));
                        self.field = None;
                    }
                }
            }
        }
    }
}
