use super::*;
use motion_ui::widgets::{Checkbox, DropdownItem};
impl Dialog {
    pub(super) fn timing_policy(&self) -> &'static str {
        if self.timing_index == 0 { "preserve_seconds" } else { "preserve_frames" }
    }

    pub(super) fn composition_ui(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        size: [f32; 2],
        input: &Input,
        locale: Locale,
    ) -> Option<Event> {
        let sheet = list.styles();
        let colors = sheet.colors();
        let zh = locale == Locale::Zh;
        let dropdown_was_open = self.preset_dropdown.open || self.timing_dropdown.open;
        if !dropdown_was_open && input.key_presses().any(|(k, _, _)| k == Key::Escape) && !input.composing {
            return Some(Event::Cancelled);
        }
        let card = Rect::new((size[0] - 640.) * 0.5, (size[1] - 510.) * 0.5, 640., 510.);
        paint::rect(
            list,
            Rect::new(0., 0., size[0], size[1]),
            sheet.color("well").with_alpha(0.9),
        );
        motion_ui::style::box_style(
            list,
            card,
            sheet.resolve("popup", Default::default()).unwrap(),
        );
        ui::text_sized(
            list,
            atlas,
            Rect::new(card.min[0] + 20., card.min[1] + 18., 540., 30.),
            if zh {
                "合成设置"
            } else {
                "Composition settings"
            },
            colors.TEXT,
            Align::Left,
            18.,
        );
        if ui::button_icon(
            list,
            Rect::new(card.max[0] - 42., card.min[1] + 16., 24., 24.),
            motion_ui::icons::Icon::Close,
            input,
            true,
        )
        .clicked
            && !dropdown_was_open
        {
            return Some(Event::Cancelled);
        }
        for (i, label) in [
            if zh { "基本" } else { "Basic" },
            if zh { "高级" } else { "Advanced" },
            "3D",
        ]
        .into_iter()
        .enumerate()
        {
            let rect = Rect::new(
                card.min[0] + 20. + i as f32 * 64.,
                card.min[1] + 58.,
                56.,
                26.,
            );
            if self.settings_tab == i {
                paint::rect(
                    list,
                    Rect::new(rect.min[0], rect.max[1] - 1., rect.width(), 1.),
                    colors.TEXT,
                );
            }
            ui::text(
                list,
                atlas,
                rect,
                label,
                if self.settings_tab == i {
                    colors.TEXT
                } else {
                    colors.TEXT_MUTED
                },
                Align::Center,
            );
            if !dropdown_was_open && input.pressed.is_some_and(|(_, p)| rect.contains(p)) {
                self.settings_tab = i;
            }
        }
        paint::rect(
            list,
            Rect::new(card.min[0] + 20., card.min[1] + 98., 600., 0.5),
            colors.BORDER,
        );
        let mut y = card.min[1] + 112.;
        let field_x = card.min[0] + 142.;
        let mut overlays = list.child();
        if self.settings_tab == 0 {
            let rows = [
                (
                    if zh {
                        "合成名称"
                    } else {
                        "Composition name"
                    },
                    Some(0),
                ),
                (if zh { "预设" } else { "Preset" }, None),
                (if zh { "宽度" } else { "Width" }, Some(1)),
                (if zh { "高度" } else { "Height" }, Some(2)),
                (
                    if zh {
                        "像素长宽比"
                    } else {
                        "Pixel aspect ratio"
                    },
                    None,
                ),
                (if zh { "帧速率" } else { "Frame rate" }, Some(3)),
                (
                    if zh {
                        "开始时间码"
                    } else {
                        "Start timecode"
                    },
                    None,
                ),
                (if zh { "持续时间" } else { "Duration" }, Some(4)),
                (if zh { "背景颜色" } else { "Background" }, None),
            ];
            for (index, (label, field_index)) in rows.into_iter().enumerate() {
                ui::text(
                    list,
                    atlas,
                    Rect::new(card.min[0] + 20., y, 120., 24.),
                    label,
                    colors.TEXT_MUTED,
                    Align::Left,
                );
                let rect = Rect::new(
                    field_x,
                    y,
                    if matches!(index, 2 | 3 | 5 | 6 | 7) {
                        164.
                    } else {
                        476.
                    },
                    24.,
                );
                if let Some(i) = field_index {
                    let mut states = motion_ui::style::States::default();
                    if self.focus == i {
                        states = states.with(motion_ui::style::State::Focused);
                    }
                    motion_ui::style::box_style(
                        list,
                        rect,
                        sheet.resolve("field", states).unwrap(),
                    );
                    let shown = if i == 4 && self.fields[4].parse::<u32>().is_ok() {
                        display_duration(&self.fields[4], &self.fields[3])
                    } else if i == 3 && self.focus != 3 {
                        display_rate(&self.fields[3])
                    } else {
                        self.fields[i].clone()
                    };
                    if self.focus == i && self.text_editor_field == Some(i) && self.text_editor.active() {
                        self.text_editor.draw(list, atlas, rect);
                    } else {
                        ui::text(
                            list,
                            atlas,
                            rect.shrink(5.),
                            &shown,
                            colors.TEXT,
                            Align::Left,
                        );
                    }
                    if !dropdown_was_open && input.pressed.is_some_and(|(_, p)| rect.contains(p)) {
                        self.focus = i;
                        self.replace = true;
                        self.text_editor.cancel();
                    }
                    if index == 2 {
                        if Checkbox::draw(
                            list,
                            atlas,
                            Rect::new(field_x + 174., y, 260., 24.),
                            if zh {
                                "锁定长宽比"
                            } else {
                                "Lock aspect ratio"
                            },
                            self.aspect_locked,
                            input,
                            true,
                            false,
                            None,
                        )
                        .clicked
                            && !dropdown_was_open
                        {
                            self.aspect_locked = !self.aspect_locked;
                            self.aspect = self.fields[1].parse::<f64>().unwrap_or(1920.)
                                / self.fields[2].parse::<f64>().unwrap_or(1080.);
                        }
                    }
                    if index == 5 && matches!(self.kind, Kind::Settings) {
                        let x = rect.max[0] + sheet.metric("auxiliary.gap", 6.);
                        let right = card.max[0] - sheet.metric("preferences.margin", 16.);
                        let items = [
                            DropdownItem { label: if zh { "保持秒时间" } else { "Keep time (seconds)" }, enabled:true },
                            DropdownItem { label: if zh { "保持帧位置" } else { "Keep frame positions" }, enabled:true },
                        ];
                        if let Some(index) = self.timing_dropdown.draw(&mut overlays, atlas,
                            Rect::new(x, rect.min[1], right-x, rect.height()), card, &items,
                            self.timing_index, input, !self.preset_dropdown.open, false) {
                            self.timing_index = index;
                        }
                    }
                } else if index == 1 {
                    let items = [
                        DropdownItem {
                            label: "HD · 1920 × 1080 · 30 fps",
                            enabled: true,
                        },
                        DropdownItem {
                            label: "4K · 3840 × 2160 · 30 fps",
                            enabled: true,
                        },
                        DropdownItem {
                            label: "Portrait · 1080 × 1920 · 30 fps",
                            enabled: true,
                        },
                        DropdownItem {
                            label: "Square · 1080 × 1080 · 30 fps",
                            enabled: true,
                        },
                    ];
                    if let Some(index) = self.preset_dropdown.draw(
                        &mut overlays,
                        atlas,
                        rect,
                        card,
                        &items,
                        self.preset_index,
                        input,
                        !self.timing_dropdown.open,
                        false,
                    ) {
                        self.preset_index = index;
                        let (w, h) =
                            [(1920, 1080), (3840, 2160), (1080, 1920), (1080, 1080)][index];
                        self.fields[1] = w.to_string();
                        self.fields[2] = h.to_string();
                        self.fields[3] = "30".into();
                    }
                } else if index == 4 {
                    motion_ui::style::box_style(
                        list,
                        rect,
                        sheet.resolve("field", Default::default()).unwrap(),
                    );
                    ui::text(
                        list,
                        atlas,
                        rect.shrink(4.),
                        if zh { "方形像素" } else { "Square pixels" },
                        colors.TEXT,
                        Align::Left,
                    );
                } else if index == 6 {
                    ui::text(
                        list,
                        atlas,
                        rect.shrink(4.),
                        "0:00:00:00",
                        colors.TEXT_MUTED,
                        Align::Left,
                    );
                } else {
                    paint::rect(
                        list,
                        Rect::new(field_x, y, 30., 20.),
                        motion_ui::Color::rgb(0, 0, 0),
                    );
                    ui::text(
                        list,
                        atlas,
                        Rect::new(field_x + 40., y, 180., 24.),
                        if zh { "黑色" } else { "Black" },
                        colors.TEXT_MUTED,
                        Align::Left,
                    );
                }
                y += 36.;
            }
            if !dropdown_was_open && !self.preset_dropdown.open && !self.timing_dropdown.open {
                self.edit_text(input);
                if self.aspect_locked && matches!(self.focus, 1 | 2) {
                    if let Ok(value) = self.fields[self.focus].parse::<f64>() {
                        if value.is_finite() && self.aspect.is_finite() && self.aspect > 0. {
                            let other = if self.focus == 1 {
                                (value / self.aspect).round()
                            } else {
                                (value * self.aspect).round()
                            };
                            self.fields[if self.focus == 1 { 2 } else { 1 }] =
                                format!("{other:.0}");
                        }
                    }
                }
            }
        } else {
            let text = if self.settings_tab == 1 {
                if zh {
                    "帧率变更采用“基本”页选择的时间策略。关键帧冲突或内容超出缩短后的范围时拒绝修改。"
                } else {
                    "Frame rate changes use the timing policy selected in Basic. Keyframe collisions or content outside a shortened composition are rejected."
                }
            } else {
                if zh {
                    "摄影机与三维图层在合成工作区的图层和属性面板中编辑。"
                } else {
                    "Edit cameras and 3D layers in the composition's layer and property panels."
                }
            };
            motion_ui::style::text_style(
                list,
                atlas,
                Rect::new(card.min[0] + 20., y, 600., 48.),
                text,
                sheet.resolve("label.muted", Default::default()).unwrap(),
                Align::Left,
            );
        }
        let footer = card.max[1] - 52.;
        ui::text(
            list,
            atlas,
            Rect::new(card.min[0] + 20., footer, 380., 32.),
            &format!(
                "{} × {} · {} {}",
                self.fields[1],
                self.fields[2],
                self.fields[4],
                if zh { "帧" } else { "frames" }
            ),
            colors.TEXT_MUTED,
            Align::Left,
        );
        if let Some(error) = &self.error {
            ui::text(
                list,
                atlas,
                Rect::new(card.min[0] + 20., footer - 24., 600., 22.),
                error,
                sheet.color("error"),
                Align::Left,
            );
        }
        if ui::button(
            list,
            atlas,
            Rect::new(card.max[0] - 208., footer, 80., 30.),
            if zh { "取消" } else { "Cancel" },
            input,
            true,
        )
        .clicked
            && !dropdown_was_open
        {
            return Some(Event::Cancelled);
        }
        let apply = ui::button_styled(
            list,
            atlas,
            Rect::new(card.max[0] - 120., footer, 100., 30.),
            if matches!(self.kind, Kind::NewComposition) {
                if zh {
                    "创建合成"
                } else {
                    "Create"
                }
            } else {
                if zh {
                    "应用"
                } else {
                    "Apply"
                }
            },
            input,
            true,
            &sheet,
            "button.primary",
        )
        .clicked
            && !dropdown_was_open
            || (!dropdown_was_open && !self.preset_dropdown.open && !self.timing_dropdown.open
                && !input.composing
                && !input.ime_consumed
                && input.key_presses().any(|(k, _, _)| k == Key::Enter));
        list.append(overlays);
        if apply {
            let result = (|| -> Result<Value, String> {
                let width = self.fields[1]
                    .parse::<u32>()
                    .map_err(|_| word(locale, "宽度无效", "Invalid width"))?;
                let height = self.fields[2]
                    .parse::<u32>()
                    .map_err(|_| word(locale, "高度无效", "Invalid height"))?;
                let fps = self.fields[3]
                    .parse::<motion_core::FrameRate>()
                    .map_err(|_| word(locale, "帧率无效", "Invalid frame rate"))?;
                let frames = parse_duration(&self.fields[4], fps)
                    .map_err(|_| word(locale, "持续时间无效", "Invalid duration"))?;
                if self.fields[0].trim().is_empty()
                    || width < 4
                    || height < 4
                    || width > 16384
                    || height > 16384
                    || !fps.is_between(1, 240)
                    || frames == 0
                {
                    return Err(word(
                        locale,
                        "请检查名称、画布尺寸、帧率和持续时间",
                        "Check name, dimensions, frame rate and duration",
                    ));
                }
                Ok(
                    serde_json::json!({"name":self.fields[0],"width":width,"height":height,"fps":fps,"frames":frames,"timing":self.timing_policy(),"shorten":"reject"}),
                )
            })();
            match result {
                Ok(value) => {
                    return Some(if matches!(self.kind, Kind::NewComposition) {
                        Event::CreateComposition(value)
                    } else {
                        Event::Settings(value)
                    })
                }
                Err(error) => self.error = Some(error),
            }
        }
        None
    }
}

fn display_rate(text: &str) -> String {
    match text.parse::<motion_core::FrameRate>() {
        Ok(rate) if rate.ratio()[1] == 1 => format!("{}.00", rate.ratio()[0]),
        Ok(rate) => rate.to_string(),
        Err(_) => text.into(),
    }
}

fn display_duration(text: &str, rate: &str) -> String {
    let formatted = (|| {
        let frame = text.parse::<u32>().ok()?;
        let fps = rate.parse::<motion_core::FrameRate>().ok()?;
        fps.format_timecode(i64::from(frame), motion_core::TimecodeMode::NonDrop).ok()
    })();
    // An incomplete rate or a custom rate without a nominal timecode clock
    // keeps the exact frame count visible instead of borrowing a 30 fps clock.
    formatted.unwrap_or_else(|| text.into())
}

fn parse_duration(text: &str, fps: motion_core::FrameRate) -> Result<u32, ()> {
    if !text.contains(':') && !text.contains(';') {
        return text.parse().map_err(|_| ());
    }
    let mode = if text.contains(';') { motion_core::TimecodeMode::Drop }
        else { motion_core::TimecodeMode::NonDrop };
    u32::try_from(fps.parse_timecode(text, mode).map_err(|_| ())?).map_err(|_| ())
}
#[cfg(test)]
mod duration_tests {
    // Keep the original integer controls verbatim through an explicit typed
    // fixture adapter; this is not a product wire-format compatibility path.
    fn parse_duration(text: &str, fps: u32) -> Result<u32, ()> {
        super::parse_duration(text, motion_core::FrameRate::integer(fps).map_err(|_| ())?)
    }
    #[test]
    fn duration_accepts_frames_and_validated_timecode() {
        assert_eq!(parse_duration("300", 30), Ok(300));
        assert_eq!(parse_duration("0:01:10:15", 30), Ok(2115));
        for input in ["0:00:00:30", "0:60:00:00", "1:2", "999999999:00:00:00"] {
            assert!(parse_duration(input, 30).is_err());
        }
        assert!(parse_duration("1", 0).is_err());
    }
    #[test]
    fn rational_duration_accepts_explicit_drop_and_non_drop_without_decimal_aliases() {
        let fps = motion_core::FrameRate::new(30000, 1001).unwrap();
        assert_eq!(super::parse_duration("00:01:00;02", fps), Ok(1800));
        assert_eq!(super::parse_duration("01:00:00;00", fps), Ok(107892));
        assert_eq!(super::parse_duration("01:00:00:00", fps), Ok(108000));
        assert!(super::parse_duration("00:01:00;00", fps).is_err());
        assert!(super::parse_duration("01:00:00;00", "29.97".parse().unwrap()).is_err());
    }

    #[test]
    fn composition_fields_keep_exact_rates_and_do_not_borrow_a_fallback_clock() {
        assert_eq!(super::display_rate("30"), "30.00");
        assert_eq!(super::display_rate("30000/1001"), "30000/1001");
        assert_eq!(super::display_rate("29.97"), "2997/100");
        assert_eq!(super::display_rate("30000/"), "30000/");
        assert_eq!(super::display_duration("2400", "24000/1001"), "00:01:40:00");
        assert_eq!(super::display_duration("108000", "30000/1001"), "01:00:00:00");
        assert_eq!(super::display_duration("108000", "29.97"), "108000");
        assert_eq!(super::display_duration("300", "30000/"), "300");
        assert_eq!(super::display_duration("00:01:00;02", "30000/1001"), "00:01:00;02");
    }
}
