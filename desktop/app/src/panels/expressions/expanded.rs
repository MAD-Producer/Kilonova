//! Property-owned drafts: the shell owns all worker replies; this module only
//! emits typed actions. A reply may update its own submitted draft, never a
//! subsequently opened editor or a newer syntax check.
use super::*;
use motion_ui::widgets::{Dropdown, DropdownItem, TextViewport};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(crate) struct Ticket(Arc<()>);
impl Ticket {
    fn new() -> Self {
        Self(Arc::new(()))
    }
    fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
#[derive(PartialEq)]
struct Document(String, String, u64);
impl Document {
    fn of(state: &Value) -> Self {
        Self(
            state["root"].as_str().unwrap_or("").into(),
            state["composition"].as_str().unwrap_or("").into(),
            state["document_generation"].as_u64().unwrap_or(0),
        )
    }
}
struct Pending {
    ticket: Ticket,
    text: String,
    apply: bool,
}
pub(super) struct Editor {
    target: ExpressionTarget,
    document: Document,
    baseline: Option<PropertyExpression>,
    source: TextEditor,
    viewport: TextViewport,
    seed: TextEditor,
    seed_focus: bool,
    snippets: Dropdown,
    pending: Option<Pending>,
    diagnostic: Option<motion_core::ExpressionSourceDiagnostic>,
    checked: Option<String>,
    message: Option<(String, bool)>,
    opened: bool,
    #[cfg(test)]
    pub(super) areas: Option<[Rect; 6]>,
}
impl Editor {
    pub(super) fn text_menu(&mut self, request: &mut super::super::text_menu::Request<'_>) {
        if self.seed_focus {
            request.visit(&mut self.seed);
        } else {
            request.visit(&mut self.source);
        }
    }
    fn current(&self, state: &Value) -> bool {
        self.document == Document::of(state)
            && self.baseline == find(state, &self.target)
            && state["observing"] != true
            && state["project"]["layers"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|l| l["id"].as_u64() == Some(self.target.object()) && l["locked"] != true)
    }
    fn submit(&mut self, apply: bool, state: &Value, locale: Locale, out: &mut Vec<Action>) {
        if self.pending.is_some() {
            return;
        }
        if apply && !self.current(state) {
            self.message = Some((
                words(
                    locale,
                    "工程、属性或锁定状态已变化；草稿已保留，请重新打开编辑器。",
                    "The document, property or lock changed. Draft retained; reopen the editor.",
                )
                .into(),
                true,
            ));
            return;
        }
        let ticket = Ticket::new();
        if apply {
            let Ok(seed) = self.seed.text.trim().parse::<u32>() else {
                self.message = Some((
                    words(
                        locale,
                        "随机种子须为 0–4294967295 的整数。",
                        "Seed must be an integer from 0 to 4294967295.",
                    )
                    .into(),
                    true,
                ));
                return;
            };
            let mut expression = self.baseline.clone().unwrap_or_else(|| PropertyExpression {
                target: self.target.clone(),
                source: String::new(),
                enabled: true,
                seed: 0,
                profile: motion_core::EXPRESSION_PROFILE.into(),
            });
            expression.source = self.source.text.clone();
            expression.seed = seed;
            out.push(Action::ExpressionApply {
                ticket: ticket.clone(),
                source: crate::layer_clipboard::KeyframeSource::of(state),
                expression,
                frame: state["frame"].as_f64().unwrap_or(0.).floor() as u32,
            });
        } else {
            out.push(Action::ExpressionCheck {
                ticket: ticket.clone(),
                target: self.target.clone(),
                text: self.source.text.clone(),
            });
        }
        self.pending = Some(Pending {
            ticket,
            text: self.source.text.clone(),
            apply,
        });
        self.message = None;
    }
}
fn words(locale: Locale, zh: &'static str, en: &'static str) -> &'static str {
    auxiliary::localized(locale, zh, en)
}
impl Controls {
    pub(crate) fn open_expanded(&mut self, state: &Value, target: ExpressionTarget, owner: Panel) {
        let baseline = find(state, &target);
        let text = if self.target.as_ref() == Some(&target) && self.draft.as_ref() == Some(&target)
        {
            self.editor.text.clone()
        } else {
            baseline
                .as_ref()
                .map_or_else(|| "value".into(), |e| e.source.clone())
        };
        let mut source = TextEditor::default();
        source.begin_multiline(&text, 8192);
        let mut seed = TextEditor::default();
        seed.text = baseline.as_ref().map_or(0, |e| e.seed).to_string();
        self.owner = Some(owner);
        self.menu.open = false;
        self.menu_target = None;
        self.expanded = Some(Editor {
            target,
            document: Document::of(state),
            baseline,
            source,
            viewport: Default::default(),
            seed,
            seed_focus: false,
            snippets: Default::default(),
            pending: None,
            diagnostic: None,
            checked: None,
            message: None,
            opened: true,
            #[cfg(test)]
            areas: None,
        });
    }

    pub(crate) fn expression_response(
        &mut self,
        ticket: &Ticket,
        result: &Result<Value, String>,
        locale: Locale,
    ) {
        let Some(editor) = &mut self.expanded else {
            return;
        };
        if !editor
            .pending
            .as_ref()
            .is_some_and(|p| p.ticket.same(ticket))
        {
            return;
        }
        let pending = editor.pending.take().expect("matching expression request");
        match result {
            Err(cause) => {
                let prefix = if pending.apply {
                    words(locale, "上次应用失败：", "Last apply failed: ")
                } else {
                    words(locale, "语法检查失败：", "Syntax check failed: ")
                };
                editor.message = Some((
                    format!("{prefix}{}", cause.chars().take(1024).collect::<String>()),
                    true,
                ));
                // Failure preserves both the source and seed drafts. Runtime
                // errors have target/frame context, but no fabricated source span.
            }
            Ok(value) if pending.apply => {
                if value["editorAction"]["cancelled"] == true {
                    editor.message = Some((
                        words(
                            locale,
                            "工程已变化，本次应用已取消；草稿已保留。",
                            "The document changed. Apply cancelled; draft retained.",
                        )
                        .into(),
                        false,
                    ));
                } else {
                    editor.baseline = find(value, &editor.target);
                    if editor.baseline.is_none() {
                        let cause = "expression apply reply is missing the submitted property";
                        log::error!("expression editor failed operation=apply phase=reply object={} cause={cause}", editor.target.object());
                        editor.message = Some((cause.into(), true));
                        return;
                    }
                    if let Some(expression) = &editor.baseline {
                        if self.target.as_ref() == Some(&editor.target) {
                            self.loaded = expression.source.clone();
                            self.editor.cancel();
                            self.editor.text = expression.source.clone();
                        }
                    }
                    let unchanged = editor.baseline.as_ref().is_some_and(|e| {
                        e.source == editor.source.text
                            && Some(e.seed) == editor.seed.text.trim().parse::<u32>().ok()
                    });
                    editor.message = Some((if unchanged {
                        words(locale,"表达式已应用。","Expression applied.")
                    } else {
                        words(locale,"已应用提交内容；当前草稿的后续修改尚未应用。","Submitted expression applied; subsequent draft changes remain unapplied.")
                    }.into(),false));
                }
            }
            Ok(value) => {
                if editor.source.text != pending.text {
                    return;
                }
                match value
                    .get("diagnostic")
                    .ok_or_else(|| "missing expression diagnostic".to_string())
                    .and_then(|v| {
                        serde_json::from_value::<Option<motion_core::ExpressionSourceDiagnostic>>(
                            v.clone(),
                        )
                        .map_err(|e| e.to_string())
                    }) {
                    Ok(diagnostic) => {
                        if diagnostic
                            .as_ref()
                            .and_then(|d| d.byte_offset)
                            .is_some_and(|at| {
                                at > editor.source.text.len()
                                    || !editor.source.text.is_char_boundary(at)
                            })
                        {
                            let cause = "expression diagnostic has an invalid source offset";
                            log::error!("expression editor failed operation=check phase=reply object={} cause={cause}", editor.target.object());
                            editor.message = Some((cause.into(), true));
                            return;
                        }
                        editor.checked = Some(pending.text);
                        editor.diagnostic = diagnostic;
                        editor.message = Some((
                            words(
                                locale,
                                "语法检查通过；应用时验证实际返回值。",
                                "Syntax valid; Apply validates the actual result.",
                            )
                            .into(),
                            false,
                        ));
                    }
                    Err(cause) => {
                        log::error!("expression editor failed operation=check phase=reply object={} cause={}", editor.target.object(), crate::fallback_observation::error_cause(&cause));
                        editor.message = Some((cause, true));
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn expanded_overlay(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        bounds: Rect,
        state: &Value,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        let Some(editor) = &mut self.expanded else {
            return;
        };
        let neutral = Input::default();
        let input = if std::mem::take(&mut editor.opened) {
            &neutral
        } else {
            input
        };
        let sheet = list.styles();
        let pad = sheet.metric("padding.panel", 8.);
        let row = sheet.metric("row.height", 22.);
        let gap = sheet.metric("padding", 5.);
        let action = sheet.metric("expression.editor.action.width", 80.);
        let width = sheet
            .metric("expression.editor.width", 512.)
            .min((bounds.width() - pad - pad).max(0.));
        let height = sheet
            .metric("expression.editor.height", 460.)
            .min((bounds.height() - pad - pad).max(0.));
        let popup = Rect::new(
            bounds.min[0] + (bounds.width() - width) * 0.5,
            bounds.min[1] + (bounds.height() - height) * 0.5,
            width,
            height,
        );
        paint::rect(list, popup, sheet.color("panel"));
        paint::stroke(
            list,
            popup,
            sheet.color("border"),
            sheet.metric("border", 1.),
        );
        let inner = Rect::new(
            popup.min[0] + pad,
            popup.min[1] + pad,
            (width - pad - pad).max(0.),
            (height - pad - pad).max(0.),
        );
        let title = Rect::new(inner.min[0], inner.min[1], inner.width(), row);
        label(
            list,
            atlas,
            title,
            words(locale, "表达式编辑器", "Expression editor"),
            false,
        );
        let target_name = target_label(state, &editor.target, locale);
        label(
            list,
            atlas,
            Rect::new(inner.min[0], title.max[1], inner.width(), row),
            &target_name,
            true,
        );
        let tools_y = title.max[1] + row + gap;
        let snippet_area = Rect::new(
            inner.min[0],
            tools_y,
            (inner.width() * 0.5 - gap).max(0.),
            row,
        );
        let seed_width = sheet
            .metric("expression.editor.seed.width", 112.)
            .min(inner.width() * 0.3);
        let seed_area = Rect::new(inner.max[0] - seed_width, tools_y, seed_width, row);
        label(
            list,
            atlas,
            Rect::new(
                snippet_area.max[0] + gap,
                tools_y,
                (seed_area.min[0] - snippet_area.max[0] - gap - gap).max(0.),
                row,
            ),
            words(locale, "种子", "Seed"),
            true,
        );
        if input
            .pressed
            .is_some_and(|(b, p)| b == MouseButton::Left && seed_area.contains(p))
        {
            editor.seed_focus = true;
            let value = editor.seed.text.clone();
            editor.seed.begin(&value);
        }
        let footer_y = inner.max[1] - row;
        let cancel = Rect::new(inner.max[0] - action, footer_y, action, row);
        let apply = Rect::new(cancel.min[0] - gap - action, footer_y, action, row);
        let check = Rect::new(inner.min[0], footer_y, action, row);
        let diagnostic_y = footer_y - gap - row - row;
        let source_area = Rect::new(
            inner.min[0],
            tools_y + row + gap,
            inner.width(),
            (diagnostic_y - gap - (tools_y + row + gap)).max(0.),
        );
        #[cfg(test)]
        {
            editor.areas = Some([source_area, seed_area, snippet_area, check, apply, cancel]);
        }
        if input
            .pressed
            .is_some_and(|(b, p)| b == MouseButton::Left && source_area.contains(p))
        {
            editor.seed_focus = false;
            editor.seed.cancel();
        }
        let escape =
            !editor.snippets.open && input.key_presses().any(|(k, _, r)| k == Key::Escape && !r);
        let mut commit = false;
        let seed_owned_input = editor.seed_focus;
        if editor.seed_focus {
            let seed_input = if editor.snippets.open {
                &neutral
            } else {
                input
            };
            if matches!(editor.seed.input(seed_input), Some(TextEvent::Commit(_))) {
                editor.seed_focus = false;
                editor.seed.cancel();
            }
        }
        editor.seed.draw_styled(
            list,
            atlas,
            seed_area,
            &sheet,
            "field.number",
            Default::default(),
        );
        // Snippet popups own keyboard/pointer input, while the source session
        // stays alive for native Edit menu commands and local draft undo.
        let source_input = if seed_owned_input || editor.snippets.open {
            &neutral
        } else {
            input
        };
        match editor.source.multiline_view(
            &mut editor.viewport,
            list,
            atlas,
            source_area,
            source_input,
        ) {
            Some(TextEvent::Commit(_)) => commit = true,
            Some(TextEvent::Cancel) => {}
            None => {}
        }
        if editor
            .checked
            .as_ref()
            .is_some_and(|s| s != &editor.source.text)
        {
            editor.checked = None;
            editor.diagnostic = None;
            editor.message = None;
        }
        let (line, column) = editor.source.caret_line_column();
        let position = if locale == Locale::Zh {
            format!(
                "行 {line}，列 {column} · {} / 8192 字节 · Ctrl+Enter 应用",
                editor.source.text.len()
            )
        } else {
            format!(
                "Ln {line}, Col {column} · {} / 8192 bytes · Ctrl+Enter applies",
                editor.source.text.len()
            )
        };
        label(
            list,
            atlas,
            Rect::new(inner.min[0], diagnostic_y, inner.width(), row),
            &position,
            true,
        );
        let diagnostic_area = Rect::new(inner.min[0], diagnostic_y + row, inner.width(), row);
        if let Some(cause) = editor.source.error.as_ref().or(editor.seed.error.as_ref()) {
            ui::text(
                list,
                atlas,
                diagnostic_area,
                cause,
                sheet.color("error"),
                Align::Left,
            );
        } else if let Some(diagnostic) = &editor.diagnostic {
            let location = diagnostic
                .byte_offset
                .and_then(|at| editor.source.line_column_at_byte(at))
                .map(|(line, column)| {
                    if locale == Locale::Zh {
                        format!("行 {line}，列 {column}: ")
                    } else {
                        format!("Line {line}, col {column}: ")
                    }
                })
                .unwrap_or_default();
            ui::text(
                list,
                atlas,
                diagnostic_area,
                &format!("{location}{}", diagnostic.message),
                sheet.color("error"),
                Align::Left,
            );
            if input
                .pressed
                .is_some_and(|(b, p)| b == MouseButton::Left && diagnostic_area.contains(p))
            {
                if let Some(at) = diagnostic.byte_offset {
                    editor.source.select_bytes(at, at);
                    editor.seed_focus = false;
                    editor.seed.cancel();
                }
            }
        } else if let Some(pending) = &editor.pending {
            label(
                list,
                atlas,
                diagnostic_area,
                if pending.apply {
                    words(
                        locale,
                        "正在应用；关闭编辑器后，已提交的操作仍会完成。",
                        "Applying; closing the editor keeps the submitted operation running.",
                    )
                } else {
                    words(locale, "正在检查语法…", "Checking syntax…")
                },
                true,
            );
        } else if let Some((message, error)) = &editor.message {
            ui::text(
                list,
                atlas,
                diagnostic_area,
                message,
                sheet.color(if *error { "error" } else { "muted" }),
                Align::Left,
            );
        } else {
            label(
                list,
                atlas,
                diagnostic_area,
                words(
                    locale,
                    "修改保留在草稿中；检查不会执行表达式。",
                    "Changes stay in the draft; Check does not execute code.",
                ),
                true,
            );
        }
        let busy = editor.pending.is_some();
        if ui::button(
            list,
            atlas,
            check,
            words(locale, "检查", "Check"),
            input,
            !busy,
        )
        .clicked
        {
            editor.submit(false, state, locale, out);
        }
        if ui::button(
            list,
            atlas,
            apply,
            if busy {
                words(locale, "处理中…", "Working…")
            } else {
                words(locale, "应用", "Apply")
            },
            input,
            !busy,
        )
        .clicked
            || commit
        {
            editor.submit(true, state, locale, out);
        }
        let close_label = if editor.pending.as_ref().is_some_and(|p| p.apply) {
            words(locale, "关闭", "Close")
        } else {
            words(locale, "取消", "Cancel")
        };
        let cancel_clicked = ui::button(list, atlas, cancel, close_label, input, true).clicked;
        let labels = [
            words(locale, "插入片段…", "Insert snippet…"),
            "value",
            "wiggle(2, 20)",
            "loopOut(\"cycle\")",
            "valueAtTime(time - 0.1)",
        ];
        let items: Vec<_> = labels
            .iter()
            .enumerate()
            .map(|(i, &label)| DropdownItem {
                label,
                enabled: i > 0,
            })
            .collect();
        if let Some(index) = editor.snippets.draw(
            list,
            atlas,
            snippet_area,
            bounds,
            &items,
            0,
            input,
            true,
            false,
        ) {
            if index > 0 {
                let snippet = Input {
                    text: labels[index].into(),
                    ..Default::default()
                };
                editor.source.input(&snippet);
                editor.seed_focus = false;
                editor.seed.cancel();
            }
        }
        if cancel_clicked || escape {
            self.expanded = None;
        }
    }
}
fn target_label(state: &Value, target: &ExpressionTarget, locale: Locale) -> String {
    let name = state["project"]["layers"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|l| l["id"].as_u64() == Some(target.object()))
        .and_then(|l| l["name"].as_str())
        .unwrap_or("?");
    let property = match target {
        ExpressionTarget::Property { property, axis, .. } => {
            let name = match property {
                Property::Position => words(locale, "位置", "Position"),
                Property::Anchor => words(locale, "锚点", "Anchor"),
                Property::Scale => words(locale, "缩放", "Scale"),
                Property::Rotation => words(locale, "旋转", "Rotation"),
                Property::Opacity => words(locale, "不透明度", "Opacity"),
                Property::Target => words(locale, "目标点", "Target"),
                Property::Roll => words(locale, "翻滚", "Roll"),
                Property::Fov => words(locale, "视野", "Field of view"),
                Property::Radius => words(locale, "半径", "Radius"),
                Property::Azimuth => words(locale, "方位角", "Azimuth"),
                Property::Elevation => words(locale, "仰角", "Elevation"),
            };
            format!(
                "{name}{}",
                axis.map_or(String::new(), |a| format!(" {a:?}"))
            )
        }
        ExpressionTarget::Effect { effect, param, .. } => {
            format!("{} {effect} · {param}", words(locale, "效果", "Effect"))
        }
    };
    format!("{name} · {property}")
}

#[cfg(test)]
mod tests;
