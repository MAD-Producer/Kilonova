//! Shared menu rows, geometry and keyboard navigation. Entries are rebuilt by
//! their owner from current state; this component never retains an action.
use crate::{
    Rect, TextAtlas,
    icons::{self, Icon},
    input::{Input, Key, MouseButton},
    paint::{self, PaintList},
    style::{State, States, StyleSheet, box_style, text_style},
    ui::Align,
};
#[cfg(test)]
mod tests;

pub struct Entry<T> {
    pub label: String,
    pub hint: String,
    pub checked: bool,
    pub enabled: bool,
    pub disabled_reason: Option<String>,
    pub choice: T,
    pub divider_after: bool,
    pub submenu: bool,
}
impl<T> Entry<T> {
    pub fn new(label: String, choice: T, enabled: bool) -> Self {
        Self {
            label,
            choice,
            enabled,
            hint: String::new(),
            checked: false,
            disabled_reason: None,
            divider_after: false,
            submenu: false,
        }
    }
    /// Older callers still expose a combined label to their command audit.
    /// Only remove the exact, supplied catalogue suffix; user text is not parsed.
    pub fn caption(&self) -> &str {
        if self.hint.is_empty() {
            return &self.label;
        }
        self.label
            .strip_suffix(&self.hint)
            .and_then(|caption| {
                caption
                    .strip_suffix("    ")
                    .or_else(|| caption.strip_suffix("  "))
            })
            .unwrap_or(&self.label)
    }
}

#[derive(Clone, Copy)]
pub struct Metrics {
    pub row: f32,
    pub pad: f32,
    pub min_width: f32,
    pub max_width: f32,
}
impl Metrics {
    pub fn application(sheet: &StyleSheet) -> Self {
        Self {
            row: sheet.metric("menu.popup.row.height", 32.).max(1.),
            pad: sheet.metric("menu.popup.padding", 4.),
            min_width: sheet.metric("menu.popup.min_width", 280.),
            max_width: sheet.metric("menu.popup.max_width", 480.),
        }
    }
    pub fn context(sheet: &StyleSheet) -> Self {
        Self {
            row: sheet.metric("context.row.height", 20.).max(1.),
            pad: sheet.metric("context.menu.padding", 3.),
            min_width: sheet.metric("context.menu.width", 220.),
            max_width: sheet.metric("context.menu.max_width", 320.),
        }
    }
    pub fn count(self, len: usize, height: f32) -> usize {
        len.min(((((height - self.pad * 2.).max(self.row)) / self.row).floor() as usize).max(1))
    }
    pub fn height(self, len: usize, bounds: Rect) -> f32 {
        (self.count(len, bounds.height()) as f32 * self.row + self.pad * 2.)
            .min(bounds.height().max(0.))
    }
    pub fn row_rect(self, card: Rect, visible_index: usize) -> Rect {
        Rect::new(
            card.min[0] + self.pad,
            card.min[1] + self.pad + visible_index as f32 * self.row,
            (card.width() - self.pad * 2.).max(0.),
            self.row,
        )
        .intersection(card)
    }
    pub fn width<T>(self, entries: &[Entry<T>], sheet: &StyleSheet, atlas: &mut TextAtlas) -> f32 {
        let style = sheet
            .resolve("context_menu.row", States::default())
            .expect("existing context menu style");
        let columns = sheet.metric("icon.slot", 20.) * 2.
            + sheet.metric("padding.panel", 8.) * 2.
            + self.pad * 2.;
        entries
            .iter()
            .map(|e| {
                (atlas.measure(e.caption()) + atlas.measure(&e.hint)) * style.font_size
                    / atlas.font_size()
                    + columns
                    + if e.hint.is_empty() {
                        0.
                    } else {
                        sheet.metric("padding.panel", 8.) * 2.
                    }
            })
            .fold(self.min_width, f32::max)
            .min(self.max_width.max(self.min_width))
    }
}

/// Clamp without assuming the window can fit a whole logical row or two cards.
pub fn popup(anchor: [f32; 2], bounds: Rect, width: f32, height: f32, above: bool) -> Rect {
    let width = width.min(bounds.width().max(0.)).max(0.);
    let height = height.min(bounds.height().max(0.)).max(0.);
    Rect::new(
        anchor[0].clamp(bounds.min[0], (bounds.max[0] - width).max(bounds.min[0])),
        (anchor[1] - if above { height } else { 0. })
            .clamp(bounds.min[1], (bounds.max[1] - height).max(bounds.min[1])),
        width,
        height,
    )
}
pub fn child(parent: Rect, row: Rect, bounds: Rect, width: f32, height: f32) -> Rect {
    let width = width.min(bounds.width().max(0.));
    let x = if parent.max[0] + width <= bounds.max[0] {
        parent.max[0]
    } else if parent.min[0] - width >= bounds.min[0] {
        parent.min[0] - width
    } else {
        (bounds.max[0] - width).max(bounds.min[0])
    };
    popup([x, row.min[1]], bounds, width, height, false)
}

#[derive(Default)]
pub struct Response {
    pub chosen: Option<usize>,
    pub hovered: Option<usize>,
}
pub fn plain_key(input: &Input, key: Key, repeats: bool) -> bool {
    !input.composing
        && !input.ime_consumed
        && input
            .key_presses()
            .any(|(k, m, r)| k == key && !m.control && !m.alt && !m.shift && (repeats || !r))
}

/// The owner keeps the open page/target and routes branch actions. Keyboard
/// Enter never repeats; navigation repeats only Up/Down/PageUp/PageDown.
pub fn draw<T>(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    card: Rect,
    metrics: Metrics,
    entries: &[Entry<T>],
    input: &Input,
    keyboard: bool,
    hover: bool,
    highlighted: &mut usize,
    scroll: &mut usize,
) -> Response {
    if entries.is_empty() {
        *highlighted = 0;
        *scroll = 0;
        return Response::default();
    }
    let count = metrics.count(entries.len(), card.height());
    let max_scroll = entries.len().saturating_sub(count);
    *highlighted = (*highlighted).min(entries.len() - 1);
    *scroll = (*scroll).min(max_scroll);
    if card.contains(input.mouse) && input.wheel[1] != 0. {
        *scroll = if input.wheel[1] < 0. {
            scroll.saturating_add(1).min(max_scroll)
        } else {
            scroll.saturating_sub(1)
        };
    }
    if keyboard && !input.focus_lost && !input.composing && !input.ime_consumed {
        for (key, mods, repeat) in input
            .key_presses()
            .filter(|(_, m, _)| !m.control && !m.alt && !m.shift)
        {
            let _ = mods;
            let direction = match key {
                Key::Down => 1,
                Key::Up => -1,
                Key::PageDown => count as isize,
                Key::PageUp => -(count as isize),
                Key::Home if !repeat => {
                    *highlighted = entries.len() - 1;
                    1
                }
                Key::End if !repeat => {
                    *highlighted = 0;
                    -1
                }
                _ => continue,
            };
            let mut next =
                (*highlighted as isize + direction).rem_euclid(entries.len() as isize) as usize;
            for _ in 0..entries.len() {
                if entries[next].enabled {
                    break;
                }
                next = (next as isize + direction.signum()).rem_euclid(entries.len() as isize)
                    as usize;
            }
            *highlighted = next;
            *scroll = (*scroll)
                .min(next)
                .max(next.saturating_sub(count.saturating_sub(1)))
                .min(max_scroll);
        }
    }
    let styles = list.styles();
    let frame = styles
        .resolve("context_menu", States::default())
        .expect("existing context menu style");
    // Preserve the current ordered-paint path, also removing covered glyphs
    // for callers using the older two-pass renderer contract.
    list.retain_glyphs(|g| {
        g.position[0] + g.size[0] <= card.min[0]
            || g.position[0] >= card.max[0]
            || g.position[1] + g.size[1] <= card.min[1]
            || g.position[1] >= card.max[1]
    });
    let mut paint = list.child();
    box_style(&mut paint, card, frame);
    let slot = styles.metric("icon.slot", 20.);
    let inset = styles.metric("padding.panel", 8.);
    let border = styles.metric("border", 1.);
    let mut response = Response::default();
    for (index, entry) in entries.iter().enumerate().skip(*scroll).take(count) {
        let area = metrics.row_rect(card, index - *scroll);
        if hover && area.contains(input.mouse) {
            *highlighted = index;
            response.hovered = Some(index);
        }
        let mut flags = States::default();
        if index == *highlighted && entry.enabled {
            flags = flags.with(State::Selected);
        }
        if !entry.enabled {
            flags = flags.with(State::Disabled);
        }
        let style = styles
            .resolve("context_menu.row", flags)
            .expect("existing context menu row style");
        box_style(&mut paint, area, style);
        let end = area.max[0] - slot;
        let hint_width = (atlas.measure(&entry.hint) * style.font_size / atlas.font_size())
            .min((area.width() * 0.5).max(0.));
        let hint = Rect::new(
            (end - hint_width).max(area.min[0]),
            area.min[1],
            hint_width,
            area.height(),
        );
        let label_x = (area.min[0] + slot).min(area.max[0]);
        let label_end = if entry.hint.is_empty() {
            end
        } else {
            hint.min[0] - inset
        };
        text_style(
            &mut paint,
            atlas,
            Rect::new(
                label_x,
                area.min[1],
                (label_end - label_x).max(0.),
                area.height(),
            ),
            entry.caption(),
            style,
            Align::Left,
        );
        text_style(&mut paint, atlas, hint, &entry.hint, style, Align::Right);
        let icon_size = styles.metric("checkbox.size", 14.);
        if entry.checked {
            icons::draw(
                &mut paint,
                Rect::new(area.min[0], area.min[1], slot, area.height()),
                Icon::Check,
                icon_size,
                style.foreground,
            );
        }
        if entry.submenu {
            icons::draw(
                &mut paint,
                Rect::new(end, area.min[1], slot, area.height()),
                Icon::ChevronRight,
                icon_size,
                style.foreground,
            );
        }
        if entry.divider_after {
            paint::rect(
                &mut paint,
                Rect::new(
                    area.min[0] + inset,
                    area.max[1] - border,
                    (area.width() - inset * 2.).max(0.),
                    border,
                ),
                styles.color("context.border"),
            );
        }
        if !input.focus_lost
            && !input.composing
            && !input.ime_consumed
            && entry.enabled
            && input.pressed.is_some_and(|(b, p)| {
                b == MouseButton::Left && card.contains(p) && area.contains(p)
            })
        {
            response.chosen = Some(index);
        }
    }
    if keyboard
        && !input.focus_lost
        && plain_key(input, Key::Enter, false)
        && entries[*highlighted].enabled
    {
        response.chosen = Some(*highlighted);
    }
    paint.clip_to(card);
    list.append(paint);
    response
}
