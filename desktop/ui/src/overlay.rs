//! Popup and modal dismissal requests. The host decides whether unsaved work may close.
use crate::{Input, Key, Rect};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Popup,
    Modal,
}
#[derive(Clone, Copy, Debug)]
pub struct Overlay {
    pub id: u64,
    pub window: u64,
    pub area: Rect,
    pub kind: Kind,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DismissReason {
    Escape,
    OutsidePress,
}
#[derive(Default)]
pub struct Overlays {
    stack: Vec<Overlay>,
}
impl Overlays {
    pub fn open(&mut self, overlay: Overlay) {
        self.stack.retain(|o| o.id != overlay.id);
        self.stack.push(overlay);
    }
    pub fn top(&self, window: u64) -> Option<Overlay> {
        self.stack
            .iter()
            .rev()
            .find(|o| o.window == window)
            .copied()
    }
    pub fn close(&mut self, id: u64) {
        self.stack.retain(|o| o.id != id);
    }
    pub fn blocks_background(&self, window: u64) -> bool {
        self.top(window).is_some() || self.stack.iter().any(|o| o.kind == Kind::Modal)
    }
    pub fn dismiss_request(&self, window: u64, input: &Input) -> Option<(u64, DismissReason)> {
        let top = self.top(window)?;
        if input.keys.iter().any(|(k, _)| *k == Key::Escape) {
            return Some((top.id, DismissReason::Escape));
        }
        if top.kind == Kind::Popup && input.pressed.is_some_and(|(_, p)| !top.area.contains(p)) {
            return Some((top.id, DismissReason::OutsidePress));
        }
        None
    }
    pub fn forget_window(&mut self, window: u64) {
        self.stack.retain(|o| o.window != window);
    }
}
