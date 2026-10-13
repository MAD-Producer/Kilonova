use crate::{
    i18n::{Locale, Text as T},
    panels::Action,
};
use motion_core::{Axis, CameraMode, Command, Property};
use motion_ui::{
    input::{Input, MouseButton, Rect},
    paint::{self, PaintList},
    text::TextAtlas,
    ui::{self, Align},
};
use serde_json::Value;
#[derive(Default)]
pub struct Controls {
    field: Option<Field>,
    pub cursor: bool,
    pub drag_readout: Option<([f32; 2], String)>,
}
struct Field {
    property: Property,
    axis: usize,
    number: super::numeric::Field,
    rect: Rect,
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
    pub fn cancel(&mut self) -> bool {
        self.field.take().is_some_and(|mut f| f.number.cancel())
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
        let styles = list.styles();
        let colors = styles.colors();

        self.cursor = false;
        self.drag_readout = None;
        if self
            .field
            .as_ref()
            .is_some_and(|f| input.pressed.is_some_and(|(_, p)| !f.rect.contains(p)))
        {
            if self.cancel() {
                out.push(Action::History(4));
            }
        }
        let camera = &state["project"]["camera"];
        let sample = &state["sampledCamera"];
        let orbit = camera["mode"] == "orbit";
        let mode = Rect::new(area.min[0] + 16., *y, area.width() - 32., 28.);
        paint::rect(list, mode, colors.SUNKEN);
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
            colors.ACCENT,
            Align::Left,
        );
        if clicked(input, mode) {
            if self.cancel() {
                out.push(Action::History(4));
            }
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
                colors.TEXT_MUTED,
                Align::Left,
            );
            *y += 22.;
            for axis in 0..dimensions {
                let value = if dimensions == 1 {
                    sample[key].as_f64().unwrap_or(0.)
                } else {
                    sample[key][axis].as_f64().unwrap_or(0.)
                };
                let inset = if dimensions > 1 { 20. } else { 0. };
                if dimensions > 1 {
                    ui::text(
                        list,
                        atlas,
                        Rect::new(area.min[0] + 30., *y, 18., 26.),
                        ["X", "Y", "Z"][axis],
                        colors.TEXT_MUTED,
                        Align::Left,
                    );
                }
                let rect = Rect::new(
                    area.min[0] + 30. + inset,
                    *y,
                    area.width() - 50. - inset,
                    26.,
                );
                let active = self
                    .field
                    .as_ref()
                    .is_some_and(|f| f.property == property && f.axis == axis);
                if clicked(input, rect) && !active {
                    if self.cancel() {
                        out.push(Action::History(4));
                    }
                    self.field = Some(Field {
                        property,
                        axis,
                        number: Default::default(),
                        rect,
                    });
                }
                if let Some(field) = self
                    .field
                    .as_mut()
                    .filter(|f| f.property == property && f.axis == axis)
                {
                    field.rect = rect;
                    self.cursor |= field.number.editor.dragging() || rect.contains(input.mouse);
                    let (min, max, speed) = match property {
                        Property::Fov => (10., 120., 0.5),
                        Property::Radius => (1., 10_000_000., 1.),
                        Property::Elevation => (-89., 89., 0.5),
                        _ => (-10_000_000., 10_000_000., 0.5),
                    };
                    let events = field.number.draw(
                        list,
                        atlas,
                        rect,
                        value,
                        motion_ui::widgets::NumberOptions {
                            min,
                            max,
                            speed,
                            decimals: 3,
                        },
                        input,
                    );
                    if let Some(note) = field.number.readout() {
                        self.drag_readout = Some((input.mouse, note));
                    }
                    if super::numeric::apply(events, out, |v| {
                        camera_command(state, property, axis, v)
                    }) {
                        self.field = None;
                    }
                } else {
                    self.cursor |= rect.contains(input.mouse);
                    super::numeric::passive(list, atlas, rect, value, input, true);
                }
                *y += 28.;
            }
        }
    }
}
fn camera_command(state: &Value, property: Property, axis: usize, value: f32) -> Command {
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
    if matches!(property, Property::Position | Property::Target) {
        if state["project"]["camera"][key]["axes"].is_object() {
            Command::SetComponent {
                object: 0,
                property,
                axis: [Axis::X, Axis::Y, Axis::Z][axis],
                frame,
                value,
            }
        } else {
            let mut xyz = std::array::from_fn(|i| {
                state["sampledCamera"][key][i].as_f64().unwrap_or(0.) as f32
            });
            xyz[axis] = value;
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
    }
}
