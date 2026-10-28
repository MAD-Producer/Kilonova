//! Desktop panels: one fixed workspace, logical-pixel geometry and typed actions.
use crate::i18n::{Locale, Text as T};
use motion_core::{Command, Property};
use motion_ui::{
    dock::{self, Node, Panel},
    input::{Input, Key, MouseButton, Rect},
    paint::{self, PaintList},
    text::TextAtlas,
    theme::metrics,
    ui::{self, Align, Icon},
};
use serde_json::Value;

// Each expansion owns one process-lifetime site counter (see observation).
// Context is formatted only when the bounded counter emits a record.
macro_rules! observe {
    ($site:literal, $result:expr, $($context:tt)*) => {{
        static COUNTER: $crate::panels::observation::Counter = $crate::panels::observation::Counter::new();
        match $result {
            Ok(value) => Ok(value),
            Err(error) => {
                COUNTER.failure($site, format_args!("{} reason={error}", format_args!($($context)*)));
                Err(error)
            }
        }
    }};
}

mod align;
mod frame_cache;
#[cfg(test)]
mod frame_cache_tests;
pub(crate) mod animation_controls;
mod animation_data;
mod audio;
mod work_area;
pub(crate) mod auxiliary;
pub(crate) mod camera_controls;
mod color_well;
mod toolbar_style;
mod composition;
mod viewer_guides;
mod curves;
pub(crate) mod effect_controls;
mod effect_removal;
mod effect_recent;
#[cfg(test)]
mod recent_effect_menu_tests;
pub(crate) mod effect_drag;
pub(crate) mod project_drag;
mod effects;
mod effect_timeline;
pub(crate) mod expressions;
pub(crate) mod effect_search;
mod graph_box;
pub(crate) mod graph_handles;
pub(crate) mod graph_keys;
mod graph_sampling;
mod curve_comparison;
mod graph_notice;
mod graph_workspace;
mod helpers;
mod keyboard;
mod modified_properties;
mod key_navigation;
mod layer_actions;
mod animation_menu;
mod layer_locks;
pub(crate) mod layer_navigation;
#[cfg(test)]
mod layer_switches;
mod menus;
mod clipboard_menu;
pub(crate) mod text_menu;
mod keyframe_clipboard;
mod key_selection;
mod path_clipboard;
pub(crate) mod numeric;
mod observation;
#[cfg(test)]
mod observation_tests;
pub(crate) mod parent_controls;
pub(crate) mod layer_sampling;
pub(crate) mod playback_switches;
pub(crate) mod path_controls;
mod paths;
pub(crate) mod project;
mod media_assets;
mod properties;
mod property_focus;
#[cfg(test)]
mod property_followup_tests;
#[cfg(test)]
mod timeline_3d_tests;
#[cfg(test)]
mod selection_tests;
mod timeline;
mod timeline_key_drag;
#[cfg(test)]
mod timeline_key_drag_tests;
mod timeline_columns;
mod shy_layers;
#[cfg(test)]
mod timeline_columns_tests;
mod timeline_graph;
mod timeline_layout;
mod timeline_shortcuts;
#[cfg(test)]
mod timeline_shortcuts_tests;
mod timeline_zoom;
mod timeline_navigation;
pub(crate) mod titlebar;
mod toolbar;
pub(crate) mod tools;
pub(crate) mod transform_controls;
mod types;

use composition::render as composition;
pub use helpers::timecode;
use helpers::{button, clicked, label, last_frame, right_clicked};
use menus::render as menus;
use project::{finish_context_menu as finish_project_context_menu, render as project};
use properties::render as properties;
use timeline::render as timeline;
#[cfg(test)]
use timeline::rows;
pub use types::{Action, Frame, EffectSearch, Panels};
pub(crate) use keyboard::layout_shortcuts_blocked;

pub(crate) fn expression_overlay_open(panels: &Panels) -> bool {
    [&panels.expressions, &panels.effect_controls.expressions, &panels.properties.effects.expressions]
        .iter().any(|controls| controls.menu_open())
}
pub(crate) use types::ColumnVisibility;
use types::{Field, ScrubState};

#[cfg(test)]
pub(crate) fn seed_curve_comparison(panels:&mut Panels,state:&Value) {
    curve_comparison::seed(panels,state);
}
#[cfg(test)]
pub(crate) fn comparison_test_anchor(panels:&Panels,index:usize)->Rect {
    panels.timeline.comparison.test_anchor(index)
}

pub const CHROME_HEIGHT: f32 = metrics::TITLE_BAR + metrics::MENU_BAR + metrics::TOOL_BAR;
const GUTTER: f32 = 640.0;

thread_local! {
    static FRAME_CACHE: std::cell::RefCell<frame_cache::Cache<Frame>> =
        const { std::cell::RefCell::new(frame_cache::Cache::new()) };
}

pub fn frames(layout: &Node, size: [f32; 2]) -> Vec<Frame> {
    if let Some(hit) = FRAME_CACHE
        .try_with(|slot| slot.try_borrow().ok().and_then(|cache| cache.get(layout, size)))
        .ok()
        .flatten()
    {
        return hit;
    }
    let result = calculate_frames(layout, size);
    // Unavailable slots and oversized/nonfinite keys keep the uncached result.
    let _ = FRAME_CACHE.try_with(|slot| {
        if let Ok(mut cache) = slot.try_borrow_mut() {
            cache.store(layout, size, &result);
        }
    });
    result
}

fn calculate_frames(layout: &Node, size: [f32; 2]) -> Vec<Frame> {
    dock::solve(
        layout,
        Rect::new(
            0.0,
            CHROME_HEIGHT,
            size[0],
            (size[1] - CHROME_HEIGHT - 20.0).max(0.0),
        ),
    )
    .into_iter()
    .filter_map(|p| {
        let panel = panel(p.id)?;
        Some(Frame {
            panel,
            tabs: Rect::new(
                p.area.min[0],
                p.area.min[1],
                p.area.width(),
                metrics::TAB_BAR,
            ),
            content: Rect::new(
                p.area.min[0],
                p.area.min[1] + metrics::TAB_BAR,
                p.area.width(),
                (p.area.height() - metrics::TAB_BAR).max(0.0),
            ),
        })
    })
    .collect()
}
pub fn composition_view(layout: &Node, size: [f32; 2]) -> Rect {
    frames(layout, size)
        .into_iter()
        .find(|f| f.panel == Panel::Composition)
        .map(|f| viewer(f.content))
        .unwrap_or_default()
}
fn viewer(area: Rect) -> Rect {
    // Compatibility geometry for existing callers/tests using the default theme.
    Rect::new(area.min[0]+8., area.min[1]+8., (area.width()-16.).max(1.), (area.height()-42.).max(1.))
}
fn styled_viewer(area: Rect, styles: &motion_ui::style::StyleSheet) -> Rect {
    let padding = styles.metric("padding.panel", 8.);
    let bottom = styles.metric("viewer.canvas.bottom", 6.);
    let controls = styles.metric("viewer.controls.height", 28.);
    Rect::new(area.min[0]+padding, area.min[1]+padding,
        (area.width()-padding*2.).max(0.), (area.height()-padding-bottom-controls).max(0.))
}
pub fn composition_view_styled(layout: &Node, size: [f32; 2], styles: &motion_ui::style::StyleSheet) -> Rect {
    frames(layout, size).into_iter().find(|f| f.panel == Panel::Composition)
        .map(|f| styled_viewer(f.content, styles)).unwrap_or_default()
}
pub fn floating_view_styled(panel: Panel, size: [f32; 2], styles: &motion_ui::style::StyleSheet) -> Option<Rect> {
    let top = styles.metric("viewer.floating.top",30.);
    let bottom = styles.metric("viewer.floating.bottom",22.);
    (panel == Panel::Composition).then(|| styled_viewer(Rect::new(0.,top,size[0],(size[1]-top-bottom).max(0.)),styles))
        .filter(|view| view.width() > 0. && view.height() > 0.)
}
/// Evidence before visibility/routing changes; no gesture is cancelled here.
pub(crate) fn observe_interaction_boundary(panels: &Panels, operation: &str) {
    let graph = panels
        .timeline
        .graph_drag
        .as_ref()
        .is_some_and(|d| d.begun())
        || panels.timeline.key_drag.as_ref().is_some_and(|d| d.begun())
        || panels.timeline.timing_drag.as_ref().is_some_and(|d| d.begun());
    let graph = graph || panels.effect_timeline.gesture_active();
    let path = panels.path_controls.gesture_active();
    let curve = panels.effect_controls.curve_gesture_active()
        || panels.properties.effects.curve_gesture_active();
    let moving = panels.timeline.moving.is_some();
    let trimming = panels.timeline.trimming.is_some();
    if graph || path || curve || moving || trimming {
        log::warn!("panel routing boundary with active gesture operation={operation} selected={:?} graph={graph} path={path} curve={curve} moving={moving} trimming={trimming}", panels.selected);
    }
}

pub fn cancel_interaction(panels: &mut Panels) -> Vec<Action> {
    panels.key_gestures.cancel();
    panels.tools.cancel();
    panels.effect_drag.cancel();
    panels.project_drag.cancel();
    panels.browser.cancel_library_click();
    panels.property_reveal.interrupt();
    let effect_timeline = panels.effect_timeline.cancel();
    let node_changed = panels.node_editor.cancel_interaction();
    panels.layer_actions.cancel();
    panels.expressions.cancel();
    panels.timeline.timeline_context_menu = None;
    panels.parent_controls.cancel();
    panels.timeline_columns.cancel();
    panels.viewer_guides.cancel();
    let camera = panels.camera_controls.cancel();
    let audio = panels.audio_controls.cancel();
    let graph_box = panels.timeline.graph_box.cancel();
    let effect = effect_timeline | panels.effect_controls.cancel()
        | panels.browser.cancel()
        | panels.properties.effects.cancel()
        | panels.properties.anchor.cancel()
        | panels.timeline_anchor.cancel();
    let auxiliary = panels.auxiliary.cancel();
    let numeric = panels
        .properties
        .field
        .take()
        .is_some_and(|field| field.scrubbing.is_some_and(|s| s.begun));
    panels.timeline.scrubbing = false;
    panels.timeline.work_area_drag = None;
    panels.timeline.horizontal_drag = None;
    panels.timeline.edge_pan_at = None;
    let moving = panels.timeline.moving.take().is_some();
    let trimming = panels.timeline.trimming.take().is_some();
    panels.timeline.reorder = None;
    let animation = panels.animation_controls.cancel();
    let graph = panels.timeline.graph_drag.take().is_some_and(|d| d.begun())
        | panels.timeline.key_drag.take().is_some_and(|d| d.begun())
        | panels.timeline.timing_drag.take().is_some_and(|d| d.begun());
    let path = panels.path_controls.cancel();
    let toolbar = panels.toolbar_style.cancel();
    let transform = panels.transform_controls.cancel();
    if let Some((_, before)) = panels.composition.pan.take() {
        panels.composition.navigation = before;
    }
    panels.composition.zoom.cancel();
    if let Some(before) = panels.composition.zoom_before.take() {
        panels.composition.navigation = before;
    }
    let mut actions = if moving
        || trimming
        || numeric
        || animation
        || graph
        || graph_box
        || path
        || transform
        || effect
        || camera
        || audio
        || auxiliary
        || toolbar
    {
        vec![Action::History(4)]
    } else {
        vec![]
    };
    tools::camera::flush(panels, &mut actions);
    if node_changed {
        actions.push(Action::Node(crate::node_editor::Action::PreviewChanged));
    }
    actions
}

pub(crate) fn reveal_mask_keys(
    panels: &mut Panels,
    state: &Value,
    object: u64,
    mask: u64,
) -> Vec<Action> {
    let actions = cancel_interaction(panels);
    path_controls::timeline::reveal(panels, state, object, mask);
    actions
}
pub(crate) fn reveal_shape_keys(panels: &mut Panels, state: &Value, object: u64) -> Vec<Action> {
    let actions = cancel_interaction(panels);
    panels.select_only(Some(object));
    panels.timeline.expanded.insert(object);
    panels
        .timeline
        .filters
        .insert(object, types::PropertyFilter::Animated);
    path_controls::groups::timeline::reveal_animated(panels, state, object);
    panels.focused = Some(Panel::Timeline);
    panels.timeline.scroll = 0.;
    actions
}

/// The chrome leaves a hole for the preview; its geometry is also used for hit testing.
pub fn chrome(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    size: [f32; 2],
    layout: &Node,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
) -> Vec<Action> {
    if let Some(styles) = &panels.styles {
        list.stylesheet = Some(styles.clone());
    }
    let styles = list.styles();
    let colors = styles.colors();

    let effect_context_input = input;
    let effect_context_owned = panels.browser.prepare_context_window(None, input)
        | panels.effect_controls.prepare_context_window(None, input)
        | panels.properties.effects.prepare_context_window(None, input);
    let effect_context_neutral = Input::default();
    let input = if effect_context_owned { &effect_context_neutral } else { input };
    expressions::picking::prepare(panels, None);
    let project_input = input;
    let project_owned = project::prepare(panels, None, state, input);
    let project_neutral = Input::default();
    let input = if project_owned { &project_neutral } else { input };
    let asset_input = input;
    let asset_owned = panels.media_assets.prepare(None, state, input);
    let asset_neutral = Input::default();
    let input = if asset_owned { &asset_neutral } else { input };
    let guide_input = input;
    let guide_owned = panels.viewer_guides.prepare(None, state, input);
    let guide_neutral = Input::default();
    let input = if guide_owned { &guide_neutral } else { input };
    tools::prepare(panels,None,input);
    let parent_pick_owned = panels.parent_controls.prepare_window(None, input);
    let parent_neutral = Input::default();
    let input = if parent_pick_owned { &parent_neutral } else { input };
    let project_drag_owned = panels.project_drag.prepare(None, state, input);
    let drag_owned = panels.effect_drag.prepare(None, state, input) || project_drag_owned;
    if drag_owned && !panels.effect_drag.pending() { panels.browser.cancel_library_click(); }
    let drag_neutral = Input::default();
    let input = if drag_owned { &drag_neutral } else { input };
    let mut out = Vec::new();
    tools::camera::flush(panels, &mut out);
    if panels.normalize_selection(state) {
        out.extend(cancel_interaction(panels));
    }
    if input.focus_lost {
        panels.menu = None;
        panels.menu_scroll = 0;
        panels.panel_menu = None;
    }
    panels.cursor = None;
    panels.drag_readout = None;
    let text_menu_owned = text_menu::prepare(panels, state, input, atlas);
    let path_context_owned = panels.path_controls.prepare_context_menu(None, state, panels.selected, input);
    let text_menu_neutral = Input::default();
    let editing_input = if text_menu_owned || path_context_owned { &text_menu_neutral } else { input };
    let toolbar_was_editing = panels.toolbar_style.editing();
    panels.toolbar_style.prepare(state, &panels.selection_ids(),
        panels.path_controls.tool == path_controls::Tool::Mask, editing_input, &mut out);
    let toolbar_modal = toolbar_was_editing || panels.toolbar_style.editing();
    let layer_modal = panels.layer_actions.prepare(
        Some(Panel::Timeline),
        state,
        editing_input,
        panels.locale,
        panels.selected,
        panels.focused == Some(Panel::Timeline)
            && !panels.timeline_columns.editing()
            && !panels.timeline.comparison.keyboard_focus()
            && !panels.parent_controls.open
            && panels.properties.field.is_none()
            && !panels.audio_controls.editing()
            && !panels.path_controls.editing()
            && !panels.timeline.timeline_search
            && !panels.expressions.editing()
            && !expressions::picking::active(panels, None),
        &mut out,
    );
    panels.node_editor.external_inspector = layout.find(Panel::NodeProperties.dock()).is_some();
    let node_modal = panels.node_editor.popup_open();
    let timeline_dropdown = panels.timeline_columns.editing() || panels.timeline.comparison.popup_open();
    let mask_header_modal = panels.path_controls.timeline_mask_add_open();
    if panels.menu.is_none() && input.pressed.is_some_and(|(_, point)|
        point[1] >= metrics::TITLE_BAR && point[1] < metrics::TITLE_BAR + metrics::MENU_BAR) {
        panels.clipboard_menu_keys = keyframe_clipboard::selection(panels, state);
        panels.clipboard_menu_paths = path_clipboard::selection(panels, state);
        panels.clipboard_menu_allowed = Some(panels.clipboard_menu_paths.is_some() && keyboard::path_clipboard_allowed(panels, state)
            || !keyboard::layout_shortcuts_blocked(panels)
            && state["observing"] != true
            && (panels.clipboard_menu_keys.is_some() || keyboard::layer_clipboard_allowed(panels, state)));
    }
    if let Some((_, point)) = input.pressed.filter(|(_, point)| {
        !panels.effect_search_in(None)
            && !node_modal
            && !timeline_dropdown
            && !panels.parent_controls.open
            && !mask_header_modal
            && !layer_modal
            && !path_context_owned
            && !panels.expressions.menu_open()
            && !expressions::picking::active(panels, None)
            && !toolbar_modal
            && panels.menu.is_none()
            && !(point[1] >= metrics::TITLE_BAR && point[1] < metrics::TITLE_BAR + metrics::MENU_BAR)
    }) {
        let focused = frames(layout, size)
            .into_iter()
            .find(|f| f.content.contains(point) || f.tabs.contains(point))
            .map(|f| f.panel);
        if focused != panels.focused {
            let node_region = |p: Option<Panel>| {
                matches!(
                    p,
                    Some(Panel::Nodes | Panel::NodeProperties | Panel::NodePreview)
                )
            };
            if !(node_region(focused) && node_region(panels.focused)) {
                out.extend(cancel_interaction(panels));
            }
            panels.project.search = false;
            panels.project.search_field.editor.cancel();
            panels.timeline.timeline_search = false;
            panels.focused = focused;
        }
    }
    if let Some(id) = panels.selected {
        if id != 0
            && !state["project"]["layers"]
                .as_array()
                .is_some_and(|ls| ls.iter().any(|l| l["id"].as_u64() == Some(id)))
        {
            let keys = std::mem::take(&mut panels.selected_keys);
            panels.select_only(None);
            panels.selected_keys = keys;
            out.extend(cancel_interaction(panels));
        }
    }
    if panels
        .properties
        .field
        .as_ref()
        .is_some_and(|field| Some(field.object) != panels.selected)
    {
        out.extend(cancel_interaction(panels));
    }
    if !panels.effect_search_in(None)
        && !(panels.effect_search.is_some() && keyboard::effect_search_close_pressed(input))
        && !node_modal
        && matches!(
            panels.focused,
            Some(
                Panel::Masks | Panel::Properties | Panel::Vector | Panel::Timeline | Panel::Curves
            )
        )
        && panels.path_controls.numeric_owner() == panels.focused
    {
        panels.path_controls.parameter_input(
            state,
            editing_input,
            &mut out,
            styles.metric("motion.drag_threshold", 3.),
        );
    }
    if panels.effect_search.is_none()
        && input
            .key_presses()
            .any(|(k, _, repeat)| k == Key::Escape && !repeat)
    {
        if !text_menu_owned && !panels.expressions.menu_open() && !expressions::picking::active(panels, None) { out.extend(cancel_interaction(panels)); }
        panels.menu = None;
        panels.panel_menu = None;
        if !text_menu_owned {
            panels.project.search = false;
            panels.timeline.timeline_search = false;
        }
        panels.effect_search = None;
        panels.timeline.timeline_context_menu = None;
        panels.project.context_menu = None;
    }
    let nodes_frame = frames(layout, size)
        .into_iter()
        .find(|f| f.panel == Panel::Nodes);
    let node_keyboard = matches!(
        panels.focused,
        Some(Panel::Nodes | Panel::NodeProperties | Panel::NodePreview)
    ) || panels.focused.is_none()
        && nodes_frame.as_ref().is_some_and(|f| {
            if let Some((_, point)) = input.pressed {
                f.content.contains(point)
            } else {
                panels.node_editor.focused()
            }
        })
        && panels.menu.is_none();
    let expression_modal = panels.expressions.menu_open() || expressions::picking::active(panels, None);
    let shortcut_modal =
        expression_modal || keyboard::route(if path_context_owned { &text_menu_neutral } else { input }, panels, state, node_keyboard, None, &mut out);
    let menu_modal = path_context_owned || text_menu_owned || layer_modal
        || toolbar_modal
        || panels.layer_actions.modal_in(Some(Panel::Timeline))
        || shortcut_modal
        || panels.menu.is_some()
        || panels.panel_menu.is_some()
        || panels.effect_search_in(None)
        || panels.timeline.timeline_context_menu.is_some()
        || timeline_dropdown
        || panels.parent_controls.open
        || panels.project.context_menu.is_some()
        || panels.timeline.timeline_context_menu.is_some();
    let effect_owner = if panels.browser.editing() {
        Panel::Effects
    } else if panels.effect_timeline.editing() {
        Panel::Timeline
    } else if panels.properties.effects.editing() {
        Panel::Properties
    } else {
        Panel::EffectControls
    };
    let effect_modal = panels.effect_controls.editing()
        || panels.browser.editing()
        || panels.properties.effects.editing()
        || panels.effect_timeline.editing();
    let audio_modal = panels.audio_controls.editing();
    let auxiliary_owner = panels.auxiliary.owner();
    let auxiliary_modal = auxiliary_owner.is_some();
    let neutral = Input::default();
    let body_input = if menu_modal
        || node_modal
        || effect_modal
        || audio_modal
        || auxiliary_modal
        || mask_header_modal
        || toolbar_modal
    {
        &neutral
    } else {
        input
    };
    panels.timeline_columns.prepare_window(None, input);
    let mut opened_panel_menu = false;
    panels.header_hits.clear();
    panels.header_size = Some(size);
    for f in frames(layout, size) {
        let title = panels.locale.panel(f.panel.dock());
        let (dock, tabs, active) = match layout.find(f.panel.dock()) {
            Some(Node::Dock { id, tabs, active }) => (*id, tabs.clone(), *active),
            _ => (f.panel.dock(), vec![f.panel.dock()], 0),
        };
        let titles: Vec<_> = tabs.iter().map(|id| panels.locale.panel(*id)).collect();
        let layer = state["project"]["layers"]
            .as_array()
            .and_then(|ls| ls.iter().find(|l| l["id"].as_u64() == panels.selected));
        let context = layer
            .and_then(|l| l["name"].as_str())
            .map(|name| format!("<{name}>"));
        let specs: Vec<_> = tabs
            .iter()
            .enumerate()
            .map(|(i, id)| {
                let inspector = matches!(id.0, 2 | 11);
                motion_ui::tabs::Tab {
                    title: titles[i],
                    context: if inspector { context.as_deref() } else { None },
                    badge: (inspector && layer.is_some()).then_some(colors.TRACKS[0]),
                    locked: inspector && layer.is_some_and(|l| l["locked"].as_bool() == Some(true)),
                    closable: tabs.len() > 1 && id.0 != 1,
                }
            })
            .collect();
        let tab_area = Rect::new(
            f.tabs.min[0],
            f.tabs.min[1],
            f.tabs.width(),
            f.tabs.height(),
        );
        for (index, region) in motion_ui::tabs::layouts(atlas, tab_area, &specs, active)
            .into_iter()
            .enumerate()
        {
            panels.header_hits.push(types::HeaderHit {
                rect: region.area,
                panel: tabs[index],
                action: false,
            });
            for rect in [region.close, region.menu].into_iter().flatten() {
                panels.header_hits.push(types::HeaderHit {
                    rect,
                    panel: tabs[index],
                    action: true,
                });
            }
        }
        if let Some(event) = motion_ui::tabs::draw(
            list,
            atlas,
            tab_area,
            &specs,
            active,
            body_input,
            tabs.len() > 1 || panels.focused == Some(f.panel),
        ) {
            match event {
                motion_ui::tabs::Event::Activate(index) => out.push(Action::Activate {
                    dock,
                    tab: tabs[index],
                }),
                motion_ui::tabs::Event::Close(index) => out.push(Action::ClosePanel(tabs[index])),
                motion_ui::tabs::Event::Menu(index) => {
                    panels.panel_menu = Some((tabs[index], [input.mouse[0], f.tabs.max[1]]));
                    opened_panel_menu = true;
                }
            }
        }
        if right_clicked(body_input, f.tabs) {
            panels.panel_menu = Some((f.panel.dock(), [input.mouse[0], f.tabs.max[1]]));
            opened_panel_menu = true;
        }
        let _ = title;
        let mut body = list.child();
        if !matches!(
            f.panel,
            Panel::Composition | Panel::Nodes | Panel::NodePreview
        ) {
            let style = styles
                .as_ref()
                .resolve("panel", motion_ui::style::States::default())
                .unwrap();
            paint::rect(&mut body, f.content, style.background);
        }
        let timeline_popup = panels.timeline.timeline_context_menu.is_some() || timeline_dropdown;
        if f.content.height() >= metrics::ROW {
            render_panel(
                f.panel,
                &mut body,
                atlas,
                f.content,
                panels,
                state,
                if (node_modal && f.panel == Panel::Nodes && !menu_modal)
                    || (effect_modal && f.panel == effect_owner && !menu_modal && !node_modal)
                    || (audio_modal && f.panel == Panel::Timeline && !menu_modal && !node_modal)
                    || (mask_header_modal
                        && f.panel == Panel::Timeline
                        && !menu_modal
                        && !node_modal)
                    || (timeline_popup && f.panel == Panel::Timeline)
                    || (auxiliary_modal && Some(f.panel) == auxiliary_owner && !menu_modal)
                {
                    input
                } else {
                    body_input
                },
                &mut out,
            );
        }
        body.clip_to(f.content);
        list.append(body);
        if panels.focused == Some(f.panel) {
            paint::stroke(
                list,
                Rect::new(
                    f.tabs.min[0],
                    f.tabs.min[1],
                    f.tabs.width(),
                    f.tabs.height() + f.content.height(),
                ),
                colors.ACCENT,
                1.,
            );
        }
    }
    let layout_area = Rect::new(
        0.,
        CHROME_HEIGHT,
        size[0],
        (size[1] - CHROME_HEIGHT - 22.).max(0.),
    );
    let splitter_handles = dock::splitters(layout, layout_area);
    for handle in &splitter_handles {
        if handle.area.contains(input.mouse) && !menu_modal {
            panels.cursor = Some(if handle.edge.is_horizontal() {
                motion_ui::cursor::Cursor::ResizeEW
            } else {
                motion_ui::cursor::Cursor::ResizeNS
            });
        }
        paint::rect(list, handle.area, styles.color("dock"));
    }
    titlebar::render(list, atlas, size, state, body_input, &mut out);
    toolbar::render(list, atlas, size, panels, state,
        if toolbar_modal && !text_menu_owned { input } else { body_input }, &mut out);
    effect_drag::finish(list, atlas, size, panels, state, &mut out);
    project_drag::finish(list, atlas, size, panels, state, &mut out);
    let status = Rect::new(0.0, size[1] - 20.0, size[0], 20.0);
    paint::rect(list, status, colors.CHROME);
    let render_error = state["renderError"].as_str().filter(|s| !s.is_empty());
    let localized_error = render_error.map(|e| panels.locale.error(e));
    let plugin_warning = (state["preview"]["pluginEditorRecovery"]["gesture"] == true).then_some(
        if panels.locale == Locale::Zh {
            "插件编辑尚未提交，可在编辑菜单中取消并恢复操作"
        } else {
            "Plugin edit is pending; cancel it from Edit to restore editing"
        },
    );
    let message = localized_error
        .as_deref()
        .or(panels.drag_readout.as_ref().map(|(_, text)| text.as_str()))
        .or(panels.effect_drag.status())
        .or(panels.message.as_deref())
        .or(plugin_warning)
        .unwrap_or(panels.locale.t(T::Ready));
    if render_error.is_some() {
        let style = styles
            .as_ref()
            .resolve("error", motion_ui::style::States::default())
            .unwrap();
        motion_ui::style::text_style(
            list,
            atlas,
            Rect::new(8., status.min[1], status.width() - 16., status.height()),
            message,
            style,
            Align::Left,
        );
    } else {
        label(
            list,
            atlas,
            Rect::new(
                8.0,
                status.min[1],
                (status.width() - 16.0).max(0.0),
                status.height(),
            ),
            message,
            render_error.is_none(),
        );
    }
    if let Some((point, note)) = &panels.drag_readout {
        let width = (atlas.measure(note) + 16.).min(size[0] - 16.).max(1.);
        let rect = Rect::new(
            (point[0] + 14.).min(size[0] - width - 4.).max(4.),
            (point[1] + 20.).min(size[1] - 52.).max(4.),
            width,
            24.,
        );
        let style = styles
            .as_ref()
            .resolve("tooltip", motion_ui::style::States::default())
            .unwrap();
        motion_ui::style::box_style(list, rect, style);
        motion_ui::style::text_style(list, atlas, rect.shrink(4.), note, style, Align::Left);
    }
    if frames(layout,size).iter().any(|f|f.panel==Panel::Timeline) {
        curve_comparison::overlay(list,atlas,layout_area,panels,state,
            if panels.timeline.comparison.popup_open(){input}else{body_input},&mut out);
    }
    // Render the application Edit menu once, above the property-owned overlay.
    // Its existing glyph occlusion must run after the editor's text is drawn.
    let defer_expression_text_menu = expression_modal && text_menu_owned;
    if !defer_expression_text_menu {
    menus(
        list,
        atlas,
        size,
        panels,
        if path_context_owned || !text_menu_owned && (shortcut_modal || node_modal || toolbar_modal) {
            &neutral
        } else {
            input
        },
        state,
        &mut out,
    );
    }
    if let Some((panel, position)) = panels.panel_menu {
        let group = match layout.find(panel) {
            Some(Node::Dock { tabs, .. }) => tabs.clone(),
            _ => vec![panel],
        };
        let entries = [
            if panels.locale == Locale::Zh {
                "关闭面板"
            } else {
                "Close Panel"
            },
            if panels.locale == Locale::Zh {
                "浮动面板"
            } else {
                "Float Panel"
            },
            if panels.locale == Locale::Zh {
                "关闭组中的其他面板"
            } else {
                "Close Other Tabs"
            },
            if panels.locale == Locale::Zh {
                "恢复默认工作区"
            } else {
                "Reset Workspace"
            },
        ];
        let card = Rect::new(
            position[0].clamp(0., (size[0] - 220.).max(0.)),
            position[1].clamp(0., (size[1] - 116.).max(0.)),
            220.,
            116.,
        );
        paint::rect(list, card, colors.CHROME);
        paint::stroke(list, card, colors.BORDER, 1.);
        for (index, text) in entries.iter().enumerate() {
            let row = Rect::new(
                card.min[0] + 3.,
                card.min[1] + 3. + index as f32 * 27. + if index == 3 { 2. } else { 0. },
                214.,
                26.,
            );
            let enabled = index != 2 || group.len() > 1;
            if enabled && row.contains(input.mouse) {
                paint::rect(list, row, colors.ACCENT.with_alpha(0.18));
            }
            label(list, atlas, row.shrink(5.), text, !enabled);
            if index == 3 {
                paint::rect(
                    list,
                    Rect::new(card.min[0] + 8., row.min[1] - 2., 204., 1.),
                    colors.BORDER,
                );
            }
            if enabled && clicked(input, row) {
                match index {
                    0 => out.push(Action::ClosePanel(panel)),
                    1 => out.push(Action::Float(panel)),
                    2 => out.extend(
                        group
                            .iter()
                            .filter(|id| **id != panel)
                            .map(|id| Action::ClosePanel(*id)),
                    ),
                    _ => out.push(Action::ResetWorkspace),
                }
                panels.panel_menu = None;
            }
        }
        if !opened_panel_menu && input.pressed.is_some_and(|(_, p)| !card.contains(p)) {
            panels.panel_menu = None;
        }
    }

    if panels
        .effect_search
        .as_ref()
        .is_some_and(|c| c.owner.is_none())
    {
        if let Some(mut search) = panels.effect_search.take() {
            let open = crate::panels::effect_search::render_effect_search(
                list,
                atlas,
                size,
                &mut search,
                &panels.effect_controls.catalogue,
                input,
                panels.locale,
                panels.editable_selection(state).first().copied(),
                &mut out,
            );
            if open {
                panels.effect_search = Some(search);
            }
        }
    }

    // Workspace owns splitter interaction and persists the resulting dock tree.
    for (index, handle) in splitter_handles.into_iter().enumerate() {
        let target = if handle.area.contains(input.mouse) {
            if input.dragging() {
                1.
            } else {
                0.6
            }
        } else {
            0.
        };
        let amount = panels.motion.value(
            (0, 1000 + index as u64),
            0.,
            target,
            styles.as_ref().metric("motion.divider_ms", 100.),
            false,
        );
        if amount > 0.001 {
            paint::rect(list, handle.area, colors.ACCENT.with_alpha(amount * 0.5));
        }
    }
    let parent_selection = panels.selection_ids();
    let parent_suspended = panels.menu.is_some() || panels.text_menu.is_some();
    panels.timeline_columns.finish_window(
        list,
        atlas,
        Rect::new(
            0.,
            CHROME_HEIGHT,
            size[0],
            (size[1] - CHROME_HEIGHT - 20.).max(1.),
        ),
        state,
        input,
        panels.locale,
        &parent_selection,
        &mut panels.parent_controls,
        parent_suspended,
        &mut out,
    );
    panels.parent_controls.finish_window(list, atlas, Rect::new(0., 0., size[0], size[1]),
        state, &parent_selection, input, panels.locale, parent_suspended, &mut out);
    panels.layer_actions.set_selection(panels.selection_ids());
    panels.layer_actions.finish_window(
        list,
        atlas,
        Rect::new(0., 0., size[0], size[1]),
        state,
        input,
        panels.locale,
        &mut out,
    );
    if frames(layout, size).iter().any(|frame| frame.panel == Panel::Effects) {
        panels.browser.finish_context_menu(list, atlas, Rect::new(0., 0., size[0], size[1]), state, effect_context_input, panels.locale, &mut out);
    }
    panels.effect_controls.finish_context_menu(
        list,
        atlas,
        Rect::new(0., 0., size[0], size[1]),
        state,
        effect_context_input,
        panels.locale,
        &mut out,
    );
    panels.path_controls.finish_context_menu(list, atlas, Rect::new(0., 0., size[0], size[1]),
        state, input, panels.locale, panels.path_clipboard_available, panels.clipboard_busy, &mut out);
    if frames(layout, size)
        .iter()
        .any(|frame| frame.panel == Panel::Properties)
    {
        panels.properties.effects.finish_context_menu(
            list,
            atlas,
            Rect::new(0., 0., size[0], size[1]),
            state,
            effect_context_input,
            panels.locale,
            &mut out,
        );
    }
    if frames(layout, size)
        .iter()
        .any(|f| Some(f.panel) == panels.expressions.owner)
    {
        panels.expressions.finish_menu(
            list,
            atlas,
            Rect::new(0., 0., size[0], size[1]),
            state,
            if text_menu_owned { &text_menu_neutral } else { input },
            panels.locale,
            &mut out,
        );
    }
    finish_project_context_menu(
        list,
        atlas,
        Rect::new(0., 0., size[0], size[1]),
        panels,
        state,
        project_input,
        panels.locale,
        None,
        &mut out,
    );
    media_assets::overlay(list, atlas, Rect::new(0., 0., size[0], size[1]), panels, state, asset_input, None, &mut out);
    let toolbar_ids = panels.selection_ids();
    panels.toolbar_style.overlay(list, atlas, size, state, &toolbar_ids, editing_input, panels.locale, &mut out);
    if let Some(notice) = &panels.toolbar_style.notice { panels.message = Some(notice.clone()); }
    if let Some(cursor) = panels.toolbar_style.cursor { panels.cursor = Some(cursor); }
    if defer_expression_text_menu { menus(list, atlas, size, panels, input, state, &mut out); }
    if expressions::picking::active(panels, None) {
        let clips: Vec<_> = frames(layout, size).into_iter().map(|frame| (frame.panel, frame.content)).collect();
        expressions::picking::finish(panels, list, atlas, Rect::new(0., 0., size[0], size[1]), None, &clips, state, input, &mut out);
    }
    let handles_busy = panels.path_controls.editing() || panels.transform_controls.gesture_active() || panels.tools.editing();
    panels.viewer_guides.menu(list, atlas, Rect::new(0., 0., size[0], size[1]), state, guide_input, panels.locale, handles_busy,
        &panels.shortcut_bindings.hint(crate::shortcuts::Scope::Viewer, crate::shortcuts::Command::ToggleViewerHandles), &mut out);
    apply_marker_selection(panels, state);
    scope_edits(out, state)
}
pub fn panel(id: dock::DockId) -> Option<Panel> {
    Panel::from_dock(id)
}
#[cfg(test)]
pub fn floating_view(panel: Panel, size: [f32; 2]) -> Option<Rect> {
    if panel == Panel::Composition {
        Some(viewer(Rect::new(
            0.0,
            30.0,
            size[0],
            (size[1] - 52.0).max(1.0),
        )))
    } else {
        None
    }
}
pub fn floating_chrome(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    panel: Panel,
    size: [f32; 2],
    panels: &mut Panels,
    state: &Value,
    input: &Input,
) -> Vec<Action> {
    let effect_context_input = input;
    let effect_context_owned = panels.browser.prepare_context_window(Some(panel), input)
        | panels.effect_controls.prepare_context_window(Some(panel), input)
        | panels.properties.effects.prepare_context_window(Some(panel), input);
    let effect_context_neutral = Input::default();
    let input = if effect_context_owned { &effect_context_neutral } else { input };
    expressions::picking::prepare(panels, Some(panel));
    let project_input = input;
    let project_owned = project::prepare(panels, Some(panel), state, input);
    let project_neutral = Input::default();
    let input = if project_owned { &project_neutral } else { input };
    let asset_input = input;
    let asset_owned = panels.media_assets.prepare(Some(panel), state, input);
    let asset_neutral = Input::default();
    let input = if asset_owned { &asset_neutral } else { input };
    let guide_input = input;
    let guide_owned = panels.viewer_guides.prepare(Some(panel), state, input);
    let guide_neutral = Input::default();
    let input = if guide_owned { &guide_neutral } else { input };
    // The toolbar belongs to the main native window. Moving input to a float
    // cancels its gesture before that window can dispatch another edit.
    let mut toolbar_actions = vec![];
    let toolbar_modal = panels.toolbar_style.editing();
    if toolbar_modal && (input.focus_lost || input.pressed.is_some() || !input.keys.is_empty() || !input.text.is_empty()) {
        if panels.toolbar_style.cancel() { toolbar_actions.push(Action::History(4)); }
    }
    let toolbar_neutral = Input::default();
    let input = if toolbar_modal { &toolbar_neutral } else { input };
    tools::prepare(panels,Some(panel),input);
    let parent_pick_owned = panels.parent_controls.prepare_window(Some(panel), input);
    let parent_neutral = Input::default();
    let input = if parent_pick_owned { &parent_neutral } else { input };
    let path_context_owned = panels.path_controls.prepare_context_menu(Some(panel), state, panels.selected, input);
    let project_drag_owned = panels.project_drag.prepare(Some(panel), state, input);
    let drag_owned = panels.effect_drag.prepare(Some(panel), state, input) || project_drag_owned;
    if drag_owned && !panels.effect_drag.pending() { panels.browser.cancel_library_click(); }
    let drag_neutral = Input::default();
    let input = if drag_owned { &drag_neutral } else { input };
    let mask_header_modal = panels.path_controls.timeline_mask_add_open();
    let mask_header_neutral = Input::default();
    let input = if mask_header_modal && panel != Panel::Timeline {
        if input.pressed.is_some() || input.focus_lost {
            panels.path_controls.dismiss_timeline_mask_add();
        }
        &mask_header_neutral
    } else {
        input
    };
    if let Some(styles) = &panels.styles {
        list.stylesheet = Some(styles.clone());
    }
    let styles = list.styles();
    let colors = styles.colors();

    let mut out = toolbar_actions;
    tools::camera::flush(panels, &mut out);
    if panels.normalize_selection(state) {
        out.extend(cancel_interaction(panels));
    }
    panels.cursor = None;
    panels.drag_readout = None;
    let timeline_dropdown = panels.timeline_columns.editing() || panels.timeline.comparison.popup_open();
    let dropdown_owned = panels.timeline_columns.editing_in(Some(panel))
        || panels.timeline.comparison.popup_open() && matches!(panel,Panel::Timeline|Panel::Curves);
    let popup_neutral = Input::default();
    let layer_modal = panels.layer_actions.prepare(
        Some(panel),
        state,
        input,
        panels.locale,
        panels.selected,
        panel == Panel::Timeline
            && !panels.timeline_columns.editing()
            && !panels.timeline.comparison.keyboard_focus()
            && !panels.parent_controls.open
            && panels.properties.field.is_none()
            && !panels.audio_controls.editing()
            && !panels.path_controls.editing()
            && !panels.timeline.timeline_search
            && !panels.expressions.editing()
            && !expressions::picking::active(panels, Some(panel)),
        &mut out,
    );
    if !panels.effect_search_in(Some(panel))
        && !(panels.effect_search.is_some() && keyboard::effect_search_close_pressed(input))
        && matches!(
            panel,
            Panel::Masks | Panel::Properties | Panel::Vector | Panel::Timeline | Panel::Curves
        )
        && panels.path_controls.numeric_owner() == Some(panel)
    {
        panels.path_controls.parameter_input(
            state,
            input,
            &mut out,
            styles.metric("motion.drag_threshold", 3.),
        );
    }
    let body = Rect::new(0.0, 30.0, size[0], (size[1] - 52.0).max(1.0));
    let mut content = list.child();
    if !panels.effect_search_in(Some(panel))
        && !path_context_owned
        && !expressions::picking::active(panels, Some(panel))
        && (input.pressed.is_some() || !input.keys.is_empty() || !input.text.is_empty())
    {
        if panels.focused != Some(panel) {
            let node_region = |p: Option<Panel>| {
                matches!(
                    p,
                    Some(Panel::Nodes | Panel::NodeProperties | Panel::NodePreview)
                )
            };
            if !(node_region(Some(panel)) && node_region(panels.focused)) {
                out.extend(cancel_interaction(panels));
            }
        }
        panels.focused = Some(panel);
    }
    let shortcut_modal = expressions::picking::active(panels, Some(panel)) || keyboard::route(
        if path_context_owned || timeline_dropdown && !dropdown_owned {
            &popup_neutral
        } else {
            input
        },
        panels,
        state,
        matches!(
            panel,
            Panel::Nodes | Panel::NodeProperties | Panel::NodePreview
        ),
        Some(panel),
        &mut out,
    );
    let neutral = Input::default();
    let body_input = if panels.expressions.menu_open() || expressions::picking::active(panels, Some(panel))
        || path_context_owned
        || layer_modal
        || panels.layer_actions.modal_in(Some(panel))
        || shortcut_modal
        || timeline_dropdown && !dropdown_owned
        || panels.parent_controls.open
    {
        &neutral
    } else {
        input
    };
    panels.timeline_columns.prepare_window(Some(panel), input);
    render_panel(
        panel,
        &mut content,
        atlas,
        body,
        panels,
        state,
        body_input,
        &mut out,
    );
    content.clip_to(body);
    list.append(content);
    if panel==Panel::Timeline {
        curve_comparison::overlay(list,atlas,body,panels,state,
            if panels.timeline.comparison.popup_open(){input}else{body_input},&mut out);
    }
    paint::rect(list, Rect::new(0.0, 0.0, size[0], 30.0), colors.HEADER);
    label(
        list,
        atlas,
        Rect::new(8.0, 0.0, (size[0] - 100.0).max(1.0), 30.0),
        panels.locale.panel(panel.dock()),
        false,
    );
    button(
        list,
        atlas,
        Rect::new(size[0] - 78.0, 4.0, 72.0, 22.0),
        panels.locale.t(T::Dock),
        if timeline_dropdown || mask_header_modal || panels.parent_controls.open {
            &neutral
        } else {
            body_input
        },
        true,
        Action::Redock(panel.dock()),
        &mut out,
    );
    effect_drag::finish(list, atlas, size, panels, state, &mut out);
    project_drag::finish(list, atlas, size, panels, state, &mut out);
    paint::rect(
        list,
        Rect::new(0.0, size[1] - 20.0, size[0], 20.0),
        colors.CHROME,
    );
    let render_error = state["renderError"].as_str().filter(|error| !error.is_empty());
    let localized_error = render_error.map(|error| panels.locale.error(error));
    motion_ui::style::text_style(
        list,
        atlas,
        Rect::new(8.0, size[1] - 22.0, (size[0] - 16.0).max(1.0), 22.0),
        localized_error
            .as_deref()
            .or(panels.drag_readout.as_ref().map(|(_, text)| text.as_str()))
            .or(panels.effect_drag.status())
            .or(panels.message.as_deref())
            .unwrap_or(panels.locale.t(T::SharedProject)),
        styles
            .resolve(
                if render_error.is_some() { "error" } else { "label.small.muted" },
                motion_ui::style::States::default(),
            )
            .unwrap(),
        Align::Left,
    );
    if panels
        .effect_search
        .as_ref()
        .is_some_and(|c| c.owner == Some(panel))
    {
        if let Some(mut search) = panels.effect_search.take() {
            let selected = panels.editable_selection(state).first().copied();
            if effect_search::render_effect_search(
                list,
                atlas,
                size,
                &mut search,
                &panels.effect_controls.catalogue,
                input,
                panels.locale,
                selected,
                &mut out,
            ) {
                panels.effect_search = Some(search);
            }
        }
    }
    let parent_selection = panels.selection_ids();
    let parent_suspended = panels.menu.is_some() || panels.text_menu.is_some();
    panels.timeline_columns.finish_window(
        list,
        atlas,
        Rect::new(0., 30., size[0], (size[1] - 50.).max(1.)),
        state,
        input,
        panels.locale,
        &parent_selection,
        &mut panels.parent_controls,
        parent_suspended,
        &mut out,
    );
    panels.parent_controls.finish_window(list, atlas, Rect::new(0., 0., size[0], size[1]),
        state, &parent_selection, input, panels.locale, parent_suspended, &mut out);
    panels.layer_actions.set_selection(panels.selection_ids());
    panels.layer_actions.finish_window(
        list,
        atlas,
        Rect::new(0., 0., size[0], size[1]),
        state,
        input,
        panels.locale,
        &mut out,
    );
    if panel == Panel::Effects {
        panels.browser.finish_context_menu(list, atlas, Rect::new(0., 0., size[0], size[1]), state, effect_context_input, panels.locale, &mut out);
    }
    panels.effect_controls.finish_context_menu(
        list,
        atlas,
        Rect::new(0., 0., size[0], size[1]),
        state,
        effect_context_input,
        panels.locale,
        &mut out,
    );
    panels.path_controls.finish_context_menu(list, atlas, Rect::new(0., 0., size[0], size[1]),
        state, input, panels.locale, panels.path_clipboard_available, panels.clipboard_busy, &mut out);
    if panel == Panel::Properties {
        panels.properties.effects.finish_context_menu(
            list,
            atlas,
            Rect::new(0., 0., size[0], size[1]),
            state,
            effect_context_input,
            panels.locale,
            &mut out,
        );
    }
    if panels.expressions.owner == Some(panel) {
        panels.expressions.finish_menu(
            list,
            atlas,
            Rect::new(0., 0., size[0], size[1]),
            state,
            input,
            panels.locale,
            &mut out,
        );
    }
    finish_project_context_menu(
        list,
        atlas,
        Rect::new(0., 0., size[0], size[1]),
        panels,
        state,
        project_input,
        panels.locale,
        Some(panel),
        &mut out,
    );
    media_assets::overlay(list, atlas, Rect::new(0., 0., size[0], size[1]), panels, state, asset_input, Some(panel), &mut out);
    if expressions::picking::active(panels, Some(panel)) {
        expressions::picking::finish(panels, list, atlas, Rect::new(0., 0., size[0], size[1]), Some(panel), &[(panel, body)], state, input, &mut out);
    }
    let handles_busy = panels.path_controls.editing() || panels.transform_controls.gesture_active() || panels.tools.editing();
    panels.viewer_guides.menu(list, atlas, Rect::new(0., 0., size[0], size[1]), state, guide_input, panels.locale, handles_busy,
        &panels.shortcut_bindings.hint(crate::shortcuts::Scope::Viewer, crate::shortcuts::Command::ToggleViewerHandles), &mut out);
    apply_marker_selection(panels, state);
    scope_edits(out, state)
}
fn apply_marker_selection(panels: &mut Panels, state: &Value) {
    if let Some(object) = panels.layer_actions.select_generated.take() {
        panels.select_only(Some(object));
        panels.audio_controls.select_generated(object, state);
    }
}
/// Bind edits to the composition that produced the input, even if another
/// window changes the active composition before the worker receives them.
fn scope_edits(actions: Vec<Action>, state: &Value) -> Vec<Action> {
    let composition = state["composition"]
        .as_str()
        .or_else(|| state["project"]["composition_id"].as_str())
        .unwrap_or(motion_core::composition::MAIN_COMPOSITION);
    if composition == motion_core::composition::MAIN_COMPOSITION {
        return actions;
    }
    let scoped = |command| match command {
        Command::InComposition { .. } => command,
        command => Command::InComposition {
            composition: composition.into(),
            command: Box::new(command),
        },
    };
    actions
        .into_iter()
        .map(|action| match action {
            Action::Edit(command) => Action::Edit(scoped(command)),
            Action::EditBatch(commands) => {
                Action::EditBatch(commands.into_iter().map(scoped).collect())
            }
            action => action,
        })
        .collect()
}
fn node_body(area: Rect) -> Rect {
    Rect::new(
        area.min[0],
        area.min[1] + 28.,
        area.width(),
        (area.height() - 28.).max(1.),
    )
}
fn node_panel(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    let styles = list.styles();
    let colors = styles.colors();
    let heading = Rect::new(area.min[0], area.min[1], area.width(), 28.);
    paint::rect(list, heading, colors.CHROME);
    let target = panels.node_editor.layer_target.clone();
    let title = target
        .as_ref()
        .map(|t| {
            format!(
                "{} · {}",
                if panels.locale == Locale::Zh {
                    "图层节点效果"
                } else {
                    "Layer Node Effect"
                },
                t.label
            )
        })
        .unwrap_or_else(|| {
            if panels.locale == Locale::Zh {
                "节点草稿 · 尚未绑定图层"
            } else {
                "Node Draft · Unbound"
            }
            .into()
        });
    label(
        list,
        atlas,
        Rect::new(
            heading.min[0] + 8.,
            heading.min[1],
            (heading.width() - if target.is_some() { 226. } else { 144. }).max(1.),
            28.,
        ),
        &title,
        false,
    );
    if ui::button_styled(
        list,
        atlas,
        Rect::new(heading.max[0] - 130., heading.min[1] + 2., 122., 24.),
        if target.is_some() {
            if panels.locale == Locale::Zh {
                "返回效果控件"
            } else {
                "Effect Controls"
            }
        } else {
            if panels.locale == Locale::Zh {
                "应用到当前图层"
            } else {
                "Apply to Layer"
            }
        },
        input,
        target.is_some()
            || panels.selected.is_some_and(|id| {
                state["project"]["layers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|l| {
                        l["id"].as_u64() == Some(id)
                            && id != 0
                            && l["locked"] != true
                            && !matches!(l["content"]["kind"].as_str(), Some("audio" | "null"))
                    })
            }),
        &styles,
        "button.icon",
    )
    .clicked
    {
        out.push(if target.is_some() {
            Action::ReturnToEffects
        } else {
            Action::ApplyNodeGraph
        });
    }
    if target.is_some()
        && ui::button_styled(
            list,
            atlas,
            Rect::new(heading.max[0] - 218., heading.min[1] + 2., 80., 24.),
            if panels.locale == Locale::Zh {
                "节点草稿"
            } else {
                "Node Draft"
            },
            input,
            true,
            &styles,
            "button.icon",
        )
        .clicked
    {
        out.push(Action::ReturnToNodeDraft);
    }
    let area = node_body(area);
    let blocked =
        panels.node_editor.bound_creating(state) && panels.node_editor.diagnostic().is_none();
    let neutral = Input::default();
    let input = if blocked { &neutral } else { input };
    let language = if panels.locale == Locale::Zh {
        crate::node_editor::Language::Chinese
    } else {
        crate::node_editor::Language::English
    };
    let (editor, preview) = if target.is_some() {
        (area, Rect::default())
    } else {
        crate::node_preview::regions(area)
    };
    let response = panels
        .node_editor
        .draw(list, atlas, editor, input, language);
    out.extend(response.actions.into_iter().map(Action::Node));
    if input
        .pressed
        .is_some_and(|(_, point)| preview.contains(point))
    {
        panels.node_editor.focus_panel();
    }
    if preview.height() >= 135. {
        panels.node_preview.draw(
            list,
            atlas,
            preview,
            input,
            panels.locale,
            panels.node_editor.stamp(),
            panels.node_editor.preview_target(),
        );
    }
}
pub fn node_view(layout: &Node, size: [f32; 2]) -> Option<Rect> {
    node_view_bound(layout, size, false)
}
pub fn node_view_bound(layout: &Node, size: [f32; 2], bound: bool) -> Option<Rect> {
    if let Some(frame) = frames(layout, size)
        .into_iter()
        .find(|f| f.panel == Panel::NodePreview)
    {
        return (frame.content.height() >= 135.)
            .then(|| crate::node_preview::panel_image_rect(frame.content));
    }
    if bound {
        return None;
    }
    frames(layout, size)
        .into_iter()
        .find(|f| f.panel == Panel::Nodes)
        .and_then(|f| {
            let (_, preview) = crate::node_preview::regions(node_body(f.content));
            (preview.height() >= 135.)
                .then(|| crate::node_preview::image_rect(node_body(f.content)))
        })
}
pub fn floating_node_view(panel: Panel, size: [f32; 2]) -> Option<Rect> {
    floating_node_view_bound(panel, size, false)
}
pub fn floating_node_view_bound(panel: Panel, size: [f32; 2], bound: bool) -> Option<Rect> {
    let body = Rect::new(0., 30., size[0], (size[1] - 52.).max(1.));
    if panel == Panel::NodePreview {
        return (body.height() >= 135.).then(|| crate::node_preview::panel_image_rect(body));
    }
    (!bound
        && panel == Panel::Nodes
        && crate::node_preview::regions(node_body(body)).1.height() >= 135.)
        .then(|| crate::node_preview::image_rect(node_body(body)))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn floating_effect_search_does_not_block_main_menu_pointer_input() {
        let mut panels = Panels {
            effect_search: Some(EffectSearch {
                position: [20., 20.],
                query: "glow".into(),
                selected_index: 0,
                owner: Some(Panel::NodeProperties),
                continuous: false,
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut atlas = TextAtlas::from_system(13.).unwrap();
        let layout = Node::dock(Panel::Project.dock());
        let state = serde_json::json!({"frame":0,"project":{"frames":120,"layers":[]}});
        let point = [24., metrics::TITLE_BAR + 12.];
        let input = Input {
            mouse: point,
            pressed: Some((MouseButton::Left, point)),
            ..Default::default()
        };
        chrome(
            &mut PaintList::default(),
            &mut atlas,
            [1440., 900.],
            &layout,
            &mut panels,
            &state,
            &input,
        );
        assert_eq!(panels.menu, Some(0));
        assert_eq!(
            panels.effect_search.as_ref().unwrap().owner,
            Some(Panel::NodeProperties)
        );
    }
    #[test]
    fn application_menus_close_when_the_window_loses_focus() {
        let mut panels = Panels {
            menu: Some(7),
            menu_scroll: 3,
            panel_menu: Some((Panel::Project.dock(), [20., 30.])),
            ..Default::default()
        };
        let mut atlas = TextAtlas::from_system(13.).unwrap();
        let state = serde_json::json!({"frame":0,"project":{"frames":120,"layers":[]}});
        let input = Input {
            focus_lost: true,
            ..Default::default()
        };
        chrome(
            &mut PaintList::default(),
            &mut atlas,
            [900., 600.],
            &Node::dock(Panel::Project.dock()),
            &mut panels,
            &state,
            &input,
        );
        assert_eq!(panels.menu, None);
        assert_eq!(panels.menu_scroll, 0);
        assert_eq!(panels.panel_menu, None);
    }

    #[test]
    fn pending_plugin_edit_can_be_cancelled_from_host_menu_even_when_history_is_disabled() {
        for locale in [Locale::Zh, Locale::En] {
            for (active, observing, expected) in [
                (true, false, true),
                (false, false, false),
                (true, true, false),
            ] {
                let mut panels = Panels {
                    locale,
                    menu: Some(1),
                    ..Default::default()
                };
                let state = serde_json::json!({"canUndo":false,"canRedo":false,"observing":observing,
                    "preview":{"pluginEditorRecovery":{"gesture":active}},"project":{"layers":[]}});
                // Clipboard commands precede cancellation in the Edit menu.
                // Exercise scrolling in the original small window before hitting
                // cancellation, rather than clicking the obsolete third row.
                let cancel_row = 2 + clipboard_menu::entries(&panels, &state).len();
                let mut atlas = TextAtlas::from_system(12.).unwrap();
                let mut out = vec![];
                for _ in 0..cancel_row {
                    menus(
                        &mut PaintList::default(), &mut atlas, [280., 240.],
                        &mut panels,
                        &Input { mouse: [20., 100.], wheel: [0., -1.], ..Default::default() },
                        &state, &mut out,
                    );
                }
                let point = [
                    20.,
                    metrics::TITLE_BAR + metrics::MENU_BAR + 4.
                        + (cancel_row - panels.menu_scroll) as f32 * 32. + 15.,
                ];
                menus(
                    &mut PaintList::default(),
                    &mut atlas,
                    [280., 240.],
                    &mut panels,
                    &Input {
                        mouse: point,
                        pressed: Some((MouseButton::Left, point)),
                        ..Default::default()
                    },
                    &state,
                    &mut out,
                );
                assert_eq!(out.len(), usize::from(expected));
                if expected {
                    assert!(
                        matches!(&out[0], Action::Plugin(v) if v["op"] == "cancel_plugin_gesture")
                    );
                }
            }
        }
    }
    #[test]
    fn window_menu_uses_scrollable_rows_at_small_sizes() {
        let mut panels = Panels {
            menu: Some(7),
            ..Default::default()
        };
        let mut atlas = TextAtlas::from_system(13.).unwrap();
        let state = serde_json::json!({"frame":0,"project":{"frames":120,"layers":[]}});
        let input = Input {
            mouse: [20., 100.],
            wheel: [0., -1.],
            ..Default::default()
        };
        menus(
            &mut PaintList::default(),
            &mut atlas,
            [280., 160.],
            &mut panels,
            &input,
            &state,
            &mut Vec::new(),
        );
        assert_eq!(panels.menu, Some(7));
        assert!(panels.menu_scroll > 0);
    }
    #[test]
    fn main_effect_search_does_not_block_floating_window_shortcuts() {
        let mut panels = Panels {
            effect_search: Some(EffectSearch {
                position: [20., 20.],
                query: "glow".into(),
                selected_index: 0,
                owner: None,
                continuous: false,
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut atlas = TextAtlas::from_system(13.).unwrap();
        let state = serde_json::json!({"frame":0,"project":{"frames":120,"layers":[]}});
        let input = Input {
            keys: vec![(
                Key::S,
                motion_ui::input::Modifiers {
                    control: true,
                    ..Default::default()
                },
            )],
            ..Default::default()
        };
        let out = floating_chrome(
            &mut PaintList::default(),
            &mut atlas,
            Panel::Info,
            [420., 600.],
            &mut panels,
            &state,
            &input,
        );
        assert!(matches!(out.as_slice(), [Action::Save]));
        assert!(panels.effect_search_in(None));
    }
    #[test]
    fn node_popup_owns_file_keys_and_outside_panel_click_without_switching_tabs() {
        let state = serde_json::json!({"frame":0,"project":{"frames":120,"layers":[{"id":1,"name":"Layer","three_d":false}]}});
        let mut panels = Panels {
            focused: Some(Panel::Nodes),
            selected: Some(1),
            ..Default::default()
        };
        panels.node_editor.focus_panel();
        let mut atlas = TextAtlas::from_system(13.).unwrap();
        let layout = Node::Split {
            edge: dock::Edge::Left,
            split: 0.25,
            first: Box::new(Node::dock(Panel::Project.dock())),
            second: Box::new(Node::dock(Panel::Nodes.dock())),
        };
        let size = [1440., 900.];
        chrome(
            &mut PaintList::default(),
            &mut atlas,
            size,
            &layout,
            &mut panels,
            &state,
            &Input::default(),
        );
        let input = Input {
            mouse: [700., 350.],
            keys: vec![(
                Key::A,
                motion_ui::input::Modifiers {
                    shift: true,
                    ..Default::default()
                },
            )],
            ..Default::default()
        };
        chrome(
            &mut PaintList::default(),
            &mut atlas,
            size,
            &layout,
            &mut panels,
            &state,
            &input,
        );
        assert!(panels.node_editor.popup_open());
        let input = Input {
            keys: vec![(
                Key::S,
                motion_ui::input::Modifiers {
                    control: true,
                    ..Default::default()
                },
            )],
            ..Default::default()
        };
        let out = chrome(
            &mut PaintList::default(),
            &mut atlas,
            size,
            &layout,
            &mut panels,
            &state,
            &input,
        );
        assert!(out.is_empty());
        assert!(panels.node_editor.popup_open());
        let project = frames(&layout, size)
            .into_iter()
            .find(|f| f.panel == Panel::Project)
            .unwrap();
        let point = [project.tabs.min[0] + 12., project.tabs.min[1] + 12.];
        let input = Input {
            mouse: point,
            pressed: Some((MouseButton::Left, point)),
            ..Default::default()
        };
        let out = chrome(
            &mut PaintList::default(),
            &mut atlas,
            size,
            &layout,
            &mut panels,
            &state,
            &input,
        );
        assert!(out.is_empty());
        assert!(!panels.node_editor.popup_open());
        assert_eq!(panels.focused, Some(Panel::Nodes));
        assert_eq!(panels.selected, Some(1));
    }
    #[test]
    fn nodes_route_file_and_delete_keys_without_project_commands() {
        let state = serde_json::json!({"frame":0,"project":{"frames":120,"layers":[{"id":1,"name":"Layer","three_d":false}]}});
        let mut panels = Panels {
            selected: Some(1),
            ..Default::default()
        };
        let mut atlas = TextAtlas::from_system(18.).unwrap();
        let layout = Node::dock(Panel::Nodes.dock());
        let size = [1280., 800.];
        let nodes = frames(&layout, size).into_iter().next().unwrap();
        assert_eq!(nodes.panel, Panel::Nodes);
        assert_eq!(composition_view(&layout, size), Rect::default());
        let (editor, _) = crate::node_preview::regions(nodes.content);
        let canvas = panels.node_editor.layout(editor).canvas;
        let point = [canvas.min[0] + 40., canvas.max[1] - 40.];
        chrome(
            &mut PaintList::default(),
            &mut atlas,
            size,
            &layout,
            &mut panels,
            &state,
            &Input {
                mouse: point,
                pressed: Some((MouseButton::Left, point)),
                ..Default::default()
            },
        );
        let out = chrome(
            &mut PaintList::default(),
            &mut atlas,
            size,
            &layout,
            &mut panels,
            &state,
            &Input {
                keys: vec![(
                    Key::S,
                    motion_ui::input::Modifiers {
                        control: true,
                        ..Default::default()
                    },
                )],
                ..Default::default()
            },
        );
        assert!(out
            .iter()
            .any(|a| matches!(a, Action::Node(crate::node_editor::Action::Save(_)))));
        assert!(!out
            .iter()
            .any(|a| matches!(a, Action::Save | Action::Edit(_) | Action::History(_))));
        let out = chrome(
            &mut PaintList::default(),
            &mut atlas,
            size,
            &layout,
            &mut panels,
            &state,
            &Input {
                keys: vec![(Key::Delete, Default::default())],
                ..Default::default()
            },
        );
        assert!(!out.iter().any(|a| matches!(a, Action::Edit(_))));
        assert_eq!(panels.selected, Some(1));
        assert!(node_view(&layout, size).is_some());
        assert!(floating_node_view(Panel::Nodes, size).is_some());
        assert!(floating_view(Panel::Nodes, size).is_none());
    }
    #[test]
    fn translated_numeric_edit_preserves_the_same_command_and_axis() {
        let state = serde_json::json!({"frame":21,"project":{"layers":[{"id":1,"name":"自定义名称","three_d":false}]},
            "sampledLayers":[{"id":1,"position":[10,20,30],"rotation":[0,0,0],"scale":[100,100,100],"opacity":1}],
            "timeline_layers":[{"object":1,"properties":{"position":{"separated":true}}}]});
        let mut commands = vec![];
        for locale in [Locale::En, Locale::Zh] {
            let mut panels = Panels {
                locale,
                selected: Some(1),
                properties: types::PropertiesState {
                    field: Some(Field {
                        object: 1,
                        property: Property::Position,
                        axis: 0,
                        text: "123".into(),
                        replace: false,
                        editor: Default::default(),
                        rect: Rect::default(),
                        scrubbing: None,
                    }),
                    ..Default::default()
                },
                ..Default::default()
            };
            let input = Input {
                keys: vec![(Key::Enter, Default::default())],
                ..Default::default()
            };
            let mut atlas = TextAtlas::from_system(18.0).unwrap();
            let mut paint = PaintList::default();
            let mut out = vec![];
            properties(
                &mut paint,
                &mut atlas,
                Rect::new(0.0, 0.0, 340.0, 600.0),
                &mut panels,
                &state,
                &input,
                &mut out,
            );
            let command = out
                .into_iter()
                .find_map(|a| match a {
                    Action::Edit(c) => Some(c),
                    _ => None,
                })
                .unwrap();
            commands.push(serde_json::to_value(command).unwrap());
        }
        assert_eq!(commands[0], commands[1]);
        assert_eq!(commands[0]["op"], "set_component");
        assert_eq!(commands[0]["axis"], "x");
        assert_eq!(commands[0]["frame"], 21);
        assert_eq!(commands[0]["value"], 123.0);
    }
    #[test]
    fn timeline_uses_real_snapshot_ids_and_independent_axis_key_times() {
        let state = serde_json::json!({"project":{"frames":120,"layers":[{"id":7,"name":"Bottom"},{"id":9,"name":"Top"}]},
            "timeline_layers":[{"object":9,"in_frame":15,"out_frame":90,"properties":{"position":{"keys":[],"axes":{"x":{"keys":[{"frame":0},{"frame":60}]},"y":{"keys":[{"frame":15},{"frame":90}]}}}}}]});
        let rows = rows(&state);
        assert_eq!(rows[0].id, 9);
        assert_eq!(rows[0].clip, [15, 90]);
        assert_eq!(rows[0].keys, vec![0.0, 15.0, 60.0, 90.0]);
        assert_eq!(rows[1].id, 7);
    }
    #[test]
    fn timecodes_step_frames_without_rounding_fractional_time() {
        assert_eq!(timecode(65.99, 30), "00:00:02:05");
        assert_eq!(timecode(108000.0, 30), "01:00:00:00");
    }
    #[test]
    fn composition_view_is_inside_window_at_small_sizes() {
        for size in [[900.0, 600.0], [1600.0, 900.0]] {
            let r = composition_view(&dock::editor_layout(), size);
            assert!(r.width() > 0.0 && r.height() > 0.0);
            assert!(Rect::new(0.0, 0.0, size[0], size[1]).contains_rect(r));
        }
    }
    #[test]
    fn timeline_popup_creates_layer_without_scrubbing_or_moving_background() {
        let mut panels = Panels::default();
        let mut atlas = TextAtlas::from_system(18.).unwrap();
        let layout = Node::dock(Panel::Timeline.dock());
        let size = [1280., 800.];
        let state = serde_json::json!({"frame":21,"project":{"frames":120,"fps":30,"layers":[]}});
        let point = [700., 200.];
        let opened = chrome(
            &mut PaintList::default(),
            &mut atlas,
            size,
            &layout,
            &mut panels,
            &state,
            &Input {
                mouse: point,
                pressed: Some((MouseButton::Right, point)),
                ..Default::default()
            },
        );
        assert!(opened.is_empty());
        assert!(panels.layer_actions.modal_in(Some(Panel::Timeline)));
        // The context menu grows upward from the right-click point.
        let clicked = [710., 200. - (7. * 20. + 6.) + 3. + 10.];
        let actions = chrome(
            &mut PaintList::default(),
            &mut atlas,
            size,
            &layout,
            &mut panels,
            &state,
            &Input {
                mouse: clicked,
                pressed: Some((MouseButton::Left, clicked)),
                ..Default::default()
            },
        );
        assert!(matches!(actions.as_slice(), [Action::AddSolid]));
        assert!(panels.timeline.timeline_context_menu.is_none());
    }
    #[test]
    fn adjustment_layer_is_reachable_from_timeline_and_layer_menu() {
        let state = serde_json::json!({"frame":21,"project":{"frames":120,"fps":30,"layers":[]}});
        let mut atlas = TextAtlas::from_system(18.).unwrap();
        for locale in [Locale::Zh, Locale::En] {
            let mut panels = Panels {
                locale,
                timeline: types::TimelineState {
                    timeline_context_menu: Some([100., 200.]),
                    ..Default::default()
                },
                ..Default::default()
            };
            let layout = Node::dock(Panel::Timeline.dock());
            let size = [1280., 800.];
            chrome(
                &mut PaintList::default(),
                &mut atlas,
                size,
                &layout,
                &mut panels,
                &state,
                &Input::default(),
            );
            let point = [110., 200. - (7. * 22. + 8.) + 4. + 3. * 22. + 11.];
            let input = Input {
                mouse: point,
                pressed: Some((MouseButton::Left, point)),
                ..Default::default()
            };
            let mut actions = chrome(
                &mut PaintList::default(),
                &mut atlas,
                size,
                &layout,
                &mut panels,
                &state,
                &input,
            );
            assert!(matches!(actions.as_slice(), [Action::AddAdjustment]));
            panels.menu = Some(3);
            let x = 8.
                + [T::File, T::Edit, T::Composition]
                    .into_iter()
                    .map(|key| atlas.measure(locale.t(key)) + 22.)
                    .sum::<f32>();
            let point = [
                x + 12.,
                metrics::TITLE_BAR + metrics::MENU_BAR + 4. + 3. * 32. + 15.,
            ];
            let input = Input {
                mouse: point,
                pressed: Some((MouseButton::Left, point)),
                ..Default::default()
            };
            actions.clear();
            menus(
                &mut PaintList::default(),
                &mut atlas,
                [1280., 800.],
                &mut panels,
                &input,
                &state,
                &mut actions,
            );
            assert!(matches!(actions.as_slice(), [Action::AddAdjustment]));
        }
    }
}

/// One dispatcher for docked and native floating panels. No panel owns a Session.
fn render_panel(
    panel: Panel,
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    let mut local = input.clone();
    let owns_popup = panel == Panel::Nodes && panels.node_editor.popup_open()
        || panel == Panel::Timeline && panels.path_controls.timeline_mask_add_open();
    if !owns_popup {
        local.pressed = local.pressed.filter(|(_, p)| area.contains(*p));
    }
    if panels.focused != Some(panel) {
        local.keys.clear();
        local.text.clear();
    }
    let input = &local;
    match panel {
        Panel::Project => project(list, atlas, area, panels, state, input, out),
        Panel::Composition => composition(list, atlas, area, panels, state, input, out),
        Panel::Properties => properties(list, atlas, area, panels, state, input, out),
        Panel::EffectControls | Panel::Effects => {
            effects::render(panel, list, atlas, area, panels, state, input, out)
        }
        Panel::Masks | Panel::Vector => {
            paths::render(panel, list, atlas, area, panels, state, input, out)
        }
        Panel::Curves => curves::render(list, atlas, area, panels, state, input, out),
        Panel::Timeline => timeline(list, atlas, area, panels, state, input, out),
        Panel::Nodes => node_panel(list, atlas, area, panels, state, input, out),
        Panel::NodeProperties => {
            let language = if panels.locale == Locale::Zh {
                crate::node_editor::Language::Chinese
            } else {
                crate::node_editor::Language::English
            };
            out.extend(
                panels
                    .node_editor
                    .draw_inspector(list, atlas, area, input, language)
                    .actions
                    .into_iter()
                    .map(Action::Node),
            );
        }
        Panel::NodePreview => {
            panels.node_preview.draw(
                list,
                atlas,
                area,
                input,
                panels.locale,
                panels.node_editor.stamp(),
                panels.node_editor.preview_target(),
            );
        }
        Panel::Audio => audio::render(list, atlas, area, panels, state, input, out),
        Panel::Align => align::render(list, atlas, area, panels, state, input, out),
        Panel::Expression => properties::render(list, atlas, area, panels, state, input, out),
        _ => auxiliary::render(panel, list, atlas, area, panels, state, input, out),
    }
    property_focus::settle(panels);
}
