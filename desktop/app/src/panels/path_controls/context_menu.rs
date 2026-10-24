//! A frozen authored target owns the popup until selection/document/window changes.
use super::*;
use crate::layer_clipboard::{controller::Intent, PathOperation, PathSource};
use motion_core::PathTarget;
use motion_ui::{
    dock::Panel,
    style::{box_style, text_style, State, States},
};

pub(super) struct Menu {
    source: PathSource,
    owner: Option<Panel>,
    object: u64,
    tool: Tool,
    point: Option<point_selection::Point>,
    closed: bool,
    clipboard: super::super::path_clipboard::Selection,
    anchor: [f32; 2],
    opened: bool,
    scroll: usize,
}
#[derive(Clone, Copy)]
enum Choice {
    First,
    Convert,
    Clipboard(Intent),
    Cancel,
}
#[cfg(test)]
mod tests;

impl Controls {
    pub(super) fn take_context_for_inspector(
        &mut self,
        state: &Value,
        object: u64,
    ) -> Option<Menu> {
        self.context_menu.take().filter(|menu| {
            menu.object == object && menu.source == PathSource::of(state) && menu.tool == self.tool
        })
    }
    pub(in crate::panels) fn prepare_context_menu(
        &mut self,
        owner: Option<Panel>,
        state: &Value,
        selected: Option<u64>,
        input: &Input,
    ) -> bool {
        self.context_owner = owner;
        let Some(menu) = &self.context_menu else {
            return false;
        };
        let own = owner == menu.owner;
        let context_changed = menu.source != PathSource::of(state)
            || selected != Some(menu.object)
            || self.tool != menu.tool;
        let foreign_input =
            !own && (input.pressed.is_some() || !input.keys.is_empty() || !input.text.is_empty());
        if context_changed || foreign_input || own && input.focus_lost {
            self.context_menu = None;
        }
        own || foreign_input
    }

    pub(super) fn open_context_menu(
        &mut self,
        clip: Rect,
        paths: &[Value],
        projection: Option<&Projection>,
        offset: [f32; 2],
        state: &Value,
        selected: Option<u64>,
        input: &Input,
    ) -> bool {
        if self.context_menu.is_some() {
            return true;
        }
        let Some((MouseButton::Right, mouse)) = input.pressed else {
            return false;
        };
        if !clip.contains(mouse) || input.composing || self.editing() || state["observing"] == true
        {
            return false;
        }
        let (Some(object), Some(projection)) = (selected, projection) else {
            return false;
        };
        let mut nearest = HIT_RADIUS;
        let mut hit = None;
        for path in paths.iter().filter(|p| !p["bounds_size"].is_array()) {
            let Some(basis) = source_geometry::projection(projection, path) else {
                continue;
            };
            for node in path["nodes"].as_array().into_iter().flatten() {
                let (Some(g), Some(id)) = (geometry(node), node["id"].as_u64()) else {
                    continue;
                };
                let Some(p) = basis.screen([g[0] - offset[0], g[1] - offset[1]]) else {
                    continue;
                };
                let d = distance(p, mouse);
                if d < nearest {
                    nearest = d;
                    hit = Some((
                        point_selection::Point::new(path, id),
                        path["closed"] == true
                            && path["nodes"]
                                .as_array()
                                .is_some_and(|nodes| nodes.len() >= 3),
                    ));
                }
            }
        }
        if let Some((point, _)) = hit {
            self.point_selection.click(point, Default::default());
            self.selected = Some((point.path, point.node));
            self.selected_item = point.item;
            if point.mask.is_some() {
                self.mask = point.mask;
            }
        }
        let Some(clipboard) = self.path_clipboard_selection(state, selected) else {
            return false;
        };
        self.context_menu = Some(Menu {
            source: PathSource::of(state),
            owner: self.context_owner,
            object,
            tool: self.tool,
            point: hit.map(|h| h.0),
            closed: hit.is_some_and(|h| h.1),
            clipboard,
            anchor: mouse,
            opened: true,
            scroll: 0,
        });
        self.focus = true;
        true
    }

    pub(in crate::panels) fn finish_context_menu(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        bounds: Rect,
        state: &Value,
        input: &Input,
        locale: Locale,
        available: bool,
        busy: bool,
        out: &mut Vec<Action>,
    ) {
        let Some(menu) = self
            .context_menu
            .as_mut()
            .filter(|m| m.owner == self.context_owner)
        else {
            return;
        };
        let opened = std::mem::take(&mut menu.opened);
        let neutral = Input::default();
        let input = if opened { &neutral } else { input };
        let zh = locale == Locale::Zh;
        let word = |zh_label: &str, en_label: &str| {
            if zh {
                zh_label.to_owned()
            } else {
                en_label.to_owned()
            }
        };
        let writable = state["observing"] != true
            && !busy
            && state["project"]["layers"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|l| l["id"] == menu.object && l["locked"] != true);
        let rows = [
            (
                word("设为首顶点", "Set First Vertex"),
                Choice::First,
                writable && menu.closed && menu.point.is_some(),
            ),
            (
                if menu.tool == Tool::Mask {
                    word("复制为形状路径", "Copy as Shape Path")
                } else {
                    word("复制为蒙版路径", "Copy as Mask Path")
                },
                Choice::Convert,
                writable && menu.point.is_some(),
            ),
            (
                word("复制路径", "Copy Paths"),
                Choice::Clipboard(Intent::Copy),
                !busy && menu.clipboard.enabled(state, Intent::Copy, available),
            ),
            (
                word("剪切路径", "Cut Paths"),
                Choice::Clipboard(Intent::Cut),
                !busy && menu.clipboard.enabled(state, Intent::Cut, available),
            ),
            (
                word("在播放头粘贴路径", "Paste Paths at Playhead"),
                Choice::Clipboard(Intent::Paste { in_place: false }),
                !busy
                    && menu
                        .clipboard
                        .enabled(state, Intent::Paste { in_place: false }, available),
            ),
            (
                word("原位粘贴路径", "Paste Paths in Place"),
                Choice::Clipboard(Intent::Paste { in_place: true }),
                !busy
                    && menu
                        .clipboard
                        .enabled(state, Intent::Paste { in_place: true }, available),
            ),
            (
                word("取消剪贴板操作", "Cancel Clipboard Operation"),
                Choice::Cancel,
                busy,
            ),
        ];
        let styles = list.styles();
        let padding = styles.metric("padding.panel", f32::default());
        let row_height = styles.metric("context.row.height", f32::default());
        let row_style = styles
            .resolve("context_menu.row", States::default())
            .unwrap();
        let width = rows.iter().map(|r| atlas.measure(&r.0)).fold(
            styles.metric("context.menu.width", f32::default()),
            f32::max,
        ) + padding * 2.;
        let width = width.min(bounds.width());
        let height = (rows.len() as f32 * row_height + padding * 2.).min(bounds.height());
        let visible = (((height - padding * 2.) / row_height).floor() as usize)
            .max(1)
            .min(rows.len());
        menu.scroll = menu.scroll.min(rows.len().saturating_sub(visible));
        let card = Rect::new(
            menu.anchor[0].clamp(bounds.min[0], (bounds.max[0] - width).max(bounds.min[0])),
            menu.anchor[1].clamp(bounds.min[1], (bounds.max[1] - height).max(bounds.min[1])),
            width,
            height,
        );
        if input.focus_lost
            || input.keys.iter().any(|(key, _)| *key == Key::Escape)
            || input.pressed.is_some_and(|(_, p)| !card.contains(p))
        {
            self.context_menu = None;
            return;
        }
        if card.contains(input.mouse) {
            if input.wheel[1] < 0. {
                menu.scroll = (menu.scroll + 1).min(rows.len().saturating_sub(visible));
            } else if input.wheel[1] > 0. {
                menu.scroll = menu.scroll.saturating_sub(1);
            }
        }
        // Text paints after geometry; covered canvas labels must not show through.
        list.retain_glyphs(|g| {
            g.position[0] + g.size[0] <= card.min[0]
                || g.position[0] >= card.max[0]
                || g.position[1] + g.size[1] <= card.min[1]
                || g.position[1] >= card.max[1]
        });
        let mut overlay = list.child();
        box_style(
            &mut overlay,
            card,
            styles.resolve("context_menu", States::default()).unwrap(),
        );
        #[cfg(test)]
        self.context_rows.clear();
        let mut chosen = None;
        for (index, (label, choice, enabled)) in
            rows.iter().enumerate().skip(menu.scroll).take(visible)
        {
            let rect = Rect::new(
                card.min[0] + padding,
                card.min[1] + padding + (index - menu.scroll) as f32 * row_height,
                (width - padding * 2.).max(0.),
                row_height,
            );
            let flags = if !enabled {
                States::default().with(State::Disabled)
            } else if rect.contains(input.mouse) {
                States::default().with(State::Hovered)
            } else {
                States::default()
            };
            let style = styles
                .resolve("context_menu.row", flags)
                .unwrap_or(row_style);
            box_style(&mut overlay, rect, style);
            text_style(&mut overlay, atlas, rect, label, style, Align::Left);
            #[cfg(test)]
            self.context_rows.push((label.clone(), rect, *enabled));
            if *enabled
                && input
                    .pressed
                    .is_some_and(|(b, p)| b == MouseButton::Left && rect.contains(p))
            {
                chosen = Some(*choice);
            }
        }
        overlay.clip_to(card);
        list.append(overlay);
        if let Some(choice) = chosen {
            let menu = self.context_menu.take().unwrap();
            let action = match choice {
                Choice::Cancel => Action::CancelLayerClipboard,
                Choice::Clipboard(intent) => menu.clipboard.action(intent),
                Choice::First | Choice::Convert => {
                    let Some(point) = menu.point else {
                        return;
                    };
                    let target = point
                        .mask
                        .map(|mask| PathTarget::Mask {
                            object: menu.object,
                            mask,
                        })
                        .unwrap_or(PathTarget::Vector {
                            object: menu.object,
                            item: point.item,
                        });
                    let operation = match choice {
                        Choice::First => PathOperation::FirstVertex {
                            target,
                            path: point.path,
                            node: point.node,
                        },
                        _ => PathOperation::CopyAs {
                            target,
                            path: point.path,
                        },
                    };
                    Action::PathOperation {
                        source: menu.source,
                        operation,
                    }
                }
            };
            out.push(action);
        }
    }
}
