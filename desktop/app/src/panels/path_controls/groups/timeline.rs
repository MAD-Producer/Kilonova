//! The same shape-content tree and property rows inside the main timeline gutter.
use super::*;
use crate::panels::{
    timeline::{property_group, Row},
    types::{PropertyFilter, TimelineState},
    Panels,
};

enum Entry {
    Node {
        id: u64,
        tag: u8,
        name: String,
        depth: usize,
        open: bool,
    },
    Parameter(u64, parameters::Parameter, usize),
    Setting(u64, parameters::Setting, usize),
}
fn entries(
    node: parameters::Node,
    object: u64,
    t: &TimelineState,
    animated: bool,
    depth: usize,
    out: &mut Vec<Entry>,
) {
    if animated && !node.animated() {
        return;
    }
    let open = t.vector_open.contains(&(object, node.id, node.tag));
    out.push(Entry::Node {
        id: node.id,
        tag: node.tag,
        name: node.name,
        depth,
        open,
    });
    if !open {
        return;
    }
    for p in node.parameters {
        if !animated || p.animated() {
            out.push(Entry::Parameter(node.id, p, depth + 1));
        }
    }
    if !animated {
        for s in node.settings {
            out.push(Entry::Setting(node.id, s, depth + 1));
        }
    }
    for n in node.children {
        entries(n, object, t, animated, depth + 1, out);
    }
}
pub(in crate::panels) fn key_frames(vector: &VectorContent, size: [f32; 2]) -> Vec<i32> {
    fn gather(n: &parameters::Node, out: &mut Vec<i32>) {
        for p in &n.parameters {
            out.extend(p.keys.iter().map(|(f, _)| *f));
        }
        for n in &n.children {
            gather(n, out);
        }
    }
    let mut keys = vec![];
    gather(&parameters::tree(vector, size, 0., Locale::En), &mut keys);
    keys.sort_unstable();
    keys.dedup();
    keys
}
#[derive(Default)]
pub(in crate::panels) struct Rows {
    root: Option<u64>,
    open: bool,
    entries: Vec<Entry>,
}
impl Rows {
    pub fn new(row: &Row, t: &TimelineState, frame: f64, locale: Locale) -> Self {
        let filter = t.filters.get(&row.id).copied().unwrap_or_default();
        let Some(tree) = row.vector.as_ref().and_then(|v| parameters::filtered_tree(
            v, row.vector_size, frame - f64::from(row.mask_offset), locale, filter)) else {
            return Self::default();
        };
        let mut rows = Self {
            root: Some(tree.id),
            open: t.vector_open.contains(&(row.id, 0, 0)),
            entries: vec![],
        };
        if rows.open {
            entries(
                tree,
                row.id,
                t,
                false,
                0,
                &mut rows.entries,
            );
        }
        rows
    }
    pub fn len(&self) -> usize {
        usize::from(self.root.is_some()) + self.entries.len()
    }
}
#[cfg(test)]
pub(in crate::panels) fn row_count(row: &Row, t: &TimelineState) -> usize {
    Rows::new(row, t, 0., Locale::En).len()
}
pub(in crate::panels) fn reveal_animated(panels: &mut Panels, state: &Value, object: u64) {
    reveal(panels, state, object, PropertyFilter::Animated);
}
pub(in crate::panels) fn reveal(panels: &mut Panels, state: &Value, object: u64, filter: PropertyFilter) {
    let Some(l) = super::super::layer(state, object) else {
        return;
    };
    let Content::Vector { vector } = &l.content else {
        return;
    };
    let Some(tree) = parameters::filtered_tree(
        vector,
        l.size,
        l.local_frame(state["frame"].as_f64().unwrap_or(0.)),
        panels.locale,
        filter,
    ) else { return; };
    fn open(n: &parameters::Node, object: u64, t: &mut TimelineState) {
        t.vector_open.insert((object, n.id, n.tag));
        for c in &n.children {
            open(c, object, t);
        }
    }
    panels.timeline.vector_open.insert((object, 0, 0));
    open(&tree, object, &mut panels.timeline);
    panels.path_controls.groups.reveal_tree(object, &tree);
    panels.timeline.set_graph_mode(false);
}
pub(in crate::panels) fn render(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    body: Rect,
    track: Rect,
    start: f32,
    height: f32,
    ppf: f32,
    offset: f32,
    row: &Row,
    rows: &Rows,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    let Some(root) = rows.root else {
        return;
    };
    let host = if panels.focused == Some(Panel::Curves) {
        Panel::Curves
    } else {
        Panel::Timeline
    };
    let locale = panels.locale;
    let styles = list.styles();
    let slot = styles.metric("icon.slot", 20.);
    let pad = styles.metric("padding.panel", 8.);
    let locked = row.locked || state["observing"] == true;
    let neutral = Input::default();
    let normal = if panels.path_controls.groups.menu_in(host) {
        &neutral
    } else {
        input
    };
    let root_open = rows.open;
    if start >= body.min[1] && start + height <= body.max[1] {
        let heading = property_group(
            list,
            atlas,
            Rect::new(
                area.min[0] + styles.metric("tree.indent", 14.),
                area.min[1],
                (area.width() - pad - slot - styles.metric("tree.indent", 14.)).max(0.),
                area.height(),
            ),
            start,
            height,
            if locale == Locale::Zh {
                "矢量内容"
            } else {
                "Vector contents"
            },
            root_open,
        );
        if click(normal, heading) {
            if panels.path_controls.cancel() {
                out.push(Action::History(4));
            }
            if !panels.timeline.vector_open.remove(&(row.id, 0, 0)) {
                panels.timeline.vector_open.insert((row.id, 0, 0));
                panels.timeline.vector_open.insert((row.id, root, 0));
            }
        }
        let rect = Rect::new(area.max[0] - pad - slot, start, slot, height);
        if icon(list, rect, Icon::Plus, normal, !locked, false) {
            if panels.selected != Some(row.id) {
                panels.select_only(Some(row.id));
            }
            panels
                .path_controls
                .groups
                .open_menu(host, row.id, root, None, rect);
        }
    }
    if root_open {
        for (index, entry) in rows.entries.iter().enumerate() {
            let y = start + (index + 1) as f32 * height;
            if y < body.min[1] || y + height > body.max[1] {
                continue;
            }
            match entry {
                Entry::Node {
                    id,
                    tag,
                    name,
                    depth,
                    open,
                } => {
                    let x = area.min[0] + styles.metric("tree.indent", 14.) * (*depth + 2) as f32;
                    let heading = property_group(
                        list,
                        atlas,
                        Rect::new(
                            x,
                            area.min[1],
                            (area.max[0] - x - if *tag == 0 { slot + pad } else { 0. }).max(0.),
                            area.height(),
                        ),
                        y,
                        height,
                        name,
                        *open,
                    );
                    if click(normal, heading) {
                        if panels.path_controls.cancel() {
                            out.push(Action::History(4));
                        }
                        if !panels.timeline.vector_open.remove(&(row.id, *id, *tag)) {
                            panels.timeline.vector_open.insert((row.id, *id, *tag));
                        }
                    }
                    if *tag == 0 {
                        let add = Rect::new(area.max[0] - pad - slot, y, slot, height);
                        if icon(list, add, Icon::Plus, normal, !locked, false) {
                            if panels.selected != Some(row.id) {
                                panels.select_only(Some(row.id));
                            }
                            panels
                                .path_controls
                                .groups
                                .open_menu(host, row.id, *id, None, add);
                        }
                    }
                    if *tag != 1
                        && normal
                            .pressed
                            .is_some_and(|(b, p)| b == MouseButton::Right && heading.contains(p))
                        && !locked
                    {
                        if panels.selected != Some(row.id) {
                            panels.select_only(Some(row.id));
                        }
                        let point = normal.mouse;
                        panels.path_controls.groups.open_menu(
                            host,
                            row.id,
                            *id,
                            if *tag == 0 && *depth == 0 {
                                None
                            } else {
                                Some(*id)
                            },
                            Rect::new(point[0], point[1], slot, styles.metric("border", 1.)),
                        );
                    }
                }
                Entry::Parameter(item, p, depth) => {
                    let fields = layout(styles.as_ref(), area, y, *depth, 0, p.values.len(), host);
                    let navigation = crate::panels::key_navigation::rail(&styles, Rect::new(area.min[0], y, area.width(), height));
                    let lane = Rect::new(track.min[0], y, track.width(), height);
                    let hit = normal.pressed.is_some_and(|(_, point)| {
                        navigation.contains(point) || fields.caption.contains(point)
                            || fields.watch.contains(point)
                            || lane.contains(point)
                            || (0..p.values.len()).any(|a| {
                                layout(styles.as_ref(), area, y, *depth, a, p.values.len(), host)
                                    .number
                                    .contains(point)
                            })
                    });
                    if hit {
                        panels.select_only(Some(row.id));
                        panels.path_controls.groups.keys.choose(row.id, *item, p);
                        if click(normal, fields.caption) {
                            panels.timeline.set_graph_mode(false);
                        }
                    }
                    panels.path_controls.groups.parameter(
                        list, atlas, area, y, row.id, *item, p, state, normal, host, !locked,
                        *depth, out,
                    );
                    if !panels.timeline.graph_mode {
                        panels.path_controls.groups.keys.lane(
                            list,
                            lane,
                            ppf,
                            offset,
                            state,
                            row.id,
                            *item,
                            p,
                            row.mask_offset,
                            locked,
                            normal,
                            out,
                        );
                    }
                }
                Entry::Setting(item, s, depth) => {
                    let f = layout(styles.as_ref(), area, y, *depth, 0, 1, host);
                    if normal.pressed.is_some_and(|(_, p)| {
                        p[1] >= y
                            && p[1] < y + height
                            && p[0] >= f.caption.min[0]
                            && p[0] < area.max[0]
                    }) && panels.selected != Some(row.id)
                    {
                        panels.select_only(Some(row.id));
                    }
                    panels.path_controls.groups.setting(
                        list, atlas, area, y, row.id, *item, s, state, normal, host, !locked,
                        *depth, out,
                    );
                }
            }
        }
    }
}
pub(in crate::panels) fn process(
    list: &PaintList,
    panels: &mut Panels,
    state: &Value,
    body: Rect,
    ppf: f32,
    offset: f32,
    input: &Input,
    out: &mut Vec<Action>,
) {
    panels
        .path_controls
        .groups
        .prepare(state, panels.selected, input, out);
    panels.path_controls.groups.keys.process(
        list,
        state,
        panels.selected,
        body,
        ppf,
        offset,
        input,
        out,
    );
    if let Some((rect, dragging)) = panels.path_controls.groups.cursor {
        if dragging || rect.contains(input.mouse) {
            panels.cursor = Some(motion_ui::widgets::numeric_behavior::cursor(
                input, dragging,
            ));
        }
    }
}
