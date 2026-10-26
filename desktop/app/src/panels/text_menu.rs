//! Application Edit retains a specific draft instead of changing document focus.
use super::*;
use motion_ui::{
    text_clipboard::TextClipboard,
    widgets::{TextCommand, TextEditor, TextSession},
};
use std::sync::Arc;

const COMMANDS: [TextCommand; 6] = [
    TextCommand::Undo,
    TextCommand::Redo,
    TextCommand::Copy,
    TextCommand::Cut,
    TextCommand::Paste,
    TextCommand::SelectAll,
];
type Context = (String, String, u64, u64);
fn context(state: &Value) -> Context {
    (
        state["root"].as_str().unwrap_or("").into(),
        state["composition"].as_str().unwrap_or("").into(),
        state["document_generation"].as_u64().unwrap_or(0),
        state["revision"].as_u64().unwrap_or(0),
    )
}
pub(crate) struct Capture {
    session: TextSession,
    context: Context,
    focused: Option<Panel>,
    enabled: [bool; 6],
}
pub(crate) struct Request<'a> {
    target: Option<&'a TextSession>,
    command: Option<TextCommand>,
    clipboard: Option<&'a Arc<TextClipboard>>,
    found: Option<(TextSession, [bool; 6])>,
}
impl Request<'_> {
    pub(crate) fn visit(&mut self, editor: &mut TextEditor) -> bool {
        if self.found.is_some()
            || !editor.active()
            || self
                .target
                .is_some_and(|target| !editor.owns_session(target))
        {
            return false;
        }
        self.found = editor.session().map(|session| {
            (
                session,
                COMMANDS.map(|command| editor.command_enabled(command)),
            )
        });
        if let Some(command) = self.command {
            let before = editor.text.clone();
            editor.command(command, self.clipboard);
            return before != editor.text;
        }
        false
    }
    pub(crate) fn value(
        &mut self,
        editor: &mut TextEditor,
        text: &mut String,
        replace: &mut bool,
    ) -> bool {
        if self.target.is_none() && self.found.is_none() {
            editor.input_value(text, replace, &Input::default());
        }
        let changed = self.visit(editor);
        if self.command.is_some()
            && self
                .target
                .is_some_and(|target| editor.owns_session(target))
        {
            if changed {
                text.clone_from(&editor.text);
                // Legacy numeric fields use replace as "not edited yet", not
                // as their current text selection. Match input_value exactly.
                *replace = false;
            }
        }
        changed
    }
    pub(crate) fn query(&mut self, editor: &mut TextEditor, text: &mut String) -> bool {
        let changed = self.visit(editor);
        if changed {
            text.clone_from(&editor.text);
        }
        changed
    }
}
fn visit(panels: &mut Panels, request: &mut Request<'_>) {
    if panels.expressions.menu_open() && panels.expressions.owner == panels.focused {
        panels.expressions.text_menu(request);
        return;
    }
    panels.parent_controls.text_menu(request);
    panels.timeline_columns.text_menu(request);
    panels.layer_actions.text_menu(request);
    panels.toolbar_style.text_menu(request);
    match panels.focused {
        Some(Panel::Project) if panels.project.search => {
            request.query(
                &mut panels.project.search_field.editor,
                &mut panels.project.query,
            );
        }
        Some(Panel::Timeline | Panel::Curves) => {
            if panels.timeline.timeline_search
                && request.query(
                    &mut panels.timeline.timeline_search_editor,
                    &mut panels.timeline.timeline_query,
                )
            {
                panels.timeline.scroll = 0.;
            }
            panels.timeline_anchor.text_menu(request);
            panels.effect_timeline.text_menu(request);
            panels.audio_controls.text_menu(request);
        }
        Some(Panel::Properties) => {
            if let Some(field) = panels
                .properties
                .field
                .as_mut()
                .filter(|field| field.scrubbing.is_none())
            {
                request.value(&mut field.editor, &mut field.text, &mut field.replace);
            }
            panels.properties.anchor.text_menu(request);
            panels.properties.effects.text_menu(request);
            panels.camera_controls.text_menu(request);
        }
        Some(Panel::EffectControls) => panels.effect_controls.text_menu(request),
        Some(Panel::Effects) => panels.browser.text_menu(request),
        Some(Panel::Nodes | Panel::NodeProperties | Panel::NodePreview) => {
            panels.node_editor.text_menu(request)
        }
        Some(Panel::Composition) => {
            request.visit(&mut panels.composition.zoom.text);
        }
        _ => {}
    }
    if panels.expressions.owner == panels.focused {
        panels.expressions.text_menu(request);
    }
    if panels.path_controls.numeric_owner() == panels.focused {
        panels.path_controls.text_menu(request);
    }
    if panels.auxiliary.owner() == panels.focused {
        panels.auxiliary.text_menu(request);
    }
}
pub(super) fn prepare(
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    atlas: &mut TextAtlas,
) -> bool {
    if input.focus_lost {
        panels.text_menu = None;
        return false;
    }
    let clicked_menu = input
        .pressed
        .filter(|(button, _)| *button == MouseButton::Left)
        .and_then(|(_, point)| {
            super::menus::regions(atlas, panels.locale)
                .iter()
                .position(|(_, rect)| rect.contains(point))
        });
    if clicked_menu.is_some_and(|index| index != 1) && panels.text_menu.is_some() {
        panels.text_menu = None;
        panels.menu = None;
    }
    let opening = clicked_menu == Some(1)
        && panels.menu != Some(1)
        && !input.composing
        && !input.ime_consumed;
    if opening {
        let mut request = Request {
            target: None,
            command: None,
            clipboard: None,
            found: None,
        };
        if !panels.node_editor.popup_open() && !panels.layer_actions.modal_in(Some(Panel::Timeline))
        {
            visit(panels, &mut request);
        }
        panels.text_menu = request.found.map(|(session, enabled)| Capture {
            session,
            enabled,
            context: context(state),
            focused: panels.focused,
        });
    } else if panels.menu.is_none() {
        panels.text_menu = None;
    }
    if let Some(mut capture) = panels.text_menu.take() {
        let mut request = Request {
            target: Some(&capture.session),
            command: None,
            clipboard: None,
            found: None,
        };
        if capture.context == context(state) && capture.focused == panels.focused {
            visit(panels, &mut request);
        }
        capture.enabled = request.found.map_or([false; 6], |(_, enabled)| enabled);
        panels.text_menu = Some(capture);
    }
    panels.text_menu.is_some() && (opening || panels.menu.is_some())
}
pub(super) fn entries(panels: &Panels, state: &Value) -> Vec<(String, TextCommand, bool)> {
    let zh = panels.locale == Locale::Zh;
    let names = if zh {
        [
            "撤销文本编辑",
            "重做文本编辑",
            "复制文本",
            "剪切文本",
            "粘贴文本",
            "全选文本",
        ]
    } else {
        [
            "Undo Text Edit",
            "Redo Text Edit",
            "Copy Text",
            "Cut Text",
            "Paste Text",
            "Select All Text",
        ]
    };
    let keys = ["Ctrl+Z", "Ctrl+Y", "Ctrl+C", "Ctrl+X", "Ctrl+V", "Ctrl+A"];
    COMMANDS
        .into_iter()
        .enumerate()
        .map(|(i, command)| {
            let enabled = panels.text_menu.as_ref().is_some_and(|capture| {
                capture.context == context(state)
                    && capture.focused == panels.focused
                    && capture.enabled[i]
            });
            (format!("{}  {}", names[i], keys[i]), command, enabled)
        })
        .collect()
}
pub(super) fn apply(
    panels: &mut Panels,
    state: &Value,
    command: TextCommand,
    clipboard: Option<&Arc<TextClipboard>>,
) {
    let Some(capture) = panels.text_menu.take() else {
        return;
    };
    if capture.context != context(state) || capture.focused != panels.focused {
        return;
    }
    let mut request = Request {
        target: Some(&capture.session),
        command: Some(command),
        clipboard,
        found: None,
    };
    visit(panels, &mut request);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_text_menu_selection_does_not_reset_the_edited_flag() {
        let cache = Arc::new(TextClipboard::default());
        let mut editor = TextEditor::default();
        editor.begin("0");
        let mut text = "0".to_string();
        let mut replace = true;
        let mut dispatch = |command| {
            let session = editor.session().unwrap();
            let mut request = Request {
                target: Some(&session),
                command: Some(command),
                clipboard: Some(&cache),
                found: None,
            };
            request.value(&mut editor, &mut text, &mut replace);
            (text.clone(), replace)
        };
        cache.store("64").unwrap();
        assert_eq!(dispatch(TextCommand::Paste), ("64".into(), false));
        assert_eq!(dispatch(TextCommand::SelectAll), ("64".into(), false));
        assert_eq!(dispatch(TextCommand::Copy), ("64".into(), false));
        cache.store("96").unwrap();
        assert_eq!(dispatch(TextCommand::Paste), ("96".into(), false));
        assert_eq!(dispatch(TextCommand::Undo), ("64".into(), false));
    }
}
