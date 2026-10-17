//! Editable dock tree. Native windows live in the shell; panel ownership lives here.
use crate::{
    dock::{self, DockId, Edge, Node},
    input::{Input, MouseButton, Rect},
    theme::metrics,
};
use std::path::Path;
mod observation;
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Workspace {
    pub root: Node,
    pub floating: Vec<DockId>,
    #[serde(default)]
    pub hidden: Vec<DockId>,
    #[serde(default, deserialize_with = "deserialize_window_geometry")]
    pub window_geometry: std::collections::BTreeMap<u32, WindowGeometry>,
    #[serde(skip)]
    drag: Option<Drag>,
}
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct WindowGeometry {
    pub size: [f64; 2],
    pub position: Option<[i32; 2]>,
}

// serde's flattened ContentDeserializer doesn't coerce JSON string keys into
// integers. Parse explicitly so app preferences can safely flatten Workspace.
fn deserialize_window_geometry<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<std::collections::BTreeMap<u32, WindowGeometry>, D::Error> {
    use serde::Deserialize;
    let entries = std::collections::BTreeMap::<String, WindowGeometry>::deserialize(deserializer)?;
    entries
        .into_iter()
        .map(|(id, geometry)| {
            let id = id.parse().map_err(serde::de::Error::custom)?;
            if geometry
                .size
                .iter()
                .any(|v| !v.is_finite() || *v <= 0. || *v > 16384.)
            {
                return Err(serde::de::Error::custom("invalid floating window size"));
            }
            Ok((id, geometry))
        })
        .collect()
}
enum Drag {
    Split {
        path: Vec<bool>,
        area: Rect,
        edge: Edge,
    },
    Tab {
        panel: DockId,
        start: [f32; 2],
    },
}
pub enum Change {
    Float(DockId),
    Changed,
}
struct Handle {
    path: Vec<bool>,
    area: Rect,
    edge: Edge,
    hit: Rect,
}
impl Default for Workspace {
    fn default() -> Self {
        Self {
            root: dock::editor_layout(),
            floating: vec![],
            hidden: vec![],
            window_geometry: Default::default(),
            drag: None,
        }
    }
}
fn panels(node: &Node, out: &mut Vec<DockId>) {
    match node {
        Node::Dock { tabs, .. } => out.extend_from_slice(tabs),
        Node::Split { first, second, .. } => {
            panels(first, out);
            panels(second, out)
        }
        _ => {}
    }
}
fn remove(node: Node, panel: DockId) -> Option<Node> {
    match node {
        Node::Dock { id, mut tabs, mut active } => {
            if let Some(index) = tabs.iter().position(|id| *id == panel) {
                dock::remove_tab(&mut tabs, &mut active, index);
            }
            if tabs.is_empty() {
                None
            } else {
                Some(Node::Dock {
                    id: if id == panel { tabs[0] } else { id },
                    active,
                    tabs,
                })
            }
        }
        Node::Split {
            edge,
            split,
            first,
            second,
        } => match (remove(*first, panel), remove(*second, panel)) {
            (Some(first), Some(second)) => Some(Node::Split {
                edge,
                split,
                first: Box::new(first),
                second: Box::new(second),
            }),
            (Some(node), None) | (None, Some(node)) => Some(node),
            (None, None) => None,
        },
        _ => None,
    }
}
fn insert(node: &mut Node, panel: DockId, target: DockId, edge: Option<Edge>) -> bool {
    match node {
        Node::Dock { tabs, active, .. } if tabs.contains(&target) => {
            if let Some(edge) = edge {
                let old = node.clone();
                let incoming = Node::dock(panel);
                let (first, second) = if matches!(edge, Edge::Left | Edge::Top) {
                    (incoming, old)
                } else {
                    (old, incoming)
                };
                // Left/Top divide is expressed as the first child's share.
                *node = Node::Split {
                    edge: if edge.is_horizontal() {
                        Edge::Left
                    } else {
                        Edge::Top
                    },
                    split: 0.5,
                    first: Box::new(first),
                    second: Box::new(second),
                };
            } else {
                tabs.push(panel);
                *active = tabs.len() - 1;
            }
            true
        }
        Node::Split { first, second, .. } => {
            insert(first, panel, target, edge) || insert(second, panel, target, edge)
        }
        _ => false,
    }
}
fn split_rect(edge: Edge, area: Rect, ratio: f32) -> (Rect, Rect) {
    let ratio = ratio.clamp(0.05, 0.95);
    if edge.is_horizontal() {
        let first = if edge == Edge::Right {
            1.0 - ratio
        } else {
            ratio
        };
        let w = area.width() * first;
        (
            Rect::new(area.min[0], area.min[1], w, area.height()),
            Rect::new(
                area.min[0] + w,
                area.min[1],
                area.width() - w,
                area.height(),
            ),
        )
    } else {
        let first = if edge == Edge::Bottom {
            1.0 - ratio
        } else {
            ratio
        };
        let h = area.height() * first;
        (
            Rect::new(area.min[0], area.min[1], area.width(), h),
            Rect::new(
                area.min[0],
                area.min[1] + h,
                area.width(),
                area.height() - h,
            ),
        )
    }
}
fn handles(node: &Node, area: Rect, path: &mut Vec<bool>, out: &mut Vec<Handle>) {
    if let Node::Split {
        edge,
        split,
        first,
        second,
    } = node
    {
        let (a, b) = split_rect(*edge, area, *split);
        let hit = if edge.is_horizontal() {
            Rect::new(a.max[0] - 3.0, area.min[1], 6.0, area.height())
        } else {
            Rect::new(area.min[0], a.max[1] - 3.0, area.width(), 6.0)
        };
        out.push(Handle {
            path: path.clone(),
            area,
            edge: *edge,
            hit,
        });
        path.push(false);
        handles(first, a, path, out);
        path.pop();
        path.push(true);
        handles(second, b, path, out);
        path.pop();
    }
}
fn set_ratio(node: &mut Node, path: &[bool], value: f32) {
    if let Node::Split {
        split,
        first,
        second,
        ..
    } = node
    {
        if let Some((branch, tail)) = path.split_first() {
            set_ratio(if *branch { second } else { first }, tail, value)
        } else {
            *split = value.clamp(0.05, 0.95);
        }
    }
}
impl Workspace {
    pub fn from_root(root: Node) -> Self {
        Self {
            root,
            ..Default::default()
        }
    }
    pub fn owned_panels(&self) -> Vec<DockId> {
        let mut ids = vec![];
        panels(&self.root, &mut ids);
        ids.extend_from_slice(&self.floating);
        ids.extend_from_slice(&self.hidden);
        ids
    }
    pub fn valid_for(&self, registry: &crate::registry::PanelRegistry) -> bool {
        self.valid()
            && self
                .owned_panels()
                .iter()
                .all(|id| registry.get(*id).is_some())
    }
    pub fn redock_to(&mut self, panel: DockId, target: DockId, edge: Option<Edge>) -> bool {
        if panel != target
            && self.root.find(target).is_none()
            && !matches!(self.root, Node::Fixed { .. })
        {
            return false;
        }
        if panel == target && self.root.find(panel).is_some() {
            return false;
        }
        self.floating.retain(|id| *id != panel);
        self.hidden.retain(|id| *id != panel);
        self.root = remove(self.root.clone(), panel).unwrap_or(Node::Fixed {
            edge: Edge::Top,
            size: 0.,
        });
        if matches!(self.root, Node::Fixed { .. }) {
            self.root = Node::dock(panel);
            return true;
        }
        insert(&mut self.root, panel, target, edge)
    }

    pub fn load(path: &Path) -> Self {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                observation::reset();
                return Self::default();
            }
            Err(error) => {
                observation::failure(path, "read", format_args!("{error}"));
                return Self::default();
            }
        };
        if bytes.len() > 16 * 1024 {
            observation::failure(path, "size", format_args!("bytes={} limit=16384", bytes.len()));
            return Self::default();
        }
        let workspace: Self = match serde_json::from_slice(&bytes) {
            Ok(workspace) => workspace,
            Err(error) => {
                observation::failure(path, "json", format_args!("{error}"));
                return Self::default();
            }
        };
        if !workspace.valid() {
            observation::failure(path, "validation", format_args!("invalid dock ownership or geometry"));
            return Self::default();
        }
        observation::reset();
        workspace
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(
            path,
            serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    }
    pub fn valid(&self) -> bool {
        let mut ids = vec![];
        panels(&self.root, &mut ids);
        ids.extend_from_slice(&self.floating);
        ids.extend_from_slice(&self.hidden);
        ids.sort_by_key(|id| id.0);
        fn sane(n: &Node) -> bool {
            match n {
                Node::Split {
                    split,
                    first,
                    second,
                    ..
                } => {
                    split.is_finite()
                        && (0.05..=0.95).contains(split)
                        && sane(first)
                        && sane(second)
                }
                Node::Dock { id, tabs, active } => {
                    !tabs.is_empty() && tabs.contains(id) && *active < tabs.len()
                }
                _ => false,
            }
        }
        // A fully floating workspace uses an empty Fixed placeholder.
        ids.windows(2).all(|pair| pair[0] != pair[1])
            && (sane(&self.root) || matches!(self.root, Node::Fixed { size: 0.0, .. }))
    }
    pub fn close(&mut self, panel: DockId) -> bool {
        let mut ids = vec![];
        panels(&self.root, &mut ids);
        if !ids.contains(&panel) && !self.floating.contains(&panel) {
            return false;
        }
        self.root = remove(self.root.clone(), panel).unwrap_or(Node::Fixed {
            edge: Edge::Top,
            size: 0.,
        });
        self.floating.retain(|id| *id != panel);
        if !self.hidden.contains(&panel) {
            self.hidden.push(panel);
        }
        self.drag = None;
        true
    }

    pub fn float(&mut self, panel: DockId) -> bool {
        if self.floating.contains(&panel) {
            return false;
        }
        let mut ids = vec![];
        panels(&self.root, &mut ids);
        if !ids.contains(&panel) {
            return false;
        }
        self.root = remove(self.root.clone(), panel).unwrap_or(Node::Fixed {
            edge: Edge::Top,
            size: 0.0,
        });
        self.floating.push(panel);
        self.drag = None;
        true
    }
    pub fn redock(&mut self, panel: DockId) {
        self.floating.retain(|id| *id != panel);
        self.hidden.retain(|id| *id != panel);
        let mut ids = vec![];
        panels(&self.root, &mut ids);
        if ids.contains(&panel) {
            return;
        }
        if let Some(target) = ids.first().copied() {
            let edge = Edge::Left;
            insert(&mut self.root, panel, target, Some(edge));
        } else {
            self.root = Node::dock(panel);
        }
    }
    pub fn move_panel(&mut self, panel: DockId, target: DockId, edge: Option<Edge>) -> bool {
        if panel == target {
            return false;
        }
        let next = remove(self.root.clone(), panel);
        let Some(mut next) = next else { return false };
        if !insert(&mut next, panel, target, edge) {
            return false;
        }
        self.root = next;
        self.drag = None;
        true
    }
    /// Drag a divider to resize; drag a title to dock on an edge or combine tabs.
    /// Releasing a title outside the workspace detaches it to a native window.
    pub fn interact(&mut self, input: &Input, area: Rect) -> Option<Change> {
        self.interact_with_header_actions(input, area, 30.)
    }
    pub fn drag_cursor(&self) -> Option<crate::cursor::Cursor> {
        use crate::cursor::Cursor;
        match &self.drag {
            Some(Drag::Split { edge, .. }) => Some(if edge.is_horizontal() {Cursor::ResizeEW}else{Cursor::ResizeNS}),
            Some(Drag::Tab { .. }) => Some(Cursor::Grabbing),
            None => None,
        }
    }
    /// Reserve space for header actions only when the host actually draws them.
    pub fn interact_with_header_actions(
        &mut self,
        input: &Input,
        area: Rect,
        action_width: f32,
    ) -> Option<Change> {
        if self.drag.is_none() && input.pressed.is_some_and(|(b, _)| b == MouseButton::Left) {
            let mut found = vec![];
            handles(&self.root, area, &mut vec![], &mut found);
            if let Some(h) = found
                .into_iter()
                .rev()
                .find(|h| h.hit.contains(input.mouse))
            {
                self.drag = Some(Drag::Split {
                    path: h.path,
                    area: h.area,
                    edge: h.edge,
                });
            } else {
                for placement in dock::solve(&self.root, area) {
                    let title = Rect::new(
                        placement.area.min[0],
                        placement.area.min[1],
                        (placement.area.width() - action_width.max(0.0)).max(0.0),
                        metrics::TAB_BAR,
                    );
                    if title.contains(input.mouse) {
                        self.drag = Some(Drag::Tab {
                            panel: placement.id,
                            start: input.mouse,
                        });
                        break;
                    }
                }
            }
        }
        if let Some(Drag::Split { path, area, edge }) = &self.drag {
            let size = if edge.is_horizontal() {
                area.width()
            } else {
                area.height()
            };
            let coordinate = if edge.is_horizontal() {
                input.mouse[0] - area.min[0]
            } else {
                input.mouse[1] - area.min[1]
            };
            let minimum = if edge.is_horizontal() {
                metrics::MIN_DOCK
            } else {
                metrics::MIN_TIMELINE
            };
            let low = (minimum / size.max(1.0)).min(0.45);
            let ratio = (coordinate / size.max(1.0)).clamp(low, 1.0 - low);
            let ratio = if matches!(edge, Edge::Right | Edge::Bottom) {
                1.0 - ratio
            } else {
                ratio
            };
            let path = path.clone();
            set_ratio(&mut self.root, &path, ratio);
            if input.released.is_some() {
                self.drag = None;
            }
            return Some(Change::Changed);
        }
        if input.released.is_some() {
            if let Some(Drag::Tab { panel, start }) = self.drag.take() {
                if (start[0] - input.mouse[0]).hypot(start[1] - input.mouse[1]) < 6.0 {
                    return None;
                }
                let placements = dock::solve(&self.root, area);
                if let Some(target) = placements
                    .into_iter()
                    .find(|p| p.area.contains(input.mouse) && p.id != panel)
                {
                    let p = input.mouse;
                    let a = target.area;
                    let dx = (p[0] - a.min[0]) / a.width().max(1.0);
                    let dy = (p[1] - a.min[1]) / a.height().max(1.0);
                    let edge = if dx < 0.25 {
                        Some(Edge::Left)
                    } else if dx > 0.75 {
                        Some(Edge::Right)
                    } else if dy < 0.25 {
                        Some(Edge::Top)
                    } else if dy > 0.75 {
                        Some(Edge::Bottom)
                    } else {
                        None
                    };
                    if self.move_panel(panel, target.id, edge) {
                        return Some(Change::Changed);
                    }
                } else if !area.contains(input.mouse) {
                    return Some(Change::Float(panel));
                }
            }
        }
        None
    }
    pub fn cancel(&mut self) {
        self.drag = None;
    }
    /// Select the actual clicked tab within the captured group, including inactive tabs.
    pub fn choose_dragged_tab(&mut self, panel: DockId) {
        if let Some(Drag::Tab { panel: current, .. }) = &mut self.drag {
            let same_group = matches!(self.root.find(*current),Some(Node::Dock{tabs,..}) if tabs.contains(&panel));
            if same_group {
                *current = panel;
            }
        }
    }
}
