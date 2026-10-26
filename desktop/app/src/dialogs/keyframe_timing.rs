//! Frozen selection, draft-only fields and one guarded edit on Apply.
use super::*;
use motion_core::timing::{
    self, TimingInterpolation, TimingProfile, TimingSettings, TimingSide, VelocityUnits,
};
use motion_ui::{
    style::{State, States, StyleSheet},
    widgets::{Checkbox, Dropdown, DropdownItem, ScrollArea, TextEditor},
};

const MODES: [TimingInterpolation; 4] = [
    TimingInterpolation::Preserve,
    TimingInterpolation::Linear,
    TimingInterpolation::Hold,
    TimingInterpolation::Bezier,
];
const UNITS: [VelocityUnits; 2] = [
    VelocityUnits::PropertyPerSecond,
    VelocityUnits::AverageMultiple,
];

struct Field {
    text: String,
    original: String,
    editor: TextEditor,
    replace: bool,
}
impl Field {
    fn new(value: Option<f64>) -> Self {
        let text = value.map_or_else(String::new, |v| v.to_string());
        Self {
            original: text.clone(),
            text,
            editor: Default::default(),
            replace: true,
        }
    }
    fn value(&self, influence: bool) -> Result<Option<f64>, ()> {
        if self.text.trim().is_empty() || self.text == self.original {
            return Ok(None);
        }
        let value = self.text.trim().parse::<f64>().map_err(|_| ())?;
        if !value.is_finite() || influence && !(value > 0. && value <= 100.) {
            return Err(());
        }
        Ok(Some(value))
    }
    fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        input: &Input,
        focused: bool,
        enabled: bool,
        influence: bool,
        locale: Locale,
    ) {
        let sheet = list.styles();
        if focused && enabled && !input.focus_lost {
            self.editor
                .input_value(&mut self.text, &mut self.replace, input);
        } else {
            self.editor.cancel();
            self.replace = true;
        }
        let mut states = States::default();
        if !enabled {
            states = states.with(State::Disabled);
        } else if area.contains(input.mouse) {
            states = states.with(State::Hovered);
        }
        if self.value(influence).is_err() {
            states = states.with(State::Invalid);
        }
        if self.editor.active() {
            self.editor
                .draw_styled(list, atlas, area, &sheet, "field.number", states);
        } else if let Some(style) = sheet.resolve("field.number", states) {
            motion_ui::style::box_style(list, area, style);
            let value = self
                .text
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .map(|v| format!("{v:.3}"));
            motion_ui::style::text_style(
                list,
                atlas,
                area.shrink(style.padding),
                value.as_deref().unwrap_or(if self.text.is_empty() {
                    if locale == Locale::Zh {
                        "保持各自值"
                    } else {
                        "Keep each value"
                    }
                } else {
                    &self.text
                }),
                style,
                Align::Right,
            );
        }
    }
}

pub(crate) struct Editor {
    pub(crate) owner: u64,
    source: crate::layer_clipboard::KeyframeSource,
    selections: Vec<motion_core::KeyframeSelection>,
    profiles: [TimingProfile; 2],
    interpolation: usize,
    units: usize,
    sides: [bool; 2],
    speeds: [[Field; 2]; 2],
    influences: [Field; 2],
    matching: bool,
    interpolation_menu: Dropdown,
    units_menu: Dropdown,
    scroll: ScrollArea,
    focus: Option<usize>,
    error: Option<String>,
}

pub(crate) struct Layout {
    pub card: Rect,
    pub body: Rect,
    pub error: Rect,
    pub cancel: Rect,
    pub apply: Rect,
}
impl Layout {
    pub(crate) fn new(sheet: &StyleSheet, size: [f32; 2]) -> Self {
        let pad = sheet.metric("preferences.margin", 16.);
        let row = sheet.metric("row.height", 22.);
        let gap = sheet.metric("auxiliary.gap", 6.);
        let section = sheet.metric("preferences.section_gap", 12.);
        let width = sheet
            .metric("preferences.width", 512.)
            .min((size[0] - pad * 2.).max(0.));
        let stacked = width - pad * 2. < sheet.metric("preferences.sidebar_breakpoint", 440.);
        let content =
            row * if stacked { 12. } else { 9. } + gap + section * if stacked { 4. } else { 3. };
        let height = (content + pad * 2. + row * 5. + section * 2. + gap)
            .min(sheet.metric("preferences.max_height", 492.))
            .min((size[1] - pad * 2.).max(0.));
        let card = Rect::new(
            (size[0] - width) * 0.5,
            (size[1] - height) * 0.5,
            width,
            height,
        );
        let footer = card.max[1] - pad - row;
        let button = sheet
            .metric("preferences.button_width", 96.)
            .min((width - pad * 2. - gap).max(0.) * 0.5);
        let error_y = footer - section - row * 2.;
        let body_y = card.min[1] + pad + row * 2. + section;
        Self {
            card,
            body: Rect::new(
                card.min[0] + pad,
                body_y,
                (width - pad * 2.).max(0.),
                (error_y - gap - body_y).max(0.),
            ),
            error: Rect::new(
                card.min[0] + pad,
                error_y,
                (width - pad * 2.).max(0.),
                row * 2.,
            ),
            cancel: Rect::new(card.max[0] - pad - button * 2. - gap, footer, button, row),
            apply: Rect::new(card.max[0] - pad - button, footer, button, row),
        }
    }
}

impl Editor {
    fn settings(&self, locale: Locale) -> Result<TimingSettings, String> {
        let cubic = matches!(
            MODES[self.interpolation],
            TimingInterpolation::Preserve | TimingInterpolation::Bezier
        );
        let mut sides = [None; 2];
        for i in 0..2 {
            if !self.sides[i] {
                continue;
            }
            let side = if cubic {
                TimingSide {
                    speed: self.speeds[self.units][i].value(false).map_err(|_| {
                        word(
                            locale,
                            "速度必须是有限数值",
                            "Speed must be a finite number.",
                        )
                    })?,
                    influence: self.influences[i].value(true).map_err(|_| {
                        word(
                            locale,
                            "影响必须大于 0 且不超过 100%",
                            "Influence must be greater than 0 and at most 100%.",
                        )
                    })?,
                }
            } else {
                TimingSide::default()
            };
            sides[i] = Some(side);
        }
        let settings = TimingSettings {
            interpolation: MODES[self.interpolation],
            units: UNITS[self.units],
            incoming: sides[0],
            outgoing: sides[1],
            match_speed: cubic && self.matching,
        };
        if self.units == 0
            && self.profiles[0].vector
            && sides
                .iter()
                .flatten()
                .any(|s| s.speed.is_some_and(|v| v < 0.))
        {
            return Err(word(
                locale,
                "向量速率不能为负；反向进度请使用平均速度倍数",
                "Vector speed must be nonnegative; use average multiples to reverse progress.",
            ));
        }
        if settings.match_speed
            && (sides[0].and_then(|s| s.speed).is_none()
                || sides[1].and_then(|s| s.speed).is_none())
        {
            return Err(word(
                locale,
                "请填写入侧与出侧的相同速度",
                "Enter the same explicit speed for both sides.",
            ));
        }
        let profile = &self.profiles[self.units];
        let unsupported = [(sides[0], &profile.incoming), (sides[1], &profile.outgoing)]
            .iter()
            .any(|(side, profile)| {
                side.is_some_and(|s| s.speed.is_some() || s.influence.is_some())
                    && !profile.progress_editable
            });
        if unsupported && settings.interpolation == TimingInterpolation::Preserve {
            return Err(word(
                locale,
                "请明确选择贝塞尔后再替换保持或速度空间曲线",
                "Choose Bezier explicitly to replace hold or velocity-space curves.",
            ));
        }
        settings.validate().map_err(|_| {
            word(
                locale,
                "请选择一侧，并修改插值、速度或影响",
                "Choose a side and change interpolation, speed or influence.",
            )
        })?;
        Ok(settings)
    }

    pub(super) fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        size: [f32; 2],
        input: &Input,
        locale: Locale,
        external_error: Option<&str>,
    ) -> Option<Event> {
        let sheet = list.styles();
        let layout = Layout::new(&sheet, size);
        let pad = sheet.metric("preferences.margin", 16.);
        let row = sheet.metric("row.height", 22.);
        let gap = sheet.metric("auxiliary.gap", 6.);
        let section = sheet.metric("preferences.section_gap", 12.);
        let was_open = self.interpolation_menu.open || self.units_menu.open;
        let key = |wanted| {
            !input.focus_lost
                && !input.composing
                && !input.ime_consumed
                && input
                    .key_presses()
                    .any(|(k, m, repeat)| k == wanted && !m.control && !m.alt && !repeat)
        };
        if key(Key::Escape) && !was_open {
            return Some(Event::Cancelled);
        }
        if input.focus_lost {
            self.focus = None;
            self.interpolation_menu.open = false;
            self.units_menu.open = false;
        }
        let neutral = Input::default();
        let controls = if was_open || input.focus_lost {
            &neutral
        } else {
            input
        };
        if key(Key::Tab) && !was_open {
            let shift = input
                .key_presses()
                .any(|(k, m, _)| k == Key::Tab && m.shift);
            self.focus = Some(match (self.focus, shift) {
                (Some(i), true) => (i + 10) % 11,
                (Some(i), false) => (i + 1) % 11,
                (None, _) => 0,
            });
        }
        if controls.pressed.is_some() {
            self.focus = None;
            self.error = None;
        }
        let focus_before_fields = self.focus;
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
                "keyframe_timing.popup",
                format_args!("style=popup fallback=panel"),
            );
            paint::rect(list, layout.card, sheet.color("panel"));
        }
        ui::text(
            list,
            atlas,
            Rect::new(
                layout.card.min[0] + pad,
                layout.card.min[1] + pad,
                layout.body.width(),
                row,
            ),
            &word(
                locale,
                "关键帧插值与速度",
                "Keyframe interpolation and velocity",
            ),
            sheet.color("text"),
            Align::Left,
        );
        let p = &self.profiles[self.units];
        let unit_label = p.units.iter().cloned().collect::<Vec<_>>().join(" / ");
        let summary = if locale == Locale::Zh {
            format!("{} 个属性 · {} 个关键帧 · {unit_label}", p.channels, p.keys)
        } else {
            format!("{} properties · {} keys · {unit_label}", p.channels, p.keys)
        };
        ui::text(
            list,
            atlas,
            Rect::new(
                layout.card.min[0] + pad,
                layout.card.min[1] + pad + row,
                layout.body.width(),
                row,
            ),
            &summary,
            sheet.color("muted"),
            Align::Left,
        );
        let stacked = layout.body.width() < sheet.metric("preferences.sidebar_breakpoint", 440.);
        let height =
            row * if stacked { 12. } else { 9. } + gap + section * if stacked { 4. } else { 3. };
        if key(Key::Tab) && !was_open {
            let side_y = row * 2. + gap + section;
            let target = self.focus.and_then(|id| {
                Some(match id {
                    0 => 0.,
                    1 => row + gap,
                    2..=7 => {
                        let outgoing = id >= 5;
                        let index = if outgoing { id - 5 } else { id - 2 };
                        side_y
                            + row * index as f32
                            + if outgoing && stacked {
                                row * 3. + section
                            } else {
                                0.
                            }
                    }
                    8 => {
                        side_y
                            + row * if stacked { 6. } else { 3. }
                            + section * if stacked { 2. } else { 1. }
                    }
                    _ => return None,
                })
            });
            if let Some(y) = target {
                if y < self.scroll.offset {
                    self.scroll.offset = y;
                } else if y + row > self.scroll.offset + layout.body.height() {
                    self.scroll.offset = (y + row - layout.body.height()).max(0.);
                }
            }
        }
        let scrolling = self.scroll.begin_view(
            list,
            layout.body,
            height,
            controls,
            !matches!(self.focus, Some(3 | 4 | 6 | 7)),
        );
        let input_form = scrolling.input.to_owned();
        let mut body = list.child();
        let area = scrolling.content;
        let label = sheet
            .metric("preferences.label_width", 180.)
            .min(area.width() * 0.38);
        let field = |y| {
            Rect::new(
                area.min[0] + label + gap,
                y,
                (area.width() - label - gap).max(0.),
                row,
            )
        };
        let mut y = area.min[1];
        let modes = if locale == Locale::Zh {
            ["保留现有插值", "线性", "保持", "贝塞尔（进度空间）"]
        } else {
            ["Keep existing", "Linear", "Hold", "Bezier (progress space)"]
        };
        let mode_items = modes.map(|label| DropdownItem {
            label,
            enabled: self.profiles[0].temporal || label == modes[2],
        });
        ui::text(
            &mut body,
            atlas,
            Rect::new(area.min[0], y, label, row),
            &word(locale, "插值", "Interpolation"),
            sheet.color("muted"),
            Align::Left,
        );
        let mode_rect = field(y);
        if input_form
            .pressed
            .is_some_and(|(_, p)| mode_rect.contains(p))
        {
            self.focus = Some(0);
        }
        let mut overlay = list.child();
        let dropdown_input = if input.focus_lost {
            &neutral
        } else if self.interpolation_menu.open {
            input
        } else {
            &input_form
        };
        if let Some(index) = self.interpolation_menu.draw_grouped(
            &mut overlay,
            atlas,
            mode_rect,
            Rect::new(0., 0., size[0], size[1]),
            &mode_items,
            self.interpolation,
            dropdown_input,
            !self.units_menu.open,
            self.focus == Some(0),
            &[],
            true,
        ) {
            self.interpolation = index;
            self.error = None;
        }
        y += row + gap;
        ui::text(
            &mut body,
            atlas,
            Rect::new(area.min[0], y, label, row),
            &word(locale, "速度单位", "Velocity units"),
            sheet.color("muted"),
            Align::Left,
        );
        let unit_rect = field(y);
        if input_form
            .pressed
            .is_some_and(|(_, p)| unit_rect.contains(p))
        {
            self.focus = Some(1);
        }
        let unit_labels = if locale == Locale::Zh {
            ["属性单位／秒（默认）", "区间平均速度倍数"]
        } else {
            [
                "Property units / second (default)",
                "Multiple of interval average",
            ]
        };
        let unit_items = unit_labels.map(|label| DropdownItem {
            label,
            enabled: true,
        });
        let dropdown_input = if input.focus_lost {
            &neutral
        } else if self.units_menu.open {
            input
        } else {
            &input_form
        };
        if let Some(index) = self.units_menu.draw_grouped(
            &mut overlay,
            atlas,
            unit_rect,
            Rect::new(0., 0., size[0], size[1]),
            &unit_items,
            self.units,
            dropdown_input,
            !self.interpolation_menu.open,
            self.focus == Some(1),
            &[],
            true,
        ) {
            self.units = index;
            self.matching = false;
            self.error = None;
        }
        y += row + section;
        let profile = &self.profiles[self.units];
        let cubic = matches!(
            MODES[self.interpolation],
            TimingInterpolation::Preserve | TimingInterpolation::Bezier
        );
        let speed_enabled = cubic && profile.temporal && (profile.numeric || self.units == 1);
        let column = (area.width() - gap) * 0.5;
        if !stacked {
            let edge = sheet.metric("tree.separator.width", 0.5);
            paint::rect(
                &mut body,
                Rect::new(
                    area.min[0] + column + gap * 0.5 - edge * 0.5,
                    y,
                    edge,
                    row * 3.,
                ),
                sheet.color("border"),
            );
        }
        for i in 0..2 {
            let x = area.min[0] + if !stacked && i == 1 { column + gap } else { 0. };
            let top = y + if stacked && i == 1 {
                row * 3. + section
            } else {
                0.
            };
            let width = if stacked { area.width() } else { column };
            let intervals = if i == 0 {
                profile.incoming.intervals
            } else {
                profile.outgoing.intervals
            };
            let enabled = intervals > 0;
            let header = Rect::new(x, top, width, row);
            let title = if i == 0 {
                word(locale, "入侧", "Incoming")
            } else {
                word(locale, "出侧", "Outgoing")
            };
            let header_focus = if i == 0 { 2 } else { 5 };
            if Checkbox::draw(
                &mut body,
                atlas,
                header,
                &title,
                self.sides[i],
                &input_form,
                enabled,
                self.focus == Some(header_focus),
                None,
            )
            .clicked
            {
                self.sides[i] = !self.sides[i];
                self.focus = Some(header_focus);
                self.matching = false;
            }
            let label_width = (label * 0.5).min(width * 0.45);
            for influence in [false, true] {
                let line = Rect::new(x, top + row * if influence { 2. } else { 1. }, width, row);
                let number = Rect::new(
                    line.min[0] + label_width + gap,
                    line.min[1],
                    (width - label_width - gap).max(0.),
                    row,
                );
                let id = if i == 0 {
                    if influence {
                        4
                    } else {
                        3
                    }
                } else if influence {
                    7
                } else {
                    6
                };
                let enabled = enabled
                    && self.sides[i]
                    && if influence {
                        cubic && profile.temporal
                    } else {
                        speed_enabled
                    };
                if enabled && input_form.pressed.is_some_and(|(_, p)| number.contains(p)) {
                    self.focus = Some(id);
                }
                ui::text(
                    &mut body,
                    atlas,
                    Rect::new(line.min[0], line.min[1], label_width, row),
                    &if influence {
                        word(locale, "影响 %", "Influence %")
                    } else {
                        word(locale, "速度", "Speed")
                    },
                    sheet.color(if enabled { "muted" } else { "disabled" }),
                    Align::Left,
                );
                let f = if influence {
                    &mut self.influences[i]
                } else {
                    &mut self.speeds[self.units][i]
                };
                f.draw(
                    &mut body,
                    atlas,
                    number,
                    &input_form,
                    self.focus == Some(id),
                    enabled,
                    influence,
                    locale,
                );
            }
        }
        if self.matching {
            let source = match self.focus {
                Some(3) => Some(0),
                Some(6) => Some(1),
                _ => None,
            };
            if let Some(i) = source {
                let text = self.speeds[0][i].text.clone();
                self.speeds[0][1 - i].text = text;
            }
        }
        y += row * if stacked { 6. } else { 3. } + section * if stacked { 2. } else { 1. };
        let match_enabled = speed_enabled
            && self.units == 0
            && self.sides.iter().all(|v| *v)
            && profile.incoming.intervals > 0
            && profile.outgoing.intervals > 0;
        if Checkbox::draw(
            &mut body,
            atlas,
            Rect::new(area.min[0], y, area.width(), row),
            &word(
                locale,
                "入侧与出侧使用相同速率（本次应用）",
                "Match incoming and outgoing rates for this edit",
            ),
            self.matching,
            &input_form,
            match_enabled,
            self.focus == Some(8),
            None,
        )
        .clicked
        {
            self.matching = !self.matching;
            self.focus = Some(8);
            if self.matching {
                let value = self.speeds[0][0].text.clone();
                self.speeds[0][1].text = value;
                // Both rates must be explicit, even if one equals the original.
                for f in &mut self.speeds[0] {
                    f.original = String::new();
                }
            }
        }
        if !match_enabled {
            self.matching = false;
        }
        y += row + section;
        for line in if self.units == 0 {
            [
                word(
                    locale,
                    if !profile.temporal {
                        "此类型仅支持保持插值，不提供速度。"
                    } else if !profile.numeric {
                        "此类型不提供属性单位速度；可使用平均倍数。"
                    } else {
                        "标量保留正负；向量速率非负且保留原方向。"
                    },
                    if !profile.temporal {
                        "This type supports Hold only; velocity is unavailable."
                    } else if !profile.numeric {
                        "No property-unit velocity; use average multiples."
                    } else {
                        "Signed scalars; vector speed keeps its original direction."
                    },
                ),
                word(
                    locale,
                    "跨属性输入按各自单位应用；留空保留原值。",
                    "Each property uses its own units. Blank fields keep values.",
                ),
                word(
                    locale,
                    "图表显示模式不变；此处只修改插值。",
                    "Graph display mode stays unchanged; this edits interpolation.",
                ),
            ]
        } else {
            [
                word(
                    locale,
                    "1 倍 = 区间位移 ÷ 时长，不是路径弧长。",
                    "1x means displacement / duration, not path arc length.",
                ),
                word(
                    locale,
                    "切换单位会保留两种单位各自的速度草稿。",
                    "Each unit mode keeps a separate velocity draft.",
                ),
                word(
                    locale,
                    "影响为区间时长百分比；留空保留原值。",
                    "Influence is a percentage of duration. Blank keeps values.",
                ),
            ]
        } {
            ui::text(
                &mut body,
                atlas,
                Rect::new(area.min[0], y, area.width(), row),
                &line,
                sheet.color("muted"),
                Align::Left,
            );
            y += row;
        }
        body.clip_to(scrolling.viewport);
        list.append(body);
        if let Some(error) = self.error.as_deref().or(external_error) {
            ui::text(
                list,
                atlas,
                layout.error,
                error,
                sheet.color("error"),
                Align::Left,
            );
        }
        let footer_input = if was_open || self.interpolation_menu.open || self.units_menu.open {
            &neutral
        } else {
            controls
        };
        let focused = |id| {
            if self.focus == Some(id) {
                States::default().with(State::Focused)
            } else {
                States::default()
            }
        };
        let cancel = ui::button_with_states(
            list,
            atlas,
            layout.cancel,
            &word(locale, "取消", "Cancel"),
            footer_input,
            true,
            &sheet,
            "button",
            focused(9),
        )
        .clicked;
        let apply = ui::button_with_states(
            list,
            atlas,
            layout.apply,
            &word(locale, "应用", "Apply"),
            footer_input,
            true,
            &sheet,
            "button",
            focused(10),
        )
        .clicked;
        list.append(overlay);
        if cancel || self.focus == Some(9) && key(Key::Enter) {
            return Some(Event::Cancelled);
        }
        let editing = matches!(focus_before_fields, Some(3 | 4 | 6 | 7));
        if editing && key(Key::Enter) {
            self.focus = None;
            return None;
        }
        if apply || !was_open && key(Key::Enter) && matches!(self.focus, None | Some(10)) {
            match self.settings(locale) {
                Ok(settings) => {
                    return Some(Event::KeyframeTiming {
                        source: self.source.clone(),
                        selections: self.selections.clone(),
                        settings,
                    });
                }
                Err(error) => self.error = Some(error),
            }
        }
        None
    }
}

impl Dialog {
    pub(crate) fn keyframe_timing(
        state: &Value,
        source: crate::layer_clipboard::KeyframeSource,
        selections: Vec<motion_core::KeyframeSelection>,
        owner: u64,
    ) -> Result<Self, String> {
        if source != crate::layer_clipboard::KeyframeSource::of(state) {
            return Err("keyframe timing selection is stale".into());
        }
        let project = serde_json::from_value(state["project"].clone())
            .map_err(|error| format!("keyframe timing snapshot is invalid: {error}"))?;
        let physical =
            timing::profile(&project, &selections, UNITS[0]).map_err(|e| e.to_string())?;
        let relative =
            timing::profile(&project, &selections, UNITS[1]).map_err(|e| e.to_string())?;
        let sides = [
            physical.incoming.intervals > 0,
            physical.outgoing.intervals > 0,
        ];
        if !sides.iter().any(|v| *v) {
            return Err("selected keyframes have no incident interval".into());
        }
        let interpolation = if physical.temporal { 0 } else { 2 };
        let speeds = [
            [&physical.incoming, &physical.outgoing].map(|p| Field::new(p.speed)),
            [&relative.incoming, &relative.outgoing].map(|p| Field::new(p.speed)),
        ];
        let influences = [&physical.incoming, &physical.outgoing].map(|p| Field::new(p.influence));
        let mut dialog = Self::settings(state);
        dialog.keyframe_timing = Some(Editor {
            owner,
            source,
            selections,
            profiles: [physical, relative],
            interpolation,
            units: 0,
            sides,
            speeds,
            influences,
            matching: false,
            interpolation_menu: Default::default(),
            units_menu: Default::default(),
            scroll: Default::default(),
            focus: None,
            error: None,
        });
        Ok(dialog)
    }
    pub(crate) fn timing_owner(&self) -> Option<u64> {
        self.keyframe_timing.as_ref().map(|e| e.owner)
    }
}

#[cfg(test)]
mod tests;
