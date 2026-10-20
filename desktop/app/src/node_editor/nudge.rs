//! Keyboard layout movement shares one undo snapshot until the arrows release.
use super::*;
use motion_ui::input::Key;

impl NodeEditor {
    pub(super) fn settle_nudge(&mut self, input: &Input) {
        let nudge = |key, modifiers| matches!(self.shortcut_bindings.resolve(
            crate::shortcuts::Scope::Nodes, key, modifiers, false),
            Some(crate::shortcuts::Command::MoveLeft | crate::shortcuts::Command::MoveRight
                | crate::shortcuts::Command::MoveUp | crate::shortcuts::Command::MoveDown));
        if input
            .key_presses()
            .any(|(key, _, repeat)| key == Key::Escape && !repeat)
        {
            return; // keyboard() rolls the gesture back on Escape
        }
        if self.nudge_before.is_some()
            && (!self.focus
                || input.pressed.is_some()
                || input.released_keys.iter().copied().any(|key|nudge(key,input.modifiers))
                || (!input.held_keys.iter().copied().any(|key|nudge(key,input.modifiers))
                    && !input.key_presses().any(|(key, modifiers, _)| nudge(key,modifiers)))
                || input
                    .key_presses()
                    .any(|(key, modifiers, _)| !nudge(key,modifiers) && key != Key::Escape))
        {
            self.finish_nudge();
        }
    }

    pub(super) fn finish_nudge(&mut self) {
        if let Some(before) = self.nudge_before.take() {
            if self.document != before.document {
                self.finish_edit(before);
            }
        }
    }

    pub(super) fn nudge_selected(&mut self, delta: [f32; 2]) {
        if self.nudge_before.is_none() {
            self.nudge_before = Some(self.state(false));
        }
        let mut changed = false;
        for node in &mut self.document.nodes {
            if self.selected.contains(&node.id) {
                let position = [
                    (node.position[0] + delta[0]).clamp(-1.0e6, 1.0e6),
                    (node.position[1] + delta[1]).clamp(-1.0e6, 1.0e6),
                ];
                changed |= position != node.position;
                node.position = position;
            }
        }
        if changed {
            // Layout-only edits do not invalidate shaders or graph semantics.
            self.revision = self.revision.wrapping_add(1);
            self.notice = None;
        }
    }
}
