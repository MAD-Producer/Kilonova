//! Local draft history. Each input pass (including a whole cut/paste) is one edit.
use crate::text_clipboard::MAX_BYTES;
use std::collections::VecDeque;

#[derive(Clone)]
pub(super) struct Snapshot {
    pub text: String,
    pub cursor: usize,
    pub select_all: bool,
}
#[derive(Clone, Default)]
struct Stack {
    entries: VecDeque<Snapshot>,
    bytes: usize,
}
impl Stack {
    fn push(&mut self, snapshot: Snapshot) {
        self.bytes += snapshot.text.len();
        self.entries.push_back(snapshot);
        while self.bytes > MAX_BYTES || self.entries.len() > 128 {
            if let Some(old) = self.entries.pop_front() {
                self.bytes -= old.text.len();
            }
        }
    }
    fn pop(&mut self) -> Option<Snapshot> {
        let snapshot = self.entries.pop_back()?;
        self.bytes -= snapshot.text.len();
        Some(snapshot)
    }
}
#[derive(Clone, Default)]
pub(super) struct History {
    undo: Stack,
    redo: Stack,
}
impl History {
    pub fn can_undo(&self) -> bool {
        !self.undo.entries.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.entries.is_empty()
    }
    pub fn record(&mut self, before: Snapshot) {
        self.redo = Stack::default();
        self.undo.push(before);
    }
    pub fn undo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let before = self.undo.pop()?;
        self.redo.push(current);
        Some(before)
    }
    pub fn redo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let after = self.redo.pop()?;
        self.undo.push(current);
        Some(after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot(text: String) -> Snapshot {
        Snapshot {
            text,
            cursor: 0,
            select_all: false,
        }
    }
    #[test]
    fn history_bounds_content_and_entry_count_and_discards_redo_after_a_new_edit() {
        let mut history = History::default();
        for _ in 0..5 {
            history.record(snapshot("x".repeat(MAX_BYTES / 4)));
        }
        assert_eq!(history.undo.entries.len(), 4);
        assert_eq!(history.undo.bytes, MAX_BYTES);
        let before = history.undo(snapshot("current".into())).unwrap();
        assert_eq!(before.text.len(), MAX_BYTES / 4);
        assert_eq!(history.redo.entries.len(), 1);
        history.record(snapshot("next".into()));
        assert!(history.redo.entries.is_empty());
        for i in 0..200 {
            history.record(snapshot(i.to_string()));
        }
        assert_eq!(history.undo.entries.len(), 128);
        let last = history.undo(snapshot(String::new())).unwrap();
        assert_eq!(last.text, "199");
    }
}
