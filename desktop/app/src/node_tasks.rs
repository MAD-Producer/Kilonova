//! Serial file operations and a separate graph/GPU compiler keep native dialogs
//! off the GUI thread and do not stall live preview compilation.
use crate::{
    i18n::{Locale, Text as T},
    node_editor::{Action, EditorSnapshot, NodeEditor},
    node_preview::{GpuContext, NodePreview},
};
use motion_graph::{
    CompileReport, Diagnostic, ErrorCode, ExportReport, GraphDocument, ValidationReport,
};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender, TryRecvError},
        Arc,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseChoice {
    Save,
    Discard,
    Cancel,
}
pub fn close_prompt(
    list: &mut motion_ui::PaintList,
    atlas: &mut motion_ui::TextAtlas,
    size: [f32; 2],
    input: &motion_ui::Input,
    locale: Locale,
) -> Option<CloseChoice> {
    let zh = locale == Locale::Zh;
    decision_prompt(list, atlas, size, input, locale, locale.t(T::NodeUnsaved),
        if zh { "退出前保存节点图？" } else { "Save the node graph before exiting?" },
        if zh { "节点图单独保存为 .msgraph；保存工程不会保存节点图。" }
        else { "Node graphs use .msgraph files; saving the project does not save this graph." }, false)
}

pub(crate) fn decision_prompt(
    list: &mut motion_ui::PaintList,
    atlas: &mut motion_ui::TextAtlas,
    size: [f32; 2],
    input: &motion_ui::Input,
    locale: Locale,
    title: &str,
    message: &str,
    note: &str,
    error: bool,
) -> Option<CloseChoice> {
    use motion_ui::{
        input::Key,
        paint,
        theme::palette,
        ui::{self, Align},
        Rect,
    };
    let zh = locale == Locale::Zh;
    let card = Rect::new((size[0] - 520.) * 0.5, (size[1] - 210.) * 0.5, 520., 210.);
    paint::rect(
        list,
        Rect::new(0., 0., size[0], size[1]),
        palette::SUNKEN.with_alpha(0.82),
    );
    paint::rect(list, card, palette::PANEL);
    paint::stroke(list, card, palette::BORDER, 1.);
    ui::text(
        list,
        atlas,
        Rect::new(card.min[0] + 22., card.min[1] + 16., 476., 32.),
        title,
        palette::TEXT,
        Align::Left,
    );
    ui::text(
        list,
        atlas,
        Rect::new(card.min[0] + 22., card.min[1] + 58., 476., 32.),
        message,
        palette::TEXT,
        Align::Left,
    );
    ui::text(
        list,
        atlas,
        Rect::new(card.min[0] + 22., card.min[1] + 97., 476., 32.),
        note,
        if error {
            motion_ui::style::default_stylesheet()
                .resolve("error", motion_ui::style::States::default())
                .map_or(palette::TEXT, |style| style.foreground)
        } else { palette::TEXT_MUTED },
        Align::Left,
    );
    if input.keys.iter().any(|(key, _)| *key == Key::Escape) {
        return Some(CloseChoice::Cancel);
    }
    for (index, choice, label) in [
        (0, CloseChoice::Save, if zh { "保存" } else { "Save" }),
        (1, CloseChoice::Discard, if zh { "放弃" } else { "Discard" }),
        (2, CloseChoice::Cancel, if zh { "取消" } else { "Cancel" }),
    ] {
        if ui::button(
            list,
            atlas,
            Rect::new(
                card.min[0] + 172. + index as f32 * 110.,
                card.max[1] - 54.,
                100.,
                30.,
            ),
            label,
            input,
            true,
        )
        .clicked
        {
            return Some(choice);
        }
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Open,
    Save,
    SaveAs,
    Validate,
    Compile,
    Export,
}
struct Job {
    id: u64,
    generation: u64,
    kind: Kind,
    snapshot: EditorSnapshot,
    locale: Locale,
    dirty: bool,
    path: Option<PathBuf>,
    gpu: Option<GpuContext>,
}
enum Outcome {
    Opened(GraphDocument, PathBuf),
    Saved(PathBuf),
    Validated(Result<ValidationReport, Diagnostic>),
    Compiled(Result<(CompileReport, Option<NodePreview>), Diagnostic>),
    Exported(ExportReport),
    Cancelled,
    Failed(String),
}
struct Completion {
    id: u64,
    generation: u64,
    snapshot: EditorSnapshot,
    outcome: Outcome,
}
pub struct ReadyPreview {
    pub snapshot: EditorSnapshot,
    pub gpu: NodePreview,
}
pub struct NodeTasks {
    io: Sender<Job>,
    compiler: Sender<Job>,
    receive: Receiver<Completion>,
    generation: Arc<AtomicU64>,
    latest_compile: Arc<AtomicU64>,
    next: u64,
    pending: BTreeMap<u64, (Kind, EditorSnapshot)>,
}
impl NodeTasks {
    pub fn start() -> Result<Self, String> {
        let (io, io_receive) = mpsc::channel::<Job>();
        let (compiler, compile_receive) = mpsc::channel::<Job>();
        let (reply, receive) = mpsc::channel();
        let generation = Arc::new(AtomicU64::new(1));
        let latest_compile = Arc::new(AtomicU64::new(0));
        for (name, receive) in [
            ("motion-node-files", io_receive),
            ("motion-node-compiler", compile_receive),
        ] {
            let reply = reply.clone();
            let generation = generation.clone();
            let latest_compile = latest_compile.clone();
            std::thread::Builder::new()
                .name(name.into())
                .spawn(move || {
                    while let Ok(job) = receive.recv() {
                        let outcome = if generation.load(Ordering::Acquire) != job.generation
                            || job.kind == Kind::Compile
                                && latest_compile.load(Ordering::Acquire) != job.id
                        {
                            Outcome::Cancelled
                        } else {
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                execute(&job, &generation, &latest_compile)
                            }))
                            .unwrap_or_else(|_| Outcome::Failed("node worker failed".into()))
                        };
                        let _ = reply.send(Completion {
                            id: job.id,
                            generation: job.generation,
                            snapshot: job.snapshot,
                            outcome,
                        });
                    }
                })
                .map_err(|e| e.to_string())?;
        }
        Ok(Self {
            io,
            compiler,
            receive,
            generation,
            latest_compile,
            next: 1,
            pending: BTreeMap::new(),
        })
    }
    pub fn busy(&self) -> bool {
        !self.pending.is_empty()
    }
    pub fn invalidate_compile(&self) {
        self.latest_compile.store(0, Ordering::Release);
    }
    pub fn compiling(&self, snapshot: &EditorSnapshot) -> bool {
        self.pending.values().any(|(k, s)| {
            *k == Kind::Compile
                && s.epoch == snapshot.epoch
                && s.preview_stamp() == snapshot.preview_stamp()
        })
    }
    pub fn submit(
        &mut self,
        action: Action,
        locale: Locale,
        dirty: bool,
        gpu: Option<GpuContext>,
    ) -> Result<(), String> {
        self.submit_inner(action, locale, dirty, None, gpu)
    }
    pub fn submit_to(
        &mut self,
        action: Action,
        path: PathBuf,
        locale: Locale,
        gpu: Option<GpuContext>,
    ) -> Result<(), String> {
        self.submit_inner(action, locale, false, Some(path), gpu)
    }
    fn submit_inner(
        &mut self,
        action: Action,
        locale: Locale,
        dirty: bool,
        path: Option<PathBuf>,
        gpu: Option<GpuContext>,
    ) -> Result<(), String> {
        let (kind, snapshot) = match action {
            Action::Open(s) => (Kind::Open, s),
            Action::Save(s) => (Kind::Save, s),
            Action::SaveAs(s) => (Kind::SaveAs, s),
            Action::Validate(s) => (Kind::Validate, s),
            Action::Compile(s) => (Kind::Compile, s),
            Action::ExportMsfx(s) => (Kind::Export, s),
            Action::Changed { .. } | Action::Examples(_) | Action::PreviewChanged => return Ok(()),
        };
        if kind == Kind::Compile && self.compiling(&snapshot) {
            return Ok(());
        }
        if self.pending.len() >= 16 {
            return Err("node operation queue is full".into());
        }
        let id = self.next;
        self.next += 1;
        if kind == Kind::Compile {
            self.latest_compile.store(id, Ordering::Release);
        }
        let sender = if kind == Kind::Compile || kind == Kind::Validate {
            &self.compiler
        } else {
            &self.io
        };
        self.pending.insert(id, (kind, snapshot.clone()));
        if sender
            .send(Job {
                id,
                generation: self.generation.load(Ordering::Acquire),
                kind,
                snapshot,
                locale,
                dirty,
                path,
                gpu,
            })
            .is_err()
        {
            self.pending.remove(&id);
            return Err("node worker is closed".into());
        }
        Ok(())
    }
    pub fn cancel(&mut self, editor: &mut NodeEditor) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        for (_, (_, snapshot)) in std::mem::take(&mut self.pending) {
            editor.cancel_compile(&snapshot);
        }
    }
    pub fn poll(&mut self, editor: &mut NodeEditor, locale: Locale) -> (bool, Vec<ReadyPreview>) {
        let mut changed = false;
        let mut ready = vec![];
        loop {
            let completion = match self.receive.try_recv() {
                Ok(c) => c,
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            };
            if self.pending.remove(&completion.id).is_none()
                || completion.generation != self.generation.load(Ordering::Acquire)
            {
                continue;
            }
            changed = true;
            let snapshot = completion.snapshot;
            if snapshot.epoch != editor.snapshot().epoch {
                editor.cancel_compile(&snapshot);
                continue;
            }
            match completion.outcome {
                Outcome::Opened(document, path) => {
                    if editor.accept_open(&snapshot, document, path.clone()) {
                        editor.report_status(format!(
                            "{}: {}",
                            locale.t(T::NodeOpened),
                            path.display()
                        ));
                    } else {
                        editor.report_error(locale.t(T::NodeStaleOpen));
                    }
                }
                Outcome::Saved(path) => {
                    if editor.accept_saved(&snapshot, path.clone()) {
                        editor.report_status(format!("{}: {}", locale.t(T::Saved), path.display()));
                    }
                }
                Outcome::Validated(result) => {
                    editor.accept_validation(&snapshot, result);
                }
                Outcome::Compiled(result) => match result {
                    Ok((report, gpu)) => {
                        if editor.accept_compile(&snapshot, Ok(report)) {
                            if let Some(gpu) = gpu {
                                ready.push(ReadyPreview { snapshot, gpu });
                            } else {
                                editor.report_status(locale.t(T::NodeCompiled));
                            }
                        }
                    }
                    Err(error) => {
                        editor.accept_compile(&snapshot, Err(error));
                    }
                },
                Outcome::Exported(report) => {
                    if editor.accept_saved(&snapshot, report.source_path) {
                        if snapshot.preview_target.is_none() {
                            editor.accept_compile(&snapshot, Ok(report.compile));
                        }
                        editor.report_status(format!(
                            "{}: {}",
                            locale.t(T::NodeExport),
                            report.path.display()
                        ));
                    }
                }
                Outcome::Cancelled => {
                    editor.cancel_compile(&snapshot);
                    if editor.stamp() == snapshot.preview_stamp() {
                        editor.report_status(locale.t(T::NodeCancelled));
                    }
                }
                Outcome::Failed(error) => {
                    editor.cancel_compile(&snapshot);
                    editor.report_error(locale.error(&error));
                }
            }
        }
        (changed, ready)
    }
}
impl Drop for NodeTasks {
    fn drop(&mut self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}
fn execute(job: &Job, generation: &AtomicU64, latest_compile: &AtomicU64) -> Outcome {
    let cancelled = || {
        generation.load(Ordering::Acquire) != job.generation
            || job.kind == Kind::Compile && latest_compile.load(Ordering::Acquire) != job.id
    };
    let result = (|| -> Result<Outcome, String> {
        if job.kind == Kind::Validate {
            return Ok(Outcome::Validated(
                job.snapshot.validate().map_err(|e| e.diagnostic()),
            ));
        }
        if job.kind == Kind::Compile {
            let result = job
                .snapshot
                .compile_preview()
                .map_err(|e| e.diagnostic())
                .and_then(|compiled| {
                    if cancelled() {
                        return Err(Diagnostic {
                            code: ErrorCode::Io,
                            node: None,
                            port: None,
                            message: "node compilation cancelled".into(),
                        });
                    }
                    let gpu = job
                        .gpu
                        .as_ref()
                        .map(|gpu| NodePreview::build(gpu, &job.snapshot, &compiled))
                        .transpose()
                        .map_err(|error| Diagnostic {
                            code: ErrorCode::Shader,
                            node: None,
                            port: None,
                            message: error,
                        })?;
                    Ok((compiled.report, gpu))
                });
            return Ok(if cancelled() {
                Outcome::Cancelled
            } else {
                Outcome::Compiled(result)
            });
        }
        if job.kind == Kind::Open
            && job.dirty
            && !rfd::MessageDialog::new()
                .set_title(job.locale.t(T::NodeUnsaved))
                .set_description(job.locale.t(T::NodeUnsavedOpen))
                .set_buttons(rfd::MessageButtons::OkCancel)
                .show()
                .eq(&rfd::MessageDialogResult::Ok)
        {
            return Ok(Outcome::Cancelled);
        }
        if cancelled() {
            return Ok(Outcome::Cancelled);
        }
        let path = if let Some(path) = &job.path {
            Some(path.clone())
        } else {
            match job.kind {
                Kind::Open => rfd::FileDialog::new()
                    .set_title(job.locale.t(T::NodeOpen))
                    .add_filter("Motion Node Graph", &["msgraph"])
                    .pick_file(),
                Kind::Save if job.snapshot.source_path.is_some() => {
                    job.snapshot.source_path.clone()
                }
                Kind::Save | Kind::SaveAs => rfd::FileDialog::new()
                    .set_title(job.locale.t(T::NodeSave))
                    .add_filter("Motion Node Graph", &["msgraph"])
                    .set_file_name(format!("{}.msgraph", job.snapshot.document.id))
                    .save_file(),
                Kind::Export => rfd::FileDialog::new()
                    .set_title(job.locale.t(T::NodeExport))
                    .add_filter("Motion Effect Package", &["msfx"])
                    .set_file_name(format!("{}.msfx", job.snapshot.document.id))
                    .save_file(),
                _ => None,
            }
        };
        let Some(mut path) = path else {
            return Ok(Outcome::Cancelled);
        };
        if cancelled() {
            return Ok(Outcome::Cancelled);
        }
        if job.kind != Kind::Open && path.extension().is_none() {
            path.set_extension(if job.kind == Kind::Export {
                "msfx"
            } else {
                "msgraph"
            });
        }
        match job.kind {
            Kind::Open => Ok(Outcome::Opened(
                GraphDocument::load(&path).map_err(|e| e.to_string())?,
                path,
            )),
            Kind::Save | Kind::SaveAs => {
                job.snapshot.save(&path).map_err(|e| e.to_string())?;
                Ok(Outcome::Saved(path))
            }
            Kind::Export => Ok(Outcome::Exported(
                job.snapshot.export_msfx(&path).map_err(|e| e.to_string())?,
            )),
            _ => unreachable!(),
        }
    })();
    if cancelled() {
        Outcome::Cancelled
    } else {
        result.unwrap_or_else(Outcome::Failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_editor::Request;
    fn drain(tasks: &mut NodeTasks, editor: &mut NodeEditor) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while tasks.busy() {
            assert!(std::time::Instant::now() < deadline);
            tasks.poll(editor, Locale::En);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    #[test]
    fn serial_source_saves_and_export_do_not_mutate_project_or_new_document() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("source.msgraph");
        let mut editor = NodeEditor::default();
        let mut tasks = NodeTasks::start().unwrap();
        tasks
            .submit_to(
                editor.request(Request::SaveAs).unwrap(),
                path.clone(),
                Locale::En,
                None,
            )
            .unwrap();
        editor
            .add_node(motion_graph::NodeOp::Float { value: 3. }, [0., 0.])
            .unwrap();
        tasks
            .submit_to(
                editor.request(Request::SaveAs).unwrap(),
                path.clone(),
                Locale::En,
                None,
            )
            .unwrap();
        drain(&mut tasks, &mut editor);
        assert_eq!(GraphDocument::load(&path).unwrap(), *editor.document());
        assert!(!editor.dirty());
        let package = directory.path().join("source.msfx");
        tasks
            .submit_to(
                editor.request(Request::ExportMsfx).unwrap(),
                package.clone(),
                Locale::En,
                None,
            )
            .unwrap();
        drain(&mut tasks, &mut editor);
        assert!(std::fs::read(package).unwrap().starts_with(b"PK"));
        tasks
            .submit_to(
                editor.request(Request::Open).unwrap(),
                path,
                Locale::En,
                None,
            )
            .unwrap();
        editor
            .add_node(motion_graph::NodeOp::Float { value: 4. }, [0., 0.])
            .unwrap();
        let current = editor.document().clone();
        drain(&mut tasks, &mut editor);
        assert_eq!(*editor.document(), current);
    }
    #[test]
    fn canceled_jobs_cannot_apply_to_replaced_graph() {
        let mut editor = NodeEditor::default();
        let mut tasks = NodeTasks::start().unwrap();
        tasks
            .submit(
                editor.request(Request::Compile).unwrap(),
                Locale::En,
                false,
                None,
            )
            .unwrap();
        tasks.cancel(&mut editor);
        editor.replace_document(motion_graph::demo_graph(), None);
        let snapshot = editor.snapshot();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let (_, ready) = tasks.poll(&mut editor, Locale::En);
        assert!(ready.is_empty());
        assert_eq!(editor.snapshot().epoch, snapshot.epoch);
        assert!(!tasks.busy());
    }
}
