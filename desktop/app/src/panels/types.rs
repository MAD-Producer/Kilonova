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
    ProjectChoice(crate::node_tasks::CloseChoice),
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
    ReloadStyle,
    Favorites(std::collections::BTreeSet<String>),
    Language(Locale),
    ResetWorkspace,
    ShowPanel(dock::DockId),
    About,
    ExportDiagnostics,
    ToggleDiagnostics,
    PreviewMode(i32),
    SelectAt([f64; 2]),
    PreviewDrag {
        object: u64,
        delta: [f32; 2],
        size: [i32; 2],
    },
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
    pub styles: Option<std::sync::Arc<motion_ui::style::StyleSheet>>,
    pub(crate) cursor: Option<motion_ui::cursor::Cursor>,
    pub(crate) drag_readout: Option<([f32; 2], String)>,
    pub(crate) motion: motion_ui::motion::Motion,
    pub(crate) project: ProjectState,
    pub(crate) properties: PropertiesState,
    pub(crate) timeline: TimelineState,
    pub locale: Locale,
    pub(crate) focused: Option<dock::Panel>,
    pub(crate) auxiliary: super::auxiliary::State,
    pub(crate) library_scroll: motion_ui::widgets::ScrollArea,
    pub(crate) library_height: f32,
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
    pub transform_controls: crate::panels::transform_controls::Controls,
}

impl Panels {
    pub fn set_stylesheet(&mut self, sheet: motion_ui::style::StyleSheet) {
        self.motion.set_easing(sheet.easing());
        self.styles = Some(std::sync::Arc::new(sheet));
    }

    pub(crate) fn fx_console_in(&self, owner: Option<motion_ui::dock::Panel>) -> bool {
        self.fx_console
            .as_ref()
            .is_some_and(|console| console.owner == owner)
    }
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
    pub last_value: f32,
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
    pub search_field: motion_ui::widgets::SearchField,
    pub asset_list: motion_ui::widgets::ListView,
    pub query: String,
    pub search: bool,
}

#[derive(Default)]
pub(crate) struct PropertiesState {
    pub consumed_press: bool,
    pub transform_collapsed: bool,
    pub checkbox_focused: bool,
    pub properties_scroll: f32,
    pub content_height: f32,
    pub field: Option<Field>,
}

#[derive(Default)]
pub(crate) struct TimelineState {
    pub rows_cache: Option<(
        (String, String, u64),
        std::sync::Arc<Vec<super::timeline::Row>>,
    )>,
    pub expanded: std::collections::HashSet<u64>,
    pub collapsed_transforms: std::collections::HashSet<u64>,
    pub filters: std::collections::HashMap<u64, PropertyFilter>,
    pub graph_mode: bool,
    pub scroll: f32,
    pub zoom: f32,
    pub offset: f32,
    pub scrubbing: bool,
    pub last_seek: Option<f64>,
    pub snap_guide: Option<f64>,
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
impl TimelineState {
    pub fn reopen_properties(&mut self, object: u64) {
        if self.filters.get(&object) == Some(&PropertyFilter::Properties(0)) {
            self.filters.remove(&object);
        }
    }
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
