//! Draft text owns clipboard/history while the graph keeps its commit boundary.
use super::*;
#[cfg(test)]
mod tests;

impl NodeEditor {
    pub(super) fn field_input(&mut self, input: &Input) {
        if !input.focus_lost && input.text.is_empty() && input.key_presses().next().is_none() {
            return;
        }
        if input.composing
            && self
                .field
                .as_ref()
                .is_some_and(|field| matches!(field.binding, Binding::InputValue(..)))
        {
            return;
        }
        let event = self.field.as_mut().and_then(|field| {
            let before = field.text.clone();
            let event = field
                .editor
                .input_value(&mut field.text, &mut field.replace, input);
            // Keep existing scrolled inline rendering; the editor owns grapheme boundaries.
            field.cursor = field.editor.caret_byte_index();
            field.replace = field.editor.is_all_selected();
            if field.text != before {
                field.error = None;
            }
            event
        });
        match event {
            Some(motion_ui::widgets::TextEvent::Cancel) => self.field = None,
            Some(motion_ui::widgets::TextEvent::Commit(_)) => {
                // Existing validation retains the field and publishes its diagnostic.
                let _ = self.commit_field();
            }
            None => {}
        }
    }
}
