//! Serial file operations and a separate CPU graph compiler keep native dialogs
//! off the GUI thread and do not stall live preview compilation.
#[cfg(test)]
mod bound_source_tests;
#[cfg(test)]
mod graph_native_effect_tests;
#[cfg(test)]
mod image_source_tests;
use crate::{
    i18n::{Locale, Text as T},
    node_editor::{Action, EditorSnapshot, NodeEditor},
};
use motion_graph::{CompiledGraph, Diagnostic, ExportReport, GraphDocument, ValidationReport};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
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
    decision_prompt(
        list,
        atlas,
        size,
        input,
        locale,
        locale.t(T::NodeUnsaved),
        if zh {
            "退出前保存节点图？"
        } else {
            "Save the node graph before exiting?"
        },
        if zh {
            "节点图单独保存为 .msgraph；保存工程不会保存节点图。"
        } else {
            "Node graphs use .msgraph files; saving the project does not save this graph."
        },
        false,
    )
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
    let styles = list.styles();
    let colors = styles.colors();

    use motion_ui::{
        input::Key,
        paint,
        ui::{self, Align},
        Rect,
    };
    let zh = locale == Locale::Zh;
    let card = Rect::new((size[0] - 520.) * 0.5, (size[1] - 210.) * 0.5, 520., 210.);
    paint::rect(
        list,
        Rect::new(0., 0., size[0], size[1]),
        colors.SUNKEN.with_alpha(0.82),
    );
    paint::rect(list, card, colors.PANEL);
    paint::stroke(list, card, colors.BORDER, 1.);
    ui::text(
        list,
        atlas,
        Rect::new(card.min[0] + 22., card.min[1] + 16., 476., 32.),
        title,
        colors.TEXT,
        Align::Left,
    );
    ui::text(
        list,
        atlas,
        Rect::new(card.min[0] + 22., card.min[1] + 58., 476., 32.),
        message,
        colors.TEXT,
        Align::Left,
    );
    ui::text(
        list,
        atlas,
        Rect::new(card.min[0] + 22., card.min[1] + 97., 476., 32.),
        note,
        if error {
            styles
                .as_ref()
                .resolve("error", motion_ui::style::States::default())
                .map_or(colors.TEXT, |style| style.foreground)
        } else {
            colors.TEXT_MUTED
        },
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
    BoundOpen,
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
    bound_source: Option<Arc<serde_json::Value>>,
    permit: Option<QueuePermit>,
}
/// Counts queued, running and unconsumed completions together. Cancelling UI
/// pending state cannot release memory still owned by a blocked worker/queue.
struct QueuePermit(Arc<AtomicUsize>);
impl QueuePermit {
    fn acquire(count: &Arc<AtomicUsize>) -> Result<Self, String> {
        count
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 16).then_some(n + 1)
            })
            .map_err(|_| "node operation queue is full".to_owned())?;
        Ok(Self(count.clone()))
    }
}
impl Drop for QueuePermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Release);
    }
}
enum Outcome {
    Opened(GraphDocument, PathBuf),
    BoundOpened(
        GraphDocument,
        motion_graph::CompileOptions,
        crate::node_layer::Target,
    ),
    Saved(PathBuf),
    Validated(Result<ValidationReport, Diagnostic>),
    Compiled(Result<(CompiledGraph, Option<CompiledGraph>), Diagnostic>),
    Exported(ExportReport),
    Cancelled,
    Failed(String),
}
struct Completion {
    id: u64,
    generation: u64,
    snapshot: EditorSnapshot,
    outcome: Outcome,
    _permit: Option<QueuePermit>,
}
pub struct ReadyPreview {
    pub snapshot: EditorSnapshot,
    pub compiled: CompiledGraph,
    pub canonical: Option<CompiledGraph>,
}
pub struct NodeTasks {
    io: Sender<Job>,
    compiler: Sender<Job>,
    receive: Receiver<Completion>,
    generation: Arc<AtomicU64>,
    latest_compile: Arc<AtomicU64>,
    next: u64,
    pending: BTreeMap<u64, (Kind, EditorSnapshot)>,
    bound_open: Option<(u64, Arc<serde_json::Value>)>,
    queued: Arc<AtomicUsize>,
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
                    // Owned by this worker, with a bounded canonical/preview
                    // pair. No process-global cache or user-supplied proof.
                    let mut compiler = motion_graph::EditableCompiler::default();
                    while let Ok(job) = receive.recv() {
                        let began = std::time::Instant::now();
                        let outcome = if job_cancelled(&job, &generation, &latest_compile) {
                            Outcome::Cancelled
                        } else {
                            log::info!(
                                "node start #{} {:?} epoch={} revision={} object={:?} effect={:?}",
                                job.id,
                                job.kind,
                                job.snapshot.epoch,
                                job.snapshot.revision,
                                job.snapshot.layer_target.as_ref().map(|t| t.object),
                                job.snapshot.layer_target.as_ref().map(|t| t.effect)
                            );
                            crate::logging::flush();
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                execute(&job, &generation, &latest_compile, &mut compiler)
                            }))
                            .unwrap_or_else(|payload| {
                                Outcome::Failed(format!(
                                    "node worker failed: {}",
                                    motion_media::panic_reason(payload)
                                ))
                            })
                        };
                        let outcome = finish(
                            &job,
                            outcome,
                            began,
                            job_cancelled(&job, &generation, &latest_compile),
                        );
                        if reply
                            .send(Completion {
                                id: job.id,
                                generation: job.generation,
                                snapshot: job.snapshot,
                                outcome,
                                _permit: job.permit,
                            })
                            .is_err()
                        {
                            log::debug!("node response receiver closed #{} {:?}", job.id, job.kind);
                        }
                    }
                    log::debug!("node worker channel closed: {name}");
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
            bound_open: None,
            queued: Arc::new(AtomicUsize::new(0)),
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
    pub fn submit(&mut self, action: Action, locale: Locale, dirty: bool) -> Result<(), String> {
        self.submit_inner(action, locale, dirty, None)
    }
    pub fn open_bound(
        &mut self,
        mut snapshot: EditorSnapshot,
        source: serde_json::Value,
        target: crate::node_layer::Target,
        locale: Locale,
    ) -> Result<(), String> {
        if self.bound_open.is_some() {
            return Err("a bound node source is already loading".into());
        }
        if motion_core::serialized_bytes(&source) > 4 * 1024 * 1024 {
            return Err("node source metadata exceeds 4 MiB".into());
        }
        motion_core::node_resources::source_resources(&source).map_err(|e| e.to_string())?;
        snapshot.layer_target = Some(target);
        self.enqueue(
            Kind::BoundOpen,
            snapshot,
            locale,
            false,
            None,
            Some(Arc::new(source)),
        )
    }
    /// Navigation/undo may change the target while file/PNG work is in flight.
    /// Cancel before consuming a completion; never bind an old source to a new
    /// project, instance, or revision of that instance's authoring document.
    pub fn reconcile_bound_open(
        &mut self,
        editor: &mut NodeEditor,
        state: &serde_json::Value,
        locale: Locale,
    ) {
        let Some((id, source)) = &self.bound_open else {
            return;
        };
        let target = self
            .pending
            .get(id)
            .and_then(|(_, s)| s.layer_target.as_ref());
        let valid = target.is_some_and(|target| {
            state["root"]
                .as_str()
                .is_some_and(|root| std::path::Path::new(root) == target.root)
                && state["composition"]
                    .as_str()
                    .or(state["project"]["composition_id"].as_str())
                    == Some(target.composition.as_str())
                && state["project"]["layers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|l| l["id"].as_u64() == Some(target.object))
                    .and_then(|l| l["effects"].as_array())
                    .into_iter()
                    .flatten()
                    .any(|e| {
                        e["id"].as_u64() == Some(target.effect)
                            && e.get("node_graph") == Some(source.as_ref())
                    })
        });
        if !valid {
            self.cancel(editor);
            editor.report_status(locale.t(T::NodeCancelled));
        }
    }
    pub fn submit_to(
        &mut self,
        action: Action,
        path: PathBuf,
        locale: Locale,
    ) -> Result<(), String> {
        self.submit_inner(action, locale, false, Some(path))
    }
    fn submit_inner(
        &mut self,
        action: Action,
        locale: Locale,
        dirty: bool,
        path: Option<PathBuf>,
    ) -> Result<(), String> {
        let (kind, snapshot) = match action {
            Action::Open(s) => (Kind::Open, s),
            Action::Save(s) => (Kind::Save, s),
            Action::SaveAs(s) => (Kind::SaveAs, s),
            Action::Validate(s) => (Kind::Validate, s),
            Action::Compile(s) => (Kind::Compile, s),
            Action::ExportMsfx(s) => (Kind::Export, s),
            Action::ShowProperties
            | Action::ViewPreferencesChanged(_)
            | Action::RecentNodesChanged(_)
            | Action::Changed { .. }
            | Action::Examples(_)
            | Action::PreviewChanged => {
                log::debug!("node task ignored non-worker action reason=handled_by_shell");
                return Ok(());
            }
        };
        self.enqueue(kind, snapshot, locale, dirty, path, None)
    }
    fn enqueue(
        &mut self,
        kind: Kind,
        snapshot: EditorSnapshot,
        locale: Locale,
        dirty: bool,
        path: Option<PathBuf>,
        bound_source: Option<Arc<serde_json::Value>>,
    ) -> Result<(), String> {
        if kind == Kind::Compile && self.compiling(&snapshot) {
            log::debug!(
                "node compile coalesced epoch={} revision={} reason=already_pending",
                snapshot.epoch,
                snapshot.revision
            );
            return Ok(());
        }
        if self.pending.len() >= 16 {
            return Err("node operation queue is full".into());
        }
        let permit = QueuePermit::acquire(&self.queued)?;
        let id = self.next;
        self.next = self
            .next
            .checked_add(1)
            .ok_or("node request ID space exhausted")?;
        if kind == Kind::Compile {
            self.latest_compile.store(id, Ordering::Release);
        }
        let sender = if kind == Kind::Compile || kind == Kind::Validate {
            &self.compiler
        } else {
            &self.io
        };
        self.pending.insert(id, (kind, snapshot.clone()));
        if let Some(source) = &bound_source {
            self.bound_open = Some((id, source.clone()));
        }
        if sender
            .send(Job {
                id,
                generation: self.generation.load(Ordering::Acquire),
                kind,
                snapshot,
                locale,
                dirty,
                path,
                bound_source,
                permit: Some(permit),
            })
            .is_err()
        {
            self.pending.remove(&id);
            if self
                .bound_open
                .as_ref()
                .is_some_and(|(pending, _)| *pending == id)
            {
                self.bound_open = None;
            }
            return Err("node worker is closed".into());
        }
        Ok(())
    }
    pub fn cancel(&mut self, editor: &mut NodeEditor) {
        self.bound_open = None;
        self.generation.fetch_add(1, Ordering::AcqRel);
        while self.receive.try_recv().is_ok() {}
        for (id, (kind, snapshot)) in std::mem::take(&mut self.pending) {
            log::info!(
                "node task cancellation requested #{id} {kind:?} epoch={} revision={} path={:?}",
                snapshot.epoch,
                snapshot.revision,
                snapshot.source_path
            );
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
            if self
                .bound_open
                .as_ref()
                .is_some_and(|(id, _)| *id == completion.id)
            {
                self.bound_open = None;
            }
            if self.pending.remove(&completion.id).is_none()
                || completion.generation != self.generation.load(Ordering::Acquire)
            {
                continue;
            }
            changed = true;
            let snapshot = completion.snapshot;
            let current = editor.snapshot();
            if snapshot.epoch != current.epoch {
                editor.cancel_compile(&snapshot);
                continue;
            }
            match completion.outcome {
                Outcome::BoundOpened(document, options, target) => {
                    if !editor.accept_layer_document(&snapshot, document, options, target) {
                        editor.report_status(locale.t(T::NodeStaleOpen));
                    }
                }
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
                    Ok((compiled, canonical)) => {
                        // The renderer acknowledges success only after GPU
                        // validation; CPU compilation does not replace it.
                        if snapshot.preview_stamp() == current.preview_stamp()
                            && snapshot.semantic_revision == current.semantic_revision
                        {
                            ready.push(ReadyPreview {
                                snapshot,
                                compiled,
                                canonical,
                            });
                        } else {
                            editor.cancel_compile(&snapshot);
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
        if !self.pending.is_empty() {
            log::debug!(
                "node tasks closing: cancelling {} pending job(s)",
                self.pending.len()
            );
        }
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}
fn job_cancelled(job: &Job, generation: &AtomicU64, latest_compile: &AtomicU64) -> bool {
    generation.load(Ordering::Acquire) != job.generation
        || job.kind == Kind::Compile && latest_compile.load(Ordering::Acquire) != job.id
}

fn finish(job: &Job, outcome: Outcome, began: std::time::Instant, cancelled: bool) -> Outcome {
    // Preserve the failure before replacing the UI completion with cancellation.
    // The generation and preview guards must still discard stale results.
    match &outcome {
        Outcome::Failed(error) => log::error!(
            "node error #{} {:?} {}ms epoch={} revision={} path={:?} object={:?} effect={:?} cancelled={cancelled}: {}",
            job.id,
            job.kind,
            began.elapsed().as_millis(),
            job.snapshot.epoch,
            job.snapshot.revision,
            job.path.as_ref().or(job.snapshot.source_path.as_ref()),
            job.snapshot.layer_target.as_ref().map(|t| t.object),
            job.snapshot.layer_target.as_ref().map(|t| t.effect),
            error.chars().take(1024).collect::<String>()
        ),
        Outcome::Validated(Err(error)) | Outcome::Compiled(Err(error)) => log::warn!(
            "node rejected #{} {:?} {}ms epoch={} revision={} path={:?} cancelled={cancelled}: {error}",
            job.id,
            job.kind,
            began.elapsed().as_millis(),
            job.snapshot.epoch,
            job.snapshot.revision,
            job.path.as_ref().or(job.snapshot.source_path.as_ref())
        ),
        _ => {}
    }
    log::debug!(
        "node done #{} {:?} {}ms",
        job.id,
        job.kind,
        began.elapsed().as_millis()
    );
    if cancelled {
        Outcome::Cancelled
    } else {
        outcome
    }
}

fn execute(
    job: &Job,
    generation: &AtomicU64,
    latest_compile: &AtomicU64,
    compiler: &mut motion_graph::EditableCompiler,
) -> Outcome {
    let cancelled = || job_cancelled(job, generation, latest_compile);
    let result = (|| -> Result<Outcome, String> {
        if job.kind == Kind::BoundOpen {
            #[cfg(test)]
            bound_source_tests::before_load(job);
            if cancelled() {
                return Ok(Outcome::Cancelled);
            }
            let target = job
                .snapshot
                .layer_target
                .as_ref()
                .ok_or("node source target missing")?;
            let source = job
                .bound_source
                .as_ref()
                .ok_or("node source metadata missing")?;
            let document = crate::node_layer::load_document(source, &target.root)
                .map_err(|e| e.to_string())?;
            let options =
                serde_json::from_value(source["options"].clone()).map_err(|e| e.to_string())?;
            return Ok(Outcome::BoundOpened(document, options, target.clone()));
        }
        if job.kind == Kind::Validate {
            return Ok(Outcome::Validated(
                job.snapshot.validate().map_err(|e| e.diagnostic()),
            ));
        }
        if job.kind == Kind::Compile {
            let snapshot = &job.snapshot;
            let result = if let Some(target) = &snapshot.preview_target {
                compiler.compile_preview(&snapshot.document, &snapshot.options, target)
            } else {
                compiler.compile(&snapshot.document, &snapshot.options)
            }
            .map_err(|e| e.diagnostic());
            return Ok(match result {
                Err(error) => Outcome::Compiled(Err(error)),
                Ok(_) if cancelled() => Outcome::Cancelled,
                Ok(compiled) => Outcome::Compiled((|| {
                    let canonical = if job.snapshot.layer_target.is_some()
                        && job.snapshot.preview_target.is_some()
                    {
                        Some(
                            compiler
                                .compile(&snapshot.document, &snapshot.options)
                                .map_err(|e| e.diagnostic())?,
                        )
                    } else {
                        None
                    };
                    Ok((compiled, canonical))
                })()),
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
                // Record the disk side effect even when the editor is replaced
                // or cancellation arrives before the completion is consumed.
                log::info!(
                    "node saved #{} epoch={} revision={} path={}",
                    job.id,
                    job.snapshot.epoch,
                    job.snapshot.revision,
                    path.display()
                );
                Ok(Outcome::Saved(path))
            }
            Kind::Export => {
                let report = job.snapshot.export_msfx(&path).map_err(|e| e.to_string())?;
                log::info!(
                    "node exported #{} epoch={} revision={} path={} source={}",
                    job.id,
                    job.snapshot.epoch,
                    job.snapshot.revision,
                    report.path.display(),
                    report.source_path.display()
                );
                Ok(Outcome::Exported(report))
            }
            _ => unreachable!(),
        }
    })();
    result.unwrap_or_else(Outcome::Failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_editor::Request;
    #[test]
    fn live_compiler_worker_changes_private_data_without_changing_baked_compile_contract() {
        use motion_graph::{Connection, Node, NodeOp};
        let mut graph = motion_graph::demo_graph();
        graph.public_parameters.clear();
        graph.nodes = vec![
            Node::new(
                "color",
                NodeOp::Color {
                    value: [0.1, 0.3, 0.8, 0.5],
                },
            ),
            Node::new("output", NodeOp::ColorOutput),
        ];
        graph.connections = vec![Connection::new("color", "value", "output", "color")];
        let mut editor = NodeEditor::new(graph);
        let mut tasks = NodeTasks::start().unwrap();
        fn compile(tasks: &mut NodeTasks, editor: &mut NodeEditor) -> ReadyPreview {
            tasks
                .submit(editor.request(Request::Compile).unwrap(), Locale::En, false)
                .unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                assert!(std::time::Instant::now() < deadline);
                let (_, mut ready) = tasks.poll(editor, Locale::En);
                if let Some(ready) = ready.pop() {
                    assert!(
                        editor.accept_compile(&ready.snapshot, Ok(ready.compiled.report.clone()))
                    );
                    return ready;
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
        let baked = editor.snapshot().compile().unwrap().source;
        let first = compile(&mut tasks, &mut editor);
        assert_eq!(first.compiled.report.portable_compilations, 1);
        assert!(first.compiled.manifest.effects[0].shader_data.is_some());
        editor.set_constant("color", 0, 0.9).unwrap();
        let second = compile(&mut tasks, &mut editor);
        assert_eq!(second.compiled.report.portable_compilations, 0);
        assert_ne!(baked, editor.snapshot().compile().unwrap().source);
        assert_eq!(first.compiled.source, second.compiled.source);
        assert_eq!(first.compiled.shader.wgsl, second.compiled.shader.wgsl);
        assert_ne!(
            first.compiled.manifest.effects[0].shader_data,
            second.compiled.manifest.effects[0].shader_data
        );
        assert!(editor.undo());
        let restored = compile(&mut tasks, &mut editor);
        assert_eq!(restored.compiled.report.portable_compilations, 0);
        assert_eq!(
            first.compiled.manifest.effects[0].shader_data,
            restored.compiled.manifest.effects[0].shader_data
        );
        assert_eq!(baked, editor.snapshot().compile().unwrap().source);
    }
    #[test]
    fn background_compile_returns_owned_cpu_work_for_renderer_validation() {
        let mut editor = NodeEditor::default();
        let snapshot = editor.snapshot();
        let mut tasks = NodeTasks::start().unwrap();
        tasks
            .submit(editor.request(Request::Compile).unwrap(), Locale::En, false)
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let ready = loop {
            assert!(std::time::Instant::now() < deadline);
            let (_, mut ready) = tasks.poll(&mut editor, Locale::En);
            if let Some(ready) = ready.pop() {
                break ready;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        };
        assert_eq!(ready.snapshot.preview_stamp(), snapshot.preview_stamp());
        assert!(!ready.compiled.shader.wgsl.is_empty());
        assert_eq!(editor.snapshot().document, snapshot.document);
        assert!(editor.accept_compile(&ready.snapshot, Ok(ready.compiled.report)));
        assert!(editor.diagnostic().is_none());
    }
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
            )
            .unwrap();
        drain(&mut tasks, &mut editor);
        assert!(std::fs::read(package).unwrap().starts_with(b"PK"));
        tasks
            .submit_to(editor.request(Request::Open).unwrap(), path, Locale::En)
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
            .submit(editor.request(Request::Compile).unwrap(), Locale::En, false)
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

#[cfg(test)]
mod observability_tests {
    use super::*;
    use crate::node_editor::Request;

    #[test]
    fn cancelled_completions_log_real_failures_before_discarding_results() {
        const CHILD: &str = "MOTION_NODE_CANCEL_LOG_TEST_CHILD";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = PathBuf::from(root);
            let _logger = crate::logging::init(root.join("logs")).unwrap();
            let editor = NodeEditor::default();
            let generation = AtomicU64::new(1);
            let latest = AtomicU64::new(0);
            std::fs::write(root.join("blocked"), b"not a directory").unwrap();
            let mut job = Job {
                id: 41,
                generation: 1,
                kind: Kind::SaveAs,
                snapshot: editor.snapshot(),
                locale: Locale::En,
                dirty: false,
                path: Some(root.join("blocked/failure.msgraph")),
                bound_source: None,
                permit: None,
            };
            let began = std::time::Instant::now();
            let outcome = execute(&job, &generation, &latest, &mut Default::default());
            assert!(matches!(&outcome, Outcome::Failed(error) if error.contains("graph I/O")));
            // Cancel at the boundary after the real operation has failed and
            // before publishing its completion, without relying on a sleep.
            generation.store(2, Ordering::Release);
            assert!(matches!(
                finish(
                    &job,
                    outcome,
                    began,
                    job_cancelled(&job, &generation, &latest)
                ),
                Outcome::Cancelled
            ));

            job.id = 42;
            job.generation = 2;
            job.kind = Kind::Compile;
            job.snapshot.document.nodes.clear();
            job.snapshot.document.connections.clear();
            latest.store(42, Ordering::Release);
            let began = std::time::Instant::now();
            let outcome = execute(&job, &generation, &latest, &mut Default::default());
            assert!(
                matches!(&outcome, Outcome::Compiled(Err(error)) if error.code == motion_graph::ErrorCode::MissingOutput)
            );
            latest.store(0, Ordering::Release);
            assert!(matches!(
                finish(
                    &job,
                    outcome,
                    began,
                    job_cancelled(&job, &generation, &latest)
                ),
                Outcome::Cancelled
            ));

            job.id = 43;
            job.snapshot = editor.snapshot();
            latest.store(43, Ordering::Release);
            let began = std::time::Instant::now();
            let outcome = execute(&job, &generation, &latest, &mut Default::default());
            assert!(matches!(&outcome, Outcome::Compiled(Ok(_))));
            latest.store(0, Ordering::Release);
            assert!(matches!(
                finish(
                    &job,
                    outcome,
                    began,
                    job_cancelled(&job, &generation, &latest)
                ),
                Outcome::Cancelled
            ));
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "node_tasks::observability_tests::cancelled_completions_log_real_failures_before_discarding_results"])
            .env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = std::fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
        let io = text
            .lines()
            .find(|line| line.contains("node error #41 SaveAs "))
            .unwrap();
        assert!(
            io.contains(" ERROR ")
                && io.contains("ms epoch=")
                && io.contains("cancelled=true:")
                && io.contains("failure.msgraph"),
            "{io}"
        );
        let compile = text
            .lines()
            .find(|line| line.contains("node rejected #42 Compile "))
            .unwrap();
        assert!(
            compile.contains(" WARN ")
                && compile.contains("cancelled=true:")
                && compile.contains("MissingOutput"),
            "{compile}"
        );
        assert!(!text.contains("node error #43") && !text.contains("node rejected #43"));
        assert!(String::from_utf8_lossy(&output.stderr).contains("node error #41 SaveAs "));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("node error #"));
    }

    #[test]
    fn source_side_effects_and_failures_are_logged_even_after_editor_replacement() {
        const CHILD: &str = "MOTION_NODE_LOG_TEST_CHILD";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = PathBuf::from(root);
            let _logger = crate::logging::init(root.join("logs")).unwrap();
            let mut editor = NodeEditor::default();
            let mut tasks = NodeTasks::start().unwrap();
            std::fs::write(root.join("absent"), b"file blocks output directory").unwrap();
            tasks
                .submit_to(
                    editor.request(Request::SaveAs).unwrap(),
                    root.join("absent/failure.msgraph"),
                    Locale::En,
                )
                .unwrap();
            drain(&mut tasks, &mut editor);
            let expected = editor.snapshot().document;
            tasks
                .submit_to(
                    editor.request(Request::SaveAs).unwrap(),
                    root.join("saved.msgraph"),
                    Locale::En,
                )
                .unwrap();
            editor.replace_document(motion_graph::demo_graph(), None);
            drain(&mut tasks, &mut editor);
            assert_eq!(
                GraphDocument::load(&root.join("saved.msgraph")).unwrap(),
                expected
            );
            assert!(editor.snapshot().source_path.is_none());
            tasks
                .submit(editor.request(Request::Compile).unwrap(), Locale::En, false)
                .unwrap();
            drain(&mut tasks, &mut editor);
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "node_tasks::observability_tests::source_side_effects_and_failures_are_logged_even_after_editor_replacement"])
            .env(CHILD,root.path()).env_remove("MOTION_LOG")
            .output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = std::fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
        assert!(text.contains("node start #1 SaveAs epoch="));
        assert!(text.contains("node error #1 SaveAs ") && text.contains("ms epoch="));
        assert!(text.contains("node saved #2 epoch=") && text.contains("saved.msgraph"));
        assert!(text
            .lines()
            .any(|line| line.contains(" INFO ") && line.contains("node start #3 Compile")));
        assert!(String::from_utf8_lossy(&output.stderr).contains("node error #1 SaveAs "));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("node saved #"));
    }

    fn drain(tasks: &mut NodeTasks, editor: &mut NodeEditor) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while tasks.busy() {
            assert!(std::time::Instant::now() < deadline);
            tasks.poll(editor, Locale::En);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}
