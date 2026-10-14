//! Audio controls use the shared gain, stereo envelope and gesture commands.
use super::*;
use motion_core::{AudioChannel, AudioLevelsAction, Track};
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
    field: Option<Field>,
    context: Option<(String, String)>,
    pub cursor: bool,
    pub drag_readout: Option<([f32; 2], String)>,
}
impl Controls {
    pub fn editing(&self) -> bool {
        self.field.is_some()
    }
    pub fn cancel(&mut self) -> bool {
        self.field
            .take()
            .is_some_and(|mut field| field.number.cancel())
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
struct Row {
    number: Rect,
    buttons: [Rect; 3],
    height: f32,
}
fn row(area: Rect, y: f32) -> Row {
    let left = area.min[0] + 10.;
    let width = (area.width() - 20.).max(1.);
    let stacked = width < 184.;
    let numeric_width = if stacked { width } else { width - 82. };
    let button_y = if stacked { y + 28. } else { y };
    Row {
        number: Rect::new(left, y, numeric_width, 24.),
        buttons: std::array::from_fn(|i| {
            Rect::new(
                (if stacked {
                    left
                } else {
                    left + numeric_width + 4.
                }) + i as f32 * 26.,
                button_y,
                24.,
                24.,
            )
        }),
        height: if stacked { 56. } else { 28. },
    }
}
fn key_times(track: &Track<f32>, offset: i64, frames: u64) -> Vec<u32> {
    track
        .keys
        .iter()
        .filter_map(|key| {
            let composition = i64::from(key.frame) + offset;
            (composition >= 0 && (composition as u64) < frames).then(|| composition as u32)
        })
        .collect()
}
fn pending_cancel(controls: &mut Controls, out: &mut Vec<Action>) {
    if controls.cancel() {
        out.push(Action::History(4));
    }
}
fn key_button(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    icon: motion_ui::icons::Icon,
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
        16.,
        if !enabled {
            colors.TEXT_MUTED.with_alpha(0.4)
        } else if active {
            colors.ACCENT
        } else {
            colors.TEXT_MUTED
        },
    );
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
    let styles = list.styles();
    paint::rect(list, area, styles.colors().PANEL);
    let selected = auxiliary::selected(panels, state).cloned();
    let Some(layer) = selected.filter(|layer| {
        matches!(
            layer["content"]["kind"].as_str(),
            Some("audio" | "video" | "composition")
        )
    }) else {
        pending_cancel(&mut panels.audio_controls, out);
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
    let locale = panels.locale;
    let height = if area.width() < 204. { 410. } else { 330. };
    let mut y = auxiliary::body_y(Panel::Audio, area, height, panels, input);
    let controls = &mut panels.audio_controls;
    controls.cursor = false;
    controls.drag_readout = None;
    let object = layer["id"].as_u64().unwrap();
    let enabled = layer["locked"] != true && state["observing"] != true;
    let context = (
        state["root"].as_str().unwrap_or("").to_string(),
        state["composition"].as_str().unwrap_or("").to_string(),
    );
    if controls.context.as_ref() != Some(&context)
        || controls.field.as_ref().is_some_and(|field| {
            field.object != object
                || !enabled
                || input
                    .pressed
                    .is_some_and(|(_, point)| !field.rect.contains(point))
        })
    {
        pending_cancel(controls, out);
    }
    controls.context = Some(context);
    auxiliary::line(
        list,
        atlas,
        area,
        &mut y,
        layer["name"].as_str().unwrap_or("Audio"),
        false,
    );
    let kind = layer["content"]["kind"].as_str().unwrap();
    if matches!(kind, "audio" | "video") {
        let audio = &layer["content"][kind];
        auxiliary::line(
            list,
            atlas,
            area,
            &mut y,
            &word(locale, "基础增益（dB）", "Base gain (dB)"),
            true,
        );
        let fields = row(area, y);
        let volume = audio["volume"].as_f64().unwrap_or(1.);
        controls.number(
            list,
            atlas,
            fields.number,
            object,
            Control::Gain,
            gain_db(volume),
            NumberOptions {
                min: SILENCE_DB,
                max: gain_db(2.),
                speed: 0.1,
                decimals: 2,
            },
            state,
            input,
            enabled,
            out,
        );
        button(
            list,
            atlas,
            Rect::new(fields.buttons[0].min[0], fields.buttons[0].min[1], 38., 24.),
            "0 dB",
            input,
            enabled,
            Action::Edit(Command::SetAudio {
                object,
                volume: Some(1.),
                muted: None,
            }),
            out,
        );
        button(
            list,
            atlas,
            Rect::new(
                fields.buttons[0].min[0] + 40.,
                fields.buttons[0].min[1],
                38.,
                24.,
            ),
            "−∞",
            input,
            enabled,
            Action::Edit(Command::SetAudio {
                object,
                volume: Some(0.),
                muted: None,
            }),
            out,
        );
        y += fields.height;
        let muted = audio["muted"] == true;
        if motion_ui::widgets::Checkbox::draw(
            list,
            atlas,
            Rect::new(area.min[0] + 10., y, area.width() - 20., 24.),
            &word(locale, "静音", "Mute"),
            muted,
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
                muted: Some(!muted),
            }));
        }
        y += 32.;
    }
    let supported = state["capabilities"]["audio"]["levels"]["supported"] == true;
    let range = &state["capabilities"]["audio"]["levels"]["range"];
    let options = NumberOptions {
        min: range[0]
            .as_f64()
            .unwrap_or(f64::from(motion_core::audio_levels::MIN_AUDIO_DB)),
        max: range[1]
            .as_f64()
            .unwrap_or(f64::from(motion_core::audio_levels::MAX_AUDIO_DB)),
        speed: 0.1,
        decimals: 2,
    };
    let current = edit_frame(state);
    let offset = layer["timeline"]["offset_frame"].as_i64().unwrap_or(0);
    let frames = state["project"]["frames"].as_u64().unwrap_or(0);
    for control in [Control::Left, Control::Right] {
        let track = if layer["audio_levels"][control.name()].is_null() {
            Track::constant(0.)
        } else {
            let Ok(track) =
                serde_json::from_value::<Track<f32>>(layer["audio_levels"][control.name()].clone())
            else {
                continue;
            };
            track
        };
        let animated = !track.keys.is_empty();
        let label_area = Rect::new(area.min[0] + 10., y, (area.width() - 98.).max(1.), 24.);
        label(
            list,
            atlas,
            label_area,
            &word(
                locale,
                if control == Control::Left {
                    "左声道（dB）"
                } else {
                    "右声道（dB）"
                },
                if control == Control::Left {
                    "Left (dB)"
                } else {
                    "Right (dB)"
                },
            ),
            true,
        );
        if motion_ui::widgets::Checkbox::draw(
            list,
            atlas,
            Rect::new(area.max[0] - 82., y, 72., 24.),
            &word(locale, "动画", "Animate"),
            animated,
            input,
            enabled && supported,
            false,
            None,
        )
        .clicked
        {
            out.push(Action::Edit(Command::AudioLevels {
                object,
                action: AudioLevelsAction::Animate {
                    channel: control.channel(),
                    frame: current,
                    enabled: !animated,
                },
            }));
        }
        y += 26.;
        let fields = row(area, y);
        let value = f64::from(track.sample(state["frame"].as_f64().unwrap_or(0.) - offset as f64))
            .clamp(options.min, options.max);
        controls.number(
            list,
            atlas,
            fields.number,
            object,
            control,
            value,
            options,
            state,
            input,
            enabled && supported,
            out,
        );
        let times = key_times(&track, offset, frames);
        let previous = times.iter().copied().filter(|frame| *frame < current).max();
        let next = times.iter().copied().filter(|frame| *frame > current).min();
        let has_key = times.contains(&current);
        key_button(
            list,
            atlas,
            fields.buttons[0],
            motion_ui::icons::Icon::Previous,
            false,
            input,
            supported && previous.is_some(),
            Action::Seek(f64::from(previous.unwrap_or(current))),
            out,
        );
        key_button(
            list,
            atlas,
            fields.buttons[1],
            motion_ui::icons::Icon::Key,
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
        key_button(
            list,
            atlas,
            fields.buttons[2],
            motion_ui::icons::Icon::Next,
            false,
            input,
            supported && next.is_some(),
            Action::Seek(f64::from(next.unwrap_or(current))),
            out,
        );
        y += fields.height + 8.;
    }
    button(
        list,
        atlas,
        Rect::new(area.min[0] + 10., y, (area.width() - 20.).max(1.), 26.),
        &word(locale, "清除左右声道包络", "Clear stereo envelopes"),
        input,
        enabled && supported && layer["audio_levels"].is_object(),
        Action::Edit(Command::AudioLevels {
            object,
            action: AudioLevelsAction::Clear,
        }),
        out,
    );
    y += 32.;
    label(
        list,
        atlas,
        Rect::new(area.min[0] + 10., y, (area.width() - 20.).max(1.), 24.),
        &word(
            locale,
            "两侧跳到前后关键帧，菱形增删当前键",
            "Previous/next seek; diamond adds/removes the current key",
        ),
        true,
    );
    if controls.cursor {
        panels.cursor = Some(motion_ui::widgets::numeric_behavior::cursor(
            input,
            controls.drag_readout.is_some(),
        ));
    }
    if controls.drag_readout.is_some() {
        panels.drag_readout = controls.drag_readout.clone();
    }
}

#[cfg(test)]
mod tests;
