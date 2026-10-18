//! Main-window dock projection. A native floating window already fills its panel.
use crate::*;

#[cfg(test)]
mod tests;

impl Shell {
    pub(crate) fn panel_maximize_shortcut(&mut self, size: [f32; 2]) -> bool {
        let pressed = self.input.key_presses().any(|(key, modifiers, repeat)| {
            shortcuts::resolve(shortcuts::Scope::Application, key, modifiers, repeat)
                == Some(shortcuts::Command::TogglePanelMaximize)
        });
        if !pressed
            || self.input.focus_lost
            || self.input.held.is_some()
            || self.input.pressed.is_some()
            || self.workspace.drag_cursor().is_some()
            || self.home.is_some()
            || self.dialog.is_some()
            || self.project_modal()
            || self.workspace_manager.is_some()
            || self.node_close_prompt.is_some()
            || self.node_examples.is_some()
            || panels::layout_shortcuts_blocked(&self.panels)
        {
            return false;
        }
        let frames = panels::frames(self.workspace.display_root(), size);
        let target = self.workspace.maximized_panel().or_else(|| {
            frames
                .iter()
                .find(|frame| {
                    frame.tabs.contains(self.input.mouse)
                        || frame.content.contains(self.input.mouse)
                })
                .or_else(|| {
                    frames
                        .iter()
                        .find(|frame| Some(frame.panel) == self.panels.focused)
                })
                .map(|frame| frame.panel.dock())
        });
        let Some(target) = target else { return false };
        let restoring = self.workspace.maximized_panel().is_some();
        if !self.workspace.toggle_panel_maximize(target) {
            return false;
        }
        if !restoring {
            self.panels.focused = panels::panel(target);
        }
        // A character key also carries text. It must not reach a field after reflow.
        self.input.keys.clear();
        self.input.key_repeats.clear();
        self.input.text.clear();
        self.dirty = true;
        true
    }
}
