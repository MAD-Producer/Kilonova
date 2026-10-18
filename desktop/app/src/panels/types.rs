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
    ImportEffectPackage,
    ImportEffectPath(std::path::PathBuf),
    ImportPath(std::path::PathBuf),
    CancelImport,
    AnalyzeAudio(u64),
    CancelAudioAnalysis,
    ExportPng,
    ExportMovie,
    CancelExport,
    AddNull,
    AddAdjustment,
    AddPath(motion_core::vector::VectorPath),
    AddShape(motion_core::vector::ShapeKind),
    NewText,
    CompositionSettings,
    NewComposition,
    CompositionPreset(u32, u32),
    Plugin(Value),
    NewNodeEffect(u64),
    EditNodeEffect {
        object: u64,
        effect: u64,
    },
    ApplyNodeGraph,
    ReturnToEffects,
    ReturnToNodeDraft,
    AddSolid,
    Pack,
    RefreshState,
    ReloadStyle,
    Favorites(std::collections::BTreeSet<String>),
    Language(Locale),
    ResetWorkspace,
    ManageWorkspace,
    ApplyWorkspace {
        visible: Vec<dock::DockId>,
        name: String,
    },
    Workspace(crate::workspace::Preset),
    WindowDrag,
    WindowClose,
    WindowMinimize,
    WindowMaximize,
    ShowPanel(dock::DockId),
    MaskTimeline {
        object: u64,
        mask: u64,
    },
    About,
    ExportDiagnostics,
    ToggleDiagnostics,
    /// Replace the optional timeline column visibility (view state only).
    TimelineColumnVisibility(ColumnVisibility),
    PreviewMode(i32),
    /// Switch the composition viewer between camera and observation views.
    ViewKind(i32),
    /// Orbit/pan the observation camera; host picks the gesture by view.
    ViewOrbit {
        delta: [f32; 2],
    },
    /// Zoom the observation camera around its target.
    ViewZoom {
        ratio: f64,
        size: [i32; 2],
    },
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
    EditChecked {
        command: Command,
        revision: u64,
    },
    EditBatch(Vec<Command>),
}

#[derive(Default)]
pub struct Panels {
    pub workspace_preset: crate::workspace::Preset,
    pub styles: Option<std::sync::Arc<motion_ui::style::StyleSheet>>,
    pub(crate) cursor: Option<motion_ui::cursor::Cursor>,
    pub(crate) drag_readout: Option<([f32; 2], String)>,
    pub(crate) motion: motion_ui::motion::Motion,
    pub(crate) project: ProjectState,
    pub(crate) properties: PropertiesState,
    pub(crate) timeline: TimelineState,
    pub(crate) layer_actions: super::layer_actions::Controls,
    pub locale: Locale,
    pub(crate) focused: Option<dock::Panel>,
    pub(crate) expressions: super::expressions::Controls,
    pub(crate) auxiliary: super::auxiliary::State,
    pub(crate) library_scroll: motion_ui::widgets::ScrollArea,
    pub(crate) library_height: f32,
    pub(crate) browser: crate::panels::effect_controls::Controls,
    pub selected: Option<u64>,
    /// Stable selection collection for timeline multi-select. `selected` is
    /// kept as the primary layer for panels whose controls still need one
    /// active object.
    pub(crate) selected_layers: Vec<u64>,
    pub(crate) selection_anchor: Option<u64>,
    pub(crate) selection_composition: Option<String>,
    pub playing: bool,
    pub message: Option<String>,
    pub(crate) menu: Option<usize>,
    /// First visible row in the application menu when the popup is taller than
    /// the available window height.
    pub(crate) menu_scroll: usize,
    pub(crate) panel_menu: Option<(dock::DockId, [f32; 2])>,
    pub(crate) header_hits: Vec<HeaderHit>,
    pub(crate) header_size: Option<[f32; 2]>,
    pub effect_search: Option<EffectSearch>,
    pub effect_controls: crate::panels::effect_controls::Controls,
    pub timeline_anchor: super::properties::AnchorControls,
    pub animation_controls: crate::panels::animation_controls::Controls,
    pub parent_controls: crate::panels::parent_controls::Controls,
    pub timeline_columns: crate::panels::timeline_columns::Controls,
    pub camera_controls: crate::panels::camera_controls::Controls,
    pub(crate) audio_controls: crate::panels::audio::Controls,
    pub node_editor: crate::node_editor::NodeEditor,
    pub node_preview: crate::node_preview::Controls,
    pub path_controls: crate::panels::path_controls::Controls,
    pub transform_controls: crate::panels::transform_controls::Controls,
    pub(crate) composition: CompositionState,
}

#[derive(Default)]
pub(crate) struct CompositionState {
    /// Last pointer position of an in-progress right-drag orbit gesture.
    pub orbit: Option<[f32; 2]>,
    pub navigation: crate::viewer_navigation::Navigation,
    pub pan: Option<([f32; 2], crate::viewer_navigation::Navigation)>,
    pub zoom: motion_ui::widgets::NumberEditor,
    pub zoom_before: Option<crate::viewer_navigation::Navigation>,
    pub context: Option<u64>,
    pub mapping_failures: u8,
}

impl Panels {
    pub(crate) fn select_only(&mut self, object: Option<u64>) {
        self.path_controls.clear_mask_selection();
        self.audio_controls.clear_selection();
        self.selected = object;
        self.selection_anchor = object.filter(|id| *id != 0);
        self.animation_controls.clear_key();
        self.selected_layers.clear();
        if let Some(object) = object.filter(|id| *id != 0) {
            self.selected_layers.push(object);
        }
    }

    pub(crate) fn selection_contains(&self, object: u64) -> bool {
        self.selected == Some(object) || self.selected_layers.contains(&object)
    }

    pub(crate) fn selection_ids(&self) -> Vec<u64> {
        let mut ids = self.selected_layers.clone();
        if let Some(primary) = self.selected.filter(|id| *id != 0) {
            ids.retain(|id| *id != primary);
            ids.insert(0, primary);
        }
        ids
    }

    pub(crate) fn editable_selection(&self, state: &serde_json::Value) -> Vec<u64> {
        self.selection_ids()
            .into_iter()
            .filter(|id| {
                state["project"]["layers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|layer| {
                        layer["id"].as_u64() == Some(*id) && layer["locked"].as_bool() != Some(true)
                    })
            })
            .collect()
    }

    /// All desktop effect entry points share the same atomic selection request.
    pub(crate) fn effect_request(
        &self,
        mut request: serde_json::Value,
        state: &serde_json::Value,
    ) -> serde_json::Value {
        if request["op"] == "add"
            && request["object"]
                .as_u64()
                .is_some_and(|id| self.selection_contains(id))
        {
            let ids = self.editable_selection(state);
            request.as_object_mut().unwrap().remove("object");
            request["objects"] = serde_json::json!(ids);
        }
        request
    }

    pub(crate) fn toggle_selection(&mut self, object: u64) {
        if object == 0 {
            self.select_only(Some(object));
            return;
        }
        self.animation_controls.clear_key();
        self.selected_layers = self.selection_ids();
        self.selection_anchor = Some(object);
        if let Some(index) = self.selected_layers.iter().position(|id| *id == object) {
            self.selected_layers.remove(index);
            if self.selected == Some(object) {
                self.selected = self.selected_layers.last().copied();
            }
        } else {
            self.selected_layers.push(object);
            self.selected = Some(object);
        }
    }

    pub(crate) fn add_selection(&mut self, object: u64) {
        if object != 0 && !self.selected_layers.contains(&object) {
            self.selected_layers.push(object);
        }
        self.selected = Some(object);
    }

    pub(crate) fn normalize_selection(&mut self, state: &serde_json::Value) -> bool {
        let mut context_changed = false;
        if let Some(composition) = state["project"]["composition_id"].as_str() {
            if self
                .selection_composition
                .as_deref()
                .is_some_and(|old| old != composition)
            {
                context_changed = true;
                self.select_only(None);
            }
            self.selection_composition = Some(composition.to_owned());
        }
        let valid = |id: &u64| {
            *id != 0
                && state["project"]["layers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|layer| layer["id"].as_u64() == Some(*id))
        };
        let invalid = self.selection_ids().iter().any(|id| !valid(id))
            || self
                .timeline
                .moving
                .as_ref()
                .is_some_and(|moving| moving.layers.iter().any(|(id, _, _)| !valid(id)))
            || self
                .timeline
                .trimming
                .as_ref()
                .is_some_and(|(id, _, _, _, _)| !valid(id));
        self.selected_layers.retain(|id| valid(id));
        if self.selected.is_some_and(|id| id != 0 && !valid(&id)) {
            self.selected = self.selected_layers.last().copied();
        }
        if let Some(primary) = self.selected.filter(|id| *id != 0) {
            if !self.selected_layers.contains(&primary) {
                self.selected_layers.insert(0, primary);
            }
        } else if self.selected.is_none() || self.selected == Some(0) {
            self.selected_layers.clear();
        }
        if self.selection_anchor.is_some_and(|id| !valid(&id)) {
            self.selection_anchor = self.selected.filter(|id| *id != 0);
        }
        context_changed || invalid
    }

    pub fn set_stylesheet(&mut self, sheet: motion_ui::style::StyleSheet) {
        self.motion.set_easing(sheet.easing());
        self.styles = Some(std::sync::Arc::new(sheet));
    }

    pub(crate) fn effect_search_in(&self, owner: Option<motion_ui::dock::Panel>) -> bool {
        self.effect_search
            .as_ref()
            .is_some_and(|search| search.owner == owner)
    }
}
pub(crate) struct HeaderHit {
    pub rect: Rect,
    pub panel: dock::DockId,
    pub action: bool,
}

/// Effect search overlay (Ctrl+Space)
#[derive(Clone, Debug)]
pub struct EffectSearch {
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
    pub context_menu: Option<[f32; 2]>,
}

pub(crate) struct PropertiesState {
    pub consumed_press: bool,
    pub transform_collapsed: bool,
    pub geometry_collapsed: bool,
    pub effects_collapsed: bool,
    pub effects: super::effect_controls::Controls,
    pub anchor: super::properties::AnchorControls,
    pub checkbox_focused: bool,
    pub properties_scroll: f32,
    pub content_height: f32,
    pub field: Option<Field>,
}

impl Default for PropertiesState {
    fn default() -> Self {
        Self {
            consumed_press: false,
            transform_collapsed: false,
            geometry_collapsed: true,
            effects_collapsed: true,
            effects: Default::default(),
            anchor: Default::default(),
            checkbox_focused: false,
            properties_scroll: 0.,
            content_height: 0.,
            field: None,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct TimelineViewport {
    zoom: f32,
    offset: f32,
    scroll: f32,
    fitted: bool,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ScrubRuler {
    #[default]
    Timeline,
    Graph,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ColumnVisibility {
    pub blend: bool,
    pub parent: bool,
    pub matte: bool,
}
impl Default for ColumnVisibility {
    fn default() -> Self {
        Self {
            blend: true,
            parent: true,
            matte: true,
        }
    }
}
/// Column groups that toggle together (the composite group shares a header).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ColumnGroup {
    Composite,
    Hierarchy,
}
impl ColumnVisibility {
    pub fn toggle_group(&mut self, group: ColumnGroup) {
        match group {
            ColumnGroup::Composite => {
                let visible = !(self.blend && self.matte);
                self.blend = visible;
                self.matte = visible;
            }
            ColumnGroup::Hierarchy => self.parent = !self.parent,
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct TimelineState {
    pub(crate) column_visibility: ColumnVisibility,
    pub(crate) graph_box: super::graph_box::Controls,
    pub rows_cache: Option<(
        (String, String, u64),
        std::sync::Arc<Vec<super::timeline::Row>>,
    )>,
    pub expanded: std::collections::HashSet<u64>,
    pub collapsed_transforms: std::collections::HashSet<u64>,
    pub collapsed_geometry: std::collections::HashSet<u64>,
    pub expanded_effects: std::collections::HashSet<u64>,
    pub collapsed_audio: std::collections::HashSet<u64>,
    pub vector_open: std::collections::HashSet<(u64, u64, u8)>,
    pub collapsed_masks: std::collections::HashSet<u64>,
    pub collapsed_mask_items: std::collections::HashSet<(u64, u64)>,
    pub filters: std::collections::HashMap<u64, PropertyFilter>,
    pub graph_mode: bool,
    pub(super) clip_view: Option<TimelineViewport>,
    pub(super) curve_view: Option<TimelineViewport>,
    pub speed_graph: bool,
    pub graph_component: usize,
    pub graph_fitted: bool,
    pub graph_grid: bool,
    pub(crate) graph_drag: Option<super::graph_handles::Drag>,
    pub(crate) key_drag: Option<super::graph_keys::Drag>,
    pub graph_y_zoom: f32,
    pub graph_y_offset: f32,
    pub scroll: f32,
    pub scroll_extent_cache: Option<(u64, f32)>,
    pub zoom: f32,
    pub offset: f32,
    pub horizontal_drag: Option<f32>,
    pub edge_pan_at: Option<std::time::Instant>,
    /// Acknowledged frames repaint the consumed pointer without advancing navigation.
    pub navigation_paint_only: bool,
    pub expression_rows: Vec<motion_core::ExpressionTarget>,
    pub zoom_track: Option<Rect>,
    pub scrubbing: bool,
    /// Only the ruler where the pointer was pressed may seek until release.
    pub scrub_ruler: ScrubRuler,
    pub last_seek: Option<f64>,
    pub snap_guide: Option<f64>,
    pub moving: Option<MovingLayers>,
    pub trimming: Option<(u64, f64, f64, bool, f64)>,
    /// Pending/active z-order drag: (layer, press position, past threshold).
    pub reorder: Option<(u64, [f32; 2], bool)>,
    pub timeline_context_menu: Option<[f32; 2]>,
    pub timeline_query: String,
    pub timeline_search: bool,
}

#[derive(Clone)]
pub(crate) struct MovingLayers {
    pub start_mouse: f32,
    pub initial: i32,
    pub last: i32,
    pub layers: Vec<(u64, i32, i32)>,
}

impl MovingLayers {
    pub(crate) fn clamp_delta(&self, delta: i64) -> i32 {
        let limit = i64::from(motion_core::MAX_FRAMES);
        let lower = self
            .layers
            .iter()
            .map(|(_, start, _)| -limit - i64::from(*start))
            .max()
            .unwrap_or(0);
        let upper = self
            .layers
            .iter()
            .map(|(_, start, duration)| limit - i64::from(*start) - i64::from(*duration))
            .min()
            .unwrap_or(0);
        delta.clamp(lower, upper) as i32
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum PropertyFilter {
    #[default]
    All,
    Properties(u8),
    Animated,
}
impl TimelineState {
    pub fn set_graph_mode(&mut self, enabled: bool) {
        if self.graph_mode == enabled {
            return;
        }
        if self.graph_drag.as_ref().is_some_and(|d| d.begun())
            || self.key_drag.as_ref().is_some_and(|d| d.begun())
        {
            log::warn!("graph mode changed with active gesture enabled={enabled} handle_drag={} key_drag={}", self.graph_drag.is_some(), self.key_drag.is_some());
        }
        let current = TimelineViewport {
            zoom: self.zoom,
            offset: self.offset,
            scroll: self.scroll,
            fitted: self.graph_fitted,
        };
        let restore = if enabled {
            self.clip_view = Some(current);
            self.curve_view
        } else {
            self.curve_view = Some(current);
            self.clip_view
        };
        if let Some(view) = restore {
            self.zoom = view.zoom;
            self.offset = view.offset;
            self.scroll = view.scroll;
            self.graph_fitted = view.fitted;
        } else if enabled {
            self.graph_fitted = false;
        }
        self.horizontal_drag = None;
        self.edge_pan_at = None;
        self.graph_mode = enabled;
        self.scrubbing = false;
        self.last_seek = None;
        self.snap_guide = None;
    }
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
