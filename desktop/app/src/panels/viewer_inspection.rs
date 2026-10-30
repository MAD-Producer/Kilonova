//! Viewer-only inspection state; none of these controls edits the project.
use super::*;
use crate::viewer_navigation::Mapping;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Identity {
    root: String,
    generation: u64,
    surface: u64,
    composition: String,
}

pub(crate) fn identity(state: &Value) -> Option<Identity> {
    Some(Identity {
        root: state["root"].as_str()?.to_owned(),
        generation: state["document_generation"].as_u64()?,
        surface: state["surfaceEpoch"].as_u64()?,
        composition: state["composition"].as_str()
            .or_else(|| state["project"]["composition_id"].as_str())?.to_owned(),
    })
}

#[derive(Default)]
struct Slot {
    captured: bool,
    busy: bool,
    compare: bool,
    region: Option<[f32; 4]>,
}
struct Menu { window: Option<Panel>, point: [f32; 2], opened: bool, scroll: motion_ui::widgets::ScrollArea }
struct Drag { window: Option<Panel>, mapping: Mapping, canvas: [f64; 2], start: [f32; 2], current: [f32; 2] }

#[derive(Default)]
pub(crate) struct Controls {
    identity: Option<Identity>,
    slots: [Slot; 2],
    window: Option<Panel>,
    menu: Option<Menu>,
    armed: Option<Option<Panel>>,
    drag: Option<Drag>,
    raw_input: Option<Input>,
}

impl Controls {
    fn slot(&self) -> usize { usize::from(self.window.is_some()) }
    pub(super) fn selecting(&self) -> bool {
        self.armed == Some(self.window) || self.drag.as_ref().is_some_and(|d| d.window == self.window)
    }
    pub(super) fn selection_input<'a>(&'a self, fallback: &'a Input) -> &'a Input {
        if self.selecting() { self.raw_input.as_ref().unwrap_or(fallback) } else { fallback }
    }
    pub(crate) fn synchronize(&mut self, state: &Value) {
        let next = identity(state);
        if self.identity != next {
            self.identity = next;
            self.slots = Default::default();
            self.cancel();
        }
    }
    pub(crate) fn cancel(&mut self) { self.menu = None; self.armed = None; self.drag = None; }
    pub(crate) fn prepare(&mut self, window: Option<Panel>, state: &Value, input: &Input) -> bool {
        self.synchronize(state);
        self.window = window;
        self.raw_input = Some(input.clone());
        let owner = self.menu.as_ref().map(|m| m.window)
            .or_else(|| self.drag.as_ref().map(|d| d.window)).or(self.armed);
        let event = input.pending || input.pressed.is_some() || input.released.is_some()
            || !input.keys.is_empty() || !input.text.is_empty() || input.wheel != [0., 0.];
        if input.focus_lost && owner == Some(window) || owner.is_some() && owner != Some(window) && event {
            self.cancel();
            return event;
        }
        if owner == Some(window) && self.menu.is_none() && !input.keys.is_empty() {
            self.armed = None;
            self.drag = None;
            return true;
        }
        owner == Some(window)
    }
    pub(crate) fn open(&mut self, floating: bool, point: [f32; 2], state: &Value) {
        self.synchronize(state);
        self.armed = None;
        self.drag = None;
        self.menu = Some(Menu { window: floating.then_some(Panel::Composition), point, opened: true, scroll: Default::default() });
    }
    pub(crate) fn frame(&self, floating: bool, channel: motion_render::ViewChannel) -> crate::engine::ViewerInspectionFrame {
        let slot = &self.slots[usize::from(floating)];
        crate::engine::ViewerInspectionFrame { channel, region: slot.region,
            comparison_split: (slot.captured && slot.compare).then_some(0.5) }
    }
    pub(crate) fn set_busy(&mut self, floating: bool, busy: bool) { self.slots[usize::from(floating)].busy = busy; }
    pub(crate) fn capture_done(&mut self, floating: bool, source: &Identity, success: bool) {
        if self.identity.as_ref() != Some(source) { return; }
        let slot = &mut self.slots[usize::from(floating)];
        slot.busy = false;
        if success { slot.captured = true; }
    }
    pub(crate) fn save_done(&mut self, floating: bool, source: &Identity) {
        if self.identity.as_ref() == Some(source) { self.slots[usize::from(floating)].busy = false; }
    }
    pub(crate) fn close_floating(&mut self) { self.slots[1] = Slot::default(); if self.window.is_some() { self.cancel(); } }

    pub(super) fn menu(&mut self, list: &mut PaintList, atlas: &mut TextAtlas, bounds: Rect,
        state: &Value, input: &Input, locale: Locale, out: &mut Vec<Action>) {
        let Some(mut menu) = self.menu.take() else { return };
        if menu.window != self.window { self.menu = Some(menu); return; }
        let styles = list.styles();
        let padding = styles.metric("padding.panel", 8.);
        let row = styles.metric("row.height", 22.);
        let width = styles.metric("viewer.overlay.menu.width", 232.).min(bounds.width());
        let top = bounds.min[1] + if self.window.is_some() { styles.metric("viewer.floating.top", 30.) } else { CHROME_HEIGHT };
        let height = (row * 6. + padding * 2.).min((bounds.max[1] - top).max(0.));
        let x = menu.point[0].clamp(bounds.min[0], (bounds.max[0] - width).max(bounds.min[0]));
        let y = (menu.point[1] - height).clamp(top.min(bounds.max[1]), (bounds.max[1] - height).max(top.min(bounds.max[1])));
        let card = Rect::new(x, y, width, height);
        let neutral = Input::default();
        let events = if menu.opened || input.composing || input.ime_consumed { &neutral } else { input };
        menu.opened = false;
        if input.focus_lost || events.key_presses().any(|(key, _, repeat)| key == Key::Escape && !repeat)
            || events.pressed.is_some_and(|(_, point)| !card.contains(point)) { return; }
        paint::rect(list, card, styles.color("panel"));
        paint::stroke(list, card, styles.color("border"), styles.metric("border", 1.));
        let body = Rect::new(x + padding, y + padding, (width - padding * 2.).max(0.), (height - padding * 2.).max(0.));
        let mut content = list.child();
        let scroll = menu.scroll.begin(&mut content, body, row * 6., events, true);
        let events = &scroll.input;
        let zh = locale == Locale::Zh;
        let floating = self.window.is_some();
        let slot = self.slot();
        let row_rect = |index: usize| Rect::new(scroll.content.min[0], scroll.content.min[1] + row * index as f32,
            scroll.content.width(), row);
        let enabled = identity(state).is_some() && !self.slots[slot].busy;
        let mut close = false;
        if motion_ui::ui::button(&mut content, atlas, row_rect(0), if zh { "捕获快照" } else { "Capture snapshot" }, events, enabled).clicked {
            out.push(Action::CaptureViewerSnapshot { floating }); close = true;
        }
        if motion_ui::ui::button(&mut content, atlas, row_rect(1), if zh { "保存快照为 PNG…" } else { "Save snapshot as PNG…" }, events,
            enabled && self.slots[slot].captured).clicked {
            out.push(Action::SaveViewerSnapshot { floating }); close = true;
        }
        if motion_ui::widgets::Checkbox::draw(&mut content, atlas, row_rect(2), if zh { "比较快照" } else { "Compare snapshot" },
            self.slots[slot].compare, events, enabled && self.slots[slot].captured, false, None).clicked {
            self.slots[slot].compare = !self.slots[slot].compare;
        }
        if motion_ui::ui::button(&mut content, atlas, row_rect(3), if zh { "选择预览区域" } else { "Select preview region" }, events, enabled).clicked {
            self.armed = Some(self.window); close = true;
        }
        if motion_ui::ui::button(&mut content, atlas, row_rect(4), if zh { "清除预览区域" } else { "Clear preview region" }, events,
            enabled && self.slots[slot].region.is_some()).clicked {
            self.slots[slot].region = None;
        }
        label(&mut content, atlas, row_rect(5), if zh { "区域仅影响预览；导出完整画面" } else { "Region affects preview; export is full" }, false);
        content.clip_to(body);
        list.append(content);
        if !close { self.menu = Some(menu); }
    }

    pub(super) fn viewport(&mut self, list: &mut PaintList, viewport: Rect, mapping: Mapping,
        canvas: [f64; 2], input: &Input) -> bool {
        let active = self.armed == Some(self.window) || self.drag.as_ref().is_some_and(|d| d.window == self.window);
        if active && (input.focus_lost || input.key_presses().any(|(key, _, repeat)| key == Key::Escape && !repeat)) {
            self.armed = None; self.drag = None; return true;
        }
        if self.drag.as_ref().is_some_and(|d| d.window == self.window && (d.mapping != mapping || d.canvas != canvas)) {
            self.armed = None; self.drag = None; return true;
        }
        if self.armed == Some(self.window) {
            if let Some((MouseButton::Left, point)) = input.pressed {
                if viewport.contains(point) && mapping.image.contains(point) {
                    self.drag = Some(Drag { window: self.window, mapping, canvas, start: point, current: point });
                }
                self.armed = None;
            }
        }
        if let Some(drag) = self.drag.as_mut().filter(|drag| drag.window == self.window) {
            drag.current = input.released.map_or(input.mouse, |(_, point)| point);
        }
        let normalized = |start: [f32; 2], end: [f32; 2]| {
            let a = mapping.local(start); let b = mapping.local(end);
            let lo = [a[0].min(b[0]).clamp(0., canvas[0]), a[1].min(b[1]).clamp(0., canvas[1])];
            let hi = [a[0].max(b[0]).clamp(0., canvas[0]), a[1].max(b[1]).clamp(0., canvas[1])];
            ((hi[0] > lo[0]) && (hi[1] > lo[1])).then_some([
                (lo[0]/canvas[0]) as f32, (lo[1]/canvas[1]) as f32,
                ((hi[0]-lo[0])/canvas[0]) as f32, ((hi[1]-lo[1])/canvas[1]) as f32])
        };
        let region = self.drag.as_ref().filter(|d| d.window == self.window)
            .and_then(|d| normalized(d.start, d.current)).or(self.slots[self.slot()].region);
        if let Some([x,y,w,h]) = region {
            let a = mapping.screen([f64::from(x)*canvas[0], f64::from(y)*canvas[1]]);
            let b = mapping.screen([f64::from(x+w)*canvas[0], f64::from(y+h)*canvas[1]]);
            let mut overlay = list.child();
            let styles = list.styles();
            paint::stroke(&mut overlay, Rect::new(a[0], a[1], b[0]-a[0], b[1]-a[1]),
                styles.color("viewer.guide"), styles.metric("viewer.overlay.line.width", 1.));
            overlay.clip_to(viewport); list.append(overlay);
        }
        if self.drag.as_ref().is_some_and(|d| d.window == self.window)
            && input.released.is_some_and(|(button, _)| button == MouseButton::Left) {
            if let Some(drag) = self.drag.take() {
                if let Some(region) = normalized(drag.start, drag.current) {
                    let index = self.slot(); self.slots[index].region = Some(region);
                }
            }
        }
        active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> Value {
        serde_json::json!({"root":"fixture", "document_generation":1, "surfaceEpoch":2,
            "composition":"comp-main", "revision":4, "project":{"width":200,"height":100}})
    }
    fn pointer(point: [f32; 2], release: bool) -> Input {
        Input { mouse: point, pressed: (!release).then_some((MouseButton::Left, point)),
            released: release.then_some((MouseButton::Left, point)), held: (!release).then_some(MouseButton::Left),
            ..Default::default() }
    }
    #[test]
    fn region_selection_uses_composition_coordinates_and_cancel_preserves_previous_region() {
        let source = state();
        let mut controls = Controls::default();
        controls.prepare(None, &source, &Input::default());
        let viewport = Rect::new(0., 0., 200., 100.);
        let mapping = crate::viewer_navigation::Navigation::default().mapping(viewport, [200., 100.]).unwrap();
        let mut paint = PaintList::default();
        controls.armed = Some(None);
        assert!(controls.viewport(&mut paint, viewport, mapping, [200., 100.], &pointer([50.,25.], false)));
        assert!(controls.viewport(&mut paint, viewport, mapping, [200., 100.], &pointer([150.,75.], true)));
        assert_eq!(controls.frame(false, motion_render::ViewChannel::Color).region, Some([0.25,0.25,0.5,0.5]));
        controls.armed = Some(None);
        controls.viewport(&mut paint, viewport, mapping, [200., 100.], &pointer([10.,10.], false));
        let cancelled = Input { focus_lost: true, ..Default::default() };
        assert!(controls.viewport(&mut paint, viewport, mapping, [200., 100.], &cancelled));
        assert_eq!(controls.frame(false, motion_render::ViewChannel::Color).region, Some([0.25,0.25,0.5,0.5]));
        assert!(controls.drag.is_none());
    }
    #[test]
    fn snapshots_are_window_owned_and_stale_completion_does_not_reenable_comparison() {
        let mut source = state();
        let mut controls = Controls::default();
        controls.prepare(None, &source, &Input::default());
        let old = identity(&source).unwrap();
        controls.set_busy(false, true);
        controls.capture_done(false, &old, true);
        controls.slots[0].compare = true;
        assert_eq!(controls.frame(false, motion_render::ViewChannel::Red).comparison_split, Some(0.5));
        assert_eq!(controls.frame(true, motion_render::ViewChannel::Red).comparison_split, None);
        source["revision"] = 9.into();
        controls.synchronize(&source);
        assert_eq!(controls.frame(false, motion_render::ViewChannel::Red).comparison_split, Some(0.5));
        source["surfaceEpoch"] = 3.into();
        controls.synchronize(&source);
        controls.capture_done(false, &old, true);
        assert_eq!(controls.frame(false, motion_render::ViewChannel::Red).comparison_split, None);
        assert!(!controls.slots[0].captured);
    }
    #[test]
    fn floating_input_cancels_pending_main_region_without_mutating_it() {
        let source = state();
        let mut controls = Controls::default();
        controls.prepare(None, &source, &Input::default());
        controls.armed = Some(None);
        let event = Input { pending: true, pressed: Some((MouseButton::Left, [4.,4.])), ..Default::default() };
        assert!(controls.prepare(Some(Panel::Composition), &source, &event));
        assert!(controls.armed.is_none());
        assert_eq!(controls.frame(false, motion_render::ViewChannel::Color).region, None);
        assert_eq!(controls.frame(true, motion_render::ViewChannel::Color).region, None);
    }
    #[test]
    fn selecting_owns_pointer_until_release_outside_viewer() {
        let source = state();
        let mut controls = Controls::default();
        controls.prepare(None, &source, &Input::default());
        controls.armed = Some(None);
        let viewport = Rect::new(0., 0., 200., 100.);
        let mapping = crate::viewer_navigation::Navigation::default().mapping(viewport, [200., 100.]).unwrap();
        let mut paint = PaintList::default();
        let press = pointer([50., 25.], false);
        assert!(controls.prepare(None, &source, &press));
        assert_eq!(controls.selection_input(&Input::default()).pressed, press.pressed);
        assert!(controls.viewport(&mut paint, viewport, mapping, [200., 100.], &press));
        let release = pointer([250., 150.], true);
        assert!(controls.prepare(None, &source, &release));
        assert_eq!(controls.selection_input(&Input::default()).released, release.released);
        assert!(controls.viewport(&mut paint, viewport, mapping, [200., 100.], &release));
        assert_eq!(controls.frame(false, motion_render::ViewChannel::Color).region, Some([0.25, 0.25, 0.75, 0.75]));
        assert!(!controls.prepare(None, &source, &Input::default()));
    }
}
