//! Data types for the panels system
use crate::i18n::Locale;
use motion_core::{Command, Property};
use motion_ui::{dock, input::Rect};
use serde_json::Value;

#[derive(Debug)]
pub enum Action {
    Node(crate::node_editor::Action),
    New,
    Open,
    Save,
    ImportMedia,
    ImportPath(std::path::PathBuf),
    CancelImport,
    ExportPng,
    ExportMovie,
    CancelExport,
    AddNull,
    AddAdjustment,
    AddPath(motion_core::vector::VectorPath),
    AddShape(motion_core::vector::ShapeKind),
    NewText,
    CompositionSettings,
    Plugin(Value),
    AddSolid,
    Pack,
    RefreshState,
    Language(Locale),
    ResetWorkspace,
    ShowPanel(dock::DockId),
    About,
    PreviewMode(i32),
    SelectAt([f64; 2]),
    OpenPath(std::path::PathBuf),
    Float(dock::DockId),
    Redock(dock::DockId),
    ClosePanel(dock::DockId),
    Activate {
        dock: dock::DockId,
        tab: dock::DockId,
    },
    History(i32),
    Seek(f64),
    TogglePlay,
    Edit(Command),
    EditBatch(Vec<Command>),
}

#[derive(Default)]
pub struct Panels {
    pub(crate) project: ProjectState,
    pub(crate) properties: PropertiesState,
    pub(crate) timeline: TimelineState,
    pub locale: Locale,
    pub(crate) focused: Option<dock::Panel>,
    pub(crate) auxiliary: super::auxiliary::State,
    pub(crate) browser: crate::panels::effect_controls::Controls,
    pub selected: Option<u64>,
    pub playing: bool,
    pub message: Option<String>,
    pub(crate) menu: Option<usize>,
    pub(crate) panel_menu: Option<(dock::DockId, [f32; 2])>,
    pub(crate) header_hits: Vec<HeaderHit>,
    pub(crate) header_size: Option<[f32; 2]>,
    pub fx_console: Option<FxConsole>,
    pub effect_controls: crate::panels::effect_controls::Controls,
    pub animation_controls: crate::panels::animation_controls::Controls,
    pub parent_controls: crate::panels::parent_controls::Controls,
    pub camera_controls: crate::panels::camera_controls::Controls,
    pub node_editor: crate::node_editor::NodeEditor,
    pub node_preview: crate::node_preview::Controls,
    pub path_controls: crate::panels::path_controls::Controls,
}
pub(crate) struct HeaderHit {
    pub rect: Rect,
    pub panel: dock::DockId,
    pub action: bool,
}

/// FX Console quick search overlay (Ctrl+Space)
#[derive(Clone, Debug)]
pub struct FxConsole {
    pub position: [f32; 2],
    pub query: String,
    pub selected_index: usize,
    pub owner: Option<dock::Panel>,
}

pub(crate) struct Field {
    pub object: u64,
    pub property: Property,
    pub axis: usize,
    pub text: String,
    pub replace: bool,
    pub rect: Rect,
    pub scrubbing: Option<ScrubState>,
}

pub(crate) struct ScrubState {
    pub start_mouse: f32,
    pub start_value: f32,
    pub begun: bool,
}

pub struct Frame {
    pub panel: dock::Panel,
    pub tabs: Rect,
    pub content: Rect,
}

#[derive(Clone, Copy)]
pub(crate) enum Tool {
    Open,
    Save,
    Undo,
    Redo,
    Solid,
    Text,
    Null,
    Rectangle,
    Ellipse,
    Play,
    Pause,
    First,
    Last,
}

#[derive(Default)]
pub(crate) struct ProjectState {
    pub query: String,
    pub search: bool,
}

#[derive(Default)]
pub(crate) struct PropertiesState {
    pub transform_collapsed: bool,
    pub properties_scroll: f32,
    pub field: Option<Field>,
}

#[derive(Default)]
pub(crate) struct TimelineState {
    pub expanded: std::collections::HashSet<u64>,
    pub filters: std::collections::HashMap<u64, PropertyFilter>,
    pub graph_mode: bool,
    pub scroll: f32,
    pub zoom: f32,
    pub offset: f32,
    pub scrubbing: bool,
    pub moving: Option<(u64, f32, i32, i32, i32)>,
    pub trimming: Option<(u64, f64, f64, bool, f64)>,
    pub timeline_context_menu: Option<[f32; 2]>,
    pub timeline_query: String,
    pub timeline_search: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PropertyFilter {
    #[default]
    All,
    Properties(u8),
    Animated,
}
pub(crate) fn property_bit(property: Property) -> u8 {
    match property {
        Property::Position => 1,
        Property::Scale => 2,
        Property::Rotation => 4,
        Property::Opacity => 8,
        _ => 0,
    }
}
impl PropertyFilter {
    pub fn admits(self, property: Property, animated: bool) -> bool {
        match self {
            Self::All => true,
            Self::Properties(mask) => mask & property_bit(property) != 0,
            Self::Animated => animated,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct KeySelection {
    pub object: u64,
    pub property: Property,
    pub axis: Option<motion_core::Axis>,
    pub frame: u32,
}
