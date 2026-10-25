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
mod lens;
#[cfg(test)]
mod tests;
mod values;
use values::{bounds, feedback, text};

#[derive(Default)]
pub struct Controls {
    pub(crate) filter: super::types::PropertyFilter,
    context: Option<(String, String, u64, u64, bool)>,
    field: Option<Field>,
    mode: motion_ui::widgets::Dropdown,
    lens: lens::Controls,
    reset: motion_ui::widgets::Dropdown,
    reset_target: Option<(Property, usize, Rect)>,
    sample_failures: u8,
    #[cfg(test)]
    rects: Vec<(Property, usize, Rect)>,
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
    pub(super) fn text_menu(&mut self, request: &mut super::text_menu::Request<'_>) {
        if let Some(field) = &mut self.field { request.visit(&mut field.number.editor.text); }
    }
    pub(super) fn field_dragging(&self) -> bool {
        self.field
            .as_ref()
            .is_some_and(|field| field.number.editor.dragging())
            || self.lens.drag_readout.is_some()
    }
    pub fn editing(&self) -> bool {
        self.field.is_some() || self.mode.open || self.lens.editing() || self.reset.open
    }
    pub fn cancel(&mut self) -> bool {
        self.mode.open = false;
        self.reset.open = false;
        self.reset_target = None;
        let lens = self.lens.cancel();
        self.field.take().is_some_and(|mut f| f.number.cancel()) || lens
    }
    pub(in crate::panels) fn render(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: &mut f32,
        state: &Value,
        object: u64,
        input: &Input,
        locale: Locale,
        selection: &mut super::animation_controls::Controls,
        graph_component: &mut usize,
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
        #[cfg(test)]
        self.rects.clear();
        let locked = state["observing"] == true
            || state["project"]["layers"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|l| l["id"].as_u64() == Some(object) && l["locked"] == true);
        let frame = state["frame"].as_f64().unwrap_or(f64::NAN);
        let editable_frame = super::key_navigation::editable_frame(frame, super::last_frame(state));
        let c = camera(state, object);
        let orbit = c["mode"] == "orbit";
        let context = (
            state["root"].as_str().unwrap_or("").into(),
            state["composition"].as_str().unwrap_or("").into(),
            object,
            frame.to_bits(),
            orbit,
        );
        let changed_context = self.context.as_ref() != Some(&context);
        let changed_camera = self
            .context
            .as_ref()
            .is_none_or(|old| old.0 != context.0 || old.1 != context.1 || old.2 != context.2);
        if changed_camera {
            self.sample_failures = 0;
        }
        self.lens.prepare(
            state,
            object,
            input,
            locked || editable_frame.is_none(),
            out,
        );
        if changed_context
            || locked
            || editable_frame.is_none()
            || !c.is_object()
            || input.focus_lost
            || input
                .key_presses()
                .any(|(key, _, repeat)| key == motion_ui::input::Key::Escape && !repeat)
            || self
                .field
                .as_ref()
                .is_some_and(|f| input.pressed.is_some_and(|(_, p)| !f.rect.contains(p)))
        {
            if self.cancel() {
                out.push(Action::History(4));
            }
        }
        self.context = Some(context);
        let sampled = sample(state, object);
        let editable = !locked && editable_frame.is_some() && c.is_object();
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
        let neutral_mode = Input::default();
        let reset_was_open = self.reset.open;
        let mode_input = if self.lens.menu_open() || reset_was_open {
            &neutral_mode
        } else {
            input
        };
        if let Some(mode) = self.mode.draw(
            &mut overlay,
            atlas,
            caption.number,
            area,
            &items,
            usize::from(orbit),
            mode_input,
            editable,
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
        let lens_input = if mode_was_open || self.mode.open || reset_was_open {
            &neutral
        } else {
            input
        };
        let original_input = input;
        let input = if mode_was_open || self.mode.open || self.lens.menu_open() || reset_was_open {
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
            values::root_rows(c, properties)
        } else {
            super::timeline::property_rows(state, object)
        };
        let mut choice = None;
        let mut unavailable = None;
        let mut selected_track = selection.track_selection(state, Some(object));
        for track in tracks {
            if !self
                .filter
                .admits(track.property, !track.keys.is_empty(), track.modified)
            {
                continue;
            }
            let super::timeline::PropertyRow {
                property,
                key,
                title,
                axis: selected_axis,
                axes,
                keys,
                ..
            } = track;
            let dimensions = axes.len();
            let axis = selected_axis.map(|index| [Axis::X, Axis::Y, Axis::Z][index]);
            let row = Rect::new(area.min[0], *y, area.width(), height);
            let layout = layout_at(*y, 0, dimensions);
            selection.record_row(
                super::animation_controls::SelectedTrack {
                    object,
                    property,
                    axis,
                },
                layout.caption,
                axes.iter().enumerate().map(|(index, component)| {
                    (*component, layout_at(*y, index, dimensions).number)
                }),
            );
            let component = axes.iter().enumerate().find_map(|(index, component)| {
                clicked(input, layout_at(*y, index, dimensions).number).then_some(*component)
            });
            if clicked(input, layout.caption) || component.is_some() {
                selection.choose_track_in(state, object, property, axis);
                if let Some(component) = component {
                    *graph_component = component;
                } else {
                    selection.clear_key();
                }
                selected_track = selection.track_selection(state, Some(object));
            }
            super::animation_controls::highlight_row(
                list,
                row,
                selected_track
                    == super::animation_controls::TrackSelection::Current(
                        super::animation_controls::SelectedTrack {
                            object,
                            property,
                            axis,
                        },
                    ),
            );
            if object == 0 {
                let adjacent = super::key_navigation::neighbours(
                    keys.iter().copied(),
                    frame,
                    super::last_frame(state),
                );
                let rail = super::key_navigation::rail(&styles, row);
                if let Some(intent) =
                    super::key_navigation::draw(list, rail, input, adjacent, editable)
                {
                    values::root_choice(state, property, axis, intent, out);
                }
            } else if let Some(intent) = super::key_navigation::property_intent(
                list, row, state, input, object, property, axis, &keys,
            ) {
                choice = Some((property, axis, intent));
            }
            super::timeline::property_branch(list, layout.watch);
            let source =
                selected_axis.map_or(&c[key], |index| &c[key]["axes"][["x", "y", "z"][index]]);
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
                editable,
            );
            if clicked(input, layout.watch) && editable {
                out.push(Action::Edit(Command::Animate {
                    object,
                    property,
                    axis,
                    frame: editable_frame.unwrap(),
                    enabled: !animated,
                }));
            }
            let title = if property == Property::Fov {
                if locale == Locale::Zh {
                    "垂直视角 °"
                } else {
                    "V. FOV °"
                }
            } else {
                locale.t(title)
            };
            let caption = selected_axis.map_or_else(
                || title.to_owned(),
                |axis| format!("{} {}", title, ["X", "Y", "Z"][axis]),
            );
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
                #[cfg(test)]
                self.rects.push((property, axis, rect));
                let value = values::sampled_value(sampled, property, key, axis);
                let options = bounds(property);
                if let Some((MouseButton::Right, point)) = input.pressed {
                    if rect.contains(point) && editable && value.is_some() {
                        if self.cancel() {
                            out.push(Action::History(4));
                        }
                        self.reset_target = Some((property, axis, rect));
                        self.reset.open = true;
                    }
                }
                let Some(value) = value else {
                    unavailable.get_or_insert((
                        property,
                        key,
                        values::sample_problem(sampled, property, key),
                    ));
                    if self.field.as_ref().is_some_and(|f| f.property == property) && self.cancel()
                    {
                        out.push(Action::History(4));
                    }
                    ui::text(
                        list,
                        atlas,
                        rect,
                        text(locale, "不可用", "Unavailable"),
                        colors.TEXT_MUTED,
                        Align::Right,
                    );
                    if rect.contains(input.mouse) {
                        self.drag_readout = Some((
                            input.mouse,
                            text(
                                locale,
                                "摄影机采样值不可用；未写入数值",
                                "Camera sample unavailable; no value written",
                            )
                            .into(),
                        ));
                    }
                    continue;
                };
                if rect.contains(input.mouse) {
                    self.drag_readout = Some((
                        input.mouse,
                        if editable_frame.is_none() {
                            text(
                                locale,
                                "请定位到整帧后编辑；子帧仍可浏览",
                                "Move to a whole frame to edit; subframes remain viewable",
                            )
                            .into()
                        } else {
                            feedback(property, locale, false)
                        },
                    ));
                }
                let active = self
                    .field
                    .as_ref()
                    .is_some_and(|f| f.property == property && f.axis == axis);
                if clicked(input, rect) && !active && editable {
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
                    let events = field.number.draw(list, atlas, rect, value, options, input);
                    if let Some(note) = field.number.readout() {
                        self.drag_readout = Some((
                            input.mouse,
                            format!("{note} · {}", feedback(property, locale, false)),
                        ));
                    }
                    if field.number.editor.text.error.is_some() {
                        self.drag_readout = Some((input.mouse, feedback(property, locale, true)));
                    }
                    if super::numeric::apply(events, out, |v| {
                        camera_command(state, object, property, axis, v)
                    }) {
                        self.field = None;
                    }
                } else {
                    self.cursor |= editable && rect.contains(input.mouse);
                    super::numeric::passive(list, atlas, rect, value, input, editable);
                }
            }
            *y += height;
        }
        self.lens.render(
            list,
            atlas,
            area,
            y,
            state,
            object,
            lens_input,
            locale,
            !editable,
            selection,
            graph_component,
            out,
        );
        self.cursor |= self.lens.cursor;
        if self.lens.drag_readout.is_some() {
            self.drag_readout = self.lens.drag_readout.clone();
        }
        // One bounded streak per root/composition/camera, across playback frames.
        // A healthy render or camera identity change resets it, never a timer.
        if let Some((property, key, cause)) = unavailable {
            let options = bounds(property);
            if self.sample_failures < 60 {
                self.sample_failures += 1;
            }
            if self.sample_failures == 1 {
                log::debug!("camera.fields sample unavailable object={object} property={property:?} field={key} frame={frame} cause={cause} min={} max={} occurrence=1", options.min, options.max);
            } else if self.sample_failures == 60 {
                log::error!("camera.fields sample unavailable object={object} property={property:?} field={key} frame={frame} cause={cause} min={} max={} occurrence=60", options.min, options.max);
                // 61 is a terminal suppression marker until recovery.
                self.sample_failures = 61;
            }
        } else {
            self.sample_failures = 0;
        }
        if let Some((property, axis, anchor)) = self.reset_target {
            self.drag_readout = None;
            self.cursor = false;
            let label = text(locale, "重置当前值", "Reset current value");
            let default = values::default_value(state, property, axis);
            let note = if default.is_some() {
                values::range_note(property, locale)
            } else {
                text(
                    locale,
                    "无法读取合成尺寸",
                    "Composition dimensions unavailable",
                )
                .into()
            };
            let items = [
                motion_ui::widgets::DropdownItem {
                    label,
                    enabled: default.is_some(),
                },
                motion_ui::widgets::DropdownItem {
                    label: &note,
                    enabled: false,
                },
            ];
            if self
                .reset
                .draw_menu(
                    &mut overlay,
                    atlas,
                    anchor,
                    area,
                    &items,
                    original_input,
                    editable,
                )
                .is_some()
            {
                if let Some(value) = default {
                    out.push(Action::Edit(camera_command(
                        state, object, property, axis, value,
                    )));
                }
            }
            if !self.reset.open {
                self.reset_target = None;
            }
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
    // Callers require editable_frame; use its nearest-integer convention.
    let frame = state["frame"].as_f64().unwrap_or(0.).round() as u32;
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
