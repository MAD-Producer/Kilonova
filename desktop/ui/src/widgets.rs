//! Stateful edit widgets. They emit transactions; callers decide how values enter an engine.
pub mod numeric_behavior;
pub mod menu;
mod property_layout;
mod inspector;
pub use inspector::InspectorColumns;
mod text_history;
mod text_input;
mod text_commands;
mod multiline;
pub use multiline::TextViewport;
pub use text_commands::{TextCommand, TextSession};
pub use property_layout::PropertyColumns;
mod composite;
pub use composite::*;
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    input::{Key, MouseButton, Rect},
    paint::{self, PaintList},
    text::TextAtlas,
    ui::{self, Align, Response},
};

#[derive(Debug, PartialEq)]
pub enum TextEvent {
    Commit(String),
    Cancel,
}
#[derive(Clone, Default)]
pub struct TextEditor {
    pub text: String,
    pub error: Option<String>,
    active: bool,
    select_all: bool,
    /// Extended grapheme index, never a position inside a displayed character.
    cursor: usize,
    anchor: Option<usize>,
    multiline: bool,
    preferred_column: Option<usize>,
    max_bytes: usize,
    dirty: bool,
    history: text_history::History,
    session: text_commands::Session,
}
impl std::fmt::Debug for TextEditor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextEditor")
            .field("active", &self.active)
            .field("contents", &"<private>")
            .finish_non_exhaustive()
    }
}
impl TextEditor {
    pub fn active(&self) -> bool {
        self.active
    }
    /// Byte position at a complete grapheme boundary for existing scrolled fields.
    pub fn caret_byte_index(&self) -> usize {
        self.byte_index()
    }
    pub fn is_all_selected(&self) -> bool {
        self.select_all
    }
    pub fn begin(&mut self, value: &str) {
        self.text = value.into();
        self.active = true;
        self.select_all = true;
        self.cursor = self.text.graphemes(true).count();
        self.anchor = None;
        self.multiline = false;
        self.preferred_column = None;
        self.max_bytes = crate::text_clipboard::MAX_BYTES;
        self.error = None;
        self.dirty = false;
        self.history = Default::default();
        self.session = Default::default();
    }
    /// Apply a domain limit without changing single-line field behavior.
    pub fn begin_bounded(&mut self, value: &str, max_bytes: usize) {
        self.begin(value);
        self.max_bytes = max_bytes.min(crate::text_clipboard::MAX_BYTES);
    }
    pub fn begin_at_end(&mut self, value: &str) {
        self.begin(value);
        self.select_all = false;
    }
    /// Adapt a live string without changing the caller's validation/commit policy.
    /// External replacement or a cancelled editor starts a fresh draft.
    pub fn input_value(
        &mut self,
        value: &mut String,
        replace: &mut bool,
        input: &impl crate::InputSource,
    ) -> Option<TextEvent> {
        if !self.active || self.text != *value {
            if *replace {
                self.begin(value);
            } else {
                self.begin_at_end(value);
            }
        }
        let event = self.input(input);
        if self.active && self.text != *value {
            value.clone_from(&self.text);
            *replace = false;
        }
        event
    }
    pub fn cancel(&mut self) {
        self.active = false;
        self.error = None;
    }
    fn byte_index(&self) -> usize {
        self.text
            .grapheme_indices(true)
            .nth(self.cursor)
            .map_or(self.text.len(), |(i, _)| i)
    }
    fn place_after_byte(&mut self, byte: usize) {
        // Edits can join both neighbors (ZWJ, combining marks, regional pairs).
        // Re-segment the complete text and snap forward out of the joined cluster.
        self.cursor = self.text
            .grapheme_indices(true)
            .take_while(|(start, _)| *start < byte)
            .count();
    }
    pub fn input(&mut self, input: &impl crate::InputSource) -> Option<TextEvent> {
        if self.multiline { return self.input_multiline(input); }
        let view = input.input_view();
        let input = &view;
        if !self.active {
            return None;
        }
        if !input.focus_lost && input.text.is_empty() && input.key_presses().next().is_none() {
            return None;
        }
        let mut before = self.snapshot();
        if input.focus_lost || input.key_presses().any(|(k, _, repeat)| k == Key::Escape && !repeat) {
            self.cancel();
            return Some(TextEvent::Cancel);
        }
        for (key, mods, repeat) in input.key_presses() {
            if self.draft_key(input, key, mods, repeat, &mut before) { continue; }
            match key {
                Key::A if mods.control => {
                    self.select_all = true;
                    self.cursor = self.text.graphemes(true).count();
                }
                Key::Left => {
                    self.cursor = self.cursor.saturating_sub(1);
                    self.select_all = false;
                }
                Key::Right => {
                    self.cursor = (self.cursor + 1).min(self.text.graphemes(true).count());
                    self.select_all = false;
                }
                Key::Home => {
                    self.cursor = 0;
                    self.select_all = false;
                }
                Key::End => {
                    self.cursor = self.text.graphemes(true).count();
                    self.select_all = false;
                }
                Key::Backspace | Key::Delete => {
                    if self.select_all {
                        self.text.clear();
                        self.cursor = 0;
                        self.select_all = false;
                    } else if key == Key::Backspace && self.cursor > 0 {
                        let end = self.byte_index();
                        self.cursor -= 1;
                        let start = self.byte_index();
                        self.text.replace_range(start..end, "");
                        self.place_after_byte(start);
                    } else if key == Key::Delete && self.cursor < self.text.graphemes(true).count() {
                        let start = self.byte_index();
                        let end = self.text[start..]
                            .graphemes(true)
                            .next()
                            .map_or(start, |grapheme| start + grapheme.len());
                        self.text.replace_range(start..end, "");
                        self.place_after_byte(start);
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
            self.place_after_byte(index + text.len());
            self.error = None;
        }
        self.checkpoint(&mut before);
        if input.key_presses().any(|(k, _, repeat)| k == Key::Enter && !repeat) {
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
        input: &impl crate::InputSource,
        area: Rect,
    ) -> Vec<NumberEvent> {
        let view = input.input_view();
        let input = &view;
        let mut events = vec![];
        if input.focus_lost || input.key_presses().any(|(k, _, repeat)| k == Key::Escape && !repeat) {
            return self.cancel();
        }
        if options.min.is_nan()
            || options.max.is_nan()
            || options.min > options.max
            || !current.is_finite()
        {
            let cancelled = self.cancel();
            self.text.error = Some("Invalid numeric field configuration".into());
            return cancelled;
        }
        if self.drag.is_none()
            && !input.composing
            && !input.ime_consumed
            && input.held.is_none_or(|button| button == MouseButton::Left)
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
            let released = input
                .released
                .is_some_and(|(button, _)| button == MouseButton::Left);
            if !drag.begun
                && numeric_behavior::started(
                    delta,
                    input.pointer_travel,
                    self.drag_threshold.unwrap_or_else(|| {
                        crate::style::default_stylesheet().metric("motion.drag_threshold", 3.)
                    }),
                )
                && (input.dragging() || released)
            {
                drag.begun = true;
                self.text.cancel();
                events.push(NumberEvent::Begin {
                    value: drag.original,
                });
            }
            if drag.begun && (input.dragging() || released) {
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
            if released {
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
        input: &impl crate::InputSource,
        enabled: bool,
    ) -> Response {
        let view = input.input_view();
        let input = &view;
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
        input: &impl crate::InputSource,
        enabled: bool,
        sheet: &crate::style::StyleSheet,
        class: &str,
    ) -> Response {
        let view = input.input_view();
        let input = &view;
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
        input: &impl crate::InputSource,
        enabled: bool,
        sheet: &crate::style::StyleSheet,
        class: &str,
        states: crate::style::States,
    ) -> Response {
        let view = input.input_view();
        let input = &view;
        if input.focus_lost || !enabled || input.key_presses().any(|(k, _, repeat)| k == Key::Escape && !repeat) {
            self.armed = false;
        }
        if enabled
            && input.held.is_none_or(|button| button == MouseButton::Left)
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
        if input
            .released
            .is_some_and(|(button, _)| button == MouseButton::Left)
        {
            self.armed = false;
        }
        let mut visual = *input;
        visual.pressed = None;
        let mut response = ui::button_with_states(
            list, atlas, area, label, &visual, enabled, sheet, class, states,
        );
        response.clicked = clicked
            || enabled
                && input.key_presses()
                    .any(|(k, _, repeat)| !repeat && matches!(k, Key::Enter | Key::Space));
        response.active = self.armed;
        response
    }
}

/// Child content and pointer hits share the same clip rectangle.
pub fn clipped_input<'a>(input: &'a impl crate::InputSource, clip: Rect) -> crate::InputView<'a> {
    let mut input = input.input_view();
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
