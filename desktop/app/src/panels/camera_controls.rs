use crate::{
    i18n::{Locale, Text as T},
    panels::Action,
};
use motion_core::{Axis, CameraMode, Command, Property};
use motion_ui::{
    input::{Input, MouseButton, Rect},
    paint::PaintList,
    text::TextAtlas,
    ui::{self, Align},
};
use serde_json::Value;

#[derive(Default)]
pub struct Controls {
    object: Option<u64>,
    field: Option<Field>,
    mode: motion_ui::widgets::Dropdown,
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
pub(super) fn camera<'a>(state: &'a Value, object: u64) -> &'a Value {
    if object == 0 {
        &state["project"]["camera"]
    } else {
        state["project"]["layers"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|l| l["id"].as_u64() == Some(object))
            .map_or(&Value::Null, |l| &l["content"]["camera"])
    }
}
fn sample<'a>(state: &'a Value, object: u64) -> &'a Value {
    if object == 0 {
        &state["sampledCamera"]
    } else {
        state["sampledLayers"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|l| l["id"].as_u64() == Some(object))
            .unwrap_or(&Value::Null)
    }
}
impl Controls {
    pub fn editing(&self) -> bool {
        self.field.is_some() || self.mode.open
    }
    pub fn cancel(&mut self) -> bool {
        self.mode.open = false;
        self.field.take().is_some_and(|mut f| f.number.cancel())
    }
    pub fn render(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: &mut f32,
        state: &Value,
        object: u64,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) -> Option<(Property, Option<Axis>, super::key_navigation::Choice)> {
        let styles = list.styles();
        let colors = styles.colors();
        let height = styles.metric("camera.row.height", 28.);
        let field_height = styles.metric("camera.field.height", 26.);
        let layout_at = |y, axis, dimensions| {
            let mut layout =
                super::properties::inspector_layout(&styles, area, y, axis, dimensions);
            layout.watch.max[1] = y + height;
            layout.caption.max[1] = y + height;
            layout.number.min[1] = y + (height - field_height) * 0.5;
            layout.number.max[1] = layout.number.min[1] + field_height;
            layout
        };
        self.cursor = false;
        self.drag_readout = None;
        let locked = state["observing"] == true
            || state["project"]["layers"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|l| l["id"].as_u64() == Some(object) && l["locked"] == true);
        if self.object != Some(object)
            || locked
            || self
                .field
                .as_ref()
                .is_some_and(|f| input.pressed.is_some_and(|(_, p)| !f.rect.contains(p)))
        {
            if self.cancel() {
                out.push(Action::History(4));
            }
        }
        self.object = Some(object);
        let c = camera(state, object);
        let sampled = sample(state, object);
        let orbit = c["mode"] == "orbit";
        let caption = layout_at(*y, 0, 1);
        ui::text(
            list,
            atlas,
            caption.caption,
            if locale == Locale::Zh {
                "模式"
            } else {
                "Mode"
            },
            colors.TEXT_MUTED,
            Align::Left,
        );
        let labels = if locale == Locale::Zh {
            ["位置", "环绕"]
        } else {
            ["Position", "Orbit"]
        };
        let items = labels.map(|label| motion_ui::widgets::DropdownItem {
            label,
            enabled: true,
        });
        // Menus append last, above the subsequent property rows.
        let mode_was_open = self.mode.open;
        let mut overlay = list.child();
        if let Some(mode) = self.mode.draw(
            &mut overlay,
            atlas,
            caption.number,
            area,
            &items,
            usize::from(orbit),
            input,
            !locked,
            false,
        ) {
            if self.cancel() {
                out.push(Action::History(4));
            }
            out.push(Action::Edit(Command::CameraMode {
                object,
                mode: if mode == 0 {
                    CameraMode::Position
                } else {
                    CameraMode::Orbit
                },
            }));
        }
        let neutral = Input::default();
        let input = if mode_was_open || self.mode.open {
            &neutral
        } else {
            input
        };
        *y += height;
        let mut properties = vec![
            (Property::Target, "target", T::CameraTarget, 3),
            (Property::Roll, "roll", T::CameraRoll, 1),
            (Property::Fov, "fov", T::CameraFov, 1),
        ];
        if orbit {
            properties.extend([
                (Property::Radius, "radius", T::CameraRadius, 1),
                (Property::Azimuth, "azimuth", T::CameraAzimuth, 1),
                (Property::Elevation, "elevation", T::CameraElevation, 1),
            ]);
        } else {
            properties.insert(0, (Property::Position, "position", T::Position, 3));
        }
        let tracks = if object == 0 {
            properties.into_iter().map(|(property, key, title, dimensions)| super::timeline::PropertyRow {
                property, key, title, axis: None, axes: (0..dimensions).collect(), keys: vec![],
            }).collect()
        } else {
            super::timeline::property_rows(state, object)
        };
        let mut choice = None;
        for track in tracks {
            let super::timeline::PropertyRow { property, key, title, axis: selected_axis, axes, keys } = track;
            let dimensions = axes.len();
            let axis = selected_axis.map(|index| [Axis::X, Axis::Y, Axis::Z][index]);
            let row = Rect::new(area.min[0], *y, area.width(), height);
            if let Some(intent) = super::key_navigation::property_intent(list, row, state, input, object, property, axis, &keys) {
                choice = Some((property, axis, intent));
            }
            let layout = layout_at(*y, 0, dimensions);
            super::timeline::property_branch(list, layout.watch);
            let source = selected_axis.map_or(&c[key], |index| &c[key]["axes"][["x", "y", "z"][index]]);
            let animated = source["keys"]
                .as_array()
                .is_some_and(|keys| !keys.is_empty())
                || ["x", "y", "z"].iter().any(|axis| {
                    source["axes"][axis]["keys"]
                        .as_array()
                        .is_some_and(|keys| !keys.is_empty())
                });
            super::timeline::property_icon(
                list,
                layout.watch,
                motion_ui::icons::Icon::Stopwatch,
                animated,
                !locked,
            );
            if clicked(input, layout.watch) && !locked {
                out.push(Action::Edit(Command::Animate {
                    object,
                    property,
                    axis,
                    frame: state["frame"].as_f64().unwrap_or(0.).floor() as u32,
                    enabled: !animated,
                }));
            }
            let caption = selected_axis.map_or_else(|| locale.t(title).to_owned(), |axis| format!("{} {}", locale.t(title), ["X", "Y", "Z"][axis]));
            ui::text(
                list,
                atlas,
                layout.caption,
                &caption,
                colors.TEXT_MUTED,
                Align::Left,
            );
            for (component, axis) in axes.into_iter().enumerate() {
                let rect = layout_at(*y, component, dimensions).number;
                let value = if property.is_scalar() {
                    sampled[key].as_f64().unwrap_or(0.)
                } else {
                    sampled[key][axis].as_f64().unwrap_or(0.)
                };
                let active = self
                    .field
                    .as_ref()
                    .is_some_and(|f| f.property == property && f.axis == axis);
                if clicked(input, rect) && !active && !locked {
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
                        Property::Roll | Property::Azimuth => (-1_000_000., 1_000_000., 0.5),
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
                        camera_command(state, object, property, axis, v)
                    }) {
                        self.field = None;
                    }
                } else {
                    self.cursor |= !locked && rect.contains(input.mouse);
                    super::numeric::passive(list, atlas, rect, value, input, !locked);
                }
            }
            *y += height;
        }
        list.append(overlay);
        choice
    }
}
fn camera_command(
    state: &Value,
    object: u64,
    property: Property,
    axis: usize,
    value: f32,
) -> Command {
    let frame = state["frame"].as_f64().unwrap_or(0.).floor() as u32;
    if matches!(property, Property::Position | Property::Target) {
        let key = if property == Property::Position {
            "position"
        } else {
            "target"
        };
        if camera(state, object)[key]["axes"].is_object() {
            Command::SetComponent {
                object,
                property,
                axis: [Axis::X, Axis::Y, Axis::Z][axis],
                frame,
                value,
            }
        } else {
            let mut xyz = std::array::from_fn(|i| {
                sample(state, object)[key][i].as_f64().unwrap_or(0.) as f32
            });
            xyz[axis] = value;
            Command::SetVector {
                object,
                property,
                frame,
                value: xyz,
            }
        }
    } else {
        Command::SetScalar {
            object,
            property,
            frame,
            value,
        }
    }
}
