//! Window, panel and widget scopes prevent focus and pointer events leaking across panels.
use crate::{
    dock::DockId,
    input::{Input, MouseButton, Rect},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Target {
    pub window: u64,
    pub panel: DockId,
    pub widget: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct HitRegion {
    pub target: Target,
    pub rect: Rect,
    pub clip: Rect,
    pub layer: u32,
    pub keyboard: bool,
}
#[derive(Default)]
pub struct Router {
    focus: Option<Target>,
    previous_focus: Option<Target>,
    capture: Option<Target>,
    hover: Option<Target>,
    modal: Option<(u64, DockId)>,
    window: Option<u64>,
    pending_focus_lost: Vec<Target>,
}
impl Router {
    pub fn focus(&self) -> Option<Target> {
        self.focus
    }
    pub fn captured(&self) -> Option<Target> {
        self.capture
    }
    pub fn set_modal(&mut self, owner: Option<(u64, DockId)>) {
        self.modal = owner;
        if let Some((window, panel)) = owner {
            if self
                .focus
                .is_some_and(|t| t.window != window || t.panel != panel)
            {
                self.pending_focus_lost.extend(self.focus);
                self.focus = None;
            }
            if self
                .capture
                .is_some_and(|t| t.window != window || t.panel != panel)
            {
                self.pending_focus_lost.extend(self.capture);
                self.capture = None;
            }
        }
    }
    pub fn begin_frame(&mut self, window: u64, input: &Input, regions: &[HitRegion]) {
        self.window = Some(window);
        self.previous_focus = self.focus;
        let modal = self.modal;
        let permitted = |t: Target| modal.is_none_or(|(w, p)| t.window == w && t.panel == p);
        let hit = |point| {
            regions
                .iter()
                .filter(|r| {
                    r.target.window == window
                        && permitted(r.target)
                        && r.rect.contains(point)
                        && r.clip.contains(point)
                })
                .max_by_key(|r| r.layer)
                .copied()
        };
        self.hover = hit(input.mouse).map(|r| r.target);
        if input.focus_lost {
            if self.capture.is_some_and(|t| t.window == window) {
                self.capture = None;
            }
            if self.focus.is_some_and(|t| t.window == window) {
                self.focus = None;
            }
            return;
        }
        if let Some((MouseButton::Left, point)) = input.pressed {
            let owner = hit(point);
            self.capture = owner.map(|r| r.target);
            self.focus = owner.filter(|r| r.keyboard).map(|r| r.target);
        }
        let tab = input.key_presses().find(|(key, modifiers, repeat)| {
            *key == crate::Key::Tab && !repeat && !modifiers.control && !modifiers.alt
        });
        if let Some((_, modifiers, _)) = tab {
            let mut targets: Vec<_> = regions
                .iter()
                .filter(|r| {
                    r.keyboard
                        && r.target.window == window
                        && permitted(r.target)
                        && r.rect.intersection(r.clip).width() > 0.
                        && r.rect.intersection(r.clip).height() > 0.
                })
                .map(|r| r.target)
                .collect();
            // A widget can contribute several non-adjacent hit regions.
            let mut seen = std::collections::HashSet::with_capacity(targets.len());
            targets.retain(|target| seen.insert(*target));
            if !targets.is_empty() {
                let current = self
                    .focus
                    .and_then(|t| targets.iter().position(|p| *p == t));
                let next = if modifiers.shift {
                    current.map_or(targets.len() - 1, |i| {
                        (i + targets.len() - 1) % targets.len()
                    })
                } else {
                    current.map_or(0, |i| (i + 1) % targets.len())
                };
                self.focus = Some(targets[next]);
            }
        }
    }
    pub fn input_for(&self, target: Target, input: &Input) -> Input {
        let mut routed = input.clone();
        let permitted = self.window == Some(target.window)
            && self
                .modal
                .is_none_or(|(w, p)| target.window == w && target.panel == p);
        let pointer = permitted
            && (self.capture == Some(target)
                || self.capture.is_none() && self.hover == Some(target));
        if !pointer {
            routed.pressed = None;
            routed.released = None;
            routed.held = None;
            routed.wheel = [0.; 2];
            routed.long_press = None;
        }
        if !permitted || self.focus != Some(target) {
            routed.keys.clear();
            routed.text.clear();
        }
        routed.focus_lost = (input.focus_lost && self.window == Some(target.window))
            || (self.previous_focus == Some(target) && self.focus != Some(target))
            || self.pending_focus_lost.contains(&target);
        routed
    }
    pub fn end_frame(&mut self, input: &Input) {
        if self.capture.is_some_and(|target| Some(target.window) == self.window)
            && (input.released.is_some_and(|(button, _)| button == MouseButton::Left)
                || input.focus_lost)
        {
            self.capture = None;
        }
        let window = self.window;
        self.pending_focus_lost.retain(|t| Some(t.window) != window);
    }
    pub fn forget_panel(&mut self, window: u64, panel: DockId) -> Option<Target> {
        let capture = self
            .capture
            .filter(|t| t.window == window && t.panel == panel);
        if capture.is_some() {
            self.capture = None;
        }
        if self
            .focus
            .is_some_and(|t| t.window == window && t.panel == panel)
        {
            self.focus = None;
        }
        capture
    }
}
