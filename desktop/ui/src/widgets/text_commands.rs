//! Menu commands use the same draft operations as keys, bound to one edit session.
use super::TextEditor;
use crate::{input::Modifiers, text_clipboard::TextClipboard, Input, Key};
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
        let key = match command {
            TextCommand::Undo => Key::Z,
            TextCommand::Redo => Key::Y,
            TextCommand::Copy => Key::C,
            TextCommand::Cut => Key::X,
            TextCommand::Paste => Key::V,
            TextCommand::SelectAll => Key::A,
        };
        self.input(&Input {
            text_clipboard: clipboard.cloned(),
            keys: vec![(
                key,
                Modifiers {
                    control: true,
                    ..Default::default()
                },
            )],
            ..Default::default()
        });
    }
}
