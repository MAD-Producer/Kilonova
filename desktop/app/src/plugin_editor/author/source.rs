use super::*;
#[path = "source_assistance.rs"]
mod assistance;
#[derive(Default)]
pub struct Source {
    editor: TextEditor,
    viewport: TextViewport,
    focus: bool,
    language: Option<lsp::Client>,
    language_attempted: bool,
    language_snapshot: Option<lsp::Snapshot>,
    language_seen: u64,
    language_error: Option<String>,
    language_synced: Option<u64>,
    language_sent: Option<Instant>,
    language_reply: Option<std::sync::Arc<lsp::Reply>>,
    language_reply_id: u64,
    definition_applied: u64,
    completion_index: usize,
    completion_scroll: usize,
    completion_open: bool,
}
impl Source {
    pub fn read_only(
        &mut self,
        text: &str,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        input: &Input,
    ) {
        if self.editor.text != text {
            self.editor.begin_multiline(text, text.len());
        }
        let mut readonly = input.clone();
        readonly.text.clear();
        readonly.composing = false;
        readonly.ime_consumed = false;
        readonly.keys.retain(|(k, m)| {
            matches!(
                k,
                Key::Left | Key::Right | Key::Up | Key::Down | Key::Home | Key::End
            ) || m.control && matches!(k, Key::A | Key::C)
        });
        readonly.key_repeats.clear();
        let sheet = list.styles();
        let colors = |_: usize, line: &str| highlight(line, &sheet);
        let _event = self.editor.multiline_view_colored(
            &mut self.viewport,
            list,
            atlas,
            area,
            &readonly,
            &colors,
        );
    }
    pub fn locate(&mut self, line: usize, column: usize) {
        let mut byte = 0;
        for (i, text) in self.editor.text.split('\n').enumerate() {
            if i + 1 == line {
                byte += text
                    .char_indices()
                    .nth(column.saturating_sub(1))
                    .map_or(text.len(), |p| p.0);
                break;
            }
            byte += text.len() + 1;
        }
        byte = byte.min(self.editor.text.len());
        self.editor.select_bytes(byte, byte);
        self.focus = true;
    }
    pub fn new(project: &PluginProject) -> Self {
        let mut value = Self::default();
        if let PluginLogic::Wgsl { document } = &project.logic {
            value.editor.begin_multiline(
                &document.source,
                motion_graph::plugin_project::MAX_SOURCE_BYTES,
            );
        }
        value
    }
    pub fn editing(&self) -> bool {
        self.focus
    }
    pub fn cancel(&mut self, draft: &mut model::Draft) {
        draft.commit();
        self.focus = false;
    }
    fn draw_text(
        &mut self,
        draft: &mut model::Draft,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        input: &Input,
    ) -> Result<(), String> {
        if press(input, area) {
            self.focus = true;
            if !self.editor.active() {
                let source = match &draft.project.logic {
                    PluginLogic::Wgsl { document } => document.source.as_str(),
                    _ => "",
                };
                self.editor
                    .begin_multiline(source, motion_graph::plugin_project::MAX_SOURCE_BYTES);
            }
        }
        if input.pressed.is_some_and(|(_, p)| !area.contains(p)) || input.focus_lost {
            draft.commit();
            self.focus = false;
        }
        let disabled = neutral();
        let event_input = if self.focus { input } else { &disabled };
        let sheet = list.styles();
        let colors = |_: usize, line: &str| highlight(line, &sheet);
        let event = self.editor.multiline_view_colored(
            &mut self.viewport,
            list,
            atlas,
            area,
            event_input,
            &colors,
        );
        if matches!(event, Some(TextEvent::Cancel)) {
            draft.cancel();
            *self = Self::new(&draft.project);
            return Ok(());
        }
        if let PluginLogic::Wgsl { document } = &draft.project.logic {
            if document.source != self.editor.text {
                let text = self.editor.text.clone();
                draft.update(|p| {
                    if let PluginLogic::Wgsl { document } = &mut p.logic {
                        document.source = text;
                    }
                    Ok(())
                })?;
            }
        }
        if matches!(event, Some(TextEvent::Commit(_))) {
            draft.commit();
        }
        Ok(())
    }
}
/// Small presentation lexer; compilation and diagnostics use Naga, not this lexer.
/// Returns exactly the original text, preserving UTF-8 and layout measurements.
pub(super) fn highlight(
    line: &str,
    s: &motion_ui::style::StyleSheet,
) -> Vec<(String, motion_ui::Color)> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < line.len() {
        let rest = &line[at..];
        if rest.starts_with("//") {
            out.push((rest.into(), s.color("syntax.comment")));
            break;
        }
        let c = rest.chars().next().expect("nonempty UTF-8 suffix");
        let mut end = at + c.len_utf8();
        if c.is_alphanumeric() || c == '_' {
            let first = c.len_utf8();
            for (i, ch) in rest[first..].char_indices() {
                if ch.is_alphanumeric() || ch == '_' || ch == '.' {
                    end = at + first + i + ch.len_utf8();
                } else {
                    break;
                }
            }
        }
        let token = &line[at..end];
        let color = if c.is_ascii_digit() {
            s.color("syntax.number")
        } else if [
            "fn", "let", "var", "return", "if", "else", "for", "while", "loop", "break",
            "continue", "struct", "const", "override", "true", "false",
        ]
        .contains(&token)
        {
            s.color("syntax.keyword")
        } else if token.starts_with("vec")
            || ["f32", "i32", "u32", "bool", "mat4x4"].contains(&token)
        {
            s.color("syntax.type")
        } else {
            s.color("text")
        };
        out.push((token.into(), color));
        at = end;
    }
    out
}
