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

mod align;
pub(crate) mod animation_controls;
mod animation_data;
mod audio;
mod auxiliary;
pub(crate) mod camera_controls;
mod composition;
mod curves;
pub(crate) mod effect_controls;
mod effects;
mod expressions;
pub(crate) mod fx_console;
pub(crate) mod graph_handles;
pub(crate) mod graph_keys;
mod graph_sampling;
mod graph_workspace;
mod helpers;
mod keyboard;
mod layer_actions;
mod menus;
pub(crate) mod numeric;
pub(crate) mod parent_controls;
pub(crate) mod path_controls;
mod paths;
mod project;
mod properties;
mod timeline;
mod timeline_columns;
#[cfg(test)]
mod timeline_columns_tests;
mod timeline_graph;
mod timeline_shortcuts;
#[cfg(test)]
mod timeline_shortcuts_tests;
pub(crate) mod titlebar;
mod toolbar;
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
pub use types::{Action, Frame, FxConsole, Panels};
use types::{Field, ScrubState};

pub const CHROME_HEIGHT: f32 = metrics::TITLE_BAR + metrics::MENU_BAR + metrics::TOOL_BAR;
const GUTTER: f32 = 640.0;

pub fn frames(layout: &Node, size: [f32; 2]) -> Vec<Frame> {
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
    Rect::new(
        area.min[0] + 8.0,
        area.min[1] + 8.0,
        (area.width() - 16.0).max(1.0),
        (area.height() - 42.0).max(1.0),
    )
}
pub fn cancel_interaction(panels: &mut Panels) -> Vec<Action> {
    let node_changed = panels.node_editor.cancel_interaction();
    panels.layer_actions.cancel();
    panels.timeline.timeline_context_menu = None;
    panels.parent_controls.open = false;
    panels.timeline_columns.cancel();
    let camera = panels.camera_controls.cancel();
    let effect = panels.effect_controls.cancel();
    let numeric = panels
        .properties
        .field
        .take()
        .is_some_and(|field| field.scrubbing.is_some_and(|s| s.begun));
    panels.timeline.scrubbing = false;
    let moving = panels.timeline.moving.take().is_some();
    let trimming = panels.timeline.trimming.take().is_some();
    let animation = panels.animation_controls.cancel();
    let graph = panels.timeline.graph_drag.take().is_some_and(|d| d.begun())
        | panels.timeline.key_drag.take().is_some_and(|d| d.begun());
    let path = panels.path_controls.cancel();
    let transform = panels.transform_controls.cancel();
    let mut actions = if moving
        || trimming
        || numeric
        || animation
        || graph
        || path
        || transform
        || effect
        || camera
    {
        vec![Action::History(4)]
    } else {
        vec![]
    };
    if node_changed {
        actions.push(Action::Node(crate::node_editor::Action::PreviewChanged));
    }
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

    let mut out = Vec::new();
    if input.focus_lost {
        panels.menu = None;
        panels.menu_scroll = 0;
        panels.panel_menu = None;
    }
    panels.cursor = None;
    panels.drag_readout = None;
    let layer_modal = panels.layer_actions.prepare(
        Some(Panel::Timeline),
        state,
        input,
        panels.locale,
        panels.selected,
        panels.focused == Some(Panel::Timeline)
            && panels.properties.field.is_none()
            && !panels.timeline.timeline_search,
        &mut out,
    );
    panels.node_editor.external_inspector = layout.find(Panel::NodeProperties.dock()).is_some();
    let node_modal = panels.node_editor.popup_open();
    let timeline_dropdown = panels.timeline_columns.editing();
    if let Some((_, point)) = input.pressed.filter(|_| {
        !panels.fx_console_in(None) && !node_modal && !timeline_dropdown && !layer_modal
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
            panels.selected = None;
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
    if !panels.fx_console_in(None)
        && !(panels.fx_console.is_some() && keyboard::console_close_pressed(input))
        && !node_modal
        && matches!(panels.focused, Some(Panel::Masks | Panel::Vector))
    {
        panels.path_controls.parameter_input(
            state,
            input,
            &mut out,
            styles.metric("motion.drag_threshold", 3.),
        );
    }
    if panels.fx_console.is_none()
        && input
            .key_presses()
            .any(|(k, _, repeat)| k == Key::Escape && !repeat)
    {
        out.extend(cancel_interaction(panels));
        panels.menu = None;
        panels.panel_menu = None;
        panels.project.search = false;
        panels.timeline.timeline_search = false;
        panels.fx_console = None;
        panels.timeline.timeline_context_menu = None;
        panels.project.context_menu = None;
    }
    let nodes_frame = frames(layout, size)
        .into_iter()
        .find(|f| f.panel == Panel::Nodes);
    let node_keyboard = matches!(
        panels.focused,
        Some(Panel::Nodes | Panel::NodeProperties | Panel::NodePreview)
    ) || nodes_frame.as_ref().is_some_and(|f| {
        if let Some((_, point)) = input.pressed {
            f.content.contains(point)
        } else {
            panels.node_editor.focused()
        }
    }) && panels.menu.is_none();
    let shortcut_modal = keyboard::route(input, panels, state, node_keyboard, None, &mut out);
    let menu_modal = layer_modal
        || panels.layer_actions.modal_in(Some(Panel::Timeline))
        || shortcut_modal
        || panels.menu.is_some()
        || panels.panel_menu.is_some()
        || panels.fx_console_in(None)
        || panels.timeline.timeline_context_menu.is_some()
        || timeline_dropdown
        || panels.effect_controls.editing()
        || panels.project.context_menu.is_some()
        || panels.timeline.timeline_context_menu.is_some();
    let neutral = Input::default();
    let body_input = if menu_modal || node_modal {
        &neutral
    } else {
        input
    };
    let project_input = body_input.clone();
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
                    || (timeline_popup && f.panel == Panel::Timeline)
                {
                    input
                } else {
                    &project_input
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
    for handle in dock::splitters(layout, layout_area) {
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
    toolbar::render(list, atlas, size, panels, state, body_input, &mut out);
    let status = Rect::new(0.0, size[1] - 20.0, size[0], 20.0);
    paint::rect(list, status, colors.CHROME);
    let render_error = state["renderError"].as_str().filter(|s| !s.is_empty());
    let localized_error = render_error.map(|e| panels.locale.error(e));
    let message = localized_error
        .as_deref()
        .or(panels.drag_readout.as_ref().map(|(_, text)| text.as_str()))
        .or(panels.message.as_deref())
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
    menus(
        list,
        atlas,
        size,
        panels,
        if shortcut_modal || node_modal {
            &neutral
        } else {
            input
        },
        state,
        &mut out,
    );
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
        .fx_console
        .as_ref()
        .is_some_and(|c| c.owner.is_none())
    {
        if let Some(mut console) = panels.fx_console.take() {
            let open = crate::panels::fx_console::render_fx_console(
                list,
                atlas,
                size,
                &mut console,
                &panels.effect_controls.catalogue,
                input,
                panels.locale,
                panels.selected.filter(|id| {
                    *id != 0
                        && !state["project"]["layers"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|l| {
                                l["id"].as_u64() == Some(*id) && l["locked"].as_bool() == Some(true)
                            })
                }),
                &mut out,
            );
            if open {
                panels.fx_console = Some(console);
            }
        }
    }

    // Workspace owns splitter interaction and persists the resulting dock tree.
    let layout_area = Rect::new(
        0.,
        CHROME_HEIGHT,
        size[0],
        (size[1] - CHROME_HEIGHT - 22.).max(0.),
    );
    for (index, handle) in dock::splitters(layout, layout_area).into_iter().enumerate() {
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
        &mut out,
    );
    panels.layer_actions.finish_window(
        list,
        atlas,
        Rect::new(0., 0., size[0], size[1]),
        state,
        input,
        panels.locale,
        &mut out,
    );
    panels.effect_controls.finish_context_menu(
        list,
        atlas,
        Rect::new(0., 0., size[0], size[1]),
        state,
        input,
        panels.locale,
        &mut out,
    );
    finish_project_context_menu(
        list,
        atlas,
        Rect::new(0., 0., size[0], size[1]),
        panels,
        input,
        panels.locale,
        &mut out,
    );
    out
}
pub fn panel(id: dock::DockId) -> Option<Panel> {
    Panel::from_dock(id)
}
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
    if let Some(styles) = &panels.styles {
        list.stylesheet = Some(styles.clone());
    }
    let styles = list.styles();
    let colors = styles.colors();

    let mut out = vec![];
    panels.cursor = None;
    let timeline_dropdown = panels.timeline_columns.editing();
    let dropdown_owned = panels.timeline_columns.editing_in(Some(panel));
    let popup_neutral = Input::default();
    let layer_modal = panels.layer_actions.prepare(
        Some(panel),
        state,
        input,
        panels.locale,
        panels.selected,
        panel == Panel::Timeline
            && panels.properties.field.is_none()
            && !panels.timeline.timeline_search,
        &mut out,
    );
    if !panels.fx_console_in(Some(panel))
        && !(panels.fx_console.is_some() && keyboard::console_close_pressed(input))
        && matches!(panel, Panel::Masks | Panel::Vector)
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
    if !panels.fx_console_in(Some(panel))
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
    let shortcut_modal = keyboard::route(
        if timeline_dropdown && !dropdown_owned {
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
    let body_input = if layer_modal
        || panels.layer_actions.modal_in(Some(panel))
        || shortcut_modal
        || timeline_dropdown && !dropdown_owned
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
        if timeline_dropdown {
            &neutral
        } else {
            body_input
        },
        true,
        Action::Redock(panel.dock()),
        &mut out,
    );
    paint::rect(
        list,
        Rect::new(0.0, size[1] - 20.0, size[0], 20.0),
        colors.CHROME,
    );
    label(
        list,
        atlas,
        Rect::new(8.0, size[1] - 22.0, (size[0] - 16.0).max(1.0), 22.0),
        panels
            .message
            .as_deref()
            .unwrap_or(panels.locale.t(T::SharedProject)),
        true,
    );
    if panels
        .fx_console
        .as_ref()
        .is_some_and(|c| c.owner == Some(panel))
    {
        if let Some(mut console) = panels.fx_console.take() {
            let selected = panels.selected.filter(|id| {
                *id != 0
                    && !state["project"]["layers"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|l| {
                            l["id"].as_u64() == Some(*id) && l["locked"].as_bool() == Some(true)
                        })
            });
            if fx_console::render_fx_console(
                list,
                atlas,
                size,
                &mut console,
                &panels.effect_controls.catalogue,
                input,
                panels.locale,
                selected,
                &mut out,
            ) {
                panels.fx_console = Some(console);
            }
        }
    }
    panels.timeline_columns.finish_window(
        list,
        atlas,
        Rect::new(0., 30., size[0], (size[1] - 50.).max(1.)),
        state,
        input,
        panels.locale,
        &mut out,
    );
    panels.layer_actions.finish_window(
        list,
        atlas,
        Rect::new(0., 0., size[0], size[1]),
        state,
        input,
        panels.locale,
        &mut out,
    );
    panels.effect_controls.finish_context_menu(
        list,
        atlas,
        Rect::new(0., 0., size[0], size[1]),
        state,
        input,
        panels.locale,
        &mut out,
    );
    finish_project_context_menu(
        list,
        atlas,
        Rect::new(0., 0., size[0], size[1]),
        panels,
        input,
        panels.locale,
        &mut out,
    );
    out
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
    if target.is_none()
        && panels.node_editor.focused()
        && !panels.node_editor.editing()
        && motion_ui::input::KeyChord::new(Key::Space, false, false, false).pressed(input)
    {
        panels.node_preview.toggle();
    }
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
    fn floating_console_does_not_block_main_menu_pointer_input() {
        let mut panels = Panels {
            fx_console: Some(FxConsole {
                position: [20., 20.],
                query: "glow".into(),
                selected_index: 0,
                owner: Some(Panel::NodeProperties),
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
            panels.fx_console.as_ref().unwrap().owner,
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
    fn main_console_does_not_block_floating_window_shortcuts() {
        let mut panels = Panels {
            fx_console: Some(FxConsole {
                position: [20., 20.],
                query: "glow".into(),
                selected_index: 0,
                owner: None,
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
        assert!(panels.fx_console_in(None));
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
    if panel != Panel::Nodes || !panels.node_editor.popup_open() {
        local.pressed = local.pressed.filter(|(_, p)| area.contains(*p));
    }
    if panels.focused != Some(panel) {
        local.keys.clear();
        local.text.clear();
    }
    let input = &local;
    match panel {
        Panel::Project => project(list, atlas, area, panels, state, input),
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
        Panel::Expression => expressions::render(list, atlas, area, panels, state, input, out),
        _ => auxiliary::render(panel, list, atlas, area, panels, state, input, out),
    }
}
