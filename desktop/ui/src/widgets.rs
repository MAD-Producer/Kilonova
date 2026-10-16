//! Stateful edit widgets. They emit transactions; callers decide how values enter an engine.
pub mod numeric_behavior;
mod property_layout;
pub use property_layout::PropertyColumns;
mod composite;
pub use composite::*;

use crate::{
    input::{Input, Key, MouseButton, Rect},
    paint::{self, PaintList},
    text::TextAtlas,
    ui::{self, Align, Response},
};

#[derive(Debug, PartialEq)]
pub enum TextEvent {
    Commit(String),
    Cancel,
}
#[derive(Default)]
pub struct TextEditor {
    pub text: String,
    pub error: Option<String>,
    active: bool,
    select_all: bool,
    cursor: usize,
    dirty: bool,
}
impl TextEditor {
    pub fn active(&self) -> bool {
        self.active
    }
    pub fn begin(&mut self, value: &str) {
        self.text = value.into();
        self.active = true;
        self.select_all = true;
        self.cursor = self.text.chars().count();
        self.error = None;
        self.dirty = false;
    }
    pub fn cancel(&mut self) {
        self.active = false;
        self.error = None;
    }
    fn byte_index(&self) -> usize {
        self.text
            .char_indices()
            .nth(self.cursor)
            .map_or(self.text.len(), |(i, _)| i)
    }
    pub fn input(&mut self, input: &Input) -> Option<TextEvent> {
        if !self.active {
            return None;
        }
        let before = self.text.clone();
        if input.focus_lost || input.keys.iter().any(|(k, _)| *k == Key::Escape) {
            self.cancel();
            return Some(TextEvent::Cancel);
        }
        for (key, mods) in &input.keys {
            match key {
                Key::A if mods.control => {
                    self.select_all = true;
                    self.cursor = self.text.chars().count();
                }
                Key::Left => {
                    self.cursor = self.cursor.saturating_sub(1);
                    self.select_all = false;
                }
                Key::Right => {
                    self.cursor = (self.cursor + 1).min(self.text.chars().count());
                    self.select_all = false;
                }
                Key::Home => {
                    self.cursor = 0;
                    self.select_all = false;
                }
                Key::End => {
                    self.cursor = self.text.chars().count();
                    self.select_all = false;
                }
                Key::Backspace | Key::Delete => {
                    if self.select_all {
                        self.text.clear();
                        self.cursor = 0;
                        self.select_all = false;
                    } else if *key == Key::Backspace && self.cursor > 0 {
                        let end = self.byte_index();
                        self.cursor -= 1;
                        let start = self.byte_index();
                        self.text.replace_range(start..end, "");
                    } else if *key == Key::Delete && self.cursor < self.text.chars().count() {
                        let start = self.byte_index();
                        let end = self.text[start..]
                            .chars()
                            .next()
                            .map_or(start, |c| start + c.len_utf8());
                        self.text.replace_range(start..end, "");
                    }
                }
                _ => {}
            }
        }
        let text: String = input.text.chars().filter(|c| !c.is_control()).collect();
        if !text.is_empty() {
            if self.select_all {
                self.text.clear();
                self.cursor = 0;
                self.select_all = false;
            }
            let index = self.byte_index();
            self.text.insert_str(index, &text);
            self.cursor += text.chars().count();
            self.error = None;
        }
        self.dirty |= self.text != before;
        if input.keys.iter().any(|(k, _)| *k == Key::Enter) {
            return Some(TextEvent::Commit(self.text.clone()));
        }
        None
    }
    pub fn draw(&self, list: &mut PaintList, atlas: &mut TextAtlas, area: Rect) {
        let styles = list.styles();

        self.draw_styled(
            list,
            atlas,
            area,
            styles.as_ref(),
            "field",
            crate::style::States::default(),
        );
    }
    pub fn draw_styled(
        &self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        sheet: &crate::style::StyleSheet,
        class: &str,
        mut states: crate::style::States,
    ) {
        use crate::style::{box_style, text_style, State, States};
        if self.active {
            states = states.with(State::Focused);
        }
        if self.error.is_some() {
            states = states.with(State::Invalid);
        }
        let style = sheet
            .resolve(class, states)
            .unwrap_or_else(|| sheet.resolve("field", states).expect("field style"));
        box_style(list, area, style);
        if self.active && self.select_all {
            box_style(
                list,
                area.shrink(style.padding),
                sheet
                    .resolve("selection", States::default())
                    .expect("selection style"),
            );
        }
        text_style(
            list,
            atlas,
            area.shrink(style.padding),
            &self.text,
            style,
            Align::Left,
        );
        if self.active && !self.select_all {
            let pad = style
                .padding
                .min(area.width().min(area.height()).max(0.) * 0.5);
            let x = (area.min[0]
                + pad
                + atlas.measure(&self.text[..self.byte_index()]) * style.font_size
                    / atlas.font_size())
            .min(area.max[0] - pad);
            paint::line(
                list,
                vec![[x, area.min[1] + pad], [x, area.max[1] - pad]],
                style.foreground,
            );
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NumberOptions {
    pub speed: f64,
    pub min: f64,
    pub max: f64,
    pub decimals: usize,
}
impl Default for NumberOptions {
    fn default() -> Self {
        Self {
            speed: 1.,
            min: f64::NEG_INFINITY,
            max: f64::INFINITY,
            decimals: 2,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NumberEvent {
    Begin { value: f64 },
    Change { value: f64 },
    Commit { value: f64 },
    Cancel { value: f64 },
}
struct Drag {
    mouse: f32,
    original: f64,
    value: f64,
    begun: bool,
}
#[derive(Default)]
pub struct NumberEditor {
    pub drag_threshold: Option<f32>,
    pub text: TextEditor,
    drag: Option<Drag>,
}
impl NumberEditor {
    pub fn drag_values(&self) -> Option<(f64, f64)> {
        self.drag
            .as_ref()
            .filter(|drag| drag.begun)
            .map(|drag| (drag.original, drag.value))
    }
    pub fn interacting(&self) -> bool {
        self.text.active() || self.drag.is_some()
    }
    pub fn dragging(&self) -> bool {
        self.drag.as_ref().is_some_and(|d| d.begun)
    }
    pub fn display_value(&self, current: f64) -> f64 {
        self.drag.as_ref().map_or(current, |d| d.value)
    }
    pub fn cancel(&mut self) -> Vec<NumberEvent> {
        let events = self
            .drag
            .take()
            .filter(|d| d.begun)
            .map(|d| NumberEvent::Cancel { value: d.original })
            .into_iter()
            .collect();
        self.text.cancel();
        events
    }
    pub fn update(
        &mut self,
        current: f64,
        options: NumberOptions,
        input: &Input,
        area: Rect,
    ) -> Vec<NumberEvent> {
        let mut events = vec![];
        if input.focus_lost || input.keys.iter().any(|(k, _)| *k == Key::Escape) {
            return self.cancel();
        }
        if options.min.is_nan()
            || options.max.is_nan()
            || options.min > options.max
            || !current.is_finite()
        {
            self.text.error = Some("Invalid numeric field configuration".into());
            return events;
        }
        if self.drag.is_none()
            && !input.composing
            && !input.ime_consumed
            && input
                .pressed
                .is_some_and(|(b, p)| b == MouseButton::Left && area.contains(p))
        {
            let original = if self.text.active() && self.text.dirty {
                self.text
                    .text
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite() && *v >= options.min && *v <= options.max)
                    .unwrap_or(current)
            } else {
                current
            };
            self.drag = Some(Drag {
                mouse: input.pressed.unwrap().1[0],
                original,
                value: original,
                begun: false,
            });
        }
        if let Some(drag) = &mut self.drag {
            let delta = input.mouse[0] - drag.mouse;
            if !drag.begun
                && numeric_behavior::started(
                    delta,
                    input.pointer_travel,
                    self.drag_threshold.unwrap_or_else(|| {
                        crate::style::default_stylesheet().metric("motion.drag_threshold", 3.)
                    }),
                )
                && (input.dragging() || input.released.is_some())
            {
                drag.begun = true;
                self.text.cancel();
                events.push(NumberEvent::Begin {
                    value: drag.original,
                });
            }
            if drag.begun && (input.dragging() || input.released.is_some()) {
                let fine = numeric_behavior::step(input);
                let speed = if options.speed.is_finite() {
                    options.speed
                } else {
                    1.
                };
                let next = (drag.original + delta as f64 * speed * fine)
                    .max(options.min)
                    .min(options.max);
                if next.is_finite() && next != drag.value {
                    drag.value = next;
                    events.push(NumberEvent::Change { value: next });
                }
            }
            if input.released.is_some() {
                if drag.begun {
                    events.push(NumberEvent::Commit { value: drag.value });
                } else if input.released.is_some_and(|(button, point)| {
                    button == MouseButton::Left && area.contains(point)
                }) && !self.text.active()
                {
                    self.text
                        .begin(&format!("{:.*}", options.decimals.min(8), current));
                }
                self.drag = None;
            }
        }
        if let Some(TextEvent::Commit(text)) = self.text.input(input) {
            let parsed = if self.text.dirty {
                text.trim().parse::<f64>().ok()
            } else {
                Some(current)
            };
            match parsed.filter(|v| v.is_finite() && *v >= options.min && *v <= options.max) {
                Some(value) => {
                    self.text.cancel();
                    events.push(NumberEvent::Begin { value: current });
                    if value != current {
                        events.push(NumberEvent::Change { value });
                    }
                    events.push(NumberEvent::Commit { value });
                }
                None => {
                    self.text.error = Some("Enter a finite value within the allowed range".into())
                }
            }
        }
        events
    }
    pub fn draw(
        &self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        current: f64,
        options: NumberOptions,
    ) {
        let styles = list.styles();

        self.draw_styled(
            list,
            atlas,
            area,
            current,
            options,
            styles.as_ref(),
            crate::style::States::default(),
        );
    }
    pub fn draw_styled(
        &self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        current: f64,
        options: NumberOptions,
        sheet: &crate::style::StyleSheet,
        states: crate::style::States,
    ) {
        if self.text.active() {
            self.text
                .draw_styled(list, atlas, area, sheet, "field.number", states);
        } else {
            let value = self.drag.as_ref().map_or(current, |d| d.value);
            let style = sheet
                .resolve("field.number", states)
                .expect("numeric field style");
            crate::style::box_style(list, area, style);
            crate::style::text_style(
                list,
                atlas,
                area.shrink(style.padding),
                &format!("{:.*}", options.decimals.min(8), value),
                style,
                Align::Right,
            );
        }
    }
}

#[derive(Default)]
pub struct Button {
    armed: bool,
}
impl Button {
    pub fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        label: &str,
        input: &Input,
        enabled: bool,
    ) -> Response {
        let styles = list.styles();

        self.draw_styled(
            list,
            atlas,
            area,
            label,
            input,
            enabled,
            styles.as_ref(),
            "button",
        )
    }
    pub fn draw_styled(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        label: &str,
        input: &Input,
        enabled: bool,
        sheet: &crate::style::StyleSheet,
        class: &str,
    ) -> Response {
        self.draw_with_states(
            list,
            atlas,
            area,
            label,
            input,
            enabled,
            sheet,
            class,
            crate::style::States::default(),
        )
    }
    pub fn draw_with_states(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        label: &str,
        input: &Input,
        enabled: bool,
        sheet: &crate::style::StyleSheet,
        class: &str,
        states: crate::style::States,
    ) -> Response {
        if input.focus_lost || !enabled || input.keys.iter().any(|(k, _)| *k == Key::Escape) {
            self.armed = false;
        }
        if enabled
            && input
                .pressed
                .is_some_and(|(b, p)| b == MouseButton::Left && area.contains(p))
        {
            self.armed = true;
        }
        let clicked = enabled
            && self.armed
            && input
                .released
                .is_some_and(|(b, p)| b == MouseButton::Left && area.contains(p));
        if input.released.is_some() {
            self.armed = false;
        }
        let mut visual = input.clone();
        visual.pressed = None;
        let mut response = ui::button_with_states(
            list, atlas, area, label, &visual, enabled, sheet, class, states,
        );
        response.clicked = clicked
            || enabled
                && input
                    .keys
                    .iter()
                    .any(|(k, _)| matches!(k, Key::Enter | Key::Space));
        response.active = self.armed;
        response
    }
}

/// Child content and pointer hits share the same clip rectangle.
pub fn clipped_input(input: &Input, clip: Rect) -> Input {
    let mut input = input.clone();
    input.pressed = input.pressed.filter(|(_, p)| clip.contains(*p));
    if !clip.contains(input.mouse) {
        input.wheel = [0.; 2];
    }
    input
}
pub fn append_clipped(list: &mut PaintList, mut child: PaintList, clip: Rect) {
    child.clip_to(clip);
    list.append(child);
}
