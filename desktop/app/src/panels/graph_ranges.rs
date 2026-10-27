//! Window-owned range menus and ephemeral, channel-scoped graph axes.
use super::graph_view::{AxisView, Range};
use super::*;
use crate::layer_clipboard::KeyframeSource;
use motion_core::KeyframeTarget;
use motion_ui::widgets::Checkbox;

const MAX_VIEWS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Surface {
    Timeline,
    Comparison,
    Mini(Panel),
}
impl Surface {
    fn time_axis(self) -> bool {
        matches!(self, Self::Timeline | Self::Comparison)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Identity {
    pub surface: Surface,
    pub target: Option<KeyframeTarget>,
    pub component: Option<usize>,
    pub speed: bool,
    pub unit: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Bounds {
    pub x: Range,
    pub y: Range,
}

#[derive(Clone, Copy)]
pub(super) struct Options {
    pub displayed: Bounds,
    pub all: Option<Bounds>,
    pub selection: Option<Bounds>,
}

#[derive(Clone)]
struct Entry {
    id: Identity,
    x: AxisView,
    y: AxisView,
}

#[derive(Clone, PartialEq, Eq)]
struct Scope {
    root: String,
    composition: String,
    generation: u64,
    geometry: [Option<u64>; 4],
}
impl Scope {
    fn of(state: &Value) -> Option<Self> {
        let root = state["root"].as_str()?;
        let composition = state["composition"]
            .as_str()
            .or_else(|| state["project"]["composition_id"].as_str())?;
        if root.is_empty() || composition.is_empty() {
            return None;
        }
        Some(Self {
            root: root.into(),
            composition: composition.into(),
            generation: state["document_generation"].as_u64()?,
            geometry: ["width", "height", "frames", "fps"]
                .map(|field| state["project"][field].as_u64()),
        })
    }
}

#[derive(Clone)]
struct Menu {
    window: Option<Panel>,
    source: KeyframeSource,
    id: Identity,
    options: Options,
    point: [f32; 2],
    opened: bool,
    focus: usize,
    mouse: Option<[f32; 2]>,
    selection: super::key_selection::Set,
    legacy: Option<super::types::KeySelection>,
    effect_key: Option<super::key_selection::Key>,
    comparison: Option<super::curve_comparison::RangeContext>,
    selected: Option<u64>,
    focused: Option<Panel>,
    track: Option<(Value, i64)>,
}

#[derive(Clone, Default)]
pub(crate) struct Controls {
    scope: Option<Scope>,
    entries: Vec<Entry>,
    window: Option<Panel>,
    menu: Option<Menu>,
    time_locked: bool,
}

fn snapshot<'a>(state: &'a Value, id: &Identity) -> Option<(&'a Value, i64)> {
    super::key_selection::authored_track(state, id.target.as_ref()?)
}

impl Controls {
    pub(super) fn sync(&mut self, state: &Value) {
        let scope = Scope::of(state);
        if scope != self.scope {
            self.entries.clear();
            self.menu = None;
            self.time_locked = false;
            self.scope = scope;
        }
    }
    pub(super) fn ready(&self) -> bool {
        self.scope.is_some()
    }
    pub(super) fn editing(&self) -> bool {
        self.menu.is_some()
    }
    pub(super) fn cancel(&mut self) {
        self.menu = None;
    }
    pub(super) fn no_property_plot(&mut self) {
        if self.menu.as_ref().is_some_and(|menu| {
            menu.id.surface == Surface::Timeline
                && matches!(menu.id.target, Some(KeyframeTarget::Property { .. }))
        }) {
            self.cancel();
        }
    }
    pub(super) fn no_timeline_plot(&mut self) {
        if self
            .menu
            .as_ref()
            .is_some_and(|menu| menu.id.surface == Surface::Timeline)
        {
            self.cancel();
        }
    }
    pub(super) fn no_comparison_plot(&mut self) {
        if self
            .menu
            .as_ref()
            .is_some_and(|menu| menu.id.surface == Surface::Comparison)
        {
            self.cancel();
        }
    }
    pub(super) fn comparison_groups(&mut self, groups: impl IntoIterator<Item = Identity>) {
        if let Some(menu) = self
            .menu
            .as_ref()
            .filter(|menu| menu.id.surface == Surface::Comparison)
        {
            if !groups.into_iter().any(|id| id == menu.id) {
                self.cancel();
            }
        }
    }
    pub(super) fn locked_x(&self) -> bool {
        self.time_locked
    }

    pub(super) fn prepare(
        &mut self,
        window: Option<Panel>,
        state: &Value,
        input: &Input,
        selection: &super::key_selection::Set,
        legacy: Option<super::types::KeySelection>,
        effect_key: Option<super::key_selection::Key>,
        comparison: &super::curve_comparison::Controls,
        selected: Option<u64>,
        focused: Option<Panel>,
    ) -> bool {
        self.window = window;
        let owner = self.menu.as_ref().map(|m| m.window);
        self.sync(state);
        let event = input.pending
            || input.pressed.is_some()
            || input.released.is_some()
            || !input.keys.is_empty()
            || !input.text.is_empty()
            || input.wheel != [0., 0.];
        let owned = owner == Some(window);
        let stale = self.menu.as_ref().is_some_and(|m| {
            m.source != KeyframeSource::of(state)
                || !m.selection.same_selection(selection)
                || m.legacy != legacy
                || matches!(m.id.target, Some(KeyframeTarget::Effect { .. }))
                    && m.effect_key != effect_key
                || m.comparison
                    .as_ref()
                    .is_some_and(|context| !comparison.matches_range_context(context))
                || m.selected != selected
                || m.focused != focused
                || match (&m.track, snapshot(state, &m.id)) {
                    (Some((before, offset)), Some((now, next))) => before != now || *offset != next,
                    (None, None) => false,
                    _ => true,
                }
        });
        if owner.is_some()
            && (self.menu.is_none()
                || stale
                || owned && input.focus_lost
                || !owned && event && !input.focus_lost)
        {
            self.menu = None;
            return owned || event;
        }
        self.menu.is_some() && owned
    }

    fn entry(&mut self, id: &Identity) -> Option<&mut Entry> {
        let index = if let Some(index) = self.entries.iter().position(|entry| entry.id == *id) {
            index
        } else {
            if self.entries.len() >= MAX_VIEWS {
                let index = self.entries.iter().position(|entry| {
                    !entry.x.locked()
                        && !entry.y.locked()
                        && self.menu.as_ref().is_none_or(|m| m.id != entry.id)
                })?;
                self.entries.remove(index);
            }
            self.entries.push(Entry {
                id: id.clone(),
                x: AxisView::default(),
                y: AxisView::default(),
            });
            self.entries.len() - 1
        };
        Some(&mut self.entries[index])
    }

    pub(super) fn displayed(&mut self, state: &Value, id: &Identity, automatic: Bounds) -> Bounds {
        self.sync(state);
        if !self.ready() {
            return automatic;
        }
        if self.menu.as_ref().is_some_and(|m| {
            m.window == self.window
                && m.id.surface == id.surface
                && id.surface != Surface::Comparison
                && m.id != *id
        }) {
            self.menu = None;
        }
        let Some(entry) = self.entry(id) else {
            return automatic;
        };
        Bounds {
            x: if id.surface.time_axis() {
                automatic.x
            } else {
                entry.x.range(automatic.x)
            },
            y: entry.y.range(automatic.y),
        }
    }

    pub(super) fn navigate_y(
        &mut self,
        id: &Identity,
        automatic: Range,
        factor: f64,
        anchor: f64,
    ) -> bool {
        let Some(entry) = self.entry(id) else {
            return false;
        };
        let mut next = entry.y;
        if !next.zoom(factor, anchor, automatic) || !renderable_y(next.range(automatic)) {
            return false;
        }
        entry.y = next;
        true
    }

    pub(super) fn locked_y(&mut self, id: &Identity) -> bool {
        self.entry(id).is_some_and(|entry| entry.y.locked())
    }

    pub(super) fn open(
        &mut self,
        point: [f32; 2],
        id: Identity,
        options: Options,
        state: &Value,
        selection: &super::key_selection::Set,
        legacy: Option<super::types::KeySelection>,
        effect_key: Option<super::key_selection::Key>,
        comparison: Option<super::curve_comparison::RangeContext>,
        selected: Option<u64>,
        focused: Option<Panel>,
    ) {
        self.sync(state);
        if !self.ready() {
            return;
        }
        let _ = self.entry(&id);
        let options = Options {
            all: options.all.filter(|bounds| renderable_y(bounds.y)),
            selection: options.selection.filter(|bounds| renderable_y(bounds.y)),
            ..options
        };
        let track = snapshot(state, &id).map(|(track, offset)| (track.clone(), offset));
        self.menu = Some(Menu {
            window: self.window,
            source: KeyframeSource::of(state),
            id,
            options,
            point,
            opened: true,
            focus: 0,
            mouse: None,
            selection: selection.clone(),
            legacy,
            effect_key,
            comparison,
            selected,
            focused,
            track,
        });
    }

    fn draw_menu(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        bounds: Rect,
        state: &Value,
        input: &Input,
        locale: Locale,
        timeline: &mut super::types::TimelineState,
    ) {
        let Some(mut menu) = self.menu.take() else {
            return;
        };
        if menu.window != self.window {
            self.menu = Some(menu);
            return;
        }
        if menu.source != KeyframeSource::of(state) {
            return;
        }
        let index = self.entries.iter().position(|entry| entry.id == menu.id);
        let available = index.is_some();
        let styles = list.styles();
        let pad = styles.metric("padding.panel", 8.);
        let row = styles.metric("row.height", 22.);
        let width = styles
            .metric("graph.range.menu.width", 232.)
            .min(bounds.width());
        let height = row * 6. + pad * 2.;
        let top = (bounds.min[1]
            + if self.window.is_some() {
                styles.metric("viewer.floating.top", 30.)
            } else {
                CHROME_HEIGHT
            })
        .min(bounds.max[1]);
        let x = menu.point[0].clamp(bounds.min[0], (bounds.max[0] - width).max(bounds.min[0]));
        let y = menu.point[1].clamp(top, (bounds.max[1] - height).max(top));
        let card = Rect::new(x, y, width, height);
        let neutral = Input::default();
        let opened = std::mem::take(&mut menu.opened);
        let events = if opened || input.composing || input.ime_consumed {
            &neutral
        } else {
            input
        };
        if input.focus_lost
            || events
                .key_presses()
                .any(|(key, _, repeat)| key == Key::Escape && !repeat)
            || events
                .pressed
                .is_some_and(|(_, point)| !card.contains(point))
        {
            return;
        }
        let time = menu.id.surface.time_axis();
        let lock_x = if time {
            self.time_locked
        } else {
            index.is_some_and(|index| self.entries[index].x.locked())
        };
        let lock_y = index.is_some_and(|index| self.entries[index].y.locked());
        let enabled = [
            available && (!lock_x || !lock_y) && menu.options.all.is_some(),
            available && (!lock_x || !lock_y) && menu.options.selection.is_some(),
            available && (!lock_x || !lock_y),
            available,
            available,
        ];
        if !enabled[menu.focus] {
            menu.focus = enabled.iter().position(|enabled| *enabled).unwrap_or(3);
        }
        for (key, modifiers, repeat) in events.key_presses() {
            if !repeat && matches!(key, Key::Tab | Key::Up | Key::Down) {
                let step = if key == Key::Up || key == Key::Tab && modifiers.shift {
                    4
                } else {
                    1
                };
                for _ in 0..5 {
                    menu.focus = (menu.focus + step) % 5;
                    if enabled[menu.focus] {
                        break;
                    }
                }
            }
        }
        if !opened && menu.mouse != Some(events.mouse) && card.contains(events.mouse) {
            let index = ((events.mouse[1] - y - pad) / row).floor();
            if (0. ..5.).contains(&index) && enabled[index as usize] {
                menu.focus = index as usize;
            }
        }
        menu.mouse = Some(input.mouse);
        let activate = events.key_presses().any(|(key, m, repeat)| {
            !repeat && !m.control && !m.alt && !m.shift && matches!(key, Key::Enter | Key::Space)
        });
        motion_ui::style::box_style(
            list,
            card,
            styles.resolve("popup", Default::default()).unwrap(),
        );
        let zh = locale == Locale::Zh;
        let labels = if zh {
            [
                "适配全部",
                "适配选区",
                "自动范围",
                "锁定时间轴 X",
                "锁定数值轴 Y",
            ]
        } else {
            [
                "Fit all",
                "Fit selection",
                "Automatic ranges",
                "Lock time axis X",
                "Lock value axis Y",
            ]
        };
        let mut operation = None;
        for item in 0..5 {
            let rect = Rect::new(
                x + pad,
                y + pad + row * item as f32,
                (width - pad * 2.).max(0.),
                row,
            );
            let enabled = enabled[item];
            let mut states = motion_ui::style::States::default();
            if menu.focus == item {
                states = states.with(motion_ui::style::State::Focused);
            }
            let clicked = if item < 3 {
                ui::button_with_states(
                    list,
                    atlas,
                    rect,
                    labels[item],
                    events,
                    enabled,
                    &styles,
                    "button",
                    states,
                )
                .clicked
                    || enabled && activate && menu.focus == item
            } else {
                Checkbox::draw(
                    list,
                    atlas,
                    rect,
                    labels[item],
                    if item == 3 { lock_x } else { lock_y },
                    events,
                    enabled,
                    menu.focus == item,
                    None,
                )
                .clicked
                    && (activate
                        || events.pressed.is_some_and(|(button, point)| {
                            button == MouseButton::Left && rect.contains(point)
                        }))
            };
            if clicked {
                operation = Some(item);
            }
        }
        let hint = if !available {
            if zh {
                "锁定范围已满，请先解锁其他视图"
            } else {
                "Unlock another view to edit ranges"
            }
        } else if menu.options.all.is_none() {
            if zh {
                "曲线范围不可用"
            } else {
                "Curve range unavailable"
            }
        } else if menu.options.selection.is_none() {
            if zh {
                "请先选择关键帧"
            } else {
                "Select keyframes to fit"
            }
        } else {
            if zh {
                "Tab 切换，空格确认"
            } else {
                "Tab to move; Space to choose"
            }
        };
        label(
            list,
            atlas,
            Rect::new(x + pad, y + pad + row * 5., (width - pad * 2.).max(0.), row),
            hint,
            true,
        );
        let Some(index) = index else {
            self.menu = Some(menu);
            return;
        };
        match operation {
            Some(item @ 0..=1) => {
                let fitted = if item == 0 {
                    menu.options.all
                } else {
                    menu.options.selection
                };
                if let Some(fitted) = fitted {
                    if time {
                        if !lock_x {
                            fit_time(timeline, fitted.x, state);
                        }
                    } else {
                        self.entries[index].x.fit(fitted.x);
                    }
                    self.entries[index].y.fit(fitted.y);
                }
                return;
            }
            Some(2) => {
                if time {
                    if !lock_x {
                        timeline.zoom = 1.;
                        timeline.offset = 0.;
                    }
                } else {
                    self.entries[index].x.automatic();
                }
                if menu.id.surface == Surface::Timeline && !lock_y {
                    timeline.graph_y_zoom = 1.;
                    timeline.graph_y_offset = 0.;
                }
                self.entries[index].y.automatic();
                return;
            }
            Some(3) => {
                if time {
                    self.time_locked = !lock_x;
                    timeline.horizontal_drag = None;
                    timeline.edge_pan_at = None;
                } else {
                    self.entries[index]
                        .x
                        .lock(!lock_x, menu.options.displayed.x);
                }
            }
            Some(4) => self.entries[index]
                .y
                .lock(!lock_y, menu.options.displayed.y),
            _ => {}
        }
        self.menu = Some(menu);
    }
}

fn renderable_y(range: Range) -> bool {
    let low = range.low as f32;
    let high = range.high as f32;
    low.is_finite() && high.is_finite() && high > low && (high - low).is_finite()
}

fn fit_time(view: &mut super::types::TimelineState, range: Range, state: &Value) {
    let total = (last_frame(state) + 1.).max(1.);
    view.zoom = (total / range.span()).clamp(1., total.max(100.)) as f32;
    view.offset = range
        .low
        .clamp(0., (total - total / f64::from(view.zoom)).max(0.)) as f32;
}

pub(super) fn launch(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    row: Rect,
    occupied_end: f32,
    input: &Input,
    enabled: bool,
) -> Option<[f32; 2]> {
    let styles = list.styles();
    let pad = styles.metric("padding.panel", 8.);
    let slot = styles.metric("graph.control.height", 24.);
    let rect = Rect::new(row.max[0] - pad - slot, row.min[1], slot, row.height());
    if rect.min[0] >= occupied_end + styles.metric("graph.control.gap", 2.)
        && ui::button(list, atlas, rect, "⋯", input, enabled).clicked
    {
        return Some([rect.min[0], rect.max[1]]);
    }
    input
        .pressed
        .filter(|(button, point)| enabled && *button == MouseButton::Right && row.contains(*point))
        .map(|(_, point)| point)
}

/// Shared composition-time/value readout. Callers evaluate their actual model;
/// normalized easing coordinates must be mapped before reaching this helper.
pub(super) fn readout(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    plot: Rect,
    mouse: [f32; 2],
    title: &str,
    frame: f64,
    value: f64,
    unit: &str,
) {
    let styles = list.styles();
    let pad = styles.metric("padding.panel", 8.);
    let row = styles.metric("row.height", 22.);
    let width = styles.metric("graph.readout.width", 232.).min(plot.width());
    let height = (row * 2. + pad * 2.).min(plot.height());
    if width <= pad * 2. || height <= pad * 2. {
        return;
    }
    let x = if mouse[0] + pad + width > plot.max[0] {
        mouse[0] - width - pad
    } else {
        mouse[0] + pad
    };
    let y = if mouse[1] + pad + height > plot.max[1] {
        mouse[1] - height - pad
    } else {
        mouse[1] + pad
    };
    let card = Rect::new(
        x.clamp(plot.min[0], (plot.max[0] - width).max(plot.min[0])),
        y.clamp(plot.min[1], (plot.max[1] - height).max(plot.min[1])),
        width,
        height,
    );
    let mut paint = list.child();
    motion_ui::style::box_style(
        &mut paint,
        card,
        styles.resolve("popup", Default::default()).unwrap(),
    );
    let mut text = list.child();
    label(
        &mut text,
        atlas,
        Rect::new(
            card.min[0] + pad,
            card.min[1] + pad,
            (width - pad * 2.).max(0.),
            row,
        ),
        &format!("{title} · {frame:.2} f"),
        true,
    );
    let number = if value != 0. && !(1e-4..1e7).contains(&value.abs()) {
        format!("{value:.4e}")
    } else {
        format!("{value:.4}")
    };
    motion_ui::style::text_style(
        &mut text,
        atlas,
        Rect::new(
            card.min[0] + pad,
            card.min[1] + pad + row,
            (width - pad * 2.).max(0.),
            row,
        ),
        &format!("{number} {unit}"),
        styles.resolve("label.accent", Default::default()).unwrap(),
        Align::Left,
    );
    text.clip_to(card.shrink(pad));
    paint.append(text);
    paint.clip_to(plot);
    list.append(paint);
}

pub(super) fn overlay(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    bounds: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
) {
    let mut controls = std::mem::take(&mut panels.timeline.graph_ranges);
    controls.draw_menu(
        list,
        atlas,
        bounds,
        state,
        input,
        panels.locale,
        &mut panels.timeline,
    );
    panels.timeline.graph_ranges = controls;
}

#[cfg(test)]
mod tests;
