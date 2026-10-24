//! Search fields reuse the native text editor for caret, selection and IME safety.
use motion_ui::{
    widgets::{TextEditor, TextEvent},
    Input, PaintList, Rect, TextAtlas,
};

pub(super) struct SearchQuery(TextEditor);
impl Default for SearchQuery {
    fn default() -> Self {
        let mut editor = TextEditor::default();
        editor.begin("");
        Self(editor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use motion_ui::input::{Key, Modifiers};
    fn key(key: Key) -> Input {
        Input {
            keys: vec![(key, Default::default())],
            ..Default::default()
        }
    }
    #[test]
    fn query_uses_native_selection_caret_delete_and_ime_rules() {
        let mut q = SearchQuery::default();
        q.input(&Input {
            text: "circle".into(),
            ..Default::default()
        });
        q.input(&key(Key::Home));
        q.input(&key(Key::Delete));
        assert_eq!(q, "ircle");
        q.input(&key(Key::End));
        q.input(&key(Key::Backspace));
        assert_eq!(q, "ircl");
        q.input(&Input {
            keys: vec![(
                Key::A,
                Modifiers {
                    control: true,
                    ..Default::default()
                },
            )],
            text: "圆形".into(),
            ..Default::default()
        });
        q.input(&key(Key::Left));
        q.input(&key(Key::Backspace));
        assert_eq!(q, "形");
        let mut ime = key(Key::Enter);
        ime.ime_consumed = true;
        assert!(q.input(&ime).1.is_none());
        assert!(matches!(
            q.input(&key(Key::Escape)).1,
            Some(TextEvent::Cancel)
        ));
        q.focus();
        assert!(matches!(
            q.input(&Input {
                focus_lost: true,
                ..Default::default()
            })
            .1,
            Some(TextEvent::Cancel)
        ));
    }
    #[test]
    fn long_unicode_paste_is_bounded_without_selecting_the_retained_text() {
        let mut q = SearchQuery::default();
        q.input(&Input {
            text: "圆".repeat(127),
            ..Default::default()
        });
        q.input(&Input {
            text: "形".repeat(8),
            ..Default::default()
        });
        assert_eq!(q.chars().count(), 128);
        q.input(&Input {
            text: "x".into(),
            ..Default::default()
        });
        assert_eq!(q.chars().count(), 128);
        assert!(q.starts_with("圆圆"));
    }
}
impl std::fmt::Debug for SearchQuery {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.text.fmt(f)
    }
}
impl std::ops::Deref for SearchQuery {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0.text
    }
}
impl PartialEq<&str> for SearchQuery {
    fn eq(&self, other: &&str) -> bool {
        self.0.text == *other
    }
}
impl SearchQuery {
    pub(super) fn text_menu(&mut self, request: &mut crate::panels::text_menu::Request<'_>) -> bool {
        let changed = request.visit(&mut self.0);
        if changed {
            // Keep the same query bound and caret behavior as keyboard paste.
            self.input(&Input::default());
        }
        changed
    }
    pub fn focus(&mut self) {
        self.0.begin(&self.0.text.clone());
    }
    pub fn clear(&mut self) {
        self.0.begin("");
    }
    pub fn input(&mut self, input: &Input) -> (bool, Option<TextEvent>) {
        let before = self.0.text.clone();
        let mut bounded = input.clone();
        bounded.text = bounded.text.chars().take(128).collect();
        let event = self.0.input(&bounded);
        if let Some((index, _)) = self.0.text.char_indices().nth(128) {
            let text = self.0.text[..index].to_owned();
            self.0.begin(&text);
            // Truncating a long paste leaves the caret at the retained end.
            self.0.input(&Input {
                keys: vec![(motion_ui::input::Key::End, Default::default())],
                ..Default::default()
            });
        }
        (before != self.0.text, event)
    }
    pub fn draw(&self, list: &mut PaintList, atlas: &mut TextAtlas, area: Rect) {
        self.0.draw(list, atlas, area);
    }
}
