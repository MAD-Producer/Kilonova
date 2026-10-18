//! Standalone N2 node panel. The host owns dialogs, workers and preview routing.
//! Drawing emits owned N1 snapshots; it never touches an editing Session or GPU queue.
use motion_graph::{
    demo_graph, node_ports, ColorOp, CompileOptions, CompileReport, Connection, Diagnostic,
    ErrorCode, GraphDocument, ImageSource, MathOp, Node, NodeOp, PortRef, PublicParameter,
    SampleCoordinates, ValueType,
};
use motion_ui::{Input, PaintList, Rect, TextAtlas};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

#[path = "node_editor/catalog.rs"]
mod catalog;
#[path = "node_editor/editing.rs"]
mod editing;
#[path = "node_editor/grouping.rs"]
mod grouping;
#[path = "node_editor/inline.rs"]
mod inline;
#[path = "node_editor/nudge.rs"]
mod nudge;
#[path = "node_editor/observability.rs"]
mod observability;
#[path = "node_editor/preview.rs"]
mod preview;
#[path = "node_editor/view/mod.rs"]
mod view;
pub use catalog::catalog;

/// Only UI localization; document IDs, ports and operation names remain N1 data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    Chinese,
    #[default]
    English,
}
impl Language {
    fn text<'a>(self, zh: &'a str, en: &'a str) -> &'a str {
        if self == Self::Chinese {
            zh
        } else {
            en
        }
    }
}

#[derive(Clone, Debug)]
pub struct EditorSnapshot {
    pub document: GraphDocument,
    pub options: CompileOptions,
    pub revision: u64,
    pub semantic_revision: u64,
    pub preview_revision: u64,
    pub preview_target: Option<PortRef>,
    pub epoch: u64,
    pub source_path: Option<PathBuf>,
    pub layer_target: Option<crate::node_layer::Target>,
}
impl EditorSnapshot {
    pub fn preview_stamp(&self) -> (u64, u64) {
        (self.epoch, self.preview_revision)
    }
    pub fn compile_preview(&self) -> motion_graph::Result<motion_graph::CompiledGraph> {
        if let Some(target) = &self.preview_target {
            motion_graph::compile_preview(&self.document, &self.options, target)
        } else {
            self.compile()
        }
    }
    pub fn validate(&self) -> motion_graph::Result<motion_graph::ValidationReport> {
        motion_graph::validate(&self.document, &self.options)
    }
    pub fn compile(&self) -> motion_graph::Result<motion_graph::CompiledGraph> {
        motion_graph::compile(&self.document, &self.options)
    }
    pub fn save(&self, path: &std::path::Path) -> motion_graph::Result<()> {
        self.document.save(path)
    }
    pub fn export_msfx(
        &self,
        path: &std::path::Path,
    ) -> motion_graph::Result<motion_graph::ExportReport> {
        motion_graph::export_msfx(&self.document, &self.options, path)
    }
}
#[derive(Clone, Debug)]
pub enum Action {
    ViewPreferencesChanged(ViewPreferences),
    ShowProperties,
    Examples(EditorSnapshot),
    Open(EditorSnapshot),
    Save(EditorSnapshot),
    SaveAs(EditorSnapshot),
    Validate(EditorSnapshot),
    Compile(EditorSnapshot),
    ExportMsfx(EditorSnapshot),
    PreviewChanged,
    Changed {
        revision: u64,
        semantic_revision: u64,
        semantic: bool,
    },
}
/// Shared by toolbar, keyboard shortcuts and host command dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    Examples,
    Open,
    Save,
    SaveAs,
    Validate,
    Compile,
    ExportMsfx,
}
#[derive(Default)]
pub struct FrameResponse {
    pub actions: Vec<Action>,
    pub wants_pointer: bool,
    pub wants_keyboard: bool,
    pub changed: bool,
    pub semantic_changed: bool,
}

/// Application workspace preferences; never part of a graph or compiled output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ViewPreferences {
    pub grid: bool,
    pub minimap: bool,
}
impl Default for ViewPreferences {
    fn default() -> Self { Self { grid: true, minimap: false } }
}

#[derive(Clone, Copy, Debug)]
pub struct CanvasView {
    pub pan: [f32; 2],
    pub zoom: f32,
}
impl Default for CanvasView {
    fn default() -> Self {
        Self {
            pan: [40., 60.],
            zoom: 0.85,
        }
    }
}
impl CanvasView {
    pub fn screen_to_world(self, point: [f32; 2], canvas: Rect) -> [f32; 2] {
        [
            (point[0] - canvas.min[0] - self.pan[0]) / self.zoom,
            (point[1] - canvas.min[1] - self.pan[1]) / self.zoom,
        ]
    }
    pub fn world_to_screen(self, point: [f32; 2], canvas: Rect) -> [f32; 2] {
        [
            canvas.min[0] + self.pan[0] + point[0] * self.zoom,
            canvas.min[1] + self.pan[1] + point[1] * self.zoom,
        ]
    }
    pub fn zoom_at(&mut self, zoom: f32, point: [f32; 2], canvas: Rect) {
        let world = self.screen_to_world(point, canvas);
        self.zoom = zoom.clamp(1.0e-6, 2.5);
        self.pan = [
            point[0] - canvas.min[0] - world[0] * self.zoom,
            point[1] - canvas.min[1] - world[1] * self.zoom,
        ];
    }
}
#[derive(Clone)]
struct History {
    document: GraphDocument,
    options: CompileOptions,
    semantic: bool,
    parents: Vec<grouping::ParentScope>,
    view: CanvasView,
    selection: BTreeSet<String>,
    primary: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Binding {
    Constant(String, usize),
    ParamDefault(String, usize),
    ParamName(String, bool),
    ParamMin(String),
    ParamMax(String),
    ParamStep(String),
    ParamUnits(String),
    GraphName(bool),
    PluginId,
    Version,
    Author,
    License,
}
struct Field {
    binding: Binding,
    text: String,
    cursor: usize,
    replace: bool,
    error: Option<String>,
}
enum Drag {
    Nodes {
        origin: [f32; 2],
        positions: Vec<(String, [f32; 2])>,
        before: History,
    },
    Pan {
        origin: [f32; 2],
        pan: [f32; 2],
        button: motion_ui::input::MouseButton,
    },
    Marquee {
        origin: [f32; 2],
        current: [f32; 2],
        base: BTreeSet<String>,
        primary: Option<String>,
        extend: bool,
        toggle: bool,
    },
    Wire {
        source: PortRef,
        old_input: Option<PortRef>,
    },
    WireInput {
        target: PortRef,
    },
    Cut {
        points: Vec<[f32; 2]>,
        last: [f32; 2],
        inputs: BTreeSet<PortRef>,
    },
}

struct AddMenu {
    position: [f32; 2],
    world: [f32; 2],
    query: String,
    category: Option<catalog::Category>,
    selected: usize,
    scroll: f32,
}

pub struct NodeEditor {
    observations: observability::Observations,
    pub layer_target: Option<crate::node_layer::Target>,
    document: GraphDocument,
    parents: Vec<grouping::ParentScope>,
    last_group_click: Option<(String, std::time::Instant, [f32; 2])>,
    options: CompileOptions,
    source_path: Option<PathBuf>,
    saved_document: Option<GraphDocument>,
    saved_revision: Option<u64>,
    pub view: CanvasView,
    pub view_preferences: ViewPreferences,
    minimap_drag: Option<view::minimap::NavigationDrag>,
    selected: BTreeSet<String>,
    primary: Option<String>,
    undo: Vec<History>,
    redo: Vec<History>,
    revision: u64,
    semantic_revision: u64,
    preview_revision: u64,
    preview_target: Option<PortRef>,
    epoch: u64,
    next_id: u64,
    focus: bool,
    drag: Option<Drag>,
    nudge_before: Option<History>,
    field: Option<Field>,
    inline: inline::InlineState,
    library_open: bool,
    inspector_open: bool,
    pub external_inspector: bool,
    library_scroll: f32,
    inspector_scroll: f32,
    search: String,
    search_focus: bool,
    diagnostic: Option<Diagnostic>,
    notice: Option<String>,
    compiled: Option<(u64, CompileReport)>,
    pending_compile: Option<u64>,
    last_canvas: Rect,
    first_frame: bool,
    context_menu: Option<ContextMenu>,
    add_menu: Option<AddMenu>,
    floating_mode: bool, // true=漂浮形态(有侧边栏), false=嵌入形态(无侧边栏)
}

#[derive(Clone, Debug)]
struct ContextMenu {
    position: [f32; 2],
    kind: MenuKind,
    selected: usize,
}
#[derive(Clone, Debug)]
enum MenuKind {
    Canvas,
    View,
    Node(String),
    Wire(PortRef),
    Preview(String),
}
impl Default for NodeEditor {
    fn default() -> Self {
        let mut editor = Self::new(demo_graph());
        editor.arrange_without_history();
        editor.primary = Some("circle".into());
        editor.selected.insert("circle".into());
        editor
    }
}
fn next_editor_epoch() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
impl NodeEditor {
    pub fn renew_session(&mut self) {
        self.epoch = next_editor_epoch();
        self.pending_compile = None;
    }
    pub fn new(document: GraphDocument) -> Self {
        let options = CompileOptions::default();
        let diagnostic = motion_graph::validate(&document, &options)
            .err()
            .map(|e| e.diagnostic());
        Self {
            observations: Default::default(),
            layer_target: None,
            document,
            parents: vec![],
            last_group_click: None,
            options,
            source_path: None,
            saved_document: None,
            saved_revision: None,
            view: Default::default(),
            view_preferences: Default::default(),
            minimap_drag: None,
            selected: BTreeSet::new(),
            primary: None,
            undo: vec![],
            redo: vec![],
            revision: 0,
            semantic_revision: 0,
            preview_revision: 0,
            preview_target: None,
            epoch: next_editor_epoch(),
            next_id: 1,
            focus: false,
            drag: None,
            nudge_before: None,
            field: None,
            inline: Default::default(),
            library_open: true,
            inspector_open: true,
            external_inspector: false,
            library_scroll: 0.,
            inspector_scroll: 0.,
            search: String::new(),
            search_focus: false,
            diagnostic,
            notice: None,
            compiled: None,
            pending_compile: None,
            last_canvas: Rect::default(),
            first_frame: true,
            context_menu: None,
            add_menu: None,
            floating_mode: false, // 默认嵌入模式（无侧边栏）
        }
    }
    pub fn document(&self) -> &GraphDocument {
        &self.document
    }
    pub fn options(&self) -> &CompileOptions {
        &self.options
    }
    pub fn selected(&self) -> Option<&str> {
        self.primary.as_deref()
    }
    pub fn dirty(&self) -> bool {
        self.saved_document.as_ref() != Some(&self.root_document())
    }
    pub fn focused(&self) -> bool {
        self.focus
    }
    pub fn text_editing(&self) -> bool {
        self.field.is_some() || self.search_focus || self.popup_open()
    }
    pub fn popup_open(&self) -> bool {
        self.add_menu.is_some() || self.context_menu.is_some() || self.inline.busy()
    }
    pub fn focus_panel(&mut self) {
        self.focus = true;
    }
    pub fn editing(&self) -> bool {
        self.drag.is_some() || self.minimap_drag.is_some() || self.nudge_before.is_some() || self.text_editing()
    }
    pub fn diagnostic(&self) -> Option<&Diagnostic> {
        self.diagnostic.as_ref()
    }
    /// 设置漂浮模式（true=有侧边栏, false=底部面板）
    pub fn set_floating_mode(&mut self, floating: bool) {
        self.floating_mode = floating;
    }
    /// 获取当前模式
    pub fn is_floating_mode(&self) -> bool {
        self.floating_mode
    }
    pub fn snapshot(&self) -> EditorSnapshot {
        EditorSnapshot {
            document: self.root_document(),
            options: self.options.clone(),
            revision: self.revision,
            semantic_revision: self.semantic_revision,
            preview_revision: self.preview_revision,
            preview_target: self
                .preview_target
                .as_ref()
                .map(|target| PortRef::new(self.qualified_node(&target.node), &target.port)),
            epoch: self.epoch,
            source_path: self.source_path.clone(),
            layer_target: self.layer_target.clone(),
        }
    }
    /// Commit a valid inspector field and an active move before capturing an owned
    /// document. The host may send this action to a worker without borrowing UI state.
    pub fn request(&mut self, request: Request) -> Result<Action, Diagnostic> {
        self.finish_nudge();
        if let Err(error) = self.commit_field() {
            self.observe_failure("request.field", format_args!("request={request:?} code={:?} fallback=retain_edit", error.code));
            return Err(error);
        }
        if let Some(Drag::Nodes { before, .. }) = self.drag.take() {
            if self.document != before.document {
                self.finish_edit(before);
            }
        }
        self.search_focus = false;
        let snapshot = self.snapshot();
        Ok(match request {
            Request::Examples => Action::Examples(snapshot),
            Request::Open => Action::Open(snapshot),
            Request::Save => Action::Save(snapshot),
            Request::SaveAs => Action::SaveAs(snapshot),
            Request::Validate => Action::Validate(snapshot),
            Request::Compile => {
                self.pending_compile = Some(self.preview_revision);
                Action::Compile(snapshot)
            }
            Request::ExportMsfx => Action::ExportMsfx(snapshot),
        })
    }
    /// A dialog/worker that finishes after another edit must not discard that edit.
    pub fn accept_open(
        &mut self,
        snapshot: &EditorSnapshot,
        document: GraphDocument,
        path: PathBuf,
    ) -> bool {
        if snapshot.epoch != self.epoch || snapshot.revision != self.revision || self.editing() {
            return false;
        }
        let target = self.layer_target.clone();
        self.replace_document(document, if target.is_some() { None } else { Some(path) });
        self.layer_target = target;
        true
    }
    pub fn replace_document(&mut self, document: GraphDocument, path: Option<PathBuf>) {
        let options = self.options.clone();
        let preferences = self.view_preferences;
        *self = Self::new(document);
        self.view_preferences = preferences;
        self.options = options;
        self.source_path = path;
        self.saved_document = Some(self.document.clone());
        self.saved_revision = Some(self.revision);
        self.refresh_diagnostic();
    }
    pub fn bound_creating(&self, state: &serde_json::Value) -> bool {
        self.layer_target.as_ref().is_some_and(|t| {
            !state["project"]["layers"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|l| l["id"].as_u64() == Some(t.object))
                .flat_map(|l| l["effects"].as_array().into_iter().flatten())
                .any(|e| e["id"].as_u64() == Some(t.effect))
        })
    }
    pub fn accept_project_saved(&mut self, snapshot: &EditorSnapshot) {
        if self.epoch == snapshot.epoch
            && self.saved_revision.is_none_or(|r| r <= snapshot.revision)
        {
            self.saved_document = Some(snapshot.document.clone());
            self.saved_revision = Some(snapshot.revision);
            if let Some(target) = &mut self.layer_target {
                target.creating = false;
            }
        }
    }
    pub fn load_layer_source(
        &mut self,
        source: &serde_json::Value,
        target: crate::node_layer::Target,
    ) -> Result<(), String> {
        let document =
            GraphDocument::from_json(&source["document"].to_string()).map_err(|e| e.to_string())?;
        let options =
            serde_json::from_value(source["options"].clone()).map_err(|e| e.to_string())?;
        self.replace_document(document, None);
        self.options = options;
        self.layer_target = Some(target);
        self.refresh_diagnostic();
        Ok(())
    }
    pub fn accept_saved(&mut self, snapshot: &EditorSnapshot, path: PathBuf) -> bool {
        if snapshot.epoch != self.epoch
            || self
                .saved_revision
                .is_some_and(|revision| snapshot.revision < revision)
        {
            return false;
        }
        self.source_path = Some(path);
        self.saved_document = Some(snapshot.document.clone());
        self.saved_revision = Some(snapshot.revision);
        self.notice = Some("Source saved".into());
        true
    }
    pub fn accept_validation(
        &mut self,
        snapshot: &EditorSnapshot,
        result: Result<motion_graph::ValidationReport, Diagnostic>,
    ) -> bool {
        if snapshot.epoch != self.epoch || snapshot.semantic_revision != self.semantic_revision {
            return false;
        }
        match result {
            Ok(report) => {
                self.diagnostic = None;
                self.notice = Some(format!(
                    "Valid graph · {} unused node(s)",
                    report.unused_nodes.len()
                ));
            }
            Err(error) => {
                self.notice = Some(error.to_string());
                self.diagnostic = Some(error);
            }
        }
        true
    }
    /// Call when a compile worker is canceled or cannot start.
    pub fn cancel_compile(&mut self, snapshot: &EditorSnapshot) {
        if snapshot.epoch == self.epoch && self.pending_compile == Some(snapshot.preview_revision) {
            self.pending_compile = None;
        }
    }
    /// Reject stale worker responses, retain the previous valid compiled result.
    pub fn accept_compile(
        &mut self,
        snapshot: &EditorSnapshot,
        result: Result<CompileReport, Diagnostic>,
    ) -> bool {
        if snapshot.preview_stamp() != self.stamp()
            || snapshot.semantic_revision != self.semantic_revision
        {
            return false;
        }
        self.pending_compile = None;
        match result {
            Ok(report) => {
                if snapshot.preview_target.is_none() {
                    self.compiled = Some((self.semantic_revision, report));
                }
                self.notice = Some("Compiled".into());
                self.refresh_diagnostic();
            }
            Err(error) => {
                self.notice = Some(error.to_string());
                self.diagnostic = Some(error);
            }
        }
        true
    }
    pub fn report_error(&mut self, error: impl Into<String>) {
        self.notice = Some(error.into());
    }
    pub fn report_status(&mut self, message: impl Into<String>) {
        self.notice = Some(message.into());
    }
    pub fn stamp(&self) -> (u64, u64) {
        (self.epoch, self.preview_revision)
    }
    pub fn cancel_interaction(&mut self) -> bool {
        let before = self.stamp();
        self.finish_nudge();
        self.cancel_edit_only();
        self.focus = false;
        before != self.stamp()
    }
    fn state(&mut self, semantic: bool) -> History {
        self.finish_nudge();
        History {
            document: self.document.clone(),
            options: self.options.clone(),
            semantic,
            parents: self.parents.clone(),
            view: self.view,
            selection: self.selected.clone(),
            primary: self.primary.clone(),
        }
    }
    fn refresh_diagnostic(&mut self) {
        self.diagnostic = motion_graph::validate(&self.root_document(), &self.options)
            .err()
            .map(|e| e.diagnostic());
    }
    fn finish_edit(&mut self, before: History) {
        self.finish_nudge();
        self.undo.push(before.clone());
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.revision = self.revision.wrapping_add(1);
        if before.semantic {
            self.semantic_revision = self.semantic_revision.wrapping_add(1);
            self.advance_preview();
        }
        self.notice = None;
        self.refresh_diagnostic();
    }
    pub fn undo(&mut self) -> bool {
        self.finish_nudge();
        self.cancel_edit_only();
        if let Some(previous) = self.undo.pop() {
            let current = self.state(previous.semantic);
            self.redo.push(current);
            self.restore_history(&previous);
            self.bump(previous.semantic);
            true
        } else {
            self.trace_noop("undo", "history_empty");
            false
        }
    }
    pub fn redo(&mut self) -> bool {
        self.finish_nudge();
        self.cancel_edit_only();
        if let Some(next) = self.redo.pop() {
            let current = self.state(next.semantic);
            self.undo.push(current);
            self.restore_history(&next);
            self.bump(next.semantic);
            true
        } else {
            self.trace_noop("redo", "history_empty");
            false
        }
    }
    fn bump(&mut self, semantic: bool) {
        self.revision = self.revision.wrapping_add(1);
        if semantic {
            self.semantic_revision = self.semantic_revision.wrapping_add(1);
            self.advance_preview();
        }
        self.selected
            .retain(|id| self.document.nodes.iter().any(|n| n.id == *id));
        if self
            .primary
            .as_ref()
            .is_some_and(|id| !self.selected.contains(id))
        {
            self.primary = self.selected.iter().next().cloned();
        }
        self.refresh_diagnostic();
    }
    fn restore_node_drag(&mut self, before: History) {
        if self.document != before.document {
            self.document = before.document;
            self.bump(false);
        }
    }
    fn cancel_canvas_interaction(&mut self) {
        if let Some(drag) = self.minimap_drag.take() { self.view.pan = drag.before; }
        self.cancel_inline();
        if let Some(before) = self.nudge_before.take() {
            self.document = before.document;
            self.bump(false);
        }
        match self.drag.take() {
            Some(Drag::Nodes { before, .. }) => self.restore_node_drag(before),
            Some(Drag::Marquee { base, primary, .. }) => {
                self.selected = base;
                self.primary = primary;
            }
            _ => {}
        }
    }
    fn cancel_edit_only(&mut self) {
        self.cancel_canvas_interaction();
        self.add_menu = None;
        self.context_menu = None;
        self.field = None;
        self.search_focus = false;
    }
    fn id(&mut self, prefix: &str) -> String {
        loop {
            let id = format!("{prefix}_{}", self.next_id);
            self.next_id += 1;
            if !self.document.nodes.iter().any(|n| n.id == id)
                && !self.document.public_parameters.iter().any(|p| p.id == id)
            {
                return id;
            }
        }
    }
    pub fn add_node(
        &mut self,
        operation: NodeOp,
        position: [f32; 2],
    ) -> Result<String, Diagnostic> {
        if self.document.nodes.len() >= self.options.max_nodes {
            return Err(diag(ErrorCode::Budget, None, None, "Node budget reached"));
        }
        if position.iter().any(|v| !v.is_finite()) {
            return Err(diag(
                ErrorCode::InvalidValue,
                None,
                None,
                "Node position must be finite",
            ));
        }
        if matches!(operation, NodeOp::ColorOutput)
            && self
                .document
                .nodes
                .iter()
                .any(|n| matches!(n.operation, NodeOp::ColorOutput))
        {
            return Err(diag(
                ErrorCode::MultipleOutputs,
                None,
                None,
                "N1 allows one Color Output",
            ));
        }
        let id = self.id(operation.kind());
        let mut node = Node::new(id.clone(), operation);
        node.position = position;
        self.check_node(&node)?;
        let mut candidate = self.document.clone();
        candidate.nodes.push(node.clone());
        self.check_nested_budget(&candidate)?;
        let before = self.state(true);
        self.document.nodes.push(node);
        self.selected.clear();
        self.selected.insert(id.clone());
        self.primary = Some(id.clone());
        self.finish_edit(before);
        Ok(id)
    }
    pub fn add_public_parameter(
        &mut self,
        value_type: ValueType,
        position: [f32; 2],
    ) -> Result<String, Diagnostic> {
        if position.iter().any(|v| !v.is_finite()) {
            return Err(diag(
                ErrorCode::InvalidValue,
                None,
                None,
                "Node position must be finite",
            ));
        }
        let mut budget = self.document.clone();
        budget
            .nodes
            .push(Node::new("budget_probe", NodeOp::Float { value: 0. }));
        self.check_nested_budget(&budget)?;
        if self.document.public_parameters.len() >= 32 {
            return Err(diag(
                ErrorCode::Budget,
                None,
                None,
                "SDK 6 allows at most 32 public parameters; reuse or remove an unused definition",
            ));
        }
        if !matches!(
            value_type,
            ValueType::Float
                | ValueType::Vec2
                | ValueType::Vec3
                | ValueType::Color
                | ValueType::Bool
        ) {
            return Err(diag(
                ErrorCode::TypeMismatch,
                None,
                None,
                "Unsupported public parameter type",
            ));
        }
        if self.document.nodes.len() >= self.options.max_nodes {
            return Err(diag(ErrorCode::Budget, None, None, "Node budget reached"));
        }
        let before = self.state(true);
        let parameter = self.id("parameter");
        let id = self.id("parameter_node");
        let (default, min, max, step) = match value_type {
            ValueType::Color => ([0.12, 0.65, 1., 1.], 0., 1., 0.01),
            ValueType::Bool => ([0.; 4], 0., 1., 1.),
            ValueType::Vec2 | ValueType::Vec3 => ([0.; 4], -4096., 4096., 0.25),
            _ => ([1., 0., 0., 0.], -1000., 1000., 0.1),
        };
        self.document.public_parameters.push(PublicParameter {
            id: parameter.clone(),
            name: parameter.clone(),
            english_name: parameter.clone(),
            value_type,
            default,
            min,
            max,
            step,
            units: String::new(),
            animatable: true,
        });
        let mut node = Node::new(id.clone(), NodeOp::Parameter { parameter });
        node.position = position;
        self.document.nodes.push(node);
        self.selected.clear();
        self.selected.insert(id.clone());
        self.primary = Some(id.clone());
        self.finish_edit(before);
        Ok(id)
    }
    pub fn select(&mut self, id: Option<&str>, extend: bool) {
        if self.commit_field().is_err() {
            return;
        }
        if !extend {
            self.selected.clear();
        }
        if let Some(id) = id.filter(|id| self.document.nodes.iter().any(|n| n.id == *id)) {
            if extend && self.selected.contains(id) {
                self.selected.remove(id);
            } else {
                self.selected.insert(id.into());
            }
            self.primary = if self.selected.contains(id) {
                Some(id.into())
            } else {
                self.selected.iter().next().cloned()
            };
        } else {
            self.primary = self.selected.iter().next().cloned();
        }
        self.field = None;
        self.inspector_scroll = 0.;
    }
    pub fn delete_selected(&mut self) -> bool {
        let protected: BTreeSet<_> = self
            .document
            .nodes
            .iter()
            .filter(|node| grouping::boundary(&node.operation))
            .map(|node| node.id.clone())
            .collect();
        let deleting: BTreeSet<_> = self.selected.difference(&protected).cloned().collect();
        if deleting.is_empty() {
            self.trace_noop("delete_selected", "no_eligible_selection");
            return false;
        }
        let before = self.state(true);
        self.field = None;
        self.drag = None;
        self.document.nodes.retain(|n| !deleting.contains(&n.id));
        self.document
            .connections
            .retain(|c| !deleting.contains(&c.from.node) && !deleting.contains(&c.to.node));
        self.selected.retain(|id| protected.contains(id));
        self.primary = self.selected.iter().next().cloned();
        self.finish_edit(before);
        true
    }
    pub fn disconnect_input(&mut self, input: &PortRef) -> bool {
        if !self.document.connections.iter().any(|c| c.to == *input) {
            self.trace_noop("disconnect_input", "no_connection");
            return false;
        }
        let before = self.state(true);
        self.document.connections.retain(|c| c.to != *input);
        self.finish_edit(before);
        true
    }
    pub fn connect_ports(&mut self, from: PortRef, to: PortRef) -> Result<(), Diagnostic> {
        self.connect_replacing(from, to, None)
    }
    fn connect_replacing(
        &mut self,
        from: PortRef,
        to: PortRef,
        old_input: Option<&PortRef>,
    ) -> Result<(), Diagnostic> {
        let edges = self.candidate_connections(from, to, old_input)?;
        if edges == self.document.connections {
            return Ok(());
        }
        let before = self.state(true);
        self.document.connections = edges;
        self.finish_edit(before);
        Ok(())
    }
    /// Preview and commit use the same atomic replacement and graph checks.
    fn candidate_connections(
        &self,
        from: PortRef,
        to: PortRef,
        old_input: Option<&PortRef>,
    ) -> Result<Vec<Connection>, Diagnostic> {
        motion_graph::check_connection(&self.document, &from, &to)
            .map_err(|error| error.diagnostic())?;
        let mut edges: Vec<_> = self
            .document
            .connections
            .iter()
            .filter(|c| c.to != to && old_input != Some(&c.to))
            .cloned()
            .collect();
        if edges.len().saturating_add(1) > self.options.max_connections {
            return Err(diag(
                ErrorCode::Budget,
                None,
                None,
                "Connection budget reached",
            ));
        }
        if (old_input.is_none() || old_input == Some(&to))
            && self
                .document
                .connections
                .iter()
                .any(|edge| edge.from == from && edge.to == to)
        {
            return Ok(self.document.connections.clone());
        }
        let mut seen = BTreeSet::new();
        let mut pending = vec![to.node.clone()];
        while let Some(id) = pending.pop() {
            if id == from.node {
                return Err(diag(
                    ErrorCode::Cycle,
                    Some(&to.node),
                    Some(&to.port),
                    "This connection would create a dependency cycle",
                ));
            }
            if seen.insert(id.clone()) {
                pending.extend(
                    edges
                        .iter()
                        .filter(|c| c.from.node == id)
                        .map(|c| c.to.node.clone()),
                );
            }
        }
        edges.push(Connection { from, to });
        if !self.parents.is_empty()
            || self
                .document
                .nodes
                .iter()
                .any(|node| matches!(node.operation, NodeOp::Group { .. }))
        {
            let mut candidate = self.document.clone();
            candidate.connections = edges.clone();
            self.check_nested_budget(&candidate)?;
        }
        Ok(edges)
    }
    fn port_type(&self, port: &PortRef, output: bool) -> Result<ValueType, Diagnostic> {
        let node = self
            .document
            .nodes
            .iter()
            .find(|n| n.id == port.node)
            .ok_or_else(|| {
                diag(
                    ErrorCode::UnknownNode,
                    Some(&port.node),
                    Some(&port.port),
                    "Node is missing",
                )
            })?;
        let spec =
            node_ports(node, &self.document.public_parameters).map_err(|e| e.diagnostic())?;
        (if output { spec.outputs } else { spec.inputs })
            .get(&port.port)
            .copied()
            .ok_or_else(|| {
                diag(
                    ErrorCode::UnknownPort,
                    Some(&port.node),
                    Some(&port.port),
                    "Port is missing",
                )
            })
    }
    pub fn set_operation(&mut self, id: &str, operation: NodeOp) -> Result<(), Diagnostic> {
        let mut candidate = self
            .document
            .nodes
            .iter()
            .find(|n| n.id == id)
            .cloned()
            .ok_or_else(|| diag(ErrorCode::UnknownNode, Some(id), None, "Node is missing"))?;
        candidate.operation = operation;
        self.check_node(&candidate)?;
        if self
            .document
            .nodes
            .iter()
            .find(|n| n.id == id)
            .unwrap()
            .operation
            == candidate.operation
        {
            return Ok(());
        }
        let before = self.state(true);
        *self.document.nodes.iter_mut().find(|n| n.id == id).unwrap() = candidate;
        let ports: BTreeMap<_, _> = self
            .document
            .nodes
            .iter()
            .filter_map(|n| {
                match node_ports(n, &self.document.public_parameters) {
                    Ok(ports) => Some((n.id.clone(), ports)),
                    Err(error) => {
                        log::warn!("node ports unavailable during operation change: edited_node={id} node={} revision={} reason={error}", n.id, self.revision);
                        None
                    }
                }
            })
            .collect();
        let count = self.document.connections.len();
        self.document.connections.retain(|c| {
            if c.from.node != id && c.to.node != id {
                return true;
            }
            ports
                .get(&c.from.node)
                .and_then(|p| p.outputs.get(&c.from.port))
                .zip(ports.get(&c.to.node).and_then(|p| p.inputs.get(&c.to.port)))
                .is_some_and(|(a, b)| a == b)
        });
        let removed = count - self.document.connections.len();
        self.finish_edit(before);
        if removed > 0 {
            self.notice = Some(format!(
                "Removed {removed} incompatible connection(s); Undo restores them"
            ));
        }
        Ok(())
    }
    fn check_node(&self, node: &Node) -> Result<(), Diagnostic> {
        node_ports(node, &self.document.public_parameters).map_err(|e| e.diagnostic())?;
        let literal: &[f32] = match &node.operation {
            NodeOp::Float { value } => std::slice::from_ref(value),
            NodeOp::Vec2 { value } => value,
            NodeOp::Vec3 { value } => value,
            NodeOp::Color { value } => value,
            _ => &[],
        };
        if literal.iter().any(|v| !v.is_finite() || v.abs() > 1.0e6)
            || matches!(&node.operation, NodeOp::Color { value } if value.iter().any(|v| !(0.0..=1.0).contains(v)))
        {
            return Err(diag(
                ErrorCode::InvalidValue,
                Some(&node.id),
                Some("value"),
                "Constants must be finite (±1e6); RGBA colors must be normalized",
            ));
        }
        let old_outputs = self
            .document
            .nodes
            .iter()
            .find(|old| old.id == node.id)
            .map_or(0, |old| grouping::output_count(&old.operation));
        let total_outputs: usize = self
            .root_document()
            .nodes
            .iter()
            .map(|n| grouping::output_count(&n.operation))
            .sum();
        if total_outputs.saturating_sub(old_outputs) + grouping::output_count(&node.operation) > 1 {
            return Err(diag(
                ErrorCode::MultipleOutputs,
                Some(&node.id),
                None,
                "N1 allows one Color Output",
            ));
        }
        Ok(())
    }
    pub fn set_constant(
        &mut self,
        id: &str,
        component: usize,
        value: f32,
    ) -> Result<(), Diagnostic> {
        if !value.is_finite() || value.abs() > 1.0e6 {
            return Err(diag(
                ErrorCode::InvalidValue,
                Some(id),
                Some("value"),
                "Enter a finite value within ±1e6",
            ));
        }
        let node = self
            .document
            .nodes
            .iter()
            .find(|n| n.id == id)
            .ok_or_else(|| diag(ErrorCode::UnknownNode, Some(id), None, "Node is missing"))?;
        let mut op = node.operation.clone();
        match &mut op {
            NodeOp::Float { value: v } if component == 0 => *v = value,
            NodeOp::Vec2 { value: v } if component < 2 => v[component] = value,
            NodeOp::Vec3 { value: v } if component < 3 => v[component] = value,
            NodeOp::Color { value: v } if component < 4 && (0.0..=1.0).contains(&value) => {
                v[component] = value
            }
            _ => {
                return Err(diag(
                    ErrorCode::InvalidValue,
                    Some(id),
                    Some("value"),
                    "Invalid component or RGBA value outside 0..1",
                ))
            }
        };
        self.set_operation(id, op)
    }
    pub fn set_parameter_default(
        &mut self,
        id: &str,
        component: usize,
        value: f32,
    ) -> Result<(), Diagnostic> {
        let p = self
            .document
            .public_parameters
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| {
                diag(
                    ErrorCode::UnknownParameter,
                    None,
                    Some(id),
                    "Parameter is missing",
                )
            })?;
        if component >= dimensions(p.value_type)
            || !value.is_finite()
            || value < p.min
            || value > p.max
            || p.value_type == ValueType::Bool && value != 0. && value != 1.
        {
            return Err(diag(
                ErrorCode::InvalidValue,
                None,
                Some(id),
                "Value is outside the parameter range",
            ));
        }
        if p.default[component] == value {
            return Ok(());
        }
        let before = self.state(true);
        self.document
            .public_parameters
            .iter_mut()
            .find(|p| p.id == id)
            .unwrap()
            .default[component] = value;
        self.finish_edit(before);
        Ok(())
    }
    pub fn fit_all(&mut self, canvas: Rect) {
        self.fit_nodes(canvas, false);
    }
    pub fn fit_selected(&mut self, canvas: Rect) {
        self.fit_nodes(canvas, !self.selected.is_empty());
    }
    fn fit_nodes(&mut self, canvas: Rect, selected_only: bool) {
        if self.document.nodes.is_empty() {
            self.view = Default::default();
            return;
        }
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for n in &self.document.nodes {
            if selected_only && !self.selected.contains(&n.id) {
                continue;
            }
            if n.position.iter().any(|v| !v.is_finite()) {
                continue;
            }
            let height = view::node_height(n, &self.document);
            lo[0] = lo[0].min(f64::from(n.position[0]));
            lo[1] = lo[1].min(f64::from(n.position[1]));
            hi[0] = hi[0].max(f64::from(n.position[0] + view::NODE_WIDTH));
            hi[1] = hi[1].max(f64::from(n.position[1] + height));
        }
        if lo.iter().chain(hi.iter()).any(|v| !v.is_finite()) {
            return;
        }
        let zoom = ((f64::from((canvas.width() - 80.).max(1.)) / (hi[0] - lo[0]).max(1.))
            .min(f64::from((canvas.height() - 80.).max(1.)) / (hi[1] - lo[1]).max(1.)))
        .clamp(1.0e-6, 1.2) as f32;
        self.view.zoom = zoom;
        self.view.pan = [
            (f64::from(canvas.width()) * 0.5 - (lo[0] + hi[0]) * 0.5 * f64::from(zoom)) as f32,
            (f64::from(canvas.height()) * 0.5 - (lo[1] + hi[1]) * 0.5 * f64::from(zoom)) as f32,
        ];
    }
    fn arrange_without_history(&mut self) {
        if let Ok(plan) = self.observed("layout.arrange", "document", motion_graph::validate(&self.document, &self.options)) {
            let mut depth: BTreeMap<String, usize> = BTreeMap::new();
            let mut rows: BTreeMap<usize, f32> = BTreeMap::new();
            for id in plan.topological_order {
                let level = self
                    .document
                    .connections
                    .iter()
                    .filter(|c| c.to.node == id)
                    .map(|c| depth.get(&c.from.node).copied().unwrap_or(0) + 1)
                    .max()
                    .unwrap_or(0);
                depth.insert(id.clone(), level);
                let row = rows.entry(level).or_default();
                let height = self
                    .document
                    .nodes
                    .iter()
                    .find(|node| node.id == id)
                    .map(|node| view::node_height(node, &self.document))
                    .unwrap_or(114.);
                if let Some(node) = self.document.nodes.iter_mut().find(|n| n.id == id) {
                    node.position = [level as f32 * 244., *row];
                }
                *row += height + 40.;
            }
        }
    }
}
fn dimensions(ty: ValueType) -> usize {
    match ty {
        ValueType::Vec2 => 2,
        ValueType::Vec3 => 3,
        ValueType::Color => 4,
        _ => 1,
    }
}
fn diag(
    code: ErrorCode,
    node: Option<&str>,
    port: Option<&str>,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic {
        code,
        node: node.map(str::to_owned),
        port: port.map(str::to_owned),
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "node_editor/tests.rs"]
mod tests;
