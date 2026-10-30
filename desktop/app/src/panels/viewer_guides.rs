//! Composition-pixel overlays and local guide previews. Only a completed,
//! version-checked edit reaches the engine; painting never changes the scene.
use super::*;
use crate::{layer_clipboard::KeyframeSource, viewer_navigation::Mapping};
use motion_core::{Guide, GuideAxis, MAX_GUIDES, MAX_GUIDE_POSITION};
use std::sync::Arc;

const MAX_GRID_LINES: usize = 2048;
mod snapping;
pub(super) use snapping::Snapping;

#[derive(Clone, Copy)]
pub(super) struct Visibility {
    pub safe: bool,
    pub grid: bool,
    pub rulers: bool,
    pub guides: bool,
    pub handles: bool,
    pub locked: bool,
}
impl Default for Visibility {
    fn default() -> Self {
        Self {
            safe: false,
            grid: false,
            rulers: false,
            guides: true,
            handles: true,
            locked: false,
        }
    }
}

#[derive(Default)]
pub(crate) struct Controls {
    pub visibility: Visibility,
    channels: [motion_render::ViewChannel; 2],
    channel_dropdown: motion_ui::widgets::Dropdown,
    window: Option<Panel>,
    drag: Option<Drag>,
    menu: Option<Menu>,
    cache: Option<(KeyframeSource, Option<Arc<Vec<Guide>>>)>,
    snapshot_fault: Option<String>,
    invalid_attempts: u8,
    notice: Option<Notice>,
}
enum Notice {
    Unavailable,
    Limit,
    ReadOnly,
}
struct Menu {
    window: Option<Panel>,
    source: KeyframeSource,
    point: [f32; 2],
    opened: bool,
}
struct Drag {
    window: Option<Panel>,
    source: KeyframeSource,
    mapping: Mapping,
    viewport: Rect,
    before: Arc<Vec<Guide>>,
    guide: Guide,
    creating: bool,
}

fn axis_index(axis: GuideAxis) -> usize {
    usize::from(axis == GuideAxis::Y)
}
fn identified(state: &Value) -> bool {
    state["root"].as_str().is_some_and(|v| !v.is_empty())
        && state["document_generation"].as_u64().is_some()
        && state["revision"].as_u64().is_some()
        && state["composition"]
            .as_str()
            .or_else(|| state["project"]["composition_id"].as_str())
            .is_some_and(|v| !v.is_empty())
}
fn guides(state: &Value) -> Result<Vec<Guide>, String> {
    let Some(value) = state["project"].get("guides") else {
        return Ok(Vec::new());
    };
    let array = value.as_array().ok_or("guides must be an array")?;
    if array.len() > MAX_GUIDES {
        return Err("composition guide limit exceeded".into());
    }
    let mut guides = Vec::with_capacity(array.len());
    for value in array {
        let object = value.as_object().ok_or("guide must be an object")?;
        if object.len() != 3
            || !object.contains_key("id")
            || !object.contains_key("axis")
            || !object.contains_key("position")
        {
            return Err("guide requires exactly id, axis and position".into());
        }
        let id = value["id"]
            .as_u64()
            .ok_or("guide ID must be an unsigned integer")?;
        let axis = match value["axis"].as_str() {
            Some("x") => GuideAxis::X,
            Some("y") => GuideAxis::Y,
            _ => return Err("guide axis must be x or y".into()),
        };
        let position = value["position"]
            .as_f64()
            .ok_or("guide position must be a number")?;
        guides.push(Guide { id, axis, position });
    }
    motion_core::guides::validate(&guides).map_err(|e| e.to_string())?;
    Ok(guides)
}
fn matches_snapshot(guides: &[Guide], state: &Value) -> bool {
    let Some(value) = state["project"].get("guides") else {
        return guides.is_empty();
    };
    value.as_array().is_some_and(|array| {
        array.len() == guides.len()
            && array.iter().zip(guides).all(|(value, guide)| {
                value.as_object().is_some_and(|o| o.len() == 3)
                    && value["id"].as_u64() == Some(guide.id)
                    && value["axis"].as_str()
                        == Some(if guide.axis == GuideAxis::X { "x" } else { "y" })
                    && value["position"].as_f64() == Some(guide.position)
            })
    })
}
fn next_id(guides: &[Guide]) -> Option<u64> {
    // A bounded list always has a free ID in 1..=MAX_GUIDES+1, even if an
    // imported guide has u64::MAX. Never increment the largest imported ID.
    (1..=MAX_GUIDES as u64 + 1).find(|id| !guides.iter().any(|guide| guide.id == *id))
}
fn edit(state: &Value, source: KeyframeSource, guides: Vec<Guide>) -> Action {
    let command = Command::SetGuides { guides };
    let composition = state["composition"]
        .as_str()
        .or_else(|| state["project"]["composition_id"].as_str())
        .unwrap_or("");
    Action::EditorAction {
        source,
        commands: vec![Command::InComposition {
            composition: composition.into(),
            command: Box::new(command),
        }],
    }
}

/// One composition-origin lattice for drawing, ruler ticks and snapping.
/// Screen spacing is never multiplied into authored guide coordinates.
pub(super) fn grid_step(mapping: Mapping, base: f64, minimum_screen: f64) -> Option<f64> {
    if !base.is_finite() || base <= 0. || !minimum_screen.is_finite() || minimum_screen <= 0. {
        return None;
    }
    let multiplier = (minimum_screen / (base * mapping.scale))
        .max(1.)
        .log2()
        .ceil()
        .exp2();
    let step = base * multiplier;
    (step.is_finite() && step > 0.).then_some(step)
}
fn ticks(mapping: Mapping, viewport: Rect, axis: usize, step: f64) -> Vec<f64> {
    if !step.is_finite() || step <= 0. || viewport.width() <= 0. || viewport.height() <= 0. {
        return vec![];
    }
    let a = mapping.local(viewport.min)[axis];
    let b = mapping.local(viewport.max)[axis];
    let first = (a.min(b) / step).ceil();
    let count = ((a.max(b) / step).floor() - first + 1.).max(0.);
    if !first.is_finite() || !count.is_finite() {
        return vec![];
    }
    (0..(count.min(MAX_GRID_LINES as f64) as usize))
        .map(|i| (first + i as f64) * step)
        .collect()
}

/// Nearest candidate within a logical-pixel tolerance. The same function is
/// used for guide dragging and transform targets; Alt bypass belongs to callers.
pub(super) fn snap_axis(
    position: f64,
    axis: GuideAxis,
    guides: &[Guide],
    canvas: [f64; 2],
    mapping: Mapping,
    tolerance: f64,
    grid: Option<f64>,
    excluded: Option<u64>,
) -> f64 {
    nearest_axis(
        position, axis, guides, canvas, mapping, tolerance, grid, excluded,
    )
    .unwrap_or(position)
}
fn nearest_axis(
    position: f64,
    axis: GuideAxis,
    guides: &[Guide],
    canvas: [f64; 2],
    mapping: Mapping,
    tolerance: f64,
    grid: Option<f64>,
    excluded: Option<u64>,
) -> Option<f64> {
    if !position.is_finite() || !tolerance.is_finite() || tolerance < 0. {
        return None;
    }
    let index = axis_index(axis);
    let mut best = None;
    let mut distance = tolerance / mapping.scale;
    let mut consider = |candidate: f64| {
        let delta = (candidate - position).abs();
        if candidate.is_finite() && delta <= distance {
            best = Some(candidate);
            distance = delta;
        }
    };
    for candidate in [0., canvas[index] * 0.5, canvas[index]] {
        consider(candidate);
    }
    if let Some(step) = grid.filter(|v| v.is_finite() && *v > 0.) {
        consider((position / step).round() * step);
    }
    for guide in guides
        .iter()
        .filter(|g| g.axis == axis && Some(g.id) != excluded)
    {
        consider(guide.position);
    }
    best
}

impl Controls {
    pub(super) fn canvas(&self, viewport: Rect, styles: &motion_ui::style::StyleSheet) -> Rect {
        if !self.visibility.rulers {
            return viewport;
        }
        Rect {
            min: [
                (viewport.min[0] + styles.metric("viewer.overlay.ruler.width", 48.).max(0.))
                    .min(viewport.max[0]),
                (viewport.min[1] + styles.metric("viewer.overlay.ruler.height", 22.).max(0.))
                    .min(viewport.max[1]),
            ],
            max: viewport.max,
        }
    }
    pub(super) fn ruler_hit(
        &self,
        viewport: Rect,
        point: [f32; 2],
        styles: &motion_ui::style::StyleSheet,
    ) -> bool {
        let [top, left] = ruler_rects(viewport, styles);
        self.visibility.rulers
            && viewport.contains(point)
            && (top.contains(point) || left.contains(point))
    }
    pub(super) fn take_notice(&mut self, locale: Locale) -> Option<String> {
        let zh = locale == Locale::Zh;
        Some(
            match self.notice.take()? {
                Notice::Unavailable => {
                    if zh {
                        "参考线数据暂不可用，请重新打开工程后重试"
                    } else {
                        "Guides are unavailable; reopen the project before editing"
                    }
                }
                Notice::Limit => {
                    if zh {
                        "每个合成最多支持 512 条参考线"
                    } else {
                        "A composition supports at most 512 guides"
                    }
                }
                Notice::ReadOnly => {
                    if zh {
                        "请回到合成视图后编辑参考线"
                    } else {
                        "Return to the composition view to edit guides"
                    }
                }
            }
            .into(),
        )
    }
    pub(super) fn editing(&self) -> bool {
        self.drag.is_some() || self.menu.is_some()
    }
    pub(super) fn dragging(&self) -> bool {
        self.drag.is_some()
    }
    pub(super) fn cancel(&mut self) {
        self.drag = None;
        self.menu = None;
        self.channel_dropdown.open = false;
        self.invalid_attempts = 0;
    }
    /// Foreign passive paints retain the owner. Foreign real input cancels and
    /// consumes that frame, including the click which dismissed the menu.
    pub(super) fn prepare(&mut self, window: Option<Panel>, state: &Value, input: &Input) -> bool {
        self.window = window;
        let owner = self
            .menu
            .as_ref()
            .map(|s| s.window)
            .or_else(|| self.drag.as_ref().map(|s| s.window));
        let source = KeyframeSource::of(state);
        let stale = self.menu.as_ref().is_some_and(|s| s.source != source)
            || self.drag.as_ref().is_some_and(|s| s.source != source);
        let event = input.pending
            || input.pressed.is_some()
            || input.released.is_some()
            || !input.keys.is_empty()
            || !input.text.is_empty()
            || input.wheel != [0., 0.];
        let owned = owner == Some(window);
        let menu_owned = self.menu.is_some() && owned;
        let superseded_press = owned && self.drag.is_some() && input.pressed.is_some();
        if stale
            || superseded_press
            || owned && input.focus_lost
            || owner.is_some() && !owned && event && !input.focus_lost
        {
            let consume = owner.is_some() && (owned || event);
            self.cancel();
            return consume;
        }
        menu_owned
    }
    fn snapshot(&mut self, state: &Value) -> Option<Arc<Vec<Guide>>> {
        let source = KeyframeSource::of(state);
        if !identified(state)
            || self.cache.as_ref().is_none_or(|(old, guides)| {
                *old != source
                    || guides
                        .as_ref()
                        .is_none_or(|guides| !matches_snapshot(guides, state))
            })
        {
            let data = match guides(state) {
                Ok(guides) => {
                    self.snapshot_fault = None;
                    Some(Arc::new(guides))
                }
                Err(cause) => {
                    self.snapshot_fault = Some(cause.chars().take(256).collect());
                    None
                }
            };
            self.cache = Some((source, data));
        }
        self.cache.as_ref()?.1.clone()
    }
    fn invalid(&mut self, state: &Value, elapsed: std::time::Duration) {
        self.invalid_attempts = self.invalid_attempts.saturating_add(1).min(61);
        let cause = self
            .snapshot_fault
            .as_deref()
            .unwrap_or("source identity missing");
        let root: String = state["root"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(256)
            .collect();
        let composition: String = state["composition"]
            .as_str()
            .or_else(|| state["project"]["composition_id"].as_str())
            .unwrap_or("")
            .chars()
            .take(128)
            .collect();
        let generation = state["document_generation"].as_u64();
        let revision = state["revision"].as_u64();
        let elapsed_ms = elapsed.as_millis();
        match self.invalid_attempts {
            1 => log::debug!("viewer guide edit rejected operation=guide_edit phase=begin root={root:?} composition={composition:?} generation={generation:?} revision={revision:?} elapsed_ms={elapsed_ms} cause={cause} count=1"),
            60 => log::error!("viewer guide edit rejected operation=guide_edit phase=begin root={root:?} composition={composition:?} generation={generation:?} revision={revision:?} elapsed_ms={elapsed_ms} cause={cause} count=60"),
            _ => {}
        }
    }
    pub(super) fn open(&mut self, point: [f32; 2], state: &Value) {
        self.drag = None;
        self.channel_dropdown.open = false;
        self.menu = Some(Menu {
            window: self.window,
            source: KeyframeSource::of(state),
            point,
            opened: true,
        });
    }
    pub(crate) fn channel(&self, floating: bool) -> motion_render::ViewChannel {
        self.channels[usize::from(floating)]
    }

    pub(super) fn current_channel(&self) -> motion_render::ViewChannel {
        self.channel(self.window.is_some())
    }

    /// Returns pointer ownership even for a rejected attempt; it cannot fall
    /// through into layer picking or a different gesture on the same press.
    pub(super) fn viewport(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        viewport: Rect,
        mapping: Mapping,
        canvas: [f64; 2],
        state: &Value,
        input: &Input,
        locale: Locale,
        allow_begin: bool,
        out: &mut Vec<Action>,
    ) -> bool {
        let styles = list.styles();
        let [top, left] = ruler_rects(viewport, &styles);
        let rulers_visible = self.visibility.rulers;
        let in_ruler = |point| {
            rulers_visible
                && viewport.contains(point)
                && (top.contains(point) || left.contains(point))
        };
        let before = self.snapshot(state);
        if identified(state) && before.is_some() {
            self.invalid_attempts = 0;
        }
        let mut consumed = self.drag.is_some()
            || input
                .pressed
                .is_some_and(|(b, p)| b == MouseButton::Left && in_ruler(p));
        let cancel = input.focus_lost
            || input
                .key_presses()
                .any(|(k, _, repeat)| k == Key::Escape && !repeat);
        if cancel
            || self.drag.as_ref().is_some_and(|s| {
                s.window == self.window
                    && (s.mapping != mapping
                        || s.viewport != viewport
                        || s.source != KeyframeSource::of(state)
                        || !matches_snapshot(&s.before, state))
            })
        {
            consumed |= self.drag.take().is_some();
        }
        if !cancel
            && allow_begin
            && !self.visibility.locked
            && self.menu.is_none()
            && self.drag.is_none()
        {
            if let Some((MouseButton::Left, point)) =
                input.pressed.filter(|(_, p)| viewport.contains(*p))
            {
                let creating = in_ruler(point);
                let hit = before.as_ref().and_then(|guides| {
                    guides
                        .iter()
                        .filter(|_| self.visibility.guides)
                        .filter_map(|guide| {
                            let index = axis_index(guide.axis);
                            let pixel = mapping.screen(if index == 0 {
                                [guide.position, 0.]
                            } else {
                                [0., guide.position]
                            })[index];
                            let distance = (pixel - point[index]).abs();
                            (distance <= styles.metric("viewer.overlay.guide.hit", 6.))
                                .then_some((guide, distance))
                        })
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                        .map(|(guide, _)| guide.clone())
                });
                if creating || hit.is_some() {
                    let began = std::time::Instant::now();
                    consumed = true;
                    if state["observing"] == true {
                        self.notice = Some(Notice::ReadOnly);
                    } else if identified(state) && before.is_some() {
                        let before = before.clone().unwrap_or_default();
                        let guide = if creating && before.len() < MAX_GUIDES {
                            let axis = if left.contains(point) {
                                GuideAxis::X
                            } else {
                                GuideAxis::Y
                            };
                            next_id(&before).map(|id| Guide {
                                id,
                                axis,
                                position: mapping.local(point)[axis_index(axis)],
                            })
                        } else if creating {
                            self.notice = Some(Notice::Limit);
                            None
                        } else {
                            hit
                        };
                        if let Some(guide) = guide {
                            self.visibility.guides = true;
                            self.drag = Some(Drag {
                                window: self.window,
                                source: KeyframeSource::of(state),
                                mapping,
                                viewport,
                                before,
                                guide,
                                creating,
                            });
                            self.invalid_attempts = 0;
                        }
                    } else {
                        self.invalid(state, began.elapsed());
                        self.notice = Some(Notice::Unavailable);
                    }
                }
            }
        }
        if let Some(mut drag) = self.drag.take() {
            // Neutral paints of another window never drive the owner's preview.
            let driving = input.pending
                || input.held == Some(MouseButton::Left)
                || input.pressed.is_some()
                || input.released.is_some();
            if drag.window == self.window && driving {
                let point = input
                    .released
                    .filter(|(b, _)| *b == MouseButton::Left)
                    .map_or(input.mouse, |(_, p)| p);
                let index = axis_index(drag.guide.axis);
                let mut position = mapping.local(point)[index];
                if !input.modifiers.alt {
                    let grid = self
                        .visibility
                        .grid
                        .then(|| {
                            grid_step(
                                mapping,
                                styles.metric("viewer.overlay.grid.spacing", 100.) as f64,
                                styles.metric("viewer.overlay.grid.min_spacing", 24.) as f64,
                            )
                        })
                        .flatten();
                    position = snap_axis(
                        position,
                        drag.guide.axis,
                        &drag.before,
                        canvas,
                        mapping,
                        styles.metric("viewer.overlay.snap.distance", 6.) as f64,
                        grid,
                        Some(drag.guide.id),
                    );
                }
                if position.is_finite() && position.abs() <= MAX_GUIDE_POSITION {
                    drag.guide.position = position;
                }
                if input.released.is_some_and(|(b, _)| b == MouseButton::Left) {
                    if viewport.contains(point)
                        && position.is_finite()
                        && position.abs() <= MAX_GUIDE_POSITION
                    {
                        let deleting = in_ruler(point);
                        let mut after = (*drag.before).clone();
                        if deleting {
                            after.retain(|g| g.id != drag.guide.id);
                        } else if drag.creating {
                            after.push(drag.guide);
                        } else if let Some(guide) = after.iter_mut().find(|g| g.id == drag.guide.id)
                        {
                            *guide = drag.guide;
                        }
                        if after.as_slice() != drag.before.as_slice() {
                            out.push(edit(state, drag.source, after));
                        }
                    }
                } else {
                    self.drag = Some(drag);
                }
            } else {
                self.drag = Some(drag);
            }
        }
        if let Some(drag) = self.drag.as_ref().filter(|d| d.window == self.window) {
            let mut shown = before.as_deref().cloned().unwrap_or_default();
            shown.retain(|g| g.id != drag.guide.id);
            shown.push(drag.guide.clone());
            self.paint(list, atlas, viewport, mapping, canvas, &shown, locale);
        } else {
            self.paint(
                list,
                atlas,
                viewport,
                mapping,
                canvas,
                before.as_deref().map_or(&[], Vec::as_slice),
                locale,
            );
        }
        consumed
    }

    pub(super) fn menu(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        bounds: Rect,
        state: &Value,
        input: &Input,
        locale: Locale,
        handles_busy: bool,
        shortcut_hint: &str,
        out: &mut Vec<Action>,
    ) {
        let Some(mut menu) = self.menu.take() else {
            return;
        };
        if menu.window != self.window {
            self.menu = Some(menu);
            return;
        }
        let styles = list.styles();
        let padding = styles.metric("padding.panel", 8.);
        let row = styles.metric("row.height", 22.);
        let width = styles
            .metric("viewer.overlay.menu.width", 232.)
            .min(bounds.width());
        let height = row * 10. + padding * 2.;
        let x = menu.point[0].clamp(bounds.min[0], (bounds.max[0] - width).max(bounds.min[0]));
        let top = (bounds.min[1]
            + if self.window.is_some() {
                styles.metric("viewer.floating.top", 30.)
            } else {
                CHROME_HEIGHT
            })
        .min(bounds.max[1]);
        let y = (menu.point[1] - height).clamp(top, (bounds.max[1] - height).max(top));
        let card = Rect::new(x, y, width, height);
        let opened = menu.opened;
        menu.opened = false;
        let neutral = Input::default();
        let events = if opened || input.composing || input.ime_consumed {
            &neutral
        } else {
            input
        };
        if events
            .key_presses()
            .any(|(k, _, repeat)| k == Key::Escape && !repeat)
            || events.pressed.is_some_and(|(_, p)| !card.contains(p))
            || input.focus_lost
        {
            self.channel_dropdown.open = false;
            return;
        }
        let sibling_events = if self.channel_dropdown.open { &neutral } else { events };
        paint::rect(list, card, styles.color("panel"));
        paint::stroke(
            list,
            card,
            styles.color("border"),
            styles.metric("border", 1.),
        );
        let zh = locale == Locale::Zh;
        let labels = if zh {
            ["安全框", "网格", "标尺", "参考线", "编辑手柄", "锁定参考线"]
        } else {
            [
                "Safety frames",
                "Grid",
                "Rulers",
                "Guides",
                "Editing handles",
                "Lock guides",
            ]
        };
        let values = [
            &mut self.visibility.safe,
            &mut self.visibility.grid,
            &mut self.visibility.rulers,
            &mut self.visibility.guides,
            &mut self.visibility.handles,
            &mut self.visibility.locked,
        ];
        for (index, (label, value)) in labels.into_iter().zip(values).enumerate() {
            let annotated;
            let label = if index == 4 && !shortcut_hint.is_empty() {
                annotated = format!("{label}  {shortcut_hint}");
                annotated.as_str()
            } else {
                label
            };
            let rect = Rect::new(
                x + padding,
                y + padding + row * index as f32,
                width - padding * 2.,
                row,
            );
            motion_ui::ui::checkbox(
                list,
                atlas,
                rect,
                label,
                value,
                if index == 4 && handles_busy {
                    &neutral
                } else {
                    sibling_events
                },
            );
        }
        let labels = if zh {
            ["添加垂直参考线", "添加水平参考线", "清除参考线"]
        } else {
            ["Add vertical guide", "Add horizontal guide", "Clear guides"]
        };
        let snapshot = self.snapshot(state);
        for (index, label) in labels.into_iter().enumerate() {
            let rect = Rect::new(
                x + padding,
                y + padding + row * (index + 6) as f32,
                width - padding * 2.,
                row,
            );
            let enabled = identified(state)
                && snapshot.is_some()
                && state["observing"] != true
                && !self.visibility.locked
                && snapshot.as_ref().is_some_and(|s| {
                    if index == 2 {
                        !s.is_empty()
                    } else {
                        s.len() < MAX_GUIDES
                    }
                });
            let caption = if index == 2 {
                snapshot.as_ref().map_or_else(
                    || label.to_owned(),
                    |guides| format!("{label} ({}/{MAX_GUIDES})", guides.len()),
                )
            } else {
                label.to_owned()
            };
            if motion_ui::ui::button(list, atlas, rect, &caption, sibling_events, enabled).clicked {
                let mut after = snapshot.as_deref().cloned().unwrap_or_default();
                if index == 2 {
                    after.clear();
                } else {
                    let axis = if index == 0 {
                        GuideAxis::X
                    } else {
                        GuideAxis::Y
                    };
                    let canvas = if index == 0 {
                        state["project"]["width"].as_f64()
                    } else {
                        state["project"]["height"].as_f64()
                    };
                    if let Some(position) = canvas
                        .filter(|v| v.is_finite() && *v > 0. && *v <= MAX_GUIDE_POSITION)
                        .map(|v| v * 0.5)
                    {
                        if let Some(id) = next_id(&after) {
                            after.push(Guide { id, axis, position });
                        }
                    }
                }
                if snapshot.as_deref() != Some(&after) {
                    self.visibility.guides = true;
                    out.push(edit(state, menu.source.clone(), after));
                    return;
                }
            }
        }
        let channel_width = styles.metric("viewer.overlay.button.width", 64.);
        let channel_gap = styles.metric("viewer.controls.gap", 4.);
        let inspection_rect = Rect::new(
                x + padding,
                y + padding + row * 9.,
                (width - padding * 2. - channel_width - channel_gap).max(0.),
                row,
            );
        let inspection_label = match (snapshot.is_none(), zh) {
            (true, true) => "参考线不可用 · 检查…",
            (true, false) => "Guides unavailable…",
            (false, true) => "通道 · 快照与区域…",
            (false, false) => "Channel · Inspect…",
        };
        if motion_ui::ui::button(list, atlas, inspection_rect, inspection_label, sibling_events, true).clicked {
            out.push(Action::OpenViewerInspection { floating: self.window.is_some(), point: [x, y + height] });
            return;
        }
        let labels = if zh { ["彩色", "红", "绿", "蓝", "Alpha"] }
            else { ["Color", "Red", "Green", "Blue", "Alpha"] };
        let items: Vec<_> = labels.iter().map(|label| motion_ui::widgets::DropdownItem { label, enabled: true }).collect();
        let channel_index = usize::from(self.window.is_some());
        let channels = [motion_render::ViewChannel::Color, motion_render::ViewChannel::Red,
            motion_render::ViewChannel::Green, motion_render::ViewChannel::Blue, motion_render::ViewChannel::Alpha];
        let selected = channels.iter().position(|channel| *channel == self.channels[channel_index]).unwrap_or(0);
        let channel_rect = Rect::new(x + width - padding - channel_width, y + padding + row * 9., channel_width, row);
        if let Some(index) = self.channel_dropdown.draw(list, atlas, channel_rect, card, &items, selected, events, true, false) {
            if let Some(channel) = channels.get(index) { self.channels[channel_index] = *channel; }
        }
        self.menu = Some(menu);
    }

    fn paint(
        &self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        viewport: Rect,
        mapping: Mapping,
        canvas: [f64; 2],
        guides: &[Guide],
        _locale: Locale,
    ) {
        let styles = list.styles();
        let width = styles.metric("viewer.overlay.line.width", 1.);
        let mut layer = list.child();
        if self.visibility.safe {
            for fraction in [0.05, 0.1] {
                let a = mapping.screen(canvas.map(|v| v * fraction));
                let b = mapping.screen(canvas.map(|v| v * (1. - fraction)));
                paint::stroke(
                    &mut layer,
                    Rect { min: a, max: b },
                    styles.color("viewer.safe"),
                    width,
                );
            }
        }
        let step = grid_step(
            mapping,
            styles.metric("viewer.overlay.grid.spacing", 100.) as f64,
            styles.metric("viewer.overlay.grid.min_spacing", 24.) as f64,
        );
        if self.visibility.grid {
            if let Some(step) = step {
                for axis in 0..2 {
                    for value in ticks(mapping, viewport.intersection(mapping.image), axis, step) {
                        let mut a = mapping.screen([0., 0.]);
                        let mut b = mapping.screen(canvas);
                        a[axis] =
                            mapping.screen(if axis == 0 { [value, 0.] } else { [0., value] })[axis];
                        b[axis] = a[axis];
                        paint::line_with_width(
                            &mut layer,
                            vec![a, b],
                            styles.color("viewer.grid"),
                            width,
                        );
                    }
                }
            }
        }
        if self.visibility.guides {
            for guide in guides {
                let index = axis_index(guide.axis);
                let value = mapping.screen(if index == 0 {
                    [guide.position, 0.]
                } else {
                    [0., guide.position]
                })[index];
                let (mut a, mut b) = (viewport.min, viewport.max);
                a[index] = value;
                b[index] = value;
                paint::line_with_width(&mut layer, vec![a, b], styles.color("viewer.guide"), width);
            }
        }
        if self.visibility.rulers {
            let ruler = styles.metric("viewer.overlay.ruler.height", 22.);
            let ruler_width = styles.metric("viewer.overlay.ruler.width", 48.);
            let tick = styles.metric("viewer.overlay.ruler.tick", 6.);
            paint::rect(
                &mut layer,
                Rect::new(viewport.min[0], viewport.min[1], viewport.width(), ruler),
                styles.color("panel"),
            );
            paint::rect(
                &mut layer,
                Rect::new(
                    viewport.min[0],
                    viewport.min[1],
                    ruler_width,
                    viewport.height(),
                ),
                styles.color("panel"),
            );
            let base = styles.metric("viewer.overlay.ruler.base", 1.) as f64;
            let ruler_step = grid_step(
                mapping,
                base,
                styles.metric("viewer.overlay.grid.min_spacing", 24.) as f64,
            );
            let label_step = grid_step(
                mapping,
                base,
                styles.metric("viewer.overlay.ruler.label.spacing", 64.) as f64,
            );
            if let (Some(step), Some(label_step)) = (ruler_step, label_step) {
                for axis in 0..2 {
                    let mut labels = list.child();
                    for value in ticks(mapping, viewport, axis, step) {
                        let point =
                            mapping.screen(if axis == 0 { [value, 0.] } else { [0., value] });
                        let cross = 1 - axis;
                        let mut a = point;
                        let mut b = point;
                        let extent = if axis == 0 { ruler } else { ruler_width };
                        a[cross] = viewport.min[cross] + extent - tick;
                        b[cross] = viewport.min[cross] + extent;
                        paint::line_with_width(
                            &mut layer,
                            vec![a, b],
                            styles.color("muted"),
                            width,
                        );
                        if (value / label_step - (value / label_step).round()).abs() < 1e-6 {
                            let rect = if axis == 0 {
                                Rect::new(
                                    point[0],
                                    viewport.min[1],
                                    label_step as f32 * mapping.scale as f32,
                                    ruler - tick,
                                )
                            } else {
                                Rect::new(viewport.min[0], point[1], ruler_width - tick, ruler)
                            };
                            let caption = if value.abs() >= 10_000. {
                                format!("{value:.0e}")
                            } else {
                                format!("{value:.0}")
                            };
                            motion_ui::ui::text_sized(
                                &mut labels,
                                atlas,
                                rect,
                                &caption,
                                styles.color("muted"),
                                Align::Left,
                                styles.metric("font.caption", 10.),
                            );
                        }
                    }
                    let strip = if axis == 0 {
                        Rect::new(
                            viewport.min[0] + ruler_width,
                            viewport.min[1],
                            (viewport.width() - ruler_width).max(0.),
                            ruler,
                        )
                    } else {
                        Rect::new(
                            viewport.min[0],
                            viewport.min[1] + ruler,
                            ruler_width,
                            (viewport.height() - ruler).max(0.),
                        )
                    };
                    labels.clip_to(strip);
                    layer.append(labels);
                }
            }
        }
        layer.clip_to(viewport);
        list.append(layer);
    }
}

#[cfg(all(test, target_os = "windows"))]
mod native_tests;
#[cfg(test)]
mod tests;
fn ruler_rects(viewport: Rect, styles: &motion_ui::style::StyleSheet) -> [Rect; 2] {
    [
        Rect::new(
            viewport.min[0],
            viewport.min[1],
            viewport.width(),
            styles.metric("viewer.overlay.ruler.height", 22.),
        ),
        Rect::new(
            viewport.min[0],
            viewport.min[1],
            styles.metric("viewer.overlay.ruler.width", 48.),
            viewport.height(),
        ),
    ]
}
#[cfg(test)]
mod worker_tests;
