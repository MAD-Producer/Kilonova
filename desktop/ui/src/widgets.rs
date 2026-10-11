//! Stateful edit widgets. They emit transactions; callers decide how values enter an engine.
use crate::{
    input::{Input, Key, MouseButton, Rect},
    paint::{self, PaintList},
    text::TextAtlas,
    theme::palette,
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
        paint::rect(list, area, palette::SUNKEN);
        paint::stroke(
            list,
            area,
            if self.active {
                palette::ACCENT
            } else {
                palette::BORDER
            },
            1.,
        );
        if self.active && self.select_all {
            paint::rect(list, area.shrink(3.), palette::ACCENT.with_alpha(0.18));
        }
        ui::text(
            list,
            atlas,
            area.shrink(5.),
            &self.text,
            palette::TEXT,
            Align::Left,
        );
        if self.active && !self.select_all {
            let x = (area.min[0] + 5. + atlas.measure(&self.text[..self.byte_index()]))
                .min(area.max[0] - 4.);
            paint::line(
                list,
                vec![[x, area.min[1] + 6.], [x, area.max[1] - 6.]],
                palette::TEXT,
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
    pub text: TextEditor,
    drag: Option<Drag>,
}
impl NumberEditor {
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
        if input
            .pressed
            .is_some_and(|(b, p)| b == MouseButton::Left && area.contains(p))
        {
            self.drag = Some(Drag {
                mouse: input.mouse[0],
                original: current,
                value: current,
                begun: false,
            });
            self.text
                .begin(&format!("{:.*}", options.decimals.min(8), current));
        }
        if let Some(drag) = &mut self.drag {
            let delta = input.mouse[0] - drag.mouse;
            if !drag.begun && delta.abs() >= 3. && (input.dragging() || input.released.is_some()) {
                drag.begun = true;
                self.text.cancel();
                events.push(NumberEvent::Begin {
                    value: drag.original,
                });
            }
            if drag.begun && (input.dragging() || input.released.is_some()) {
                let fine = if input.modifiers.shift {
                    0.1
                } else if input.modifiers.control {
                    10.
                } else {
                    1.
                };
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
        if self.text.active() {
            self.text.draw(list, atlas, area);
        } else {
            let value = self.drag.as_ref().map_or(current, |d| d.value);
            ui::text(
                list,
                atlas,
                area.shrink(4.),
                &format!("{:.*}", options.decimals.min(8), value),
                palette::ACCENT,
                Align::Left,
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
        let mut response = ui::button(list, atlas, area, label, &visual, enabled);
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
