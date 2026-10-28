//! Menu commands use the same draft operations as keys, bound to one edit session.
use super::TextEditor;
use crate::{input::KeyChord, text_clipboard::TextClipboard, Input, Key};
use std::sync::{Arc, Weak};

#[derive(Default)]
pub(super) struct Session(Arc<()>);
impl Clone for Session {
    fn clone(&self) -> Self {
        Self::default()
    }
}
#[derive(Clone, Debug)]
pub struct TextSession(Weak<()>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextCommand {
    Undo,
    Redo,
    Copy,
    Cut,
    Paste,
    SelectAll,
}
impl TextCommand {
    /// Fixed draft-editor commands are independent of document key remapping.
    /// Menu hints and programmatic dispatch share this catalogue.
    pub const fn binding(self) -> (KeyChord, &'static str) {
        let (key, hint) = match self {
            Self::Undo => (Key::Z, "Ctrl+Z"),
            Self::Redo => (Key::Y, "Ctrl+Y"),
            Self::Copy => (Key::C, "Ctrl+C"),
            Self::Cut => (Key::X, "Ctrl+X"),
            Self::Paste => (Key::V, "Ctrl+V"),
            Self::SelectAll => (Key::A, "Ctrl+A"),
        };
        (KeyChord::new(key, true, false, false), hint)
    }
}

impl TextEditor {
    pub fn session(&self) -> Option<TextSession> {
        self.active()
            .then(|| TextSession(Arc::downgrade(&self.session.0)))
    }
    pub fn owns_session(&self, session: &TextSession) -> bool {
        self.active() && Weak::ptr_eq(&Arc::downgrade(&self.session.0), &session.0)
    }
    pub fn command_enabled(&self, command: TextCommand) -> bool {
        self.active()
            && match command {
                TextCommand::Undo => self.history.can_undo(),
                TextCommand::Redo => self.history.can_redo(),
                TextCommand::Copy | TextCommand::Cut => self.selected_text().is_some(),
                TextCommand::Paste => true,
                TextCommand::SelectAll => !self.text.is_empty(),
            }
    }
    pub fn command(&mut self, command: TextCommand, clipboard: Option<&Arc<TextClipboard>>) {
        if !self.command_enabled(command) {
            return;
        }
        let (chord, _) = command.binding();
        self.input(&Input {
            text_clipboard: clipboard.cloned(),
            keys: vec![(chord.key, chord.modifiers)],
            ..Default::default()
        });
    }
}
