//! Opt-in multiline drafts share the field editor's bounded history, native
//! clipboard sessions and grapheme boundaries. Existing fields stay single-line.
use super::*;
use crate::{input::MouseButton, InputSource};
use std::ops::Range;

#[derive(Default)]
pub struct TextViewport {
    pub scroll: [f32; 2],
    dragging: bool,
    last_caret: Option<usize>,
    bar_drag: Option<(f32, f32)>,
}

impl TextEditor {
    pub fn begin_multiline(&mut self, value: &str, max_bytes: usize) {
        self.begin_at_end(value);
        self.multiline = true;
        self.max_bytes = max_bytes.min(crate::text_clipboard::MAX_BYTES);
    }
    pub fn selection_bytes(&self) -> Option<Range<usize>> {
        if self.select_all {
            return (!self.text.is_empty()).then_some(0..self.text.len());
        }
        let anchor = self.anchor?;
        if anchor == self.cursor {
            return None;
        }
        Some(self.byte_at(anchor.min(self.cursor))..self.byte_at(anchor.max(self.cursor)))
    }
    pub fn selected_text(&self) -> Option<&str> {
        self.selection_bytes().map(|r| &self.text[r])
    }
    fn byte_at(&self, index: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .nth(index)
            .map_or(self.text.len(), |(byte, _)| byte)
    }
    pub fn select_bytes(&mut self, start: usize, end: usize) {
        self.place_after_byte(start.min(self.text.len()));
        self.anchor = Some(self.cursor);
        self.place_after_byte(end.min(self.text.len()));
        self.select_all = false;
        self.preferred_column = None;
    }
    pub(super) fn remove_selection(&mut self) -> bool {
        let Some(range) = self.selection_bytes() else {
            return false;
        };
        let start = range.start;
        self.text.replace_range(range, "");
        self.place_after_byte(start);
        self.anchor = None;
        self.select_all = false;
        true
    }
    pub(super) fn filtered_text(&self, text: &str) -> String {
        text.chars()
            .filter(|c| !c.is_control() || self.multiline && matches!(c, '\n' | '\t'))
            .collect()
    }
    fn navigate(&mut self, next: usize, extend: bool) {
        if extend {
            if self.select_all {
                self.anchor = Some(0);
            } else if self.anchor.is_none() {
                self.anchor = Some(self.cursor);
            }
        } else {
            self.anchor = None;
        }
        self.cursor = next.min(self.text.graphemes(true).count());
        self.select_all = false;
    }
    pub fn caret_line_column(&self) -> (usize, usize) {
        self.line_column_at_byte(self.byte_index())
            .expect("grapheme caret is a UTF-8 boundary")
    }
    pub fn line_column_at_byte(&self, byte: usize) -> Option<(usize, usize)> {
        let prefix = self.text.get(..byte)?;
        Some((
            prefix.bytes().filter(|b| *b == b'\n').count() + 1,
            prefix
                .rsplit('\n')
                .next()
                .unwrap_or("")
                .graphemes(true)
                .count()
                + 1,
        ))
    }
    fn line_bounds(&self) -> (usize, usize) {
        let at = self.byte_index();
        (
            self.text[..at].rfind('\n').map_or(0, |p| p + 1),
            self.text[at..]
                .find('\n')
                .map_or(self.text.len(), |n| at + n),
        )
    }
    fn vertical(&mut self, down: bool, extend: bool) {
        let (start, end) = self.line_bounds();
        let column = self
            .preferred_column
            .unwrap_or_else(|| self.text[start..self.byte_index()].graphemes(true).count());
        let target = if down && end < self.text.len() {
            let start = end + 1;
            let end = self.text[start..]
                .find('\n')
                .map_or(self.text.len(), |n| start + n);
            start..end
        } else if !down && start > 0 {
            let end = start - 1;
            self.text[..end].rfind('\n').map_or(0, |p| p + 1)..end
        } else {
            return;
        };
        let byte = self.text[target.clone()]
            .grapheme_indices(true)
            .nth(column)
            .map_or(target.end, |(i, _)| target.start + i);
        let next = self.text[..byte].graphemes(true).count();
        self.navigate(next, extend);
        self.preferred_column = Some(column);
    }
    pub(super) fn input_multiline(&mut self, input: &impl InputSource) -> Option<TextEvent> {
        if !self.active {
            return None;
        }
        let input = input.input_view();
        // Focus changes may be caused by a menu or another native window. Keep
        // the draft; an explicit Escape/Cancel decides whether to discard it.
        if input.key_presses().any(|(k, _, r)| k == Key::Escape && !r) {
            self.cancel();
            return Some(TextEvent::Cancel);
        }
        let mut before = self.snapshot();
        for (key, mods, repeat) in input.key_presses() {
            if self.draft_key(&input, key, mods, repeat, &mut before) {
                continue;
            }
            match key {
                Key::A if mods.control => {
                    self.select_all = true;
                    self.anchor = None;
                    self.cursor = self.text.graphemes(true).count();
                }
                Key::Left | Key::Right => {
                    let next = if let Some(r) = self.selection_bytes().filter(|_| !mods.shift) {
                        self.text[..if key == Key::Left { r.start } else { r.end }]
                            .graphemes(true)
                            .count()
                    } else if key == Key::Left {
                        self.cursor.saturating_sub(1)
                    } else {
                        self.cursor + 1
                    };
                    self.navigate(next, mods.shift);
                    self.preferred_column = None;
                }
                Key::Up | Key::Down => self.vertical(key == Key::Down, mods.shift),
                Key::Home | Key::End => {
                    let byte = if mods.control {
                        if key == Key::Home {
                            0
                        } else {
                            self.text.len()
                        }
                    } else {
                        let (s, e) = self.line_bounds();
                        if key == Key::Home {
                            s
                        } else {
                            e
                        }
                    };
                    self.navigate(self.text[..byte].graphemes(true).count(), mods.shift);
                    self.preferred_column = None;
                }
                Key::Backspace | Key::Delete => {
                    if !self.remove_selection() {
                        let (start, end) = if key == Key::Backspace {
                            (self.cursor.saturating_sub(1), self.cursor)
                        } else {
                            (self.cursor, self.cursor + 1)
                        };
                        let (start, end) = (self.byte_at(start), self.byte_at(end));
                        self.text.replace_range(start..end, "");
                        self.place_after_byte(start);
                    }
                    self.error = None;
                    self.preferred_column = None;
                }
                Key::Enter if mods.control => {
                    self.checkpoint(&mut before);
                    if !repeat {
                        return Some(TextEvent::Commit(self.text.clone()));
                    }
                }
                Key::Enter | Key::Tab if !mods.control && !mods.alt => {
                    self.remove_selection();
                    let text = if key == Key::Enter { "\n" } else { "\t" };
                    let at = self.byte_index();
                    self.text.insert_str(at, text);
                    self.place_after_byte(at + text.len());
                    self.error = None;
                    self.preferred_column = None;
                }
                _ => (),
            }
        }
        let text = self.filtered_text(input.text);
        if !text.is_empty() {
            self.remove_selection();
            let at = self.byte_index();
            self.text.insert_str(at, &text);
            self.place_after_byte(at + text.len());
            self.error = None;
            self.preferred_column = None;
        }
        self.checkpoint(&mut before);
        None
    }

    /// Draw and edit a multiline draft with a line gutter, pointer selection and
    /// bounded scrolling. This does not commit to any application model.
    pub fn multiline_view(
        &mut self,
        viewport: &mut TextViewport,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        input: &impl InputSource,
    ) -> Option<TextEvent> {
        let sheet = list.styles();
        let states = crate::style::States::default().with(crate::style::State::Focused);
        let style = sheet
            .resolve("field", states)
            .expect("validated stylesheet requires field");
        crate::style::box_style(list, area, style);
        let pad = style.padding;
        let line_height = sheet
            .metric("text.editor.line.height", 22.)
            .max(f32::EPSILON);
        let gutter = sheet
            .metric("text.editor.gutter.width", 44.)
            .min(area.width());
        let bar_width = sheet.metric("text.editor.scrollbar.width", 8.);
        let body = Rect::new(
            area.min[0] + gutter,
            area.min[1] + pad,
            (area.width() - gutter - pad - bar_width).max(0.),
            (area.height() - pad - pad).max(0.),
        );
        let input = input.input_view();
        if input.focus_lost {
            viewport.dragging = false;
            viewport.bar_drag = None;
        }
        let rows = self.text.bytes().filter(|b| *b == b'\n').count() + 1;
        let max_scroll = (rows as f32 * line_height - body.height()).max(0.);
        let thumb_height = (body.height() * body.height() / (rows as f32 * line_height))
            .max(sheet.metric("text.editor.scrollbar.thumb.min_height", 16.))
            .min(body.height());
        let travel = (body.height() - thumb_height).max(0.);
        if input.wheel != [0., 0.] && area.contains(input.mouse) {
            if input.modifiers.shift {
                viewport.scroll[0] += input.wheel[1] + input.wheel[0];
            } else {
                viewport.scroll[1] += input.wheel[1];
                viewport.scroll[0] += input.wheel[0];
            }
        }
        let track = Rect::new(body.max[0], body.min[1], bar_width, body.height());
        if max_scroll > 0. {
            if let Some((MouseButton::Left, p)) = input.pressed.filter(|(_, p)| track.contains(*p))
            {
                let thumb_y = body.min[1] + travel * viewport.scroll[1] / max_scroll;
                if p[1] >= thumb_y && p[1] <= thumb_y + thumb_height {
                    viewport.bar_drag = Some((p[1], viewport.scroll[1]));
                } else {
                    viewport.scroll[1] = ((p[1] - body.min[1] - thumb_height * 0.5) / travel
                        * max_scroll)
                        .clamp(0., max_scroll);
                }
            }
            if let Some((start, offset)) = viewport
                .bar_drag
                .filter(|_| input.held == Some(MouseButton::Left))
            {
                if travel > 0. {
                    viewport.scroll[1] = offset + (input.mouse[1] - start) * max_scroll / travel;
                }
            }
        }
        if input.held != Some(MouseButton::Left) {
            viewport.bar_drag = None;
        }
        viewport.scroll[1] = viewport.scroll[1].clamp(0., max_scroll);
        let tabs = sheet
            .metric("text.editor.tab.columns", 4.)
            .round()
            .clamp(1., 16.) as usize;
        let measure = |atlas: &mut TextAtlas, text: &str| {
            atlas.measure(&expand_tabs(text, tabs)) * style.font_size / atlas.font_size()
        };
        let pointer_byte =
            |editor: &TextEditor, point: [f32; 2], view: &TextViewport, atlas: &mut TextAtlas| {
                let row = ((point[1] - body.min[1] + view.scroll[1]) / line_height)
                    .floor()
                    .max(0.) as usize;
                let mut start = 0;
                for (index, line) in editor.text.split('\n').enumerate() {
                    if index == row {
                        let x = (point[0] - body.min[0] + view.scroll[0]).max(0.);
                        let mut previous = 0.;
                        for (at, cluster) in line.grapheme_indices(true) {
                            let end = at + cluster.len();
                            let next = measure(atlas, &line[..end]);
                            if x < (previous + next) * 0.5 {
                                return start + at;
                            }
                            previous = next;
                        }
                        return start + line.len();
                    }
                    start += line.len() + 1;
                }
                editor.text.len()
            };
        if let Some((MouseButton::Left, p)) = input.pressed.filter(|(_, p)| body.contains(*p)) {
            let byte = pointer_byte(self, p, viewport, atlas);
            let next = self.text[..byte].graphemes(true).count();
            self.navigate(next, input.modifiers.shift);
            self.preferred_column = None;
            viewport.dragging = true;
        } else if viewport.dragging && input.held == Some(MouseButton::Left) {
            let byte = pointer_byte(self, input.mouse, viewport, atlas);
            self.navigate(self.text[..byte].graphemes(true).count(), true);
        }
        if input.held != Some(MouseButton::Left) || input.released.is_some() {
            viewport.dragging = false;
        }
        let event = self.input(&input);
        let (line, _) = self.caret_line_column();
        let caret_byte = self.byte_index();
        let line_text = self.text[..caret_byte].rsplit('\n').next().unwrap_or("");
        let caret_x = measure(atlas, line_text);
        if viewport.last_caret != Some(caret_byte) {
            let y = (line - 1) as f32 * line_height;
            if y < viewport.scroll[1] {
                viewport.scroll[1] = y;
            }
            if y + line_height > viewport.scroll[1] + body.height() {
                viewport.scroll[1] = (y + line_height - body.height()).max(0.);
            }
            if caret_x < viewport.scroll[0] {
                viewport.scroll[0] = caret_x;
            }
            if caret_x + pad > viewport.scroll[0] + body.width() {
                viewport.scroll[0] = (caret_x + pad - body.width()).max(0.);
            }
            viewport.last_caret = Some(caret_byte);
        }
        let max_x = self
            .text
            .split('\n')
            .map(|s| measure(atlas, s))
            .fold(0., f32::max);
        viewport.scroll[0] = viewport.scroll[0].clamp(0., (max_x + pad - body.width()).max(0.));
        let current_rows = self.text.bytes().filter(|b| *b == b'\n').count() + 1;
        viewport.scroll[1] = viewport.scroll[1].clamp(
            0.,
            (current_rows as f32 * line_height - body.height()).max(0.),
        );
        let selection = self.selection_bytes();
        let first = (viewport.scroll[1] / line_height).floor() as usize;
        let last = first + (body.height() / line_height).ceil() as usize + 1;
        let mut gutter_paint = list.child();
        let mut text_paint = list.child();
        let mut start = 0;
        for (index, text) in self.text.split('\n').enumerate() {
            if index >= first && index <= last {
                let y = body.min[1] + index as f32 * line_height - viewport.scroll[1];
                ui::text_sized(
                    &mut gutter_paint,
                    atlas,
                    Rect::new(
                        area.min[0] + pad,
                        y,
                        (gutter - pad - pad).max(0.),
                        line_height,
                    ),
                    &(index + 1).to_string(),
                    sheet.color("muted"),
                    Align::Right,
                    style.font_size,
                );
                if let Some(range) = &selection {
                    let s = range.start.max(start);
                    let e = range.end.min(start + text.len());
                    if s < e || range.start <= start && range.end > start + text.len() {
                        let x = measure(atlas, &text[..s.saturating_sub(start).min(text.len())]);
                        let end = measure(atlas, &text[..e.saturating_sub(start).min(text.len())]);
                        paint::rect(
                            &mut text_paint,
                            Rect::new(
                                body.min[0] + x - viewport.scroll[0],
                                y,
                                (end - x).max(pad),
                                line_height,
                            ),
                            sheet.color("selected"),
                        );
                    }
                }
                let text_width = measure(atlas, text) + pad;
                ui::text_sized(
                    &mut text_paint,
                    atlas,
                    Rect::new(body.min[0] - viewport.scroll[0], y, text_width, line_height),
                    &expand_tabs(text, tabs),
                    style.foreground,
                    Align::Left,
                    style.font_size,
                );
                if self.active && index + 1 == line {
                    let x = body.min[0] + caret_x - viewport.scroll[0];
                    paint::line(
                        &mut text_paint,
                        vec![[x, y], [x, y + line_height]],
                        sheet.color("focus"),
                    );
                }
            }
            start += text.len() + 1;
        }
        gutter_paint.clip_to(area.shrink(pad));
        text_paint.clip_to(body);
        list.append(gutter_paint);
        list.append(text_paint);
        if max_scroll > 0. && travel > 0. {
            paint::rect(
                list,
                Rect::new(
                    track.min[0],
                    body.min[1] + travel * viewport.scroll[1] / max_scroll,
                    bar_width,
                    thumb_height,
                ),
                sheet.color("border"),
            );
        }
        event
    }
}

fn expand_tabs(text: &str, tab: usize) -> String {
    let mut column = 0;
    let mut output = String::new();
    for cluster in text.graphemes(true) {
        if cluster == "\t" {
            let count = tab - column % tab;
            output.extend(std::iter::repeat_n(' ', count));
            column += count;
        } else {
            output.push_str(cluster);
            column += 1;
        }
    }
    output
}

#[cfg(test)]
mod tests;
