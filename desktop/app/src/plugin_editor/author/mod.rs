//! Shared author workspace; window ownership and engine delivery stay outside.
mod designer;
mod guide;
mod lsp;
mod model;
#[cfg(all(test, target_os = "windows"))]
mod native;
mod parameters;
mod source;
#[cfg(test)]
mod tests;
mod worker;
use super::*;
use motion_graph::plugin_project::{PluginLogic, PluginProject};
use motion_ui::widgets::{DropdownItem, TextEditor, TextEvent, TextViewport};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    time::{Duration, Instant},
};
pub(super) use worker::Ready;
pub struct Workspace {
    draft: model::Draft,
    path: PathBuf,
    page: usize,
    interaction: bool,
    preview: Preview,
    designer: designer::Designer,
    nodes: Option<crate::node_editor::NodeEditor>,
    source: source::Source,
    parameters: parameters::Parameters,
    worker: worker::Worker,
    busy: bool,
    checked: u64,
    changed: Instant,
    observed: u64,
    runtime_revision: Option<u64>,
    worker_failed: bool,
    generated: String,
    generated_view: source::Source,
    logic_tab: usize,
    source_tab: usize,
    guide: guide::Guide,
    pub ready: Option<(u64, Ready)>,
    pub message: Option<worker::Fault>,
    pub close_prompt: bool,
    pub close_after_save: bool,
    pub close_ready: bool,
    pub redraw: bool,
    zh: bool,
}
impl Workspace {
    pub fn new(project: PluginProject, path: PathBuf, zh: bool) -> Result<Self, String> {
        let definitions = project.parameters().map_err(|e| e.to_string())?;
        // Drafts with dangling controls are allowed; they are diagnosed rather
        // than rewritten. Their last valid runtime is installed only on Check.
        let preview = Preview::new(empty_layout(), definitions, zh)?;
        let nodes = match &project.logic {
            PluginLogic::Graph { document } => {
                let mut n = crate::node_editor::NodeEditor::new(document.clone());
                n.set_authoring_mode(project.compile_options());
                Some(n)
            }
            _ => None,
        };
        let source = source::Source::new(&project);
        Ok(Self {
            draft: model::Draft::new(project),
            path,
            page: 1,
            interaction: false,
            preview,
            designer: Default::default(),
            nodes,
            source,
            parameters: Default::default(),
            worker: worker::Worker::new()?,
            busy: false,
            checked: u64::MAX,
            changed: Instant::now() - Duration::from_secs(1),
            observed: 0,
            runtime_revision: None,
            worker_failed: false,
            generated: String::new(),
            generated_view: Default::default(),
            logic_tab: 0,
            source_tab: 1,
            guide: Default::default(),
            ready: None,
            message: None,
            close_prompt: false,
            close_after_save: false,
            close_ready: false,
            redraw: true,
            zh,
        })
    }
    fn fault(&mut self, phase: &'static str, error: impl std::fmt::Display) {
        let message = crate::fallback_observation::error_cause(error);
        if !self
            .message
            .as_ref()
            .is_some_and(|old| old.phase == phase && old.message == message)
        {
            log::error!(
                "plugin.author error phase={phase} revision={} source={} cause={message}",
                self.draft.revision,
                crate::fallback_observation::short(self.path.display())
            );
        }
        self.message = Some(worker::Fault {
            phase,
            message,
            location: None,
        });
        self.redraw = true;
    }
    pub fn pending(&self) -> bool {
        self.source.pending()
            || !self.worker_failed && (self.busy || self.checked != self.draft.revision)
    }
    pub fn cancel(&mut self) {
        self.designer.cancel(&mut self.draft);
        self.parameters.cancel(&mut self.draft);
        if let Some(n) = &mut self.nodes {
            if n.editing() {
                self.draft.cancel();
            }
            n.cancel_interaction();
        }
        self.source.cancel(&mut self.draft);
        self.redraw = true;
    }
    fn sync_views(&mut self) {
        self.source.replace_project(&self.draft.project);
        match &self.draft.project.logic {
            PluginLogic::Graph { document } => {
                if let Some(n) = &mut self.nodes {
                    n.replace_document(document.clone(), None);
                } else {
                    let mut n = crate::node_editor::NodeEditor::new(document.clone());
                    n.set_authoring_mode(self.draft.project.compile_options());
                    self.nodes = Some(n);
                }
            }
            _ => self.nodes = None,
        }
        self.designer.selected = None;
        self.redraw = true;
    }
    fn submit(&mut self, job: worker::Job) -> Result<(), String> {
        match self.worker.submit(job) {
            Ok(()) => self.busy = true,
            Err(error) => {
                self.worker_failed = true;
                self.fault("worker", error);
            }
        }
        self.redraw = true;
        Ok(())
    }
    pub fn poll(&mut self) -> Result<(), String> {
        match self.source.poll_language() {
            Ok(changed) => self.redraw |= changed,
            Err(error) => self.fault("language", error),
        }
        if self.worker_failed {
            return Ok(());
        }
        use std::sync::mpsc::TryRecvError;
        match self.worker.reply.try_recv() {
            Ok(event) => {
                self.busy = false;
                self.redraw = true;
                match event {
                    worker::Event::Loaded(path, project) => {
                        let next = self.draft.revision.wrapping_add(1);
                        self.draft = model::Draft::new(project);
                        self.draft.revision = next;
                        self.path = path;
                        self.checked = u64::MAX;
                        self.observed = next;
                        self.generated.clear();
                        self.message = None;
                        self.sync_views();
                    }
                    worker::Event::Compiled(revision, result) => {
                        if revision == self.draft.revision {
                            match result {
                                Ok(ready) => {
                                    self.generated = ready.generated.clone();
                                    self.ready = Some((revision, ready));
                                    self.message = None;
                                }
                                Err(fault) => self.message = Some(fault),
                            }
                        }
                        // Older compilation results are normal superseded work.
                    }
                    worker::Event::Saved(_revision, path, snapshot) => {
                        self.draft.saved(snapshot);
                        self.path = path;
                        if self.close_after_save && !self.draft.dirty() {
                            self.close_ready = true;
                        }
                        self.close_after_save = false;
                    }
                    worker::Event::Failed(fault) => {
                        self.message = Some(fault);
                        self.close_after_save = false;
                    }
                    worker::Event::Cancelled => self.close_after_save = false,
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.worker_failed = true;
                self.busy = false;
                self.fault(
                    "worker",
                    "plugin author source worker disconnected before completion; draft retained",
                );
                return Ok(());
            }
        }
        if self.observed != self.draft.revision {
            self.observed = self.draft.revision;
            self.changed = Instant::now();
            self.redraw = true;
        }
        if !self.busy
            && (!self.draft.active() || (self.page == 0 && self.nodes.is_none()))
            && self.checked != self.draft.revision
            && self.changed.elapsed() >= Duration::from_millis(300)
        {
            self.check()?;
        }
        Ok(())
    }
    fn finish_fields(&mut self) -> Result<(), String> {
        self.designer.finish(&mut self.draft)?;
        self.parameters.finish(&mut self.draft)?;
        if let Some(nodes) = &mut self.nodes {
            nodes
                .request(crate::node_editor::Request::Save)
                .map_err(|e| e.to_string())?;
            let snapshot = nodes.snapshot();
            self.draft.update(|p| {
                p.logic = PluginLogic::Graph {
                    document: snapshot.document,
                };
                p.package = motion_graph::plugin_project::PackageIdentity {
                    id: snapshot.options.plugin_id,
                    version: snapshot.options.version,
                    author: snapshot.options.author,
                    license: snapshot.options.license,
                };
                Ok(())
            })?;
        }
        self.draft.commit();
        Ok(())
    }
    fn check(&mut self) -> Result<(), String> {
        self.draft.commit();
        self.checked = self.draft.revision;
        self.submit(worker::Job::Compile(
            self.draft.revision,
            self.draft.project.clone(),
        ))
    }
    fn save(&mut self, save_as: bool) -> Result<(), String> {
        self.draft.commit();
        self.submit(worker::Job::Save(
            self.draft.revision,
            self.draft.project.clone(),
            (!save_as).then(|| self.path.clone()),
        ))
    }
    pub fn request_close(&mut self) {
        if let Err(error) = self.finish_fields() {
            self.fault("close", error);
            return;
        }
        self.draft.commit();
        self.cancel();
        if self.draft.dirty() || self.busy {
            self.close_prompt = true;
        } else {
            self.close_ready = true;
        }
        self.redraw = true;
    }
    pub(super) fn recovery_source(&mut self) -> Result<(PluginProject, PathBuf), String> {
        if self.busy {
            return Err("author source operation is still running; draft retained".into());
        }
        self.finish_fields()?;
        Ok((self.draft.project.clone(), self.path.clone()))
    }
    pub(super) fn unsaved(&self) -> bool {
        self.draft.dirty()
            || self.designer.busy()
            || self.parameters.editing()
            || self.source.editing()
    }
    pub fn take_ready(&mut self) -> Option<(u64, Ready)> {
        self.ready.take().filter(|(r, _)| *r == self.draft.revision)
    }
    pub fn runtime_error(&mut self, error: String) {
        self.fault("gpu_install", error);
    }
    fn history(&mut self, redo: bool) {
        self.cancel();
        if redo {
            self.draft.redo();
        } else {
            self.draft.undo();
        }
        self.sync_views();
    }
    pub fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        input: &Input,
    ) -> Result<Option<Rect>, String> {
        self.redraw = false;
        let sheet = list.styles();
        let row = sheet.metric("panel.header.height", 24.);
        let pad = sheet.metric("padding.panel", 8.);
        let action = sheet.metric("plugin.author.action.width", 80.);
        let disabled = neutral();
        let raw_input = input;
        let modal = self.close_prompt;
        let input = if modal { &disabled } else { input };
        let mut file_action = None;
        let text_owned = self.source.editing() && self.page == 0
            || self.designer.busy()
            || self.parameters.editing()
            || self
                .nodes
                .as_ref()
                .is_some_and(|n| n.text_editing() || n.popup_open());
        if !input.composing && !input.ime_consumed && !input.focus_lost {
            if input.key_presses().any(|(k, _, r)| k == Key::F1 && !r) {
                self.draft.commit();
                self.cancel();
                self.page = 3;
            }
            for (k, m, r) in input.key_presses() {
                if !r && m.control {
                    match k {
                        Key::S if !self.busy => {
                            file_action = Some(if m.shift { 2 } else { 1 });
                        }
                        Key::O if !self.busy => {
                            file_action = Some(0);
                        }
                        Key::Z
                            if !text_owned
                                && !(self.interaction && self.page != 0 || self.page == 2) =>
                        {
                            self.history(m.shift)
                        }
                        Key::Y
                            if !text_owned
                                && !(self.interaction && self.page != 0 || self.page == 2) =>
                        {
                            self.history(true)
                        }
                        _ => {}
                    }
                }
            }
        }
        let mut x = area.min[0];
        let buttons = [
            (
                tr(self.zh, "打开", "Open"),
                !self.busy && !self.draft.dirty(),
            ),
            (tr(self.zh, "保存", "Save"), !self.busy),
            (tr(self.zh, "另存为", "Save as"), !self.busy),
            (tr(self.zh, "检查预览", "Check"), !self.busy),
            (
                tr(self.zh, "撤销", "Undo"),
                if self.interaction && self.page != 0 || self.page == 2 {
                    self.preview.session.can_undo()
                } else {
                    self.draft.can_undo()
                },
            ),
            (
                tr(self.zh, "重做", "Redo"),
                if self.interaction && self.page != 0 || self.page == 2 {
                    self.preview.session.can_redo()
                } else {
                    self.draft.can_redo()
                },
            ),
        ];
        for (i, (label, enabled)) in buttons.into_iter().enumerate() {
            let r = Rect::new(x, area.min[1], action, row);
            if ui::button(list, atlas, r, &label, input, enabled).clicked {
                match i {
                    0..=3 => file_action = Some(i),
                    4 => {
                        if self.interaction && self.page != 0 || self.page == 2 {
                            self.preview.session.undo();
                        } else {
                            self.history(false);
                        }
                    }
                    _ => {
                        if self.interaction && self.page != 0 || self.page == 2 {
                            self.preview.session.redo();
                        } else {
                            self.history(true);
                        }
                    }
                }
            }
            x += action + sheet.metric("padding", 5.);
        }
        let title = format!(
            "{}{}",
            self.path.file_name().unwrap_or_default().to_string_lossy(),
            if self.draft.dirty() { " *" } else { "" }
        );
        ui::text(
            list,
            atlas,
            Rect::new(x, area.min[1], (area.max[0] - x).max(0.), row),
            &title,
            sheet.color("muted"),
            Align::Right,
        );
        let tabs = Rect::new(area.min[0], area.min[1] + row + pad, area.width(), row);
        if let Some(page) = ui::tab_strip(
            list,
            atlas,
            tabs,
            &[
                &tr(self.zh, "逻辑", "Logic"),
                &tr(self.zh, "界面", "Interface"),
                &tr(self.zh, "预览与诊断", "Preview & diagnostics"),
                &tr(self.zh, "使用教程", "Guide"),
            ],
            self.page,
            input,
        ) {
            self.draft.commit();
            self.cancel();
            self.page = page;
            self.sync_views();
        }
        let bottom = area.max[1] - row * 2.;
        let content = Rect::new(
            area.min[0],
            tabs.max[1] + pad,
            area.width(),
            (bottom - tabs.max[1] - pad).max(0.),
        );
        let mut slot = None;
        let result: Result<(), String> = (|| {
            if self.page == 3 {
                if let Some(page) = self.guide.draw(list, atlas, content, input, self.zh) {
                    self.page = page;
                    self.redraw = true;
                }
            } else if self.page == 0 {
                if let Some(nodes) = &mut self.nodes {
                    let mut filtered = input.clone();
                    let text_owner = nodes.text_editing() || nodes.popup_open();
                    filtered.keys.retain(|(k, m)| {
                        !m.control
                            || (!matches!(k, Key::S | Key::O)
                                && (text_owner || !matches!(k, Key::Z | Key::Y)))
                    });
                    filtered.key_repeats.clear();
                    let top = Rect::new(content.min[0], content.min[1], content.width(), row);
                    if let Some(tab) = ui::tab_strip(
                        list,
                        atlas,
                        top,
                        &[
                            &tr(self.zh, "节点图", "Node graph"),
                            &tr(self.zh, "生成的 WGSL（只读）", "Generated WGSL (read only)"),
                        ],
                        self.logic_tab,
                        input,
                    ) {
                        self.logic_tab = tab;
                    }
                    let node_area = Rect::new(
                        content.min[0],
                        top.max[1],
                        content.width(),
                        (content.height() - row).max(0.),
                    );
                    let response = if self.logic_tab == 0 {
                        nodes.draw(
                            list,
                            atlas,
                            node_area,
                            &filtered,
                            if self.zh {
                                crate::node_editor::Language::Chinese
                            } else {
                                crate::node_editor::Language::English
                            },
                        )
                    } else {
                        self.generated_view.read_only(
                            &self.generated,
                            list,
                            atlas,
                            node_area,
                            input,
                        );
                        crate::node_editor::FrameResponse::default()
                    };
                    let snapshot = nodes.snapshot();
                    let document = snapshot.document;
                    let package = motion_graph::plugin_project::PackageIdentity {
                        id: snapshot.options.plugin_id,
                        version: snapshot.options.version,
                        author: snapshot.options.author,
                        license: snapshot.options.license,
                    };
                    if self.draft.project.package != package
                        || matches!(&self.draft.project.logic,PluginLogic::Graph{document:old} if old!=&document)
                    {
                        self.draft.update(|p| {
                            p.logic = PluginLogic::Graph { document };
                            p.package = package;
                            Ok(())
                        })?;
                    }
                    if !nodes.editing() {
                        self.draft.commit();
                    }
                    for action in response.actions {
                        match action {
                            crate::node_editor::Action::Compile(_)
                            | crate::node_editor::Action::Validate(_) => {
                                if !self.busy {
                                    self.check()?;
                                }
                            }
                            crate::node_editor::Action::Save(_)
                            | crate::node_editor::Action::SaveAs(_) => {
                                if !self.busy {
                                    self.save(false)?;
                                }
                            }
                            crate::node_editor::Action::ImportImage { path, .. } => {
                                self.fault("image_import", format!("author resource import is not connected yet; draft retained source={}",crate::fallback_observation::short(path.display())));
                            }
                            crate::node_editor::Action::ViewPreferencesChanged(_)
                            | crate::node_editor::Action::RecentNodesChanged(_)
                            | crate::node_editor::Action::PreviewChanged
                            | crate::node_editor::Action::Changed { .. } => { /* Native view/model already applied; no host side effect. */
                            }
                            _ => self.fault(
                                "node_command",
                                "this command requires host integration or package publishing",
                            ),
                        }
                    }
                } else {
                    let width = sheet.metric("plugin.preview.row.min_width", 288.);
                    let wide = content.width()
                        >= width + sheet.metric("plugin.author.code.min_width", 480.) + pad;
                    let mut body = content;
                    if !wide {
                        if let Some(tab) = ui::tab_strip(
                            list,
                            atlas,
                            Rect::new(content.min[0], content.min[1], content.width(), row),
                            &[&tr(self.zh, "公开参数", "Public parameters"), "WGSL"],
                            self.source_tab,
                            input,
                        ) {
                            self.parameters.finish(&mut self.draft)?;
                            self.source.cancel(&mut self.draft);
                            self.source_tab = tab;
                        }
                        body.min[1] += row + pad;
                    }
                    let params = Rect::new(
                        body.min[0],
                        body.min[1],
                        if wide { width } else { body.width() },
                        body.height(),
                    );
                    if wide || self.source_tab == 0 {
                        self.parameters.draw(
                            &mut self.draft,
                            list,
                            atlas,
                            params,
                            input,
                            self.zh,
                        )?;
                    }
                    if wide || self.source_tab == 1 {
                        let code = if wide {
                            Rect::new(
                                params.max[0] + pad,
                                body.min[1],
                                (body.width() - width - pad).max(0.),
                                body.height(),
                            )
                        } else {
                            body
                        };
                        self.source.draw(
                            &mut self.draft,
                            list,
                            atlas,
                            code,
                            if self.parameters.editing() {
                                &disabled
                            } else {
                                input
                            },
                            self.zh,
                        )?;
                    }
                }
            } else if self.page == 1 {
                let mode = Rect::new(content.min[0], content.min[1], content.width(), row);
                if let Some(m) = ui::tab_strip(
                    list,
                    atlas,
                    mode,
                    &[
                        &tr(self.zh, "设计布局", "Design layout"),
                        &tr(self.zh, "交互预览（示例值）", "Interact (sample values)"),
                    ],
                    usize::from(self.interaction),
                    input,
                ) {
                    self.cancel();
                    self.interaction = m == 1;
                }
                let body = Rect::new(
                    content.min[0],
                    mode.max[1] + pad,
                    content.width(),
                    (content.height() - row - pad).max(0.),
                );
                if self.interaction {
                    self.preview.layout = self.draft.project.interface.clone();
                    slot = self.preview.draw(list, atlas, body, body, input)?;
                } else {
                    slot = self.designer.draw(
                        &mut self.draft,
                        &mut self.preview,
                        list,
                        atlas,
                        body,
                        input,
                        self.zh,
                    )?;
                }
            } else {
                let width = sheet
                    .metric("plugin.author.canvas.wide", 480.)
                    .min(content.width() * 0.5);
                let canvas = Rect::new(content.min[0], content.min[1], width, content.height());
                let side = canvas.width().min(canvas.height());
                slot = Some(Rect::new(canvas.min[0], canvas.min[1], side, side));
                let panel = Rect::new(
                    canvas.max[0] + pad,
                    content.min[1],
                    (content.max[0] - canvas.max[0] - pad).max(0.),
                    content.height(),
                );
                self.preview.layout = self.draft.project.interface.clone();
                let _ = self.preview.draw(list, atlas, panel, panel, input)?;
            }
            Ok(())
        })();
        if let Err(e) = result {
            self.fault("interaction", e);
        }
        if let Some(action) = file_action {
            let result = (|| {
                self.finish_fields()?;
                match action {
                    0 => {
                        if self.draft.dirty() {
                            Err(
                                "save or discard the current draft before opening another source"
                                    .into(),
                            )
                        } else {
                            self.submit(worker::Job::Load(None))
                        }
                    }
                    1 => self.save(false),
                    2 => self.save(true),
                    _ => self.check(),
                }
            })();
            if let Err(error) = result {
                self.fault("author_command", error);
            }
        }
        let status = if let Some(f) = &self.message {
            format!(
                "{}{}: {}",
                f.phase,
                f.location
                    .map(|(l, c)| format!(" {l}:{c}"))
                    .unwrap_or_default(),
                f.message
            )
        } else if self.busy {
            let flight = self
                .worker
                .flight
                .lock()
                .map(|f| f.clone())
                .map_err(|_| "author in-flight lock poisoned")?;
            flight
                .map(|f| {
                    format!(
                        "#{} {} · {} ms · {}",
                        f.request,
                        f.phase,
                        f.began.elapsed().as_millis(),
                        tr(self.zh, "保留上一次有效预览", "Last valid preview retained")
                    )
                })
                .unwrap_or_else(|| tr(self.zh, "正在处理作者工程…", "Processing author source…"))
        } else {
            tr(
                self.zh,
                "独立示例 · 未连接宿主 · 设计只改布局，交互只改示例值",
                "Independent sample · no host · layout and sample gestures are separate",
            )
        };
        ui::text(
            list,
            atlas,
            Rect::new(
                area.min[0],
                bottom,
                (area.width() - action - pad).max(0.),
                row * 2.,
            ),
            &status,
            if self.message.is_some() {
                sheet.color("error")
            } else {
                sheet.color("muted")
            },
            Align::Left,
        );
        if let Some((line, column)) = self.message.as_ref().and_then(|f| f.location) {
            if ui::button(
                list,
                atlas,
                Rect::new(area.max[0] - action, bottom, action, row),
                &tr(self.zh, "定位源码", "Locate source"),
                input,
                true,
            )
            .clicked
            {
                self.draft.commit();
                self.page = 0;
                self.source.locate(line, column);
                self.redraw = true;
            }
        }
        if self.close_prompt {
            let w = (action + pad) * 3. + pad;
            let r = Rect::new(
                area.min[0] + (area.width() - w) * 0.5,
                area.min[1] + area.height() * 0.5 - row,
                w,
                row * 3. + pad * 2.,
            );
            paint::rect(list, r, sheet.color("panel"));
            ui::text(
                list,
                atlas,
                Rect::new(r.min[0] + pad, r.min[1] + pad, r.width() - pad * 2., row),
                &tr(
                    self.zh,
                    "保存未完成的作者草稿？",
                    "Save the author draft before closing?",
                ),
                sheet.color("text"),
                Align::Left,
            );
            for (i, label) in [
                tr(self.zh, "保存并关闭", "Save & close"),
                tr(self.zh, "放弃并关闭", "Discard"),
                tr(self.zh, "继续编辑", "Keep editing"),
            ]
            .into_iter()
            .enumerate()
            {
                let b = Rect::new(
                    r.min[0] + pad + i as f32 * (action + pad),
                    r.max[1] - pad - row,
                    action,
                    row,
                );
                if ui::button(list, atlas, b, &label, raw_input, !self.busy || i == 2).clicked {
                    match i {
                        0 => {
                            self.close_after_save = true;
                            self.save(false)?;
                        }
                        1 => self.close_ready = true,
                        _ => {
                            self.close_prompt = false;
                            // A save already queued still completes, but its reply
                            // must not revive the close approval just cancelled.
                            self.close_after_save = false;
                        }
                    }
                }
            }
        }
        if self.observed != self.draft.revision {
            self.redraw = true;
        }
        Ok(slot)
    }
    pub fn values(&self) -> &[[f32; 4]] {
        self.preview.session.values()
    }
    pub fn value_revision(&self) -> u64 {
        self.preview.session.revision
    }
    pub fn install_preview(
        &mut self,
        revision: u64,
        definitions: Vec<motion_effects::ParamDefinition>,
    ) -> Result<(), String> {
        self.runtime_revision = Some(revision);
        self.preview.cancel();
        self.preview.session = session::Session::new(definitions)?;
        self.redraw = true;
        Ok(())
    }
    pub fn runtime_current(&self) -> bool {
        self.runtime_revision == Some(self.draft.revision)
    }
}
fn empty_layout() -> motion_effects::native_layout::NativeLayout {
    use motion_effects::native_layout::*;
    NativeLayout {
        format_version: LAYOUT_VERSION,
        root: LayoutNode {
            id: "root".into(),
            width: LayoutExtent::Auto,
            height: LayoutExtent::Auto,
            content: LayoutContent::Column {
                gap: LayoutSpacing::None,
                align: LayoutAlign::Stretch,
                children: vec![],
            },
        },
    }
}
