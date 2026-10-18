//! A temporary view of a dock, independent of persistent panel ownership.
use super::*;

#[cfg(test)]
mod tests;

impl Workspace {
    pub fn display_root(&self) -> &dock::Node {
        self.maximized
            .and_then(|id| self.layout.root.find(id))
            .unwrap_or(&self.layout.root)
    }

    pub fn maximized_panel(&self) -> Option<DockId> {
        self.maximized
            .filter(|id| self.layout.root.find(*id).is_some())
    }

    pub fn restore_panel(&mut self) -> bool {
        self.maximized.take().is_some()
    }

    pub fn toggle_panel_maximize(&mut self, panel: DockId) -> bool {
        if self.restore_panel() {
            return true;
        }
        if !matches!(self.layout.root.find(panel), Some(dock::Node::Dock { .. })) {
            return false;
        }
        self.layout.cancel();
        self.maximized = Some(panel);
        true
    }
}
