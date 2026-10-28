use super::*;
use std::time::{Duration, Instant};
#[cfg(all(test, target_os = "windows"))]
mod native_tests;

#[derive(Default)]
pub(crate) struct Controls {
    balance: numeric::Field,
    context: Option<(String, String, u64)>,
    levels: crate::audio_output::Levels,
    full_scale_until: [Option<Instant>; 2],
    last: Option<Instant>,
    playing: bool,
    waveform: waveform::Waveform,
    pub(super) balance_rect: Option<Rect>,
}
impl Controls {
    pub(crate) fn busy(&self) -> bool {
        self.waveform.busy()
    }
    pub fn editing(&self) -> bool {
        self.balance.editor.interacting()
    }
    pub fn cancel(&mut self) -> bool {
        self.balance.cancel()
    }
    pub(in crate::panels) fn text_menu(
        &mut self,
        request: &mut super::super::text_menu::Request<'_>,
    ) {
        request.visit(&mut self.balance.editor.text);
    }
    pub(crate) fn poll(&mut self, audio: Option<&crate::audio_output::AudioOutput>) -> bool {
        let wave = self.waveform.poll();
        if self
            .last
            .is_some_and(|t| t.elapsed() < Duration::from_millis(33))
        {
            return wave;
        }
        self.last = Some(Instant::now());
        let levels = audio.map(|a| a.take_levels()).unwrap_or_default();
        let before = self.full_scale_until;
        let now = Instant::now();
        for (channel, until) in self.full_scale_until.iter_mut().enumerate() {
            if audio.is_some() && levels.full_scale(channel) {
                *until = Some(now + Duration::from_secs(1));
            } else if audio.is_none() || until.is_some_and(|t| t <= now) {
                *until = None;
            }
        }
        let changed = levels != self.levels
            || self.playing != audio.is_some()
            || before != self.full_scale_until;
        self.levels = levels;
        self.playing = audio.is_some();
        wave || changed
    }
}
pub(crate) fn prepare(
    panels: &mut Panels,
    window: Option<Panel>,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    let context = panels.selected.map(|object| {
        (
            state["root"].as_str().unwrap_or("").into(),
            state["composition"].as_str().unwrap_or("").into(),
            object,
        )
    });
    let unlocked =
        auxiliary::selected(panels, state).is_some_and(|l| l["locked"] != true && is_audio(l));
    let supported = state["capabilities"]["audio"]["balance"]["supported"] == true;
    let value = auxiliary::selected(panels, state)
        .and_then(|l| l["audio_levels"]["balance"].as_f64())
        .unwrap_or(0.)
        * 100.;
    let controls = &mut panels.auxiliary.audio;
    if controls.context == context
        && unlocked
        && supported
        && state["observing"] != true
        && controls.balance.editor.text.active()
        && !input.composing
        && !input.ime_consumed
        && !input.focus_lost
    {
        if let Some(rect) = controls
            .balance_rect
            .filter(|rect| input.pressed.is_some_and(|(_, p)| !rect.contains(p)))
        {
            // Use the same numeric validation and transaction path as Enter.
            let finish = Input {
                keys: vec![(Key::Enter, Default::default())],
                text: input.text.clone(),
                ..Default::default()
            };
            let events = controls
                .balance
                .editor
                .update(value, balance_options(), &finish, rect);
            if let Some((_, _, object)) = &context {
                numeric::apply(events, out, |value| Command::AudioLevels {
                    object: *object,
                    action: AudioLevelsAction::Balance {
                        value: value / 100.,
                    },
                });
            }
            if controls.balance.editor.text.error.is_some() {
                panels.message = Some(word(
                    panels.locale,
                    "请输入 -100 到 100 之间的有效平衡值",
                    "Enter a finite balance value between -100 and 100",
                ));
            }
        }
    }
    if controls.context != context
        || !unlocked
        || !supported
        || state["observing"] == true
        || input.focus_lost
        || (window.is_some_and(|p| p != Panel::Audio)
            && (input.pressed.is_some() || !input.keys.is_empty() || !input.text.is_empty()))
    {
        if controls.cancel() {
            out.push(Action::History(4));
        }
    }
    if controls.context != context {
        controls.waveform.request(None);
        controls.context = context;
    }
}

fn balance_options() -> NumberOptions {
    NumberOptions {
        min: -100.,
        max: 100.,
        speed: 1.,
        decimals: 1,
    }
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
    let mut clipped;
    let input = if input
        .pressed
        .is_some_and(|(_, point)| !area.contains(point))
    {
        clipped = input.clone();
        clipped.pressed = None;
        &clipped
    } else {
        input
    };
    let styles = list.styles();
    let pad = styles.metric("padding.panel", 8.);
    let row = styles.metric("row.height", 22.);
    let line = styles.metric("border", 1.);
    let value_width = styles.metric("audio.balance.value.width", 64.);
    let wave_height = styles.metric("audio.waveform.height", 88.);
    let content_height =
        row * 9. + styles.metric("audio.meter.bar.height", 6.) * 2. + wave_height + pad * 7.;
    let mut y = auxiliary::body_y(&styles, Panel::Audio, area, content_height, panels, input);
    let x = area.min[0] + pad;
    let width = (area.width() - pad * 2.).max(0.);
    let locale = panels.locale;
    label(
        list,
        atlas,
        Rect::new(x, y, width, row),
        &word(locale, "主输出 · 设备峰值", "Master output · device peak"),
        false,
    );
    y += row;
    for channel in 0..2 {
        let levels = panels.auxiliary.audio.levels;
        let db = levels.db(channel);
        let value = if db.is_finite() {
            format!("{db:.1} dBFS")
        } else {
            "-inf dBFS".into()
        };
        let full_scale = levels.full_scale(channel)
            || panels.auxiliary.audio.full_scale_until[channel].is_some();
        let note = if full_scale {
            word(locale, " · 满幅", " · Full scale")
        } else {
            String::new()
        };
        label(
            list,
            atlas,
            Rect::new(x, y, width, row),
            &format!("{}   {value}{note}", if channel == 0 { "L" } else { "R" }),
            true,
        );
        y += row;
        let bar = styles.metric("audio.meter.bar.height", 6.);
        let track = Rect::new(x, y, width, bar);
        paint::rect(list, track, styles.color("well"));
        let fraction = ((db.max(-60.) + 60.) / 60.).clamp(0., 1.);
        paint::rect(
            list,
            Rect::new(x, y, width * fraction, bar),
            styles.color(if full_scale { "error" } else { "curve" }),
        );
        y += bar + pad;
    }
    y += pad;
    paint::rect(list, Rect::new(x, y, width, line), styles.color("border"));
    y += pad;
    let Some(layer) = auxiliary::selected(panels, state).filter(|l| is_audio(l)) else {
        panels.auxiliary.audio.waveform.request(None);
        label(
            list,
            atlas,
            Rect::new(x, y, width, row),
            &word(
                locale,
                "选择音频、视频或子合成图层",
                "Select an audio, video or composition layer",
            ),
            true,
        );
        return;
    };
    let object = layer["id"].as_u64().unwrap_or(0);
    label(
        list,
        atlas,
        Rect::new(x, y, width, row),
        layer["name"].as_str().unwrap_or("Audio"),
        false,
    );
    y += row;
    let number_width = value_width.min(width * 0.5);
    let field = Rect::new(x + width - number_width, y, number_width, row);
    panels.auxiliary.audio.balance_rect = Some(field);
    label(
        list,
        atlas,
        Rect::new(x, y, (width - number_width - pad).max(0.), row),
        &word(locale, "平衡 L / R (%)", "Balance L / R (%)"),
        true,
    );
    let enabled = layer["locked"] != true
        && state["observing"] != true
        && state["capabilities"]["audio"]["balance"]["supported"] == true;
    let value = layer["audio_levels"]["balance"].as_f64().unwrap_or(0.) * 100.;
    let controls = &mut panels.auxiliary.audio;
    if enabled {
        let events = controls
            .balance
            .draw(list, atlas, field, value, balance_options(), input);
        numeric::apply(events, out, |value| Command::AudioLevels {
            object,
            action: AudioLevelsAction::Balance {
                value: value / 100.,
            },
        });
        if area.contains(input.mouse) && field.contains(input.mouse)
            || controls.balance.editor.dragging()
        {
            panels.cursor = Some(if controls.balance.editor.text.active() {
                motion_ui::cursor::Cursor::Text
            } else {
                motion_ui::cursor::Cursor::ResizeEW
            });
        }
        if let Some(note) = controls.balance.readout() {
            panels.drag_readout = Some((input.mouse, format!("{note} %")));
        }
    } else {
        numeric::passive_with_precision(list, atlas, field, value, input, false, 1);
    }
    y += row;
    if !enabled && state["capabilities"]["audio"]["balance"]["supported"] != true {
        label(
            list,
            atlas,
            Rect::new(x, y, width, row),
            &word(
                locale,
                "后端不支持平衡控制",
                "Balance control is unsupported by this backend",
            ),
            true,
        );
        y += row;
    }
    let request = waveform::Request::from_state(state, layer, width.ceil().max(1.) as usize);
    match request {
        Ok(request) => controls.waveform.request(request),
        Err(error) => {
            let _ = observe!(
                "audio.waveform_request",
                Err::<(), _>(&error),
                "object={object}"
            );
            controls.waveform.request(None);
            controls.waveform.error = Some(error);
        }
    }
    let wave = Rect::new(x, y + pad, width, wave_height);
    label(
        list,
        atlas,
        Rect::new(x, y, width, row),
        &word(locale, "源波形 · 片段范围", "Source waveform · clip range"),
        true,
    );
    y += row;
    let wave = Rect::new(wave.min[0], y, wave.width(), wave.height());
    paint::rect(list, wave, styles.color("well"));
    let center = wave.min[1] + wave.height() * 0.5;
    paint::rect(
        list,
        Rect::new(x, center, width, line),
        styles.color("border"),
    );
    let values = &controls.waveform.values;
    for (index, value) in values.iter().enumerate() {
        let column = width / values.len() as f32;
        let top = center - value.max.clamp(-1., 1.) * wave.height() * 0.5;
        let bottom = center - value.min.clamp(-1., 1.) * wave.height() * 0.5;
        paint::rect(
            list,
            Rect::new(
                x + index as f32 * column,
                top,
                column.max(line),
                (bottom - top).max(line),
            ),
            styles.color("focus"),
        );
    }
    let status = if let Some(error) = &controls.waveform.error {
        format!(
            "{}: {error}",
            word(locale, "波形不可用", "Waveform unavailable")
        )
    } else if controls.waveform.loading() {
        word(locale, "正在读取波形…", "Reading waveform…")
    } else if values.is_empty() {
        word(
            locale,
            "选择源音频图层查看波形",
            "Select a source audio layer to view its waveform",
        )
    } else {
        String::new()
    };
    if !status.is_empty() {
        label(list, atlas, wave.shrink(pad), &status, true);
    }
    y = wave.max[1] + pad;
    if controls.waveform.error.is_some() {
        if ui::button(
            list,
            atlas,
            Rect::new(x, y, width, row),
            &word(locale, "重新读取波形", "Reload waveform"),
            input,
            true,
        )
        .clicked
        {
            controls.waveform.refresh();
        }
        y += row + pad;
    }
    if ui::button(
        list,
        atlas,
        Rect::new(x, y, width, row),
        &word(locale, "在时间轴调节音量", "Edit levels in timeline"),
        input,
        true,
    )
    .clicked
    {
        panels.timeline.expanded.insert(object);
        panels.timeline.collapsed_audio.remove(&object);
        panels.timeline.scroll = 0.;
        out.push(Action::ShowPanel(Panel::Timeline.dock()));
    }
}
