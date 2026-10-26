//! Draft clipboard and history keys share the existing text editor selection.
use super::{text_history, TextEditor};
use crate::input::{InputView, Key, Modifiers};

impl TextEditor {
    pub(super) fn snapshot(&self) -> text_history::Snapshot {
        text_history::Snapshot {
            text: self.text.clone(),
            cursor: self.cursor,
            select_all: self.select_all,
            anchor: self.anchor,
        }
    }
    fn restore(&mut self, snapshot: text_history::Snapshot) {
        self.text = snapshot.text;
        self.cursor = snapshot.cursor;
        self.select_all = snapshot.select_all;
        self.anchor = snapshot.anchor;
        self.error = None;
    }
    pub(super) fn checkpoint(&mut self, before: &mut text_history::Snapshot) {
        if self.text == before.text {
            return;
        }
        if self.text.len() > self.max_bytes
            || before.text.len() > self.max_bytes
        {
            self.restore(before.clone());
            self.draft_too_large("edit");
            return;
        }
        let previous = std::mem::replace(before, self.snapshot());
        self.history.record(previous);
        self.dirty = true;
    }
    fn clipboard_failure(
        &mut self,
        operation: &'static str,
        error: crate::text_clipboard::Error,
        observed: bool,
    ) {
        if !observed {
            log::error!("text clipboard failed operation={operation} reason={error}");
        }
        self.error = Some(error.to_string());
    }
    fn draft_too_large(&mut self, operation: &'static str) {
        if self.max_bytes==crate::text_clipboard::MAX_BYTES {
            self.clipboard_failure(operation,crate::text_clipboard::Error::TooLarge,false);
        } else {
            // A domain draft limit is explicit input validation. Keep the UI
            // feedback; repeated rejected keystrokes are not a host fault.
            log::debug!("text draft rejected operation={operation} byte_limit={}",self.max_bytes);
            self.error=Some(format!("Text exceeds the {} byte draft limit",self.max_bytes));
        }
    }
    pub(super) fn draft_key(
        &mut self,
        input: &InputView<'_>,
        key: Key,
        mods: Modifiers,
        repeat: bool,
        before: &mut text_history::Snapshot,
    ) -> bool {
        if mods.control
            && !mods.alt
            && matches!(key, Key::Z | Key::Y)
            && !(key == Key::Y && mods.shift)
        {
            if repeat {
                return true;
            }
            self.checkpoint(before);
            let current = self.snapshot();
            let restored = if key == Key::Y || mods.shift {
                self.history.redo(current)
            } else {
                self.history.undo(current)
            };
            if let Some(restored) = restored {
                self.restore(restored);
                self.dirty = true;
            }
            *before = self.snapshot();
            return true;
        }
        if mods.control && !mods.shift && !mods.alt && matches!(key, Key::C | Key::X | Key::V) {
            if repeat {
                return true;
            }
            if key != Key::V && self.selected_text().is_none() {
                return true;
            }
            let Some(clipboard) = input.text_clipboard else {
                self.clipboard_failure("access", crate::text_clipboard::Error::Unavailable, false);
                return true;
            };
            if key == Key::V {
                match clipboard.read() {
                    Ok(Some(text)) => {
                        let text = self.filtered_text(&text);
                        let retained = self.text.len()-self.selection_bytes().map_or(0, |r|r.len());
                        if retained.saturating_add(text.len()) > self.max_bytes
                            || self.text.len() > self.max_bytes
                        {
                            self.draft_too_large("paste");
                            return true;
                        }
                        self.checkpoint(before);
                        if !text.is_empty() {
                            self.remove_selection();
                            let index = self.byte_index();
                            self.text.insert_str(index, &text);
                            self.place_after_byte(index + text.len());
                            self.error = None;
                            self.checkpoint(before);
                        }
                    }
                    Ok(None) => {}
                    Err(error) => self.clipboard_failure("paste", error, true),
                }
            } else {
                match clipboard.store(self.selected_text().unwrap_or("")) {
                    Ok(()) if key == Key::X => {
                        self.checkpoint(before);
                        self.remove_selection();
                        self.error = None;
                        self.checkpoint(before);
                    }
                    Ok(()) => {}
                    Err(error) => self.clipboard_failure("copy", error, true),
                }
            }
            return true;
        }
        false
    }
}
