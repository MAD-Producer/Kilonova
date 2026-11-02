use super::*;
use crate::dialogs::keyframe_timing::Layout;
use motion_ui::{
    style::{State, States},
    widgets::{Checkbox, DropdownItem},
};

fn pressed(input: &Input, area: Rect) -> bool {
    input
        .pressed
        .is_some_and(|(button, p)| button == MouseButton::Left && area.contains(p))
}
fn text(list: &mut PaintList, atlas: &mut TextAtlas, area: Rect, value: &str, muted: bool) {
    let sheet = list.styles();
    ui::text(
        list,
        atlas,
        area,
        value,
        sheet.color(if muted { "muted" } else { "text" }),
        Align::Left,
    );
}
impl Field {
    fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        input: &Input,
        focused: bool,
        invalid: bool,
    ) {
        let sheet = list.styles();
        static MISSING: crate::fallback_observation::Counter =
            crate::fallback_observation::Counter::new();
        if focused && !input.focus_lost {
            self.editor
                .input_value(&mut self.text, &mut self.replace, input);
        } else {
            self.editor.cancel();
            self.replace = true;
        }
        let mut states = States::default();
        if area.contains(input.mouse) {
            states = states.with(State::Hovered);
        }
        if focused {
            states = states.with(State::Focused);
        }
        if invalid {
            states = states.with(State::Invalid);
        }
        if self.editor.active() {
            self.editor
                .draw_styled(list, atlas, area, &sheet, "field.number", states);
        } else if let Some(style) = sheet.resolve("field.number", states) {
            MISSING.success();
            motion_ui::style::box_style(list, area, style);
            motion_ui::style::text_style(
                list,
                atlas,
                area.shrink(style.padding),
                &self.text,
                style,
                Align::Right,
            );
        } else {
            MISSING.failure(
                "layer_time.field",
                format_args!("style=field.number fallback=text"),
            );
            text(list, atlas, area, &self.text, false);
        }
    }
}
impl Editor {
    fn menu_open(&self) -> bool {
        self.menus.iter().any(|m| m.open)
    }
    pub(in crate::dialogs) fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        size: [f32; 2],
        input: &Input,
        locale: Locale,
        external: Option<&str>,
    ) -> Option<Event> {
        let sheet = list.styles();
        let layout = Layout::new(&sheet, size);
        let row = sheet.metric("row.height", 22.);
        let gap = sheet.metric("auxiliary.gap", 6.);
        let pad = sheet.metric("preferences.margin", 16.);
        let was_open = self.menu_open();
        let key = |wanted| {
            !input.focus_lost
                && !input.composing
                && !input.ime_consumed
                && input
                    .key_presses()
                    .any(|(key, m, repeat)| key == wanted && !m.control && !m.alt && !repeat)
        };
        if key(Key::Escape) && !was_open {
            return Some(Event::Cancelled);
        }
        if input.focus_lost {
            self.focus = None;
            for menu in &mut self.menus {
                menu.open = false;
            }
        }
        let neutral = Input::default();
        let controls = if was_open || input.focus_lost {
            &neutral
        } else {
            input
        };
        if key(Key::Tab) && !was_open {
            let reverse = input
                .key_presses()
                .any(|(k, m, _)| k == Key::Tab && m.shift);
            let mut next = match (self.focus, reverse) {
                (Some(i), true) => (i + 10) % 11,
                (Some(i), false) => (i + 1) % 11,
                (None, _) => 0,
            };
            if self.remap != 1 && (5..=8).contains(&next) {
                next = if reverse { 4 } else { 9 };
            }
            if self.remap == 1 && self.key_edit.is_none() && (5..=7).contains(&next) {
                if let Err(e) = self.select_key(0) {
                    self.error = Some(e);
                }
            }
            self.focus = Some(next);
        }
        if controls.pressed.is_some() {
            self.focus = None;
            self.error = None;
        }
        paint::rect(
            list,
            Rect::new(0., 0., size[0], size[1]),
            sheet.color("shadow"),
        );
        static MISSING: crate::fallback_observation::Counter =
            crate::fallback_observation::Counter::new();
        if let Some(style) = sheet.resolve("popup", Default::default()) {
            MISSING.success();
            motion_ui::style::box_style(list, layout.card, style);
        } else {
            MISSING.failure(
                "layer_time.popup",
                format_args!("style=popup fallback=panel"),
            );
            paint::rect(list, layout.card, sheet.color("panel"));
        }
        let header = Rect::new(
            layout.card.min[0] + pad,
            layout.card.min[1] + pad,
            layout.body.width(),
            row,
        );
        text(
            list,
            atlas,
            header,
            &word(locale, "图层时间", "Layer time"),
            false,
        );
        text(
            list,
            atlas,
            Rect::new(header.min[0], header.max[1], header.width(), row),
            &if locale == Locale::Zh {
                format!("{} 个图层 · 设置仅在应用后生效", self.layers.len())
            } else {
                format!("{} layers · draft until Apply", self.layers.len())
            },
            true,
        );
        let content_rows = if self.remap == 1 {
            10usize.saturating_add(self.keys.len())
        } else {
            8
        };
        if key(Key::Tab) {
            let y = match self.focus {
                Some(0..=4) => self.focus.unwrap() as f32 * row,
                Some(5..=7) => row * (9. + self.key_edit.as_ref().map_or(0, |e| e.index) as f32),
                Some(8) => row * (9. + self.keys.len() as f32),
                _ => self.scroll.offset,
            };
            if y < self.scroll.offset {
                self.scroll.offset = y;
            } else if y + row > self.scroll.offset + layout.body.height() {
                self.scroll.offset = (y + row - layout.body.height()).max(0.);
            }
        }
        let scrolling = self.scroll.begin_view(
            list,
            layout.body,
            content_rows as f32 * row,
            controls,
            !matches!(self.focus, Some(0 | 5 | 6)),
        );
        let form = scrolling.input.to_owned();
        let area = scrolling.content;
        let popup_input = if was_open { input } else { &form };
        let mut body = list.child();
        let mut overlay = list.child();
        let label = sheet
            .metric("preferences.label_width", 180.)
            .min(area.width() * 0.4);
        let full = |n: f32| Rect::new(area.min[0], area.min[1] + n * row, area.width(), row);
        let value = |n: f32| {
            Rect::new(
                area.min[0] + label + gap,
                area.min[1] + n * row,
                (area.width() - label - gap).max(0.),
                row,
            )
        };
        for (index, title) in [
            word(locale, "时长（%）", "Duration (%)"),
            word(locale, "播放方向", "Direction"),
            word(locale, "伸缩固定点", "Stretch anchor"),
            word(locale, "片段区间", "Clip interval"),
            word(locale, "素材时间映射", "Source remapping"),
        ]
        .iter()
        .enumerate()
        {
            text(
                &mut body,
                atlas,
                Rect::new(area.min[0], area.min[1] + index as f32 * row, label, row),
                title,
                true,
            );
        }
        if pressed(&form, value(0.)) {
            self.focus = Some(0);
        }
        let invalid = self.desired_stretch().is_err();
        self.stretch.draw(
            &mut body,
            atlas,
            value(0.),
            &form,
            self.focus == Some(0),
            invalid,
        );
        let titles = [
            [
                word(locale, "保留各自方向", "Keep each direction"),
                word(locale, "正向", "Forward"),
                word(locale, "反向", "Reverse"),
            ],
            [
                word(locale, "图层起点", "Layer start"),
                word(locale, "当前播放头", "Current playhead"),
                word(locale, "图层终点", "Layer end"),
            ],
            [
                word(locale, "保留现有映射", "Keep existing"),
                word(locale, "编辑重映射…", "Edit remapping…"),
                word(locale, "移除重映射", "Remove remapping"),
            ],
        ];
        for i in 0..3 {
            let n = [1., 2., 4.][i];
            let focus = [1, 2, 4][i];
            let selected = [self.direction, self.anchor, self.remap][i];
            let items: Vec<_> = titles[i]
                .iter()
                .enumerate()
                .map(|(index, label)| DropdownItem {
                    label,
                    enabled: i != 2 || index != 1 || self.remappable(),
                })
                .collect();
            if pressed(&form, value(n)) {
                self.focus = Some(focus);
            }
            let active = self.menus[i].open || !was_open;
            if let Some(next) = self.menus[i].draw_grouped(
                &mut overlay,
                atlas,
                value(n),
                Rect::new(0., 0., size[0], size[1]),
                &items,
                selected,
                if active { popup_input } else { &neutral },
                active,
                self.focus == Some(focus),
                &[],
                true,
            ) {
                match i {
                    0 => self.direction = next,
                    1 => self.anchor = next,
                    _ => self.remap = next,
                };
                self.error = None;
            }
        }
        if Checkbox::draw(
            &mut body,
            atlas,
            value(3.),
            &word(locale, "随时长调整", "Resize with duration"),
            self.resize,
            &form,
            true,
            self.focus == Some(3),
            None,
        )
        .clicked
        {
            self.resize = !self.resize;
            self.focus = Some(3);
        }
        text(
            &mut body,
            atlas,
            full(5.),
            &word(
                locale,
                "200% 时长 = 50% 速度；留空保留原值。",
                "200% duration = 50% speed; blank keeps values.",
            ),
            true,
        );
        text(
            &mut body,
            atlas,
            full(6.),
            &word(
                locale,
                "音高随播放速度变化。",
                "Audio pitch follows playback speed.",
            ),
            true,
        );
        if self.remap == 1 {
            text(
                &mut body,
                atlas,
                full(7.),
                &word(
                    locale,
                    "图层帧已包含伸缩/倒放，素材帧为映射输出。",
                    "Layer time includes stretch/reverse; source is the output.",
                ),
                true,
            );
            let column = (area.width() - gap * 2.).max(0.) / 3.;
            let cell = |n: usize, c: usize| {
                Rect::new(
                    area.min[0] + c as f32 * (column + gap),
                    area.min[1] + (8 + n) as f32 * row,
                    column,
                    row,
                )
            };
            for (i, title) in [
                word(locale, "图层帧", "Layer frame"),
                word(locale, "素材帧", "Source frame"),
                word(locale, "区间插值", "Interpolation"),
            ]
            .iter()
            .enumerate()
            {
                text(
                    &mut body,
                    atlas,
                    Rect::new(cell(0, i).min[0], full(7.).max[1], column, row),
                    title,
                    true,
                );
            }
            // Virtualize the author-sized key table. Only the selected row owns
            // two text editors and one dropdown, regardless of key count.
            let first = ((scrolling.viewport.min[1] - area.min[1]) / row - 9.)
                .floor()
                .max(0.) as usize;
            let end = (((scrolling.viewport.max[1] - area.min[1]) / row - 9.)
                .ceil()
                .max(0.) as usize)
                .saturating_add(1)
                .min(self.keys.len());
            for index in first.min(end)..end {
                let cells = [cell(index + 1, 0), cell(index + 1, 1), cell(index + 1, 2)];
                if cells.iter().any(|r| pressed(&form, *r)) {
                    match self.select_key(index) {
                        Ok(()) => {
                            self.focus =
                                Some(5 + cells.iter().position(|r| pressed(&form, *r)).unwrap_or(0))
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
                if let Some(edit) = self.key_edit.as_mut().filter(|edit| edit.index == index) {
                    let invalid = edit.frame.number().is_err();
                    edit.frame.draw(
                        &mut body,
                        atlas,
                        cells[0],
                        &form,
                        self.focus == Some(5),
                        invalid,
                    );
                    let invalid = edit.source.number().is_err();
                    edit.source.draw(
                        &mut body,
                        atlas,
                        cells[1],
                        &form,
                        self.focus == Some(6),
                        invalid,
                    );
                    let labels = [
                        word(locale, "线性", "Linear"),
                        word(locale, "保持", "Hold"),
                        word(locale, "缓入", "Ease in"),
                        word(locale, "缓出", "Ease out"),
                        word(locale, "缓入缓出", "Ease in/out"),
                        word(locale, "保留自定义曲线", "Custom curve"),
                    ];
                    let easing = self.keys[index].easing;
                    let selected = if easing.curve.is_some() {
                        5
                    } else {
                        match easing.ease {
                            motion_core::Ease::Linear => 0,
                            motion_core::Ease::Hold => 1,
                            motion_core::Ease::In => 2,
                            motion_core::Ease::Out => 3,
                            motion_core::Ease::InOut => 4,
                        }
                    };
                    let items: Vec<_> = labels
                        .iter()
                        .enumerate()
                        .map(|(i, label)| DropdownItem {
                            label,
                            enabled: i != 5 || easing.curve.is_some(),
                        })
                        .collect();
                    let active = self.menus[3].open || !was_open;
                    if let Some(next) = self.menus[3].draw_grouped(
                        &mut overlay,
                        atlas,
                        cells[2],
                        Rect::new(0., 0., size[0], size[1]),
                        &items,
                        selected,
                        if active { popup_input } else { &neutral },
                        active,
                        self.focus == Some(7),
                        &[],
                        true,
                    ) {
                        if next < 5 {
                            self.keys[index].easing = Easing {
                                ease: [
                                    motion_core::Ease::Linear,
                                    motion_core::Ease::Hold,
                                    motion_core::Ease::In,
                                    motion_core::Ease::Out,
                                    motion_core::Ease::InOut,
                                ][next],
                                curve: None,
                            };
                        }
                    }
                } else {
                    for (area, value) in [
                        (cells[0], self.keys[index].frame),
                        (cells[1], self.keys[index].source_frame),
                    ] {
                        let states = if area.contains(form.mouse) {
                            States::default().with(State::Hovered)
                        } else {
                            States::default()
                        };
                        if let Some(style) = sheet.resolve("field.number", states) {
                            motion_ui::style::text_style(
                                &mut body,
                                atlas,
                                area.shrink(style.padding),
                                &value.to_string(),
                                style,
                                Align::Right,
                            );
                        } else {
                            ui::text(
                                &mut body,
                                atlas,
                                area,
                                &value.to_string(),
                                sheet.color("text"),
                                Align::Right,
                            );
                        }
                    }
                    let label = if self.keys[index].easing.curve.is_some() {
                        word(locale, "自定义曲线", "Custom curve")
                    } else {
                        match self.keys[index].easing.ease {
                            motion_core::Ease::Linear => word(locale, "线性", "Linear"),
                            motion_core::Ease::Hold => word(locale, "保持", "Hold"),
                            motion_core::Ease::In => word(locale, "缓入", "Ease in"),
                            motion_core::Ease::Out => word(locale, "缓出", "Ease out"),
                            motion_core::Ease::InOut => word(locale, "缓入缓出", "Ease in/out"),
                        }
                    };
                    text(&mut body, atlas, cells[2], &label, true);
                }
            }
            let toolbar = full(9. + self.keys.len() as f32);
            let width = sheet
                .metric("preferences.button_width", 96.)
                .min((toolbar.width() - gap).max(0.) * 0.5);
            if ui::button_with_states(
                &mut body,
                atlas,
                Rect::new(toolbar.min[0], toolbar.min[1], width, row),
                &word(locale, "播放头添加", "Add at playhead"),
                &form,
                self.keys.len() < layer_time::MAX_TIME_REMAP_KEYS,
                &sheet,
                "button",
                if self.focus == Some(8) {
                    States::default().with(State::Focused)
                } else {
                    States::default()
                },
            )
            .clicked
            {
                if let Err(e) = self.add_key() {
                    self.error = Some(e);
                }
            }
            if ui::button(
                &mut body,
                atlas,
                Rect::new(toolbar.min[0] + width + gap, toolbar.min[1], width, row),
                &word(locale, "移除所选", "Remove selected"),
                &form,
                self.key_edit.is_some() && self.keys.len() > 1,
            )
            .clicked
            {
                match self.commit_key() {
                    Ok(()) => {
                        let index = self.key_edit.take().unwrap().index;
                        self.keys.remove(index);
                        self.focus = None;
                    }
                    Err(e) => self.error = Some(e),
                }
            }
        } else if !self.remappable() {
            text(
                &mut body,
                atlas,
                full(7.),
                &word(
                    locale,
                    "编辑重映射需单选视频、音频、合成或模型。",
                    "Select one media, composition or model to edit remapping.",
                ),
                true,
            );
        }
        body.clip_to(scrolling.viewport);
        list.append(body);
        if let Some(error) = self.error.as_deref().or(external) {
            let message =
                error
                    .split_once(" / ")
                    .map_or(error, |(zh, en)| if locale == Locale::Zh { zh } else { en });
            ui::text(
                list,
                atlas,
                layout.error,
                message,
                sheet.color("error"),
                Align::Left,
            );
        }
        let footer = if was_open || self.menu_open() {
            &neutral
        } else {
            controls
        };
        let cancel = ui::button(
            list,
            atlas,
            layout.cancel,
            &word(locale, "取消", "Cancel"),
            footer,
            true,
        )
        .clicked;
        let apply = ui::button(
            list,
            atlas,
            layout.apply,
            &word(locale, "应用", "Apply"),
            footer,
            true,
        )
        .clicked;
        list.append(overlay);
        if cancel || self.focus == Some(9) && key(Key::Enter) {
            return Some(Event::Cancelled);
        }
        if !was_open && key(Key::Enter) && matches!(self.focus, Some(0 | 5 | 6)) {
            self.focus = None;
            return None;
        }
        if apply || !was_open && key(Key::Enter) && matches!(self.focus, None | Some(10)) {
            match self.commands() {
                Ok(commands) => {
                    return Some(Event::LayerTime {
                        source: self.source.clone(),
                        commands,
                    })
                }
                Err(e) => self.error = Some(e),
            }
        }
        None
    }
}
