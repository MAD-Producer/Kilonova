//! A focused form field owns its draft history, not the project undo stack.
use super::*;
#[cfg(test)]
mod tests;

impl Dialog {
    pub(super) fn edit_text(&mut self, input: &Input) {
        if self.text_editor_field != Some(self.focus) {
            self.text_editor.cancel();
            self.text_editor_field = Some(self.focus);
        }
        self.text_editor
            .input_value(&mut self.fields[self.focus], &mut self.replace, input);
        for (key, modifiers, repeat) in input.key_presses() {
            if key == Key::Tab && !modifiers.control && !modifiers.alt && !repeat {
                self.focus = if modifiers.shift {
                    (self.focus + self.fields.len() - 1) % self.fields.len()
                } else {
                    (self.focus + 1) % self.fields.len()
                };
                self.replace = true;
                self.text_editor.cancel();
                self.text_editor_field = None;
            }
        }
    }
}
