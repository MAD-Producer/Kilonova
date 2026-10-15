//! Audio controls use the shared gain, stereo envelope and gesture commands.
use super::*;
use motion_core::{AudioChannel, AudioLevelsAction, Track};
use motion_ui::icons::Icon;
use motion_ui::widgets::NumberOptions;

const SILENCE_DB: f64 = -192.;
fn gain_db(gain: f64) -> f64 {
    if gain <= 0. {
        SILENCE_DB
    } else {
        20. * gain.log10()
    }
}
fn db_gain(db: f64) -> f32 {
    if db <= SILENCE_DB {
        0.
    } else {
        10f64.powf(db / 20.).clamp(0., 2.) as f32
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Control {
    Gain,
    Left,
    Right,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Item {
    Key(AudioChannel),
    Marker(u64),
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Selection {
    object: u64,
    item: Item,
    frame: u32,
}
struct Drag {
    selection: Selection,
    start: f32,
    current: u32,
    begun: bool,
}
impl Control {
    fn channel(self) -> AudioChannel {
        match self {
            Self::Left => AudioChannel::Left,
            _ => AudioChannel::Right,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            _ => "right",
        }
    }
}
struct Field {
    object: u64,
    control: Control,
    rect: Rect,
    number: numeric::Field,
}
#[derive(Default)]
pub(crate) struct Controls {
    batch: Option<(u64, Vec<u64>)>,
    field: Option<Field>,
    context: Option<(String, String)>,
    pub cursor: bool,
    pub drag_readout: Option<([f32; 2], String)>,
    selection: Option<Selection>,
    active_channel: Option<AudioChannel>,
    drag: Option<Drag>,
}
impl Controls {
    pub fn clear_selection(&mut self) {
        self.batch = None;
        self.selection = None;
        self.active_channel = None;
    }
    pub fn select_generated(&mut self, object: u64, state: &Value) {
        self.clear_selection();
        let ids = state["project"]["layers"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|layer| layer["id"] == object)
            .into_iter()
            .flat_map(|layer| layer["audio_markers"].as_array().into_iter().flatten())
            .filter(|marker| marker["generated"] == true)
            .filter_map(|marker| marker["id"].as_u64())
            .collect();
        self.batch = Some((object, ids));
    }
    pub fn editing(&self) -> bool {
        self.field.is_some() || self.drag.is_some()
    }
    pub fn cancel(&mut self) -> bool {
        let field = self
            .field
            .take()
            .is_some_and(|mut field| field.number.cancel());
        let drag = self.drag.take().is_some_and(|drag| {
            if drag.begun {
                self.selection = Some(drag.selection);
            }
            drag.begun
        });
        field || drag
    }
    fn number(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        rect: Rect,
        object: u64,
        control: Control,
        value: f64,
        options: NumberOptions,
        state: &Value,
        input: &Input,
        enabled: bool,
        out: &mut Vec<Action>,
    ) {
        if enabled
            && clicked(input, rect)
            && !self.field.as_ref().is_some_and(|f| f.control == control)
        {
            if self.cancel() {
                out.push(Action::History(4));
            }
            self.field = Some(Field {
                object,
                control,
                rect,
                number: Default::default(),
            });
        }
        let mut number_paint = list.child();
        if let Some(field) = self
            .field
            .as_mut()
            .filter(|f| f.control == control && f.object == object)
        {
            field.rect = rect;
            self.cursor |= field.number.editor.dragging() || rect.contains(input.mouse);
            if control == Control::Gain
                && field.number.editor.text.active()
                && input.key_presses().any(|(key, _, _)| key == Key::Enter)
                && !input.composing
                && !input.ime_consumed
                && matches!(field.number.editor.text.text.trim(), "-inf" | "-∞" | "−∞")
            {
                field.number.editor.text.text = SILENCE_DB.to_string();
            }
            let events = field
                .number
                .draw(&mut number_paint, atlas, rect, value, options, input);
            if let Some(note) = field.number.readout() {
                self.drag_readout = Some((input.mouse, format!("{note} dB")));
            }
            let finished = numeric::apply(events, out, |value| {
                if control == Control::Gain {
                    Command::SetAudio {
                        object,
                        volume: Some(db_gain(f64::from(value))),
                        muted: None,
                    }
                } else {
                    Command::AudioLevels {
                        object,
                        action: AudioLevelsAction::Set {
                            channel: control.channel(),
                            frame: edit_frame(state),
                            value,
                            animated: None,
                        },
                    }
                }
            });
            if finished || !field.number.editor.interacting() {
                self.field = None;
            }
        } else {
            self.cursor |= enabled && rect.contains(input.mouse);
            numeric::passive(&mut number_paint, atlas, rect, value, input, enabled);
        }
        if control == Control::Gain
            && value <= SILENCE_DB
            && !self.field.as_ref().is_some_and(|f| f.control == control)
        {
            number_paint.retain_glyphs(|_| false);
            let styles = list.styles();
            let style = styles
                .as_ref()
                .resolve(
                    "field.number",
                    if enabled {
                        motion_ui::style::States::default()
                    } else {
                        motion_ui::style::States::default().with(motion_ui::style::State::Disabled)
                    },
                )
                .unwrap();
            motion_ui::style::text_style(
                &mut number_paint,
                atlas,
                rect.shrink(style.padding),
                "−∞",
                style,
                Align::Left,
            );
        }
        list.append(number_paint);
    }
}
fn edit_frame(state: &Value) -> u32 {
    state["frame"].as_f64().unwrap_or(0.).floor().max(0.) as u32
}
fn word(locale: Locale, zh: &str, en: &str) -> String {
    if locale == Locale::Zh { zh } else { en }.into()
}
pub(super) fn is_audio(layer: &Value) -> bool {
    matches!(
        layer["content"]["kind"].as_str(),
        Some("audio" | "video" | "composition")
    )
}
fn source(layer: &Value) -> &Value {
    &layer["content"][layer["content"]["kind"].as_str().unwrap_or("")]
}
pub(super) fn rows(layer: &Value) -> usize {
    controls(layer).len()
}
fn controls(layer: &Value) -> Vec<Control> {
    let mut rows = vec![];
    if matches!(layer["content"]["kind"].as_str(), Some("audio" | "video")) {
        rows.push(Control::Gain);
    }
    if layer["audio_source_channel"] != "right" {
        rows.push(Control::Left);
    }
    if layer["audio_source_channel"] != "left" {
        rows.push(Control::Right);
    }
    rows
}
pub(super) struct Row {
    pub number: Rect,
    pub buttons: [Rect; 3],
    pub watch: Rect,
    pub caption: Rect,
}
pub(super) fn row(area: Rect, y: f32, height: f32) -> Row {
    let x = area.min[0];
    let field_width = 64f32.min((area.width() * 0.35).max(1.));
    Row {
        number: Rect::new(area.max[0] - field_width - 6., y, field_width, height),
        buttons: std::array::from_fn(|i| Rect::new(x + 14. + i as f32 * 16., y, 16., height)),
        watch: Rect::new(x + 64., y, 16., height),
        caption: Rect::new(
            x + 84.,
            y,
            (area.width() - field_width - 94.).max(1.),
            height,
        ),
    }
}
fn key_times(track: &Track<f32>, offset: i64, frames: u64) -> Vec<u32> {
    track
        .keys
        .iter()
        .filter_map(|key| {
            let composition = i64::from(key.frame) + offset;
            (composition >= 0 && (composition as u64) < frames).then_some(composition as u32)
        })
        .collect()
}
fn track(layer: &Value, control: Control) -> Track<f32> {
    serde_json::from_value(layer["audio_levels"][control.name()].clone())
        .unwrap_or_else(|_| Track::constant(0.))
}
fn key_button(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    icon: Icon,
    active: bool,
    input: &Input,
    enabled: bool,
    action: Action,
    out: &mut Vec<Action>,
) {
    button(list, atlas, area, "", input, enabled, action, out);
    let colors = list.styles().colors();
    motion_ui::icons::draw(
        list,
        area,
        icon,
        14.,
        if !enabled {
            colors.TEXT_MUTED.with_alpha(0.4)
        } else if active {
            colors.ACCENT
        } else {
            colors.TEXT_MUTED
        },
    );
}
pub(super) fn prepare(panels: &mut Panels, state: &Value, input: &Input, out: &mut Vec<Action>) {
    let context = (
        state["root"].as_str().unwrap_or("").into(),
        state["composition"].as_str().unwrap_or("").into(),
    );
    let controls = &mut panels.audio_controls;
    let context_changed = controls.context.as_ref() != Some(&context);
    if context_changed
        || controls.field.as_ref().is_some_and(|f| {
            panels.selected != Some(f.object)
                || state["observing"] == true
                || state["project"]["layers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|l| l["id"] == f.object)
                    .is_none_or(|l| l["locked"] == true)
                || input.pressed.is_some_and(|(_, p)| !f.rect.contains(p))
        })
    {
        if controls.cancel() {
            out.push(Action::History(4));
        }
    }
    if context_changed {
        controls.clear_selection();
    }
    controls.context = Some(context);
    if controls
        .selection
        .is_some_and(|s| panels.selected != Some(s.object))
    {
        controls.selection = None;
    }
    if controls
        .batch
        .as_ref()
        .is_some_and(|(object, _)| panels.selected != Some(*object))
        || input.keys.iter().any(|(key, _)| *key == Key::Escape)
    {
        controls.batch = None;
    }
    controls.cursor = false;
    controls.drag_readout = None;
}
pub(super) fn render_rows(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    gutter: Rect,
    body: Rect,
    track_area: Rect,
    y: f32,
    height: f32,
    ppf: f32,
    offset: f32,
    object: u64,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    let Some(layer) = state["project"]["layers"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|l| l["id"] == object)
    else {
        return;
    };
    let enabled = layer["locked"] != true && state["observing"] != true;
    let supported = state["capabilities"]["audio"]["levels"]["supported"] == true;
    let current = edit_frame(state);
    let frames = state["project"]["frames"].as_u64().unwrap_or(1);
    let local_offset = layer["timeline"]["offset_frame"].as_i64().unwrap_or(0);
    let locale = panels.locale;
    let controls_list = controls(layer);
    for (i, control) in controls_list.iter().copied().enumerate() {
        let py = y + i as f32 * height;
        if py < body.min[1] || py + height > body.max[1] {
            continue;
        }
        let fields = row(gutter, py, height);
        paint::rect(
            list,
            Rect::new(gutter.min[0], py, gutter.width(), height),
            list.styles().colors().PANEL,
        );
        let gain = control == Control::Gain;
        let value = if gain {
            gain_db(source(layer)["volume"].as_f64().unwrap_or(1.))
        } else {
            f64::from(
                track(layer, control)
                    .sample(state["frame"].as_f64().unwrap_or(0.) - local_offset as f64),
            )
        };
        let caption = word(
            locale,
            match control {
                Control::Gain => "增益 (dB)",
                Control::Left => "左 (dB)",
                Control::Right => "右 (dB)",
            },
            match control {
                Control::Gain => "Gain (dB)",
                Control::Left => "Left (dB)",
                Control::Right => "Right (dB)",
            },
        );
        label(list, atlas, fields.caption, &caption, true);
        if clicked(input, fields.caption) || clicked(input, fields.number) {
            panels.select_only(Some(object));
            panels.audio_controls.active_channel = (!gain).then_some(control.channel());
            panels.audio_controls.selection = None;
        }
        let range = &state["capabilities"]["audio"]["levels"]["range"];
        let options = NumberOptions {
            min: if gain {
                SILENCE_DB
            } else {
                range[0].as_f64().unwrap_or(SILENCE_DB)
            },
            max: if gain {
                gain_db(2.)
            } else {
                range[1].as_f64().unwrap_or(24.)
            },
            speed: 0.1,
            decimals: 2,
        };
        panels.audio_controls.number(
            list,
            atlas,
            fields.number,
            object,
            control,
            value,
            options,
            state,
            input,
            enabled && (gain || supported),
            out,
        );
        if gain {
            if motion_ui::widgets::Checkbox::draw(
                list,
                atlas,
                fields.watch,
                "",
                source(layer)["muted"] == true,
                input,
                enabled,
                false,
                None,
            )
            .clicked
            {
                out.push(Action::Edit(Command::SetAudio {
                    object,
                    volume: None,
                    muted: Some(source(layer)["muted"] != true),
                }));
            }
            continue;
        }
        let keys = key_times(&track(layer, control), local_offset, frames);
        let animated = !keys.is_empty();
        let has_key = keys.contains(&current);
        key_button(
            list,
            atlas,
            fields.watch,
            Icon::Stopwatch,
            animated,
            input,
            enabled && supported,
            Action::Edit(Command::AudioLevels {
                object,
                action: AudioLevelsAction::Animate {
                    channel: control.channel(),
                    frame: current,
                    enabled: !animated,
                },
            }),
            out,
        );
        let prev = keys.iter().copied().filter(|f| *f < current).max();
        let next = keys.iter().copied().filter(|f| *f > current).min();
        for (n, key) in [prev, next].into_iter().enumerate() {
            key_button(
                list,
                atlas,
                fields.buttons[if n == 0 { 0 } else { 2 }],
                if n == 0 { Icon::Previous } else { Icon::Next },
                false,
                input,
                supported && key.is_some(),
                Action::Seek(f64::from(key.unwrap_or(current))),
                out,
            );
        }
        key_button(
            list,
            atlas,
            fields.buttons[1],
            Icon::Key,
            has_key,
            input,
            enabled && supported,
            Action::Edit(Command::AudioLevels {
                object,
                action: if has_key {
                    AudioLevelsAction::DeleteKey {
                        channel: control.channel(),
                        frame: current,
                    }
                } else {
                    AudioLevelsAction::Set {
                        channel: control.channel(),
                        frame: current,
                        value: value as f32,
                        animated: Some(true),
                    }
                },
            }),
            out,
        );
        for frame in keys {
            draw_reference(
                list,
                track_area,
                py,
                height,
                ppf,
                offset,
                Selection {
                    object,
                    item: Item::Key(control.channel()),
                    frame,
                },
                false,
                enabled && supported,
                panels,
                input,
                out,
            );
        }
    }
    if panels.audio_controls.cursor {
        panels.cursor = Some(motion_ui::widgets::numeric_behavior::cursor(
            input,
            panels.audio_controls.drag_readout.is_some(),
        ));
    }
    if panels.audio_controls.drag_readout.is_some() {
        panels.drag_readout = panels.audio_controls.drag_readout.clone();
    }
}
/// Audio references share the top time ruler; they are never envelope keys.
pub(super) fn render_ruler(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    ruler: Rect,
    ppf: f32,
    offset: f32,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) -> bool {
    let frames = state["project"]["frames"].as_u64().unwrap_or(1);
    let mut owned = panels
        .audio_controls
        .drag
        .as_ref()
        .is_some_and(|drag| matches!(drag.selection.item, Item::Marker(_)));
    let band = Rect::new(ruler.min[0], ruler.min[1], ruler.width(), 12.);
    // Draw the selected layer last when references from several layers overlap.
    let mut layers: Vec<_> = state["project"]["layers"]
        .as_array()
        .into_iter()
        .flatten()
        .collect();
    layers.sort_by_key(|layer| layer["id"].as_u64() == panels.selected);
    let hit = layers.iter().rev().find_map(|layer| {
        let object = layer["id"].as_u64()?;
        let local_offset = layer["timeline"]["offset_frame"].as_i64().unwrap_or(0);
        layer["audio_markers"]
            .as_array()?
            .iter()
            .rev()
            .find_map(|marker| {
                let frame = marker["frame"].as_i64()? + local_offset;
                let x = ruler.min[0] + (frame as f32 - offset) * ppf;
                (reference_visible(layer, frame, frames)
                    && band.contains(input.mouse)
                    && (input.mouse[0] - x).abs() <= 6.)
                    .then_some((object, marker["id"].as_u64()?))
            })
    });
    for layer in layers {
        let Some(object) = layer["id"].as_u64() else {
            continue;
        };
        let local_offset = layer["timeline"]["offset_frame"].as_i64().unwrap_or(0);
        for marker in layer["audio_markers"].as_array().into_iter().flatten() {
            let frame = marker["frame"].as_i64().unwrap_or(0) + local_offset;
            if !reference_visible(layer, frame, frames) {
                continue;
            }
            let id = marker["id"].as_u64().unwrap_or(0);
            let neutral = Input::default();
            let active = if hit == Some((object, id)) {
                input
            } else {
                &neutral
            };
            draw_reference(
                list,
                ruler,
                band.min[1],
                band.height(),
                ppf,
                offset,
                Selection {
                    object,
                    item: Item::Marker(id),
                    frame: frame as u32,
                },
                marker["generated"] == true && marker["corrected"] != true,
                layer["locked"] != true && state["observing"] != true,
                panels,
                active,
                out,
            );
        }
    }
    owned |= hit.is_some() && input.pressed.is_some_and(|(b, _)| b == MouseButton::Left);
    if hit.is_some() {
        panels.cursor = Some(motion_ui::cursor::Cursor::Grab);
    }
    if let Some((object, _)) =
        hit.filter(|_| input.pressed.is_some_and(|(b, _)| b == MouseButton::Right))
    {
        panels.select_only(Some(object));
        panels
            .layer_actions
            .open_markers(object, input.mouse, state);
        owned = true;
    }
    let add = Rect::new(ruler.min[0] - 28., ruler.min[1] + 3., 24., 22.);
    let target = panels.selected.and_then(|id| {
        state["project"]["layers"]
            .as_array()?
            .iter()
            .find(|layer| layer["id"] == id && is_audio(layer))
    });
    let now = edit_frame(state);
    let enabled = target.is_some_and(|layer| {
        layer["locked"] != true && reference_visible(layer, i64::from(now), frames)
    }) && state["observing"] != true;
    button(
        list,
        atlas,
        add,
        "+",
        input,
        enabled,
        Action::Edit(Command::AudioMarkers {
            object: panels.selected.unwrap_or(0),
            action: motion_core::AudioMarkersAction::Add {
                frame: now,
                label: String::new(),
            },
        }),
        out,
    );
    // Alt-click adds at the cursor; an ordinary ruler click keeps seeking.
    if hit.is_none() && clicked(input, ruler) && input.modifiers.alt {
        let frame = ((input.mouse[0] - ruler.min[0]) / ppf + offset)
            .round()
            .clamp(0., frames as f32 - 1.) as u32;
        if target.is_some_and(|layer| {
            layer["locked"] != true && reference_visible(layer, i64::from(frame), frames)
        }) && state["observing"] != true
        {
            out.push(Action::Edit(Command::AudioMarkers {
                object: panels.selected.unwrap(),
                action: motion_core::AudioMarkersAction::Add {
                    frame,
                    label: String::new(),
                },
            }));
        }
        owned = true;
    }
    owned
}
fn reference_visible(layer: &Value, frame: i64, frames: u64) -> bool {
    let begin = layer["subframe_range"]["in_frame"]
        .as_f64()
        .or_else(|| layer["timeline"]["in_frame"].as_f64())
        .unwrap_or(0.);
    let end = layer["subframe_range"]["out_frame"]
        .as_f64()
        .or_else(|| layer["timeline"]["out_frame"].as_f64())
        .unwrap_or(frames as f64);
    frame >= 0 && (frame as u64) < frames && frame as f64 >= begin && (frame as f64) < end
}
fn draw_reference(
    list: &mut PaintList,
    track: Rect,
    y: f32,
    height: f32,
    ppf: f32,
    offset: f32,
    selection: Selection,
    generated: bool,
    enabled: bool,
    panels: &mut Panels,
    input: &Input,
    out: &mut Vec<Action>,
) {
    let x = track.min[0] + (selection.frame as f32 - offset) * ppf;
    if x < track.min[0] || x > track.max[0] {
        return;
    }
    let colors = list.styles().colors();
    let selected = panels.audio_controls.selection == Some(selection)
        || matches!(selection.item, Item::Marker(id) if panels.audio_controls.batch.as_ref()
            .is_some_and(|(object, ids)| *object == selection.object && ids.contains(&id)));
    let color = if selected {
        colors.KEYFRAME
    } else if generated {
        colors.TEXT_MUTED
    } else {
        colors.ACCENT
    };
    if matches!(selection.item, Item::Marker(_)) {
        paint::line(list, vec![[x, y + 2.], [x, y + height]], color);
        paint::rect(
            list,
            Rect::new(x, y + 2., 8f32.min(track.max[0] - x), 6.),
            color,
        );
    } else {
        paint::diamond(
            list,
            [x, y + height * 0.5],
            if selected { 5. } else { 4. },
            color,
        );
    }
    if clicked(input, Rect::new(x - 6., y, 12., height)) {
        panels.select_only(Some(selection.object));
        panels.audio_controls.selection = Some(selection);
        panels.audio_controls.active_channel = if let Item::Key(channel) = selection.item {
            Some(channel)
        } else {
            None
        };
        if enabled {
            panels.audio_controls.drag = Some(Drag {
                selection,
                start: input.mouse[0],
                current: selection.frame,
                begun: false,
            });
        }
        out.push(Action::Seek(f64::from(selection.frame)));
    }
}
pub(super) fn process_drag(
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    ppf: f32,
    out: &mut Vec<Action>,
) {
    let Some(drag) = panels.audio_controls.drag.as_mut() else {
        return;
    };
    let layer = state["project"]["layers"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|l| l["id"] == drag.selection.object);
    if input.focus_lost
        || input.keys.iter().any(|(k, _)| *k == Key::Escape)
        || state["observing"] == true
        || layer.is_none_or(|l| l["locked"] == true)
    {
        if panels.audio_controls.cancel() {
            out.push(Action::History(4));
        }
        return;
    }
    let delta = input.mouse[0] - drag.start;
    if (input.dragging() || input.released.is_some()) && delta.abs() >= 3. {
        let layer = layer.unwrap();
        let begin = layer["subframe_range"]["in_frame"]
            .as_f64()
            .or_else(|| layer["timeline"]["in_frame"].as_f64())
            .unwrap_or(0.)
            .ceil()
            .max(0.);
        let end = layer["subframe_range"]["out_frame"]
            .as_f64()
            .or_else(|| layer["timeline"]["out_frame"].as_f64())
            .unwrap_or(state["project"]["frames"].as_f64().unwrap_or(1.));
        let last = (end.ceil() - 1.).min(state["project"]["frames"].as_f64().unwrap_or(1.) - 1.);
        if last < begin {
            return;
        }
        let to = (drag.selection.frame as f64 + f64::from(delta / ppf.max(0.001)))
            .round()
            .clamp(begin, last) as u32;
        if to != drag.current {
            if !drag.begun {
                out.push(Action::History(2));
                drag.begun = true;
            }
            out.push(Action::Edit(match drag.selection.item {
                Item::Key(channel) => Command::AudioLevels {
                    object: drag.selection.object,
                    action: AudioLevelsAction::MoveKey {
                        channel,
                        from: drag.current,
                        to,
                    },
                },
                Item::Marker(id) => Command::AudioMarkers {
                    object: drag.selection.object,
                    action: motion_core::AudioMarkersAction::Move { id, frame: to },
                },
            }));
            drag.current = to;
            panels.audio_controls.selection = Some(Selection {
                frame: to,
                ..drag.selection
            });
        }
    }
    if input.released.is_some() || input.held.is_none() {
        if drag.begun {
            out.push(Action::History(3));
        }
        panels.audio_controls.drag = None;
    }
}
pub(super) fn shortcut(
    command: crate::shortcuts::Command,
    panels: &mut Panels,
    state: &Value,
    out: &mut Vec<Action>,
) -> bool {
    use crate::shortcuts::Command as Shortcut;
    if panels.focused != Some(Panel::Timeline) {
        return false;
    }
    let Some(object) = panels.selected else {
        return false;
    };
    let Some(layer) = state["project"]["layers"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|l| l["id"] == object && is_audio(l))
    else {
        return false;
    };
    if command == Shortcut::Delete {
        if let Some((target, ids)) = panels.audio_controls.batch.take() {
            if target == object && layer["locked"] != true && state["observing"] != true {
                let commands: Vec<_> = layer["audio_markers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|marker| {
                        marker["generated"] == true
                            && marker["id"].as_u64().is_some_and(|id| ids.contains(&id))
                    })
                    .map(|marker| Command::AudioMarkers {
                        object,
                        action: motion_core::AudioMarkersAction::Remove {
                            id: marker["id"].as_u64().unwrap(),
                        },
                    })
                    .collect();
                if !commands.is_empty() {
                    out.push(Action::EditBatch(commands));
                }
            }
            return true;
        }
        if let Some(selection) = panels
            .audio_controls
            .selection
            .filter(|s| s.object == object)
        {
            let exists = match selection.item {
                Item::Key(channel) => {
                    let control = if channel == AudioChannel::Left {
                        Control::Left
                    } else {
                        Control::Right
                    };
                    key_times(
                        &track(layer, control),
                        layer["timeline"]["offset_frame"].as_i64().unwrap_or(0),
                        state["project"]["frames"].as_u64().unwrap_or(1),
                    )
                    .contains(&selection.frame)
                }
                Item::Marker(id) => layer["audio_markers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|m| m["id"] == id),
            };
            if !exists {
                panels.audio_controls.selection = None;
                return true;
            }
            if layer["locked"] != true && state["observing"] != true {
                out.push(Action::Edit(match selection.item {
                    Item::Key(channel) => Command::AudioLevels {
                        object,
                        action: AudioLevelsAction::DeleteKey {
                            channel,
                            frame: selection.frame,
                        },
                    },
                    Item::Marker(id) => Command::AudioMarkers {
                        object,
                        action: motion_core::AudioMarkersAction::Remove { id },
                    },
                }));
                panels.audio_controls.selection = None;
            }
            return true;
        }
    }
    if matches!(command, Shortcut::PreviousKey | Shortcut::NextKey)
        && (layer["content"]["kind"] == "audio" || panels.audio_controls.active_channel.is_some())
    {
        let offset = layer["timeline"]["offset_frame"].as_i64().unwrap_or(0);
        let frames = state["project"]["frames"].as_u64().unwrap_or(1);
        let mut times = vec![];
        for control in controls(layer).into_iter().filter(|c| *c != Control::Gain) {
            if panels
                .audio_controls
                .active_channel
                .is_none_or(|c| c == control.channel())
            {
                times.extend(key_times(&track(layer, control), offset, frames));
            }
        }
        let now = state["frame"].as_f64().unwrap_or(0.);
        let next = if command == Shortcut::NextKey {
            times.into_iter().filter(|f| f64::from(*f) > now).min()
        } else {
            times.into_iter().filter(|f| f64::from(*f) < now).max()
        };
        if let Some(frame) = next {
            out.push(Action::Seek(f64::from(frame)));
        }
        return true;
    }
    false
}
pub(super) fn render(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    let Some(layer) = auxiliary::selected(panels, state).filter(|l| is_audio(l)) else {
        label(
            list,
            atlas,
            area.shrink(10.),
            &word(
                panels.locale,
                "选择音频、视频或子合成图层",
                "Select an audio, video or composition layer",
            ),
            true,
        );
        return;
    };
    let object = layer["id"].as_u64().unwrap_or(0);
    let mut y = area.min[1] + 10.;
    auxiliary::line(
        list,
        atlas,
        area,
        &mut y,
        layer["name"].as_str().unwrap_or("Audio"),
        false,
    );
    if matches!(layer["content"]["kind"].as_str(), Some("audio" | "video")) {
        let gain = gain_db(source(layer)["volume"].as_f64().unwrap_or(1.));
        auxiliary::line(
            list,
            atlas,
            area,
            &mut y,
            &format!(
                "{}: {} dB",
                word(panels.locale, "增益", "Gain"),
                if gain <= SILENCE_DB {
                    "−∞".into()
                } else {
                    format!("{gain:.2}")
                }
            ),
            true,
        );
    }
    button(
        list,
        atlas,
        Rect::new(area.min[0] + 10., y + 8., (area.width() - 20.).max(1.), 26.),
        &word(panels.locale, "在时间轴调节", "Edit in timeline"),
        input,
        true,
        Action::ShowPanel(Panel::Timeline.dock()),
        out,
    );
    if clicked(
        input,
        Rect::new(area.min[0] + 10., y + 8., (area.width() - 20.).max(1.), 26.),
    ) {
        panels.timeline.expanded.insert(object);
        panels.timeline.collapsed_audio.remove(&object);
        panels.timeline.scroll = 0.;
    }
}

#[cfg(test)]
mod tests;
