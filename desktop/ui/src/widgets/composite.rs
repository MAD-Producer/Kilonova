//! Reusable composites. Focus is supplied by the host; widgets never take global keys.
use super::{clipped_input, TextEditor, TextEvent};
use crate::{
    icons::{self, Icon},
    paint,
    style::{box_style, text_style, State, States},
    ui::{Align, Response},
    Input, Key, MouseButton, PaintList, Rect, TextAtlas,
};

fn press(input: &Input, area: Rect) -> bool {
    input
        .pressed
        .is_some_and(|(button, p)| button == MouseButton::Left && area.contains(p))
}
fn key(input: &Input, wanted: Key) -> bool {
    !input.composing
        && !input.ime_consumed
        && input
            .key_presses()
            .any(|(k, m, _)| k == wanted && !m.control && !m.alt)
}
fn states(input: &Input, area: Rect, enabled: bool, focused: bool) -> States {
    let mut s = States::default();
    if focused {
        s = s.with(State::Focused);
    }
    if enabled && area.contains(input.mouse) {
        s = s.with(State::Hovered);
    }
    if !enabled {
        s = s.with(State::Disabled);
    }
    s
}

pub struct Checkbox;
impl Checkbox {
    /// The full label row is a hit target. Icon switches share checkbox state styles.
    pub fn draw(
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        label: &str,
        checked: bool,
        input: &Input,
        enabled: bool,
        focused: bool,
        icon: Option<Icon>,
    ) -> Response {
        Self::draw_with_states(
            list,
            atlas,
            area,
            label,
            checked,
            input,
            enabled,
            focused,
            icon,
            States::default(),
        )
    }
    pub fn draw_with_states(
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        label: &str,
        checked: bool,
        input: &Input,
        enabled: bool,
        focused: bool,
        icon: Option<Icon>,
        extra: States,
    ) -> Response {
        let sheet = list.styles();
        let enabled = enabled && !extra.contains(State::Disabled);
        let focused = focused || extra.contains(State::Focused);
        let mut s = states(input, area, enabled, focused);
        if extra.contains(State::Invalid) {
            s = s.with(State::Invalid);
        }
        if enabled && input.held == Some(MouseButton::Left) && area.contains(input.mouse) {
            s = s.with(State::Pressed);
        }
        if checked {
            s = s.with(State::Selected);
        }
        let style = sheet.resolve("checkbox", s).unwrap();
        let edge = sheet
            .metric("checkbox.size", 14.)
            .min(area.height())
            .min(area.width());
        let box_area = Rect::new(
            area.min[0],
            area.min[1] + (area.height() - edge) * 0.5,
            edge,
            edge,
        );
        box_style(list, box_area, style);
        if let Some(icon) = icon.or(checked.then_some(Icon::Check)) {
            icons::draw(
                list,
                box_area.shrink(2.),
                icon,
                (edge - 4.).max(1.),
                style.foreground,
            );
        }
        if !label.is_empty() {
            let mut text = sheet.resolve("label.small", s).unwrap();
            if !enabled {
                text.foreground = sheet.color("disabled");
            }
            text_style(
                list,
                atlas,
                Rect::new(
                    box_area.max[0] + 6.,
                    area.min[1],
                    (area.max[0] - box_area.max[0] - 6.).max(0.),
                    area.height(),
                ),
                label,
                text,
                Align::Left,
            );
        }
        Response {
            hovered: enabled && area.contains(input.mouse),
            clicked: enabled
                && (press(input, area)
                    || (focused && (key(input, Key::Space) || key(input, Key::Enter)))),
            active: checked,
        }
    }
}

#[derive(Default)]
pub struct SearchField {
    pub editor: TextEditor,
}
#[derive(Default, Debug)]
pub struct SearchResponse {
    pub changed: bool,
    pub submitted: bool,
    pub active: bool,
}
impl SearchField {
    pub fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        query: &mut String,
        placeholder: &str,
        input: &Input,
        enabled: bool,
    ) -> SearchResponse {
        let sheet = list.styles();
        let before = query.clone();
        let icon_edge = area.height().min(20.);
        let icon_area = Rect::new(
            area.min[0] + 3.,
            area.min[1] + (area.height() - icon_edge) * 0.5,
            icon_edge,
            icon_edge,
        );
        let clear = Rect::new(
            (area.max[0] - icon_edge - 3.).max(area.min[0]),
            icon_area.min[1],
            icon_edge,
            icon_edge,
        );
        let text_area = Rect::new(
            icon_area.max[0] + 3.,
            area.min[1],
            (clear.min[0] - icon_area.max[0] - 6.).max(0.),
            area.height(),
        );
        if !enabled {
            self.editor.cancel();
        }
        if enabled && press(input, clear) && !query.is_empty() {
            query.clear();
            self.editor.begin("");
        } else if enabled && press(input, area) && !self.editor.active() {
            self.editor.begin(query);
        }
        if input.pressed.is_some_and(|(_, p)| !area.contains(p)) {
            self.editor.cancel();
        }
        let mut submitted = false;
        if enabled {
            let mut editing_input = input.clone();
            if input.composing || input.ime_consumed {
                editing_input.keys.clear();
            }
            let event = self.editor.input(&editing_input);
            if self.editor.active() {
                *query = self.editor.text.clone();
            }
            if let Some(TextEvent::Commit(text)) = event {
                *query = text;
                submitted = true;
                self.editor.cancel();
            }
        }
        let s = states(input, area, enabled, self.editor.active());
        let style = sheet.resolve("field", s).unwrap();
        box_style(list, area, style);
        icons::draw(list, icon_area, Icon::Search, 14., style.foreground);
        if !query.is_empty() {
            icons::draw(list, clear, Icon::Close, 12., style.foreground);
        }
        if self.editor.active() {
            self.editor
                .draw_styled(list, atlas, text_area, &sheet, "field", s);
        } else {
            text_style(
                list,
                atlas,
                text_area,
                if query.is_empty() { placeholder } else { query },
                sheet
                    .resolve(
                        if query.is_empty() {
                            "label.muted"
                        } else {
                            "label.small"
                        },
                        s,
                    )
                    .unwrap(),
                Align::Left,
            );
        }
        SearchResponse {
            changed: *query != before,
            submitted,
            active: self.editor.active(),
        }
    }
}

#[derive(Default)]
pub struct ScrollArea {
    pub offset: f32,
    dragging: Option<(f32, f32)>,
}
pub struct ScrollFrame {
    pub viewport: Rect,
    pub content: Rect,
    pub input: Input,
}
impl ScrollArea {
    pub fn begin(
        &mut self,
        list: &mut PaintList,
        area: Rect,
        height: f32,
        input: &Input,
        focused: bool,
    ) -> ScrollFrame {
        let sheet = list.styles();
        let max = (height - area.height()).max(0.);
        if area.contains(input.mouse) {
            self.offset += input.wheel[1];
        }
        if focused {
            if key(input, Key::PageDown) {
                self.offset += area.height();
            }
            if key(input, Key::PageUp) {
                self.offset -= area.height();
            }
            if key(input, Key::Home) {
                self.offset = 0.;
            }
            if key(input, Key::End) {
                self.offset = max;
            }
        }
        self.offset = self.offset.clamp(0., max);
        let bar_width = if max > 0. { 8. } else { 0. };
        let viewport = Rect::new(
            area.min[0],
            area.min[1],
            (area.width() - bar_width).max(0.),
            area.height(),
        );
        let thumb_height = if height > 0. {
            (area.height() * area.height() / height)
                .max(16.)
                .min(area.height())
        } else {
            area.height()
        };
        let travel = (area.height() - thumb_height).max(0.);
        let thumb = Rect::new(
            viewport.max[0],
            area.min[1]
                + if max > 0. {
                    travel * self.offset / max
                } else {
                    0.
                },
            bar_width,
            thumb_height,
        );
        if max > 0. && press(input, thumb) {
            self.dragging = Some((input.mouse[1], self.offset));
        }
        if input.focus_lost || key(input, Key::Escape) {
            self.dragging = None;
        }
        if let Some((start, offset)) = self.dragging {
            if travel > 0. {
                self.offset = (offset + (input.mouse[1] - start) * max / travel).clamp(0., max);
            }
            if input.released.is_some() {
                self.dragging = None;
            }
        }
        if max > 0. {
            paint::box_rect(
                list,
                thumb.shrink(2.),
                sheet.color("muted").with_alpha(0.4),
                sheet.color("transparent"),
                0.,
                2.,
            );
        }
        let mut child_input = clipped_input(input, viewport);
        child_input.wheel = [0.; 2];
        if self.dragging.is_some() {
            child_input.pressed = None;
            child_input.released = None;
            child_input.held = None;
        }
        ScrollFrame {
            viewport,
            content: Rect::new(
                viewport.min[0],
                viewport.min[1] - self.offset,
                viewport.width(),
                height,
            ),
            input: child_input,
        }
    }
}

#[derive(Default)]
pub struct Dropdown {
    pub open: bool,
    highlighted: usize,
    scroll: usize,
}
pub struct DropdownItem<'a> {
    pub label: &'a str,
    pub enabled: bool,
}
impl Dropdown {
    /// Returns a committed index. The host must suppress siblings while open.
    pub fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        bounds: Rect,
        items: &[DropdownItem<'_>],
        selected: usize,
        input: &Input,
        enabled: bool,
        focused: bool,
    ) -> Option<usize> {
        self.draw_grouped(
            list,
            atlas,
            area,
            bounds,
            items,
            selected,
            input,
            enabled,
            focused,
            &[],
            false,
        )
    }
    /// Group starts add separators; marked menus show a dot on the current value.
    pub fn draw_grouped(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        bounds: Rect,
        items: &[DropdownItem<'_>],
        selected: usize,
        input: &Input,
        enabled: bool,
        focused: bool,
        group_starts: &[usize],
        marked: bool,
    ) -> Option<usize> {
        self.draw_grouped_styled(
            list,
            atlas,
            area,
            bounds,
            items,
            selected,
            input,
            enabled,
            focused,
            group_starts,
            marked,
            Some("field"),
        )
    }
    /// Inline enum: a passive value stays transparent until it is edited.
    pub fn draw_inline(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        bounds: Rect,
        items: &[DropdownItem<'_>],
        selected: usize,
        input: &Input,
        enabled: bool,
        focused: bool,
    ) -> Option<usize> {
        self.draw_grouped_styled(
            list,
            atlas,
            area,
            bounds,
            items,
            selected,
            input,
            enabled,
            focused,
            &[],
            true,
            Some("field.number"),
        )
    }
    /// Menu for an independently rendered action button. The host opens it.
    pub fn draw_menu(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        anchor: Rect,
        bounds: Rect,
        items: &[DropdownItem<'_>],
        input: &Input,
        enabled: bool,
    ) -> Option<usize> {
        self.draw_grouped_styled(
            list,
            atlas,
            anchor,
            bounds,
            items,
            0,
            input,
            enabled,
            true,
            &[],
            false,
            None,
        )
    }
    fn draw_grouped_styled(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        bounds: Rect,
        items: &[DropdownItem<'_>],
        selected: usize,
        input: &Input,
        enabled: bool,
        focused: bool,
        group_starts: &[usize],
        marked: bool,
        anchor_style: Option<&str>,
    ) -> Option<usize> {
        let enabled = enabled && !items.is_empty();
        self.highlighted = self.highlighted.min(items.len().saturating_sub(1));
        let sheet = list.styles();
        let row = sheet.metric("row.height", 22.).max(1.);
        if !enabled || input.focus_lost || key(input, Key::Escape) {
            self.open = false;
        }
        let was_open = self.open;
        if enabled
            && anchor_style.is_some()
            && (press(input, area)
                || (focused && !self.open && (key(input, Key::Space) || key(input, Key::Enter))))
        {
            self.open = !self.open;
            self.highlighted = selected.min(items.len().saturating_sub(1));
        }
        if let Some(anchor_style) = anchor_style {
            let style = sheet
                .resolve(
                    anchor_style,
                    states(input, area, enabled, focused || self.open),
                )
                .unwrap();
            box_style(list, area, style);
            text_style(
                list,
                atlas,
                Rect::new(
                    area.min[0] + style.padding,
                    area.min[1],
                    (area.width() - 24. - style.padding).max(0.),
                    area.height(),
                ),
                items.get(selected).map_or("", |i| i.label),
                style,
                Align::Left,
            );
            let cx = area.max[0] - 11.;
            let cy = area.min[1] + area.height() * 0.5;
            paint::line(
                list,
                vec![[cx - 3., cy - 1.], [cx, cy + 2.], [cx + 3., cy - 1.]],
                style.foreground,
            );
        }
        if !self.open || items.is_empty() {
            return None;
        }
        let below = (bounds.max[1] - area.max[1]).max(0.);
        let above = (area.min[1] - bounds.min[1]).max(0.);
        let max_rows = if marked { 32 } else { 8 };
        let desired = items.len().min(max_rows) as f32 * row
            + group_starts
                .iter()
                .filter(|i| **i > 0 && **i < items.len().min(max_rows))
                .count() as f32
                * 6.;
        let below_first = below >= desired || below >= above;
        let available = if below_first { below } else { above };
        let mut count = ((available / row).floor() as usize)
            .min(max_rows)
            .min(items.len());
        while count > 0
            && count as f32 * row
                + group_starts
                    .iter()
                    .filter(|i| **i > self.scroll && **i < self.scroll + count)
                    .count() as f32
                    * 6.
                > available
        {
            count -= 1;
        }
        if count == 0 {
            self.open = false;
            return None;
        }
        let navigated = key(input, Key::Down) || key(input, Key::Up);
        if navigated {
            let forward = key(input, Key::Down);
            for _ in 0..items.len() {
                self.highlighted = if forward {
                    (self.highlighted + 1) % items.len()
                } else {
                    (self.highlighted + items.len() - 1) % items.len()
                };
                if items[self.highlighted].enabled {
                    break;
                }
            }
        }
        if !was_open || navigated {
            if self.highlighted < self.scroll {
                self.scroll = self.highlighted;
            }
            if self.highlighted >= self.scroll + count {
                self.scroll = self.highlighted + 1 - count;
            }
        }
        self.scroll = self.scroll.min(items.len() - count);
        loop {
            let groups = group_starts
                .iter()
                .filter(|i| **i > self.scroll && **i < self.scroll + count)
                .count();
            if count as f32 * row + groups as f32 * 6. <= available {
                break;
            }
            count -= 1;
            if count == 0 {
                self.open = false;
                return None;
            }
            if navigated && self.highlighted >= self.scroll + count {
                self.scroll = self.highlighted + 1 - count;
            }
            self.scroll = self.scroll.min(items.len() - count);
        }
        let width = items
            .iter()
            .map(|item| atlas.measure(item.label) + if marked { 34. } else { 16. })
            .fold(
                area.width().max(if marked {
                    sheet.metric("dropdown.menu_min_width", 160.)
                } else {
                    0.
                }),
                f32::max,
            )
            .min(sheet.metric("dropdown.max_width", 320.))
            .min(bounds.width());
        let height = count as f32 * row
            + group_starts
                .iter()
                .filter(|i| **i > self.scroll && **i < self.scroll + count)
                .count() as f32
                * 6.;
        let popup = Rect::new(
            area.min[0].clamp(bounds.min[0], (bounds.max[0] - width).max(bounds.min[0])),
            if below_first {
                area.max[1]
            } else {
                area.min[1] - height
            },
            width,
            height,
        );
        if popup.contains(input.mouse) {
            if input.wheel[1] > 0. {
                self.scroll = self.scroll.saturating_add(1);
            } else if input.wheel[1] < 0. {
                self.scroll = self.scroll.saturating_sub(1);
            }
        }
        self.scroll = self.scroll.min(items.len() - count);
        if input
            .pressed
            .is_some_and(|(_, p)| !area.contains(p) && !popup.contains(p))
        {
            self.open = false;
            return None;
        }
        let mut overlay = list.child();
        box_style(
            &mut overlay,
            popup,
            sheet.resolve("popup", States::default()).unwrap(),
        );
        let mut chosen = None;
        let mut y = popup.min[1];
        for (index, item) in items.iter().enumerate().skip(self.scroll).take(count) {
            if index > self.scroll && group_starts.contains(&index) {
                paint::rect(
                    &mut overlay,
                    Rect::new(
                        popup.min[0] + 8.,
                        y + 3.,
                        (popup.width() - 16.).max(1.),
                        0.5,
                    ),
                    sheet.color("border").with_alpha(0.7),
                );
                y += 6.;
            }
            let rect = Rect::new(popup.min[0], y, popup.width(), row);
            y += row;
            let mut s = states(input, rect, item.enabled, false);
            if index == self.highlighted {
                s = s.with(State::Selected);
            }
            let style = sheet.resolve("row", s).unwrap();
            box_style(&mut overlay, rect, style);
            if marked && index == selected {
                paint::box_rect(
                    &mut overlay,
                    Rect::new(rect.min[0] + 9., rect.min[1] + row * 0.5 - 1.5, 3., 3.),
                    style.foreground,
                    sheet.color("transparent"),
                    0.,
                    1.5,
                );
            }
            let caption = Rect::new(
                rect.min[0] + if marked { 22. } else { 4. },
                rect.min[1],
                (rect.width() - if marked { 26. } else { 8. }).max(0.),
                rect.height(),
            );
            text_style(&mut overlay, atlas, caption, item.label, style, Align::Left);
            if item.enabled && press(input, rect) {
                chosen = Some(index);
            }
        }
        if was_open
            && key(input, Key::Enter)
            && !press(input, area)
            && items[self.highlighted].enabled
        {
            chosen = Some(self.highlighted);
        }
        if chosen.is_some() {
            self.open = false;
        }
        overlay.clip_to(bounds);
        list.append(overlay);
        chosen
    }
}

pub struct ListItem<'a> {
    pub label: &'a str,
    pub depth: usize,
    pub expanded: Option<bool>,
    pub enabled: bool,
}
#[derive(Default)]
pub struct ListView {
    pub selected: Option<usize>,
    pub scroll: ScrollArea,
}
#[derive(Default)]
pub struct ListResponse {
    pub selected: Option<usize>,
    pub activated: Option<usize>,
    pub toggled: Option<usize>,
}
impl ListView {
    pub fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        items: &[ListItem<'_>],
        input: &Input,
        focused: bool,
    ) -> ListResponse {
        let sheet = list.styles();
        let row = sheet.metric("row.height", 22.).max(1.);
        self.selected = self.selected.filter(|i| *i < items.len());
        let mut response = ListResponse::default();
        if focused && !items.is_empty() && (key(input, Key::Down) || key(input, Key::Up)) {
            let forward = key(input, Key::Down);
            let mut next = if forward {
                self.selected.map_or(0, |i| (i + 1).min(items.len() - 1))
            } else {
                self.selected.unwrap_or(0).saturating_sub(1)
            };
            while !items[next].enabled {
                if forward && next + 1 < items.len() {
                    next += 1;
                } else if !forward && next > 0 {
                    next -= 1;
                } else {
                    break;
                }
            }
            if items[next].enabled {
                self.selected = Some(next);
                response.selected = Some(next);
                self.scroll.offset = self
                    .scroll
                    .offset
                    .min(next as f32 * row)
                    .max(((next + 1) as f32 * row - area.height()).max(0.));
            }
        }
        if focused {
            if let Some(index) = self.selected {
                if items[index].enabled
                    && items[index].expanded.is_some_and(|expanded| {
                        (key(input, Key::Left) && expanded) || (key(input, Key::Right) && !expanded)
                    })
                {
                    response.toggled = Some(index);
                }
            }
        }
        let frame = self
            .scroll
            .begin(list, area, items.len() as f32 * row, input, focused);
        let mut child = list.child();
        let first = (self.scroll.offset / row).floor() as usize;
        for (i, item) in items
            .iter()
            .enumerate()
            .skip(first)
            .take((area.height() / row).ceil() as usize + 1)
        {
            let rect = Rect::new(
                frame.content.min[0],
                frame.content.min[1] + i as f32 * row,
                frame.content.width(),
                row,
            );
            let mut s = states(
                &frame.input,
                rect,
                item.enabled,
                focused && self.selected == Some(i),
            );
            if self.selected == Some(i) {
                s = s.with(State::Selected);
            }
            let style = sheet.resolve("row", s).unwrap();
            box_style(&mut child, rect, style);
            let indent = item.depth.min(16) as f32 * sheet.metric("icon.slot", 20.);
            let slot = Rect::new(rect.min[0] + indent, rect.min[1], 20., row);
            if let Some(expanded) = item.expanded {
                text_style(
                    &mut child,
                    atlas,
                    slot,
                    if expanded { "⌄" } else { "›" },
                    style,
                    Align::Center,
                );
                if item.enabled && press(&frame.input, slot) {
                    response.toggled = Some(i);
                }
            }
            text_style(
                &mut child,
                atlas,
                Rect::new(
                    slot.max[0],
                    rect.min[1],
                    (rect.max[0] - slot.max[0]).max(0.),
                    row,
                ),
                item.label,
                style,
                Align::Left,
            );
            if item.enabled && press(&frame.input, rect) {
                self.selected = Some(i);
                response.selected = Some(i);
            }
        }
        if focused && key(input, Key::Enter) {
            response.activated = self.selected.filter(|i| items[*i].enabled);
        }
        child.clip_to(frame.viewport);
        list.append(child);
        response
    }
}

pub struct StarRating;
impl StarRating {
    pub fn draw(
        list: &mut PaintList,
        area: Rect,
        value: usize,
        max: usize,
        input: &Input,
        enabled: bool,
        focused: bool,
    ) -> Option<usize> {
        let sheet = list.styles();
        let max = max.clamp(1, 5);
        let step = area.width() / max as f32;
        let mut changed = None;
        for i in 0..max {
            let rect = Rect::new(
                area.min[0] + i as f32 * step,
                area.min[1],
                step,
                area.height(),
            );
            let mut s = states(input, rect, enabled, focused);
            if i < value {
                s = s.with(State::Selected);
            }
            let style = sheet.resolve("button.icon", s).unwrap();
            box_style(list, rect, style);
            icons::draw(list, rect, Icon::Star, 14., style.foreground);
            if enabled && press(input, rect) {
                changed = Some(if value == i + 1 { 0 } else { i + 1 });
            }
        }
        if enabled && focused {
            if key(input, Key::Right) {
                changed = Some((value + 1).min(max));
            }
            if key(input, Key::Left) {
                changed = Some(value.saturating_sub(1));
            }
        }
        changed
    }
}
