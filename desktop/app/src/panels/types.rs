//! Data types for the panels system
use crate::i18n::Locale;
use motion_core::{Command, Property};
use motion_ui::{dock, input::Rect};
use serde_json::Value;

#[derive(Debug)]
pub enum Action {
    OpenPluginEditor,
    OpenViewerInspection { floating: bool, point: [f32; 2] },
    CaptureViewerSnapshot { floating: bool },
    SaveViewerSnapshot { floating: bool },
    BeginEffectPick(super::effect_pick::Ticket),
    EffectPick { ticket: super::effect_pick::Ticket, point: [f64; 2] },
    SaveAnimationPreset { object: u64, effects: Option<Vec<u64>> },
    ApplyAnimationPreset { path: std::path::PathBuf, objects: Vec<u64> },
    OpenAnimationPreset,
    RefreshAnimationPresets,
    CancelAnimationPreset,
    LocateLibraryEffect(String),
    RevealLibraryEffect { object: u64, effect: u64 },
    DistributeLayers {
        objects: Vec<u64>,
        distribution: motion_host::ops::alignment::Distribution,
    },
    AlignLayers {
        objects: Vec<u64>,
        edge: motion_host::ops::alignment::Edge,
        target: motion_host::ops::alignment::Target,
    },
    Node(crate::node_editor::Action),
    New,
    Open,
    Save,
    ProjectChoice(crate::node_tasks::CloseChoice),
    ImportMedia,
    ProjectImport {
        source: crate::layer_clipboard::KeyframeSource,
        folder: Option<u64>,
        path: Option<std::path::PathBuf>,
    },
    ProjectViewChanged(super::project::ViewPreferences),
    ProjectOpen { source: crate::layer_clipboard::KeyframeSource, composition: String },
    ProjectPlace { source: crate::layer_clipboard::KeyframeSource, target: motion_core::media_assets::AssetRef, at_frame: u32 },
    ProjectPlaceModel { source: crate::layer_clipboard::KeyframeSource, asset: u64, at_frame: u32 },
    MediaAsset { source: crate::media_assets::Source, intent: crate::media_assets::Intent },
    ImportEffectPackage,
    ImportEffectPath(std::path::PathBuf),
    ImportPath(std::path::PathBuf),
    CancelImport,
    LayerClipboard(crate::layer_clipboard::controller::Intent),
    CurveClipboard {
        intent: crate::layer_clipboard::controller::Intent,
        source: crate::layer_clipboard::KeyframeSource,
        selections: Vec<motion_core::KeyframeSelection>,
    },
    KeyframeClipboard {
        intent: crate::layer_clipboard::controller::Intent,
        source: crate::layer_clipboard::KeyframeSource,
        selections: Vec<motion_core::KeyframeSelection>,
        targets: Vec<motion_core::KeyframeTarget>,
    },
    PathClipboard {
        intent: crate::layer_clipboard::controller::Intent,
        source: crate::layer_clipboard::PathSource,
        selections: Vec<motion_core::PathSelection>,
        target: motion_core::PathDestination,
    },
    CancelLayerClipboard,
    PathOperation {
        source: crate::layer_clipboard::PathSource,
        operation: crate::layer_clipboard::PathOperation,
    },
    AnalyzeAudio(u64),
    CancelAudioAnalysis,
    ExportPng,
    ExportMovie,
    RenderQueue(crate::render_queue::controller::Intent),
    CancelExport,
    AddNull,
    AddAdjustment,
    AddPath(motion_core::vector::VectorPath),
    AddShape(motion_core::vector::ShapeKind),
    PlaceShape {
        shape: motion_core::vector::ShapeKind,
        size: [f32; 2],
        position: [f32; 3],
    },
    NewText,
    TextChooseFont { ticket: super::auxiliary::text_style::Ticket },
    TextApply {
        ticket: super::auxiliary::text_style::Ticket,
        source: crate::layer_clipboard::KeyframeSource,
        request: Value,
    },
    CompositionSettings,
    ProjectPrecisionSettings,
    KeyframeTimingSettings {
        source: crate::layer_clipboard::KeyframeSource,
        selections: Vec<motion_core::KeyframeSelection>,
    },
    Preferences,
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
    SamplingColumnVisibility(super::layer_sampling::Visibility),
    HideShyLayers(bool),
    PreviewMode(i32),
    PreviewPlayback(crate::preview_playback::Options),
    MovieExportScope(motion_core::FrameScope),
    /// Switch the composition viewer between camera and observation views.
    ViewKind(i32),
    /// Atomic observer-only gesture, independent of authored camera history.
    CameraGesture(motion_host::ops::preview::CameraGesture),
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
    ToggleAudioPreview,
    Edit(Command),
    EditChecked {
        command: Command,
        revision: u64,
    },
    EditBatch(Vec<Command>),
    EditorAction {
        source: crate::layer_clipboard::KeyframeSource,
        commands: Vec<Command>,
    },
    ExpressionCheck {
        ticket: super::expressions::Ticket,
        target: motion_core::ExpressionTarget,
        text: String,
    },
    ExpressionApply {
        ticket: super::expressions::Ticket,
        source: crate::layer_clipboard::KeyframeSource,
        expression: motion_core::PropertyExpression,
        frame: u32,
    },
    OrderLayers {
        objects: Vec<u64>,
        order: super::layer_navigation::Order,
    },
}

#[derive(Default)]
pub struct Panels {
    pub(crate) effect_pick: super::effect_pick::State,
    pub(crate) shortcut_bindings: std::sync::Arc<crate::shortcuts::bindings::Bindings>,
    pub(crate) property_reveal: super::modified_properties::RevealSequence,
    pub workspace_preset: crate::workspace::Preset,
    pub styles: Option<std::sync::Arc<motion_ui::style::StyleSheet>>,
    pub(crate) cursor: Option<motion_ui::cursor::Cursor>,
    pub(crate) drag_readout: Option<([f32; 2], String)>,
    pub(crate) motion: motion_ui::motion::Motion,
    pub(crate) project: ProjectState,
    pub(crate) media_assets: super::media_assets::Controls,
    pub(crate) properties: PropertiesState,
    pub(crate) timeline: TimelineState,
    pub(crate) layer_actions: super::layer_actions::Controls,
    pub locale: Locale,
    pub(crate) focused: Option<dock::Panel>,
    pub(crate) expressions: super::expressions::Controls,
    pub(crate) auxiliary: super::auxiliary::State,
    pub(crate) preview_playback: crate::preview_playback::Options,
    pub(crate) movie_export_scope: motion_core::FrameScope,
    pub(crate) render_queue: crate::render_queue::controller::Presentation,
    pub(crate) library_scroll: motion_ui::widgets::ScrollArea,
    pub(crate) library_height: f32,
    pub(crate) effect_drag: super::effect_drag::Controls,
    pub(crate) project_drag: super::project_drag::Controls,
    pub(crate) browser: crate::panels::effect_controls::Controls,
    pub(crate) recent_effect_menu: super::effect_recent::Cache,
    pub selected: Option<u64>,
    /// Stable selection collection for timeline multi-select. `selected` is
    /// kept as the primary layer for panels whose controls still need one
    /// active object.
    pub(crate) selected_layers: Vec<u64>,
    pub(crate) selected_keys: super::key_selection::Set,
    pub(crate) key_gestures: super::key_selection::ui::Controls,
    pub(crate) selection_anchor: Option<u64>,
    pub(crate) selection_context: Option<(String, String)>,
    pub playing: bool,
    pub message: Option<String>,
    pub(crate) menu: Option<usize>,
    pub(crate) menu_navigation: super::menus::Navigation,
    pub(crate) animation_menu_scope: Option<super::animation_menu::MenuScope>,
    pub(crate) animation_type_cache: std::cell::RefCell<super::animation_menu::TypeCache>,
    pub(crate) clipboard_available: bool,
    pub(crate) keyframe_clipboard_available: bool,
    pub(crate) path_clipboard_available: bool,
    pub(crate) clipboard_busy: bool,
    /// Eligibility captured before opening application chrome clears focus.
    pub(crate) clipboard_menu_allowed: Option<bool>,
    pub(crate) clipboard_menu_keys: Option<super::keyframe_clipboard::Selection>,
    pub(crate) clipboard_menu_paths: Option<super::path_clipboard::Selection>,
    pub(crate) text_menu: Option<super::text_menu::Capture>,
    /// First visible row in the application menu when the popup is taller than
    /// the available window height.
    pub(crate) menu_scroll: usize,
    pub(crate) panel_menu: Option<(dock::DockId, [f32; 2])>,
    pub(crate) header_hits: Vec<HeaderHit>,
    pub(crate) header_size: Option<[f32; 2]>,
    pub effect_search: Option<EffectSearch>,
    pub effect_controls: crate::panels::effect_controls::Controls,
    pub(crate) effect_timeline: super::effect_timeline::Controls,
    pub timeline_anchor: super::properties::AnchorControls,
    pub animation_controls: crate::panels::animation_controls::Controls,
    pub parent_controls: crate::panels::parent_controls::Controls,
    pub timeline_columns: crate::panels::timeline_columns::Controls,
    pub camera_controls: crate::panels::camera_controls::Controls,
    pub(crate) audio_controls: crate::panels::audio::Controls,
    pub node_editor: crate::node_editor::NodeEditor,
    pub node_preview: crate::node_preview::Controls,
    pub(crate) tools: super::tools::State,
    pub path_controls: crate::panels::path_controls::Controls,
    pub(crate) toolbar_style: super::toolbar_style::Controls,
    pub transform_controls: crate::panels::transform_controls::Controls,
    pub(crate) composition: CompositionState,
    pub(crate) viewer_guides: super::viewer_guides::Controls,
    pub(crate) viewer_inspection: super::viewer_inspection::Controls,
}

#[derive(Default)]
pub(crate) struct CompositionState {
    pub navigation: crate::viewer_navigation::Navigation,
    pub pan: Option<([f32; 2], crate::viewer_navigation::Navigation)>,
    pub zoom: motion_ui::widgets::NumberEditor,
    pub zoom_before: Option<crate::viewer_navigation::Navigation>,
    pub context: Option<u64>,
    pub mapping_failures: u8,
}

impl Panels {
    pub(crate) fn select_only(&mut self, object: Option<u64>) {
        self.selected_keys.release();
        self.property_reveal.interrupt();
        self.path_controls.clear_mask_selection();
        self.audio_controls.clear_selection();
        self.selected = object;
        self.selection_anchor = object.filter(|id| *id != 0);
        self.animation_controls.clear_track();
        self.effect_controls.clear_key_channel();
        self.properties.effects.clear_key_channel();
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
        self.selected_keys.release();
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
        self.selected_keys.release();
        if object != 0 && !self.selected_layers.contains(&object) {
            self.selected_layers.push(object);
        }
        self.selected = Some(object);
    }

    pub(crate) fn normalize_selection(&mut self, state: &serde_json::Value) -> bool {
        let mut context_changed = false;
        let root = state["root"].as_str().unwrap_or("");
        if let Some(composition) = state["composition"].as_str()
            .or_else(|| state["project"]["composition_id"].as_str()) {
            if self.selection_context.as_ref().is_some_and(|old|
                old.0 != root || old.1 != composition) {
                context_changed = true;
                // Automatic context changes invalidate authored key identities;
                // only an explicit layer choice may relinquish their ownership.
                let keys = std::mem::take(&mut self.selected_keys);
                self.select_only(None);
                self.selected_keys = keys;
            }
            if self.selection_context.as_ref().is_none_or(|old|
                old.0 != root || old.1 != composition) {
                self.selection_context = Some((root.to_owned(), composition.to_owned()));
            }
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
        if context_changed || invalid {
            self.timeline.reveal_layer = None;
        }
        let hidden_changed = super::shy_layers::reconcile_selection(self, state);
        context_changed || invalid || hidden_changed
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
#[derive(Clone, Debug, Default)]
pub struct EffectSearch {
    pub position: [f32; 2],
    pub query: String,
    pub selected_index: usize,
    pub owner: Option<dock::Panel>,
    /// Explicit opt-in; a newly opened search always applies once and closes.
    pub continuous: bool,
    pub(crate) cache: super::effect_search::Cache,
    pub(crate) query_editor: motion_ui::widgets::TextEditor,
}

pub(crate) struct Field {
    pub object: u64,
    pub property: Property,
    pub axis: usize,
    pub text: String,
    pub replace: bool,
    pub editor: motion_ui::widgets::TextEditor,
    pub rect: Rect,
    pub scrubbing: Option<ScrubState>,
}

pub(crate) struct ScrubState {
    pub start_mouse: f32,
    pub start_value: f32,
    pub last_value: f32,
    pub begun: bool,
}

#[derive(Clone, Copy)]
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
    pub browser: super::project::Controls,
    pub thumbnails: super::project::thumbnails::Thumbnails,
    pub preview: crate::project_preview::Player,
    pub search_field: motion_ui::widgets::SearchField,
    pub query: String,
    pub search: bool,
    pub context_menu: Option<[f32; 2]>,
    pub context_owner: Option<motion_ui::dock::Panel>,
}

pub(crate) struct PropertiesState {
    pub consumed_press: bool,
    pub transform_collapsed: bool,
    pub effects_collapsed: bool,
    pub effects: super::effect_controls::Controls,
    pub anchor: super::properties::AnchorControls,
    pub properties_scroll: f32,
    pub content_height: f32,
    pub field: Option<Field>,
}

impl Default for PropertiesState {
    fn default() -> Self {
        Self {
            consumed_press: false,
            transform_collapsed: false,
            effects_collapsed: true,
            effects: Default::default(),
            anchor: Default::default(),
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
    pub(crate) hide_shy: bool,
    pub(crate) reveal_layer: Option<u64>,
    pub sampling_visibility: super::layer_sampling::Visibility,
    pub(crate) column_visibility: ColumnVisibility,
    pub(crate) graph_box: super::graph_box::Controls,
    pub rows_cache: Option<(
        (String, String, u64),
        std::sync::Arc<Vec<super::timeline::Row>>,
    )>,
    pub expanded: std::collections::HashSet<u64>,
    pub collapsed_transforms: std::collections::HashSet<u64>,
    pub expanded_effects: std::collections::HashSet<u64>,
    pub collapsed_effect_items: std::collections::HashSet<(u64, u64)>,
    pub collapsed_audio: std::collections::HashSet<u64>,
    pub vector_open: std::collections::HashSet<(u64, u64, u8)>,
    pub collapsed_masks: std::collections::HashSet<u64>,
    pub collapsed_mask_items: std::collections::HashSet<(u64, u64)>,
    pub filters: std::collections::HashMap<u64, PropertyFilter>,
    pub graph_mode: bool,
    pub(super) graph_ranges: super::graph_ranges::Controls,
    pub(super) comparison: super::curve_comparison::Controls,
    pub(super) clip_view: Option<TimelineViewport>,
    pub(super) curve_view: Option<TimelineViewport>,
    pub speed_graph: bool,
    pub graph_component: usize,
    pub graph_fitted: bool,
    pub graph_grid: bool,
    pub(crate) graph_drag: Option<super::graph_handles::Drag>,
    pub(crate) key_drag: Option<super::graph_keys::Drag>,
    pub(crate) timing_drag: Option<super::timeline_key_drag::Drag>,
    pub(crate) work_area_drag: Option<super::work_area::Drag>,
    #[cfg(test)]
    pub(crate) work_area_band: Option<Rect>,
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
    pub three_d_probe: super::property_focus::HitProbe,
    pub playhead_view: Option<(Rect, f32, f32)>,
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
    pub(crate) timeline_search_editor: motion_ui::widgets::TextEditor,
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
    Modified,
}
impl TimelineState {
    pub fn set_graph_mode(&mut self, enabled: bool) {
        if self.graph_mode == enabled {
            return;
        }
        if self.graph_drag.as_ref().is_some_and(|d| d.begun())
            || self.key_drag.as_ref().is_some_and(|d| d.begun())
              || self.timing_drag.as_ref().is_some_and(|d| d.begun())
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
        Property::Anchor => 16,
        Property::Position => 1,
        Property::Scale => 2,
        Property::Rotation => 4,
        Property::Opacity => 8,
        _ => 0,
    }
}
impl PropertyFilter {
    pub fn admits(self, property: Property, animated: bool, modified: bool) -> bool {
        match self {
            Self::All => true,
            Self::Properties(mask) => mask & property_bit(property) != 0,
            Self::Animated => animated,
            Self::Modified => modified,
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
