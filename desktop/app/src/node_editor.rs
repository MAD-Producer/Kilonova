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
    pub epoch: u64,
    pub source_path: Option<PathBuf>,
}
impl EditorSnapshot {
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
    Examples(EditorSnapshot),
    Open(EditorSnapshot),
    Save(EditorSnapshot),
    SaveAs(EditorSnapshot),
    Validate(EditorSnapshot),
    Compile(EditorSnapshot),
    ExportMsfx(EditorSnapshot),
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
        self.zoom = zoom.clamp(0.25, 2.5);
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
}

pub struct NodeEditor {
    document: GraphDocument,
    options: CompileOptions,
    source_path: Option<PathBuf>,
    saved_document: Option<GraphDocument>,
    saved_revision: Option<u64>,
    pub view: CanvasView,
    selected: BTreeSet<String>,
    primary: Option<String>,
    undo: Vec<History>,
    redo: Vec<History>,
    revision: u64,
    semantic_revision: u64,
    epoch: u64,
    next_id: u64,
    focus: bool,
    drag: Option<Drag>,
    field: Option<Field>,
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
    floating_mode: bool, // true=漂浮形态(有侧边栏), false=嵌入形态(无侧边栏)
}

#[derive(Clone, Debug)]
struct ContextMenu {
    position: [f32; 2],
    target_node: Option<String>, // None = 画布菜单, Some(id) = 节点菜单
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
impl NodeEditor {
    pub fn new(document: GraphDocument) -> Self {
        let options = CompileOptions::default();
        let diagnostic = motion_graph::validate(&document, &options)
            .err()
            .map(|e| e.diagnostic());
        Self {
            document,
            options,
            source_path: None,
            saved_document: None,
            saved_revision: None,
            view: Default::default(),
            selected: BTreeSet::new(),
            primary: None,
            undo: vec![],
            redo: vec![],
            revision: 0,
            semantic_revision: 0,
            epoch: 1,
            next_id: 1,
            focus: false,
            drag: None,
            field: None,
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
        self.saved_document.as_ref() != Some(&self.document)
    }
    pub fn focused(&self) -> bool {
        self.focus
    }
    pub fn focus_panel(&mut self) {
        self.focus = true;
    }
    pub fn editing(&self) -> bool {
        self.drag.is_some() || self.field.is_some() || self.search_focus
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
            document: self.document.clone(),
            options: self.options.clone(),
            revision: self.revision,
            semantic_revision: self.semantic_revision,
            epoch: self.epoch,
            source_path: self.source_path.clone(),
        }
    }
    /// Commit a valid inspector field and an active move before capturing an owned
    /// document. The host may send this action to a worker without borrowing UI state.
    pub fn request(&mut self, request: Request) -> Result<Action, Diagnostic> {
        self.commit_field()?;
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
                self.pending_compile = Some(self.semantic_revision);
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
        self.replace_document(document, Some(path));
        true
    }
    pub fn replace_document(&mut self, document: GraphDocument, path: Option<PathBuf>) {
        let next_epoch = self.epoch.wrapping_add(1);
        let options = self.options.clone();
        *self = Self::new(document);
        self.epoch = next_epoch;
        self.options = options;
        self.source_path = path;
        self.saved_document = Some(self.document.clone());
        self.saved_revision = Some(self.revision);
        self.refresh_diagnostic();
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
        if snapshot.epoch == self.epoch && self.pending_compile == Some(snapshot.semantic_revision)
        {
            self.pending_compile = None;
        }
    }
    /// Reject stale worker responses, retain the previous valid compiled result.
    pub fn accept_compile(
        &mut self,
        snapshot: &EditorSnapshot,
        result: Result<CompileReport, Diagnostic>,
    ) -> bool {
        if snapshot.epoch != self.epoch || snapshot.semantic_revision != self.semantic_revision {
            return false;
        }
        self.pending_compile = None;
        match result {
            Ok(report) => {
                self.compiled = Some((self.semantic_revision, report));
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
        (self.epoch, self.semantic_revision)
    }
    pub fn cancel_interaction(&mut self) {
        self.cancel_edit_only();
        self.focus = false;
    }
    fn state(&self, semantic: bool) -> History {
        History {
            document: self.document.clone(),
            options: self.options.clone(),
            semantic,
        }
    }
    fn refresh_diagnostic(&mut self) {
        self.diagnostic = motion_graph::validate(&self.document, &self.options)
            .err()
            .map(|e| e.diagnostic());
    }
    fn finish_edit(&mut self, before: History) {
        self.undo.push(before.clone());
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.revision = self.revision.wrapping_add(1);
        if before.semantic {
            self.semantic_revision = self.semantic_revision.wrapping_add(1);
            self.pending_compile = None;
        }
        self.notice = None;
        self.refresh_diagnostic();
    }
    pub fn undo(&mut self) -> bool {
        self.cancel_edit_only();
        if let Some(previous) = self.undo.pop() {
            let current = self.state(previous.semantic);
            self.redo.push(current);
            self.document = previous.document;
            self.options = previous.options;
            self.bump(previous.semantic);
            true
        } else {
            false
        }
    }
    pub fn redo(&mut self) -> bool {
        self.cancel_edit_only();
        if let Some(next) = self.redo.pop() {
            let current = self.state(next.semantic);
            self.undo.push(current);
            self.document = next.document;
            self.options = next.options;
            self.bump(next.semantic);
            true
        } else {
            false
        }
    }
    fn bump(&mut self, semantic: bool) {
        self.revision = self.revision.wrapping_add(1);
        if semantic {
            self.semantic_revision = self.semantic_revision.wrapping_add(1);
            self.pending_compile = None;
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
    fn cancel_edit_only(&mut self) {
        match self.drag.take() {
            Some(Drag::Nodes { before, .. }) => self.document = before.document,
            Some(Drag::Marquee { base, primary, .. }) => {
                self.selected = base;
                self.primary = primary;
            }
            _ => {}
        }
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
        if self.selected.is_empty() {
            return false;
        }
        let before = self.state(true);
        self.field = None;
        self.drag = None;
        self.document
            .nodes
            .retain(|n| !self.selected.contains(&n.id));
        self.document.connections.retain(|c| {
            !self.selected.contains(&c.from.node) && !self.selected.contains(&c.to.node)
        });
        self.selected.clear();
        self.primary = None;
        self.finish_edit(before);
        true
    }
    pub fn disconnect_input(&mut self, input: &PortRef) -> bool {
        if !self.document.connections.iter().any(|c| c.to == *input) {
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
        motion_graph::check_connection(&self.document, &from, &to)
            .map_err(|error| error.diagnostic())?;
        let mut edges: Vec<_> = self
            .document
            .connections
            .iter()
            .filter(|c| c.to != to && old_input != Some(&c.to))
            .cloned()
            .collect();
        if self.document.connections.len() >= self.options.max_connections
            && edges.len() == self.document.connections.len()
        {
            return Err(diag(
                ErrorCode::Budget,
                None,
                None,
                "Connection budget reached",
            ));
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
        if edges == self.document.connections {
            return Ok(());
        }
        let before = self.state(true);
        self.document.connections = edges;
        self.finish_edit(before);
        Ok(())
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
                node_ports(n, &self.document.public_parameters)
                    .ok()
                    .map(|p| (n.id.clone(), p))
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
        if matches!(node.operation, NodeOp::ColorOutput)
            && self
                .document
                .nodes
                .iter()
                .any(|n| n.id != node.id && matches!(n.operation, NodeOp::ColorOutput))
        {
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
        if self.document.nodes.is_empty() {
            self.view = Default::default();
            return;
        }
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for n in &self.document.nodes {
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
        let zoom = ((f64::from(canvas.width().max(1.) - 80.) / (hi[0] - lo[0]).max(1.))
            .min(f64::from(canvas.height().max(1.) - 80.) / (hi[1] - lo[1]).max(1.)))
        .clamp(0.25, 1.2) as f32;
        self.view.zoom = zoom;
        self.view.pan = [
            (f64::from(canvas.width()) * 0.5 - (lo[0] + hi[0]) * 0.5 * f64::from(zoom)) as f32,
            (f64::from(canvas.height()) * 0.5 - (lo[1] + hi[1]) * 0.5 * f64::from(zoom)) as f32,
        ];
    }
    fn arrange_without_history(&mut self) {
        if let Ok(plan) = motion_graph::validate(&self.document, &self.options) {
            let mut depth: BTreeMap<String, usize> = BTreeMap::new();
            let mut rows: BTreeMap<usize, usize> = BTreeMap::new();
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
                if let Some(node) = self.document.nodes.iter_mut().find(|n| n.id == id) {
                    node.position = [level as f32 * 244., *row as f32 * 154.];
                }
                *row += 1;
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
