use super::*;
use serde_json::Value;
impl Source {
    fn locate_language(&mut self, line: usize, column: usize, operation: &'static str) -> bool {
        let Some(byte) = lsp::byte(&self.editor.text, line, column) else {
            // A server position is not trusted editor input. Keep the draft and
            // selection intact, and expose the refusal without logging source.
            log::error!("plugin.language navigation operation={operation} line={line} utf16_column={column} source_bytes={} cause=invalid UTF-16 position", self.editor.text.len());
            self.language_error = Some(format!(
                "语言服务返回了无效位置，请重新检查源码 / Invalid language-service position: line={line}, UTF-16 column={column}"
            ));
            self.language_reply = None;
            self.completion_open = false;
            return false;
        };
        self.editor.select_bytes(byte, byte);
        self.focus = true;
        self.language_error = None;
        true
    }
    #[cfg(test)]
    pub fn test_language_snapshot(&self) -> Option<lsp::Snapshot> {
        self.language.as_ref().map(|c| c.snapshot().unwrap())
    }
    #[cfg(test)]
    pub fn test_completion_labels(&self) -> Vec<String> {
        self.language_reply
            .as_ref()
            .map(|r| {
                completion_items(&r.result, &self.editor.text, self.editor.caret_byte_index())
                    .iter()
                    .map(|v| v["label"].as_str().unwrap_or("").to_owned())
                    .collect()
            })
            .unwrap_or_default()
    }
    #[cfg(test)]
    pub fn test_text(&self) -> &str {
        &self.editor.text
    }
    pub fn replace_project(&mut self, project: &PluginProject) {
        let mut next = Self::new(project);
        next.language = self.language.take();
        next.language_attempted = self.language_attempted;
        next.language_snapshot = self.language_snapshot.take();
        next.language_seen = self.language_seen;
        next.language_reply_id = self.language_reply_id;
        *self = next;
    }
    pub fn pending(&self) -> bool {
        self.language_snapshot
            .as_ref()
            .is_some_and(|s| s.flight.is_some())
            || self
                .language_sent
                .is_some_and(|t| t.elapsed() < Duration::from_secs(1))
    }
    pub fn poll_language(&mut self) -> Result<bool, String> {
        let Some(client) = &self.language else {
            return Ok(false);
        };
        let snapshot = client.snapshot()?;
        let changed = snapshot.changed != self.language_seen;
        self.language_seen = snapshot.changed;
        if let Some(reply) = &snapshot.reply {
            if reply.id != self.language_reply_id {
                self.language_reply_id = reply.id;
                self.language_reply = Some(reply.clone());
                self.completion_index = 0;
                self.completion_scroll = 0;
                self.completion_open = reply.request.kind == lsp::Kind::Completion;
            }
        }
        self.language_snapshot = Some(snapshot);
        Ok(changed)
    }
    fn ensure_language(&mut self) -> Result<(), String> {
        if !self.language_attempted {
            self.language_attempted = true;
            if let Some(path) = std::env::var_os("MOTION_WGSL_ANALYZER").filter(|p| !p.is_empty()) {
                self.language = Some(lsp::Client::start(PathBuf::from(path))?);
                self.language_snapshot =
                    self.language.as_ref().map(|c| c.snapshot()).transpose()?;
            }
        }
        Ok(())
    }
    fn query(&mut self, draft: &model::Draft, kind: lsp::Kind) -> Result<bool, String> {
        let Some(client) = &self.language else {
            return Ok(false);
        };
        let Some((text, prefix)) = draft
            .project
            .wgsl_language_document()
            .map_err(|e| e.to_string())?
        else {
            return Ok(false);
        };
        let position = lsp::position(&self.editor.text, self.editor.caret_byte_index())
            .ok_or("language caret is not a UTF-8 boundary")?;
        let accepted = client.submit(lsp::Request {
            revision: draft.revision,
            text,
            prefix,
            position,
            kind,
        })?;
        if accepted {
            self.language_synced = Some(draft.revision);
            self.language_sent = Some(Instant::now());
            self.language_error = None;
            self.poll_language()?;
        }
        Ok(accepted)
    }
    pub fn draw(
        &mut self,
        draft: &mut model::Draft,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        input: &Input,
        zh: bool,
    ) -> Result<(), String> {
        self.ensure_language()?;
        self.poll_language()?;
        let sheet = list.styles();
        let row = sheet.metric("row.height", 22.);
        let pad = sheet.metric("padding.panel", 8.);
        let action = sheet.metric("plugin.author.action.width", 80.);
        let compact = area.width() < (action + pad) * 5.;
        let top = Rect::new(
            area.min[0],
            area.min[1],
            area.width(),
            if compact { row * 2. } else { row },
        );
        let ready = self
            .language_snapshot
            .as_ref()
            .is_some_and(|s| s.status == lsp::Status::Ready && s.flight.is_none());
        let mut request = None;
        for (i, (kind, title)) in [
            (lsp::Kind::Completion, tr(zh, "补全", "Complete")),
            (lsp::Kind::Hover, tr(zh, "符号说明", "Symbol info")),
            (lsp::Kind::Definition, tr(zh, "转到定义", "Definition")),
        ]
        .into_iter()
        .enumerate()
        {
            if ui::button(
                list,
                atlas,
                Rect::new(
                    top.min[0] + i as f32 * (action + pad),
                    top.min[1],
                    action,
                    row,
                ),
                &title,
                input,
                ready,
            )
            .clicked
            {
                request = Some(kind);
            }
        }
        let status = match self.language_snapshot.as_ref() {
            Some(s) => match &s.status {
                lsp::Status::Starting => tr(zh, "语言服务正在连接…", "Connecting language server…"),
                lsp::Status::Stopped => tr(zh, "语言服务已关闭", "Language service closed"),
                lsp::Status::Ready => {
                    if let Some((phase, start)) = s.flight {
                        format!("{phase} · {} ms", start.elapsed().as_millis())
                    } else {
                        tr(zh, "WGSL 语言服务已连接", "WGSL language server connected")
                    }
                }
                lsp::Status::Failed(e) => {
                    format!("{}: {e}", tr(zh, "语言服务失败", "Language service failed"))
                }
            },
            None => tr(
                zh,
                "未配置语言服务 · 参见使用教程",
                "Language server not configured · see Guide",
            ),
        };
        let status_x = if compact {
            top.min[0]
        } else {
            top.min[0] + (action + pad) * 3.
        };
        ui::text(
            list,
            atlas,
            Rect::new(
                status_x,
                top.min[1] + if compact { row } else { 0. },
                (top.max[0] - status_x).max(0.),
                row,
            ),
            &status,
            if self
                .language_snapshot
                .as_ref()
                .is_some_and(|s| matches!(s.status, lsp::Status::Failed(_)))
            {
                sheet.color("error")
            } else {
                sheet.color("muted")
            },
            Align::Left,
        );
        let detail_height = sheet
            .metric("plugin.author.code.details_height", 132.)
            .min((area.height() - row * 4.).max(row));
        let details = Rect::new(
            area.min[0],
            area.max[1] - detail_height,
            area.width(),
            detail_height,
        );
        let code = Rect::new(
            area.min[0],
            top.max[1] + pad,
            area.width(),
            (details.min[1] - top.max[1] - pad * 2.).max(0.),
        );
        let mut filtered = input.clone();
        let mut insertion = None;
        let shortcut_allowed =
            self.focus && !input.composing && !input.ime_consumed && !input.focus_lost;
        if shortcut_allowed {
            for (key, mods, repeat) in input.key_presses() {
                if !repeat && ready {
                    if key == Key::Space && mods.control {
                        request = Some(lsp::Kind::Completion);
                    }
                    if key == Key::F12 {
                        request = Some(lsp::Kind::Definition);
                    }
                }
            }
        }
        retain_keys(&mut filtered, |key, mods| {
            !(*key == Key::Space && mods.control || *key == Key::F12)
        });
        if self
            .language_reply
            .as_ref()
            .is_some_and(|r| r.request.revision != draft.revision)
        {
            self.language_reply = None;
            self.completion_open = false;
        }
        if let Some(reply) = self.language_reply.clone() {
            let mut drawing = list.child();
            paint::rect(&mut drawing, details, sheet.color("panel"));
            let mut y = details.min[1];
            if let Some(error) = reply.result["languageError"].as_str() {
                guide::wrapped(&mut drawing, atlas, details, error, sheet.color("error"));
            } else {
                match reply.request.kind {
                    lsp::Kind::Completion if self.completion_open => {
                        let items = completion_items(
                            &reply.result,
                            &self.editor.text,
                            self.editor.caret_byte_index(),
                        );
                        if items.is_empty() {
                            ui::text(
                                &mut drawing,
                                atlas,
                                details,
                                &tr(
                                    zh,
                                    "当前位置没有补全建议",
                                    "No completions at this position",
                                ),
                                sheet.color("muted"),
                                Align::Left,
                            );
                        } else {
                            if details.contains(input.mouse) {
                                let direction = if input.wheel[1] > 0. {
                                    -1
                                } else if input.wheel[1] < 0. {
                                    1
                                } else {
                                    0
                                };
                                self.completion_scroll =
                                    self.completion_scroll.saturating_add_signed(direction);
                                if direction != 0 {
                                    self.completion_index =
                                        self.completion_scroll.min(items.len() - 1);
                                }
                            }
                            if shortcut_allowed {
                                for (key, _, repeat) in input.key_presses() {
                                    if !repeat {
                                        match key {
                                            Key::Up => {
                                                self.completion_index =
                                                    self.completion_index.saturating_sub(1)
                                            }
                                            Key::Down => {
                                                self.completion_index =
                                                    (self.completion_index + 1).min(items.len() - 1)
                                            }
                                            Key::Enter | Key::Tab => {
                                                insertion = Some(
                                                    items[self
                                                        .completion_index
                                                        .min(items.len() - 1)]
                                                    .clone(),
                                                )
                                            }
                                            Key::Escape => self.completion_open = false,
                                            _ => {}
                                        }
                                    }
                                }
                            }
                            let visible = (details.height() / row).floor().max(1.) as usize;
                            self.completion_scroll = self
                                .completion_scroll
                                .min(items.len().saturating_sub(visible));
                            if self.completion_index < self.completion_scroll {
                                self.completion_scroll = self.completion_index;
                            }
                            if self.completion_index >= self.completion_scroll + visible {
                                self.completion_scroll = self.completion_index + 1 - visible;
                            }
                            for (i, item) in items
                                .iter()
                                .enumerate()
                                .skip(self.completion_scroll)
                                .take(visible)
                            {
                                let r = Rect::new(details.min[0], y, details.width(), row);
                                if i == self.completion_index {
                                    paint::rect(&mut drawing, r, sheet.color("selected"));
                                }
                                let label = format!(
                                    "{}  {}",
                                    item["label"].as_str().unwrap_or(""),
                                    item["detail"].as_str().unwrap_or("")
                                );
                                ui::text(
                                    &mut drawing,
                                    atlas,
                                    r,
                                    &label,
                                    sheet.color("text"),
                                    Align::Left,
                                );
                                if press(input, r) {
                                    self.completion_index = i;
                                    insertion = Some(item.clone());
                                }
                                y += row;
                            }
                        }
                        retain_keys(&mut filtered, |k, _| {
                            !matches!(k, Key::Up | Key::Down | Key::Enter | Key::Tab | Key::Escape)
                        });
                    }
                    lsp::Kind::Hover => {
                        let text = hover_text(&reply.result);
                        guide::wrapped(
                            &mut drawing,
                            atlas,
                            details,
                            &if text.is_empty() {
                                tr(zh, "当前位置没有说明；可用转到定义查看声明", "No symbol information; use Definition to inspect the declaration")
                            } else {
                                text
                            },
                            sheet.color("text"),
                        );
                    }
                    lsp::Kind::Definition => {
                        let text = definition_text(&reply, zh);
                        guide::wrapped(&mut drawing, atlas, details, &text, sheet.color("text"));
                    }
                    _ => {}
                }
            }
            drawing.clip_to(details);
            list.append(drawing);
        } else {
            paint::rect(list, details, sheet.color("panel"));
            let mut y = details.min[1];
            if let Some(error) = &self.language_error {
                guide::wrapped(list, atlas, details, error, sheet.color("error"));
            } else if let Some(s) = &self.language_snapshot {
                let diagnostics: Vec<_> = s
                    .diagnostics
                    .iter()
                    .filter(|_| s.revision == draft.revision)
                    .collect();
                let mut navigation = None;
                for d in diagnostics
                    .into_iter()
                    .take((details.height() / row) as usize)
                {
                    let r = Rect::new(details.min[0], y, details.width(), row);
                    ui::text(
                        list,
                        atlas,
                        r,
                        &format!("{}:{}  {}", d.line + 1, d.column + 1, d.message),
                        if d.error {
                            sheet.color("error")
                        } else {
                            sheet.color("muted")
                        },
                        Align::Left,
                    );
                    if press(input, r) {
                        navigation = Some((d.line, d.column));
                    }
                    y += row;
                }
                if let Some((line, column)) = navigation {
                    self.locate_language(line, column, "diagnostic");
                }
                if y == details.min[1] {
                    guide::wrapped(list,atlas,details,&tr(zh,"Ctrl+Space 补全 · F12 定义 · 符号说明取决于服务器支持。最终结果以检查预览为准。","Ctrl+Space: complete · F12: definition · Symbol info depends on server support. Check preview performs final validation."),sheet.color("muted"));
                }
            } else {
                guide::wrapped(list,atlas,details,&tr(zh,"代码辅助可连接本机 wgsl-analyzer。设置 MOTION_WGSL_ANALYZER 后重启；完整步骤见使用教程。当前仍可输入、保存和检查预览。","Set MOTION_WGSL_ANALYZER to a local wgsl-analyzer executable and restart. See Guide for setup. Editing, saving and preview checks remain available."),sheet.color("muted"));
            }
        }
        if details.contains(input.mouse) {
            filtered.wheel = [0.; 2];
            filtered.pressed = None;
            filtered.released = None;
        }
        if let Some(item) = insertion {
            if let Some(reply) = &self.language_reply {
                match completion_edit(
                    &self.editor.text,
                    self.editor.caret_byte_index(),
                    reply,
                    &item,
                ) {
                    Ok((start, end, text)) => {
                        self.editor.select_bytes(start, end);
                        filtered = neutral();
                        filtered.text = text;
                        self.focus = true;
                        self.language_reply = None;
                        self.completion_open = false;
                    }
                    Err(e) => {
                        self.language_error = Some(e);
                        self.language_reply = None;
                        self.completion_open = false;
                    }
                }
            }
        }
        if request.is_some() {
            self.focus = true;
            filtered.pressed = None;
        }
        if !self.completion_open
            && self
                .language_reply
                .as_ref()
                .is_some_and(|r| r.request.kind == lsp::Kind::Completion)
        {
            self.language_reply = None;
        }
        let before = draft.revision;
        self.draw_text(draft, list, atlas, code, &filtered)?;
        if before != draft.revision {
            self.language_reply = None;
            self.completion_open = false;
            self.language_sent = Some(Instant::now());
        }
        if let Some(kind) = request {
            self.query(draft, kind)?;
        } else if ready
            && self.language_synced != Some(draft.revision)
            && self
                .language_sent
                .is_none_or(|t| t.elapsed() >= Duration::from_millis(300))
        {
            self.query(draft, lsp::Kind::Check)?;
        }
        // Definition requests only navigate within this unsaved source. Host and
        // parameter declarations are shown as context, never opened for editing.
        if let Some(reply) = &self.language_reply {
            if reply.request.kind == lsp::Kind::Definition
                && reply.request.revision == draft.revision
                && self.definition_applied != reply.id
            {
                self.definition_applied = reply.id;
                if let Some((line, column)) = definition_position(reply) {
                    if line >= reply.request.prefix {
                        self.locate_language(line - reply.request.prefix, column, "definition");
                    }
                }
            }
        }
        Ok(())
    }
}
pub(super) fn completion_items(value: &Value, source: &str, caret: usize) -> Vec<Value> {
    let prefix = source
        .get(..caret)
        .unwrap_or("")
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>()
        .to_lowercase();
    value
        .as_array()
        .or_else(|| value["items"].as_array())
        .into_iter()
        .flatten()
        .filter(|item| {
            item["filterText"]
                .as_str()
                .or_else(|| item["label"].as_str())
                .is_some_and(|label| label.to_lowercase().starts_with(&prefix))
        })
        .take(128)
        .cloned()
        .collect()
}
fn hover_text(value: &Value) -> String {
    fn part(v: &Value) -> String {
        if let Some(s) = v.as_str() {
            s.into()
        } else if let Some(s) = v["value"].as_str() {
            s.into()
        } else if let Some(a) = v.as_array() {
            a.iter().map(part).collect::<Vec<_>>().join("\n")
        } else {
            String::new()
        }
    }
    lsp::bounded(&part(&value["contents"]), 8192)
}
fn definition_position(reply: &lsp::Reply) -> Option<(usize, usize)> {
    let value = reply
        .result
        .as_array()
        .and_then(|v| v.first())
        .unwrap_or(&reply.result);
    let uri = value["uri"]
        .as_str()
        .or_else(|| value["targetUri"].as_str())?;
    if uri != lsp::URI {
        return None;
    }
    let range = value.get("targetSelectionRange").unwrap_or(&value["range"]);
    Some((
        range["start"]["line"].as_u64()? as usize,
        range["start"]["character"].as_u64()? as usize,
    ))
}
fn definition_text(reply: &lsp::Reply, zh: bool) -> String {
    if let Some((line, column)) = definition_position(reply) {
        if line < reply.request.prefix {
            format!(
                "{}\n{}",
                tr(
                    zh,
                    "宿主 / 公共参数声明（只读）",
                    "Host / public parameter declaration (read only)"
                ),
                reply.request.text.lines().nth(line).unwrap_or("")
            )
        } else {
            format!(
                "{} {}:{}",
                tr(zh, "已定位定义", "Definition located"),
                line - reply.request.prefix + 1,
                column + 1
            )
        }
    } else {
        tr(
            zh,
            "未找到当前作者源码内的定义",
            "No definition in this author source",
        )
    }
}
pub(super) fn completion_edit(
    source: &str,
    caret: usize,
    reply: &lsp::Reply,
    item: &Value,
) -> Result<(usize, usize, String), String> {
    if lsp::position(source, caret) != Some(reply.request.position) {
        return Err("光标已移动，请重新请求补全 / Caret moved; request completion again".into());
    }
    if item["additionalTextEdits"]
        .as_array()
        .is_some_and(|v| !v.is_empty())
    {
        return Err("此补全需要额外编辑，尚不支持；草稿已保留 / Additional edits unsupported; draft retained".into());
    }
    if item["insertTextFormat"].as_u64() == Some(2) {
        return Err(
            "此服务器返回了未请求的片段补全 / Server returned unsupported snippet completion"
                .into(),
        );
    }
    let text = item["textEdit"]["newText"]
        .as_str()
        .or_else(|| item["insertText"].as_str())
        .or_else(|| item["label"].as_str())
        .ok_or("completion contains no insertion text")?
        .to_owned();
    if let Some(range) = item["textEdit"]
        .get("range")
        .or_else(|| item["textEdit"].get("replace"))
    {
        let to_byte = |value: &Value| -> Option<usize> {
            let line = value["line"].as_u64()? as usize;
            let line = line.checked_sub(reply.request.prefix)?;
            lsp::byte(source, line, value["character"].as_u64()? as usize)
        };
        let start = to_byte(&range["start"]).ok_or("completion starts outside authored source")?;
        let end = to_byte(&range["end"]).ok_or("completion ends outside authored source")?;
        if start > end {
            return Err("completion range is reversed".into());
        }
        Ok((start, end, text))
    } else {
        let prefix = source.get(..caret).ok_or("completion caret is invalid")?;
        let start = prefix
            .char_indices()
            .rev()
            .find(|(_, c)| !c.is_alphanumeric() && *c != '_')
            .map_or(0, |(i, c)| i + c.len_utf8());
        Ok((start, caret, text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(windows)]
    #[ignore = "explicit native language navigation failure capture; Windows GPU required"]
    fn invalid_language_diagnostic_click_retains_draft_and_renders_feedback() {
        use crate::engine::{AtlasUpload, Engine, UiFrame};
        use winit::{dpi::LogicalSize, event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows, window::Window};
        let temp = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_PLUGIN_NAVIGATION_CAPTURE_DIR")
            .map(PathBuf::from).unwrap_or_else(|| temp.path().to_owned());
        std::fs::create_dir_all(&root).unwrap();
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let mut events = EventLoop::<()>::builder().with_any_thread(true).build().unwrap();
        #[allow(deprecated)]
        let window = std::sync::Arc::new(events.create_window(Window::default_attributes()
            .with_visible(false).with_inner_size(LogicalSize::new(980., 660.))).unwrap());
        let engine = Engine::start(root.join("sample"), motion_desktop_media::platform(), crate::total_memory()).unwrap();
        let attached = engine.attach_async(window, 980, 660).unwrap();
        crate::native_test_support::receive(&mut events, &attached, Duration::from_secs(60)).unwrap().unwrap();
        let styles = motion_ui::style::default_stylesheet_arc();
        let mut atlas = TextAtlas::from_family(styles.font_family(), styles.base_font_size(), true).unwrap();
        let area = Rect::new(0., 0., 980., 660.).shrink(styles.metric("padding.panel", 8.));
        for zh in [true, false] {
            let mut project = super::super::super::tests::example();
            let text = "// 😀 Unicode diagnostic\nfn plugin_fx(p: vec2<f32>) -> vec4<f32> { return vec4<f32>(1.0); }";
            if let PluginLogic::Wgsl { document } = &mut project.logic { document.source = text.into(); }
            let mut draft = model::Draft::new(project.clone());
            let mut source = Source::new(&project);
            source.language_attempted = true;
            source.language_snapshot = Some(lsp::Snapshot {
                status: lsp::Status::Ready, flight: None, reply: None,
                diagnostics: vec![lsp::Diagnostic { line: 0, column: 4, message: "Controlled invalid server column".into(), error: true }],
                revision: draft.revision, changed: 1,
            });
            let before_caret = source.editor.caret_byte_index();
            let row = styles.metric("row.height", 22.);
            let point = [area.min[0] + 1., area.max[1] - styles.metric("plugin.author.code.details_height", 132.) + row * 0.5];
            let click = Input { mouse: point, pressed: Some((MouseButton::Left, point)), ..neutral() };
            let mut paint = PaintList { stylesheet: Some(styles.clone()), ..Default::default() };
            source.draw(&mut draft, &mut paint, &mut atlas, area, &click, zh).unwrap();
            assert!(source.language_error.is_some(), "invalid diagnostic click was silently ignored");
            assert_eq!(source.editor.caret_byte_index(), before_caret);
            assert_eq!(draft.project, project);
            let mut paint = PaintList { stylesheet: Some(styles.clone()), ..Default::default() };
            source.draw(&mut draft, &mut paint, &mut atlas, area, &neutral(), zh).unwrap();
            let cfg = atlas.config();
            crate::native_test_support::capture_ui(&mut events, &engine, UiFrame { views: None,
                paint, atlas: Some(AtlasUpload { pixels: atlas.pixel_data().to_vec(), width: cfg.width, height: cfg.height }),
                logical_size: [980., 660.], composition: None, composition_image: None,
                inspection: Default::default(), window: 0, node_preview: None,
            }, root.join(if zh { "zh-invalid-navigation.png" } else { "en-invalid-navigation.png" })).unwrap();
            assert_eq!(draft.project, project);
        }
    }
    #[test]
    fn language_navigation_preserves_utf16_selection_and_reports_invalid_positions() {
        const CHILD: &str = "MOTION_PLUGIN_NAVIGATION_CHILD";
        const TEST: &str = "plugin_editor::author::source::assistance::tests::language_navigation_preserves_utf16_selection_and_reports_invalid_positions";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = PathBuf::from(root);
            let _logger = crate::logging::init(root.join("logs")).unwrap();
            let text = "// private_source_😀中\nfn main() {}";
            let mut source = Source::default();
            source.editor.begin_multiline(text, 1024);
            let start = text.find('😀').unwrap();
            let column = text[..start].encode_utf16().count();
            assert!(source.locate_language(0, column + 2, "diagnostic"));
            let caret = source.editor.caret_byte_index();
            assert_eq!(caret, start + '😀'.len_utf8());
            for (line, column, operation) in [
                (0, column + 1, "diagnostic"),
                (0, 500, "definition"),
                (500, 0, "diagnostic"),
            ] {
                assert!(!source.locate_language(line, column, operation));
                assert!(source.language_error.as_deref().unwrap().contains("Invalid language-service position"));
                assert_eq!(source.editor.caret_byte_index(), caret);
                assert_eq!(source.editor.text, text);
            }
            assert!(source.locate_language(1, 3, "definition"));
            assert!(source.language_error.is_none());
            assert_eq!(source.editor.caret_byte_index(), text.find("main").unwrap());
            crate::logging::flush();
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture"])
            .env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
        assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        let file = std::fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        for sink in [&file[..], &stderr[..]] {
            assert_eq!(sink.matches("plugin.language navigation operation=").count(), 3);
            assert!(sink.contains("operation=definition line=0 utf16_column=500"));
            assert!(sink.contains("cause=invalid UTF-16 position"));
            assert!(!sink.contains("private_source_"));
        }
        assert!(!stdout.contains("plugin.language navigation"));
    }
    #[test]
    fn completion_maps_utf16_and_rejects_stale_or_external_edits() {
        let text = "// 😀\nlet value = par;";
        let caret = text.find("par;").unwrap() + 3;
        let request = lsp::Request {
            revision: 4,
            text: format!("host\n{text}"),
            prefix: 1,
            position: lsp::position(text, caret).unwrap(),
            kind: lsp::Kind::Completion,
        };
        let reply = lsp::Reply {
            id: 1,
            request,
            result: Value::Null,
        };
        let item = serde_json::json!({"label":"param_amount","textEdit":{"range":{"start":{"line":2,"character":12},"end":{"line":2,"character":15}},"newText":"param_amount()"}});
        let (start, end, insert) = completion_edit(text, caret, &reply, &item).unwrap();
        assert_eq!(&text[start..end], "par");
        assert_eq!(insert, "param_amount()");
        assert!(completion_edit(text, caret - 1, &reply, &item).is_err());
        let mut external = item.clone();
        external["textEdit"]["range"]["start"]["line"] = serde_json::json!(0);
        assert!(completion_edit(text, caret, &reply, &external).is_err());
        let mut edits = item;
        edits["additionalTextEdits"] = serde_json::json!([{"newText":"x"}]);
        assert!(completion_edit(text, caret, &reply, &edits).is_err());
    }
}

fn retain_keys(input: &mut Input, keep: impl Fn(&Key, &motion_ui::input::Modifiers) -> bool) {
    let keys: Vec<_> = input.key_presses().filter(|(k, m, _)| keep(k, m)).collect();
    input.keys = keys.iter().map(|(k, m, _)| (*k, *m)).collect();
    input.key_repeats = keys.iter().map(|(_, _, repeat)| *repeat).collect();
}
