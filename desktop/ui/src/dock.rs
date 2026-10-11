//! After Effects style dock layout.
//!
//! The layout is a small tree: the window holds a horizontal split of vertical
//! splits. Each leaf is a dock that stacks tabs, and each dock lives on one
//! edge of its parent. Dragging a splitter resizes one dock pair, which is the
//! same model After Effects uses for Project / Effect Controls / Composition and
//! for the timeline beneath, so muscle memory transfers.

use crate::input::Rect;
use crate::theme::metrics;

/// Which edge of the parent a dock occupies.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

impl Edge {
    pub fn is_horizontal(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

/// Identifies a dock. Stable across layout edits so open tabs keep their panel.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, serde::Serialize, serde::Deserialize,
)]
pub struct DockId(pub u32);

/// A layout node.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum Node {
    /// Two children sharing the parent along `edge`, sized by `split` in 0.05..0.95.
    Split {
        edge: Edge,
        split: f32,
        first: Box<Node>,
        second: Box<Node>,
    },
    /// A tabbed panel stack.
    Dock {
        id: DockId,
        tabs: Vec<DockId>,
        active: usize,
    },
    /// Fixed chrome that is never a drop target, such as the menu bar.
    Fixed { edge: Edge, size: f32 },
}

impl Node {
    pub fn dock(id: DockId) -> Self {
        Self::Dock {
            id,
            tabs: vec![id],
            active: 0,
        }
    }

    /// Split `self` into two docks along `edge`, keeping the existing content
    /// as the first child. Returns the new second dock.
    pub fn split(&mut self, edge: Edge, new: DockId) -> bool {
        match self {
            Node::Split { split, .. } => {
                *split = split.clamp(0.05, 0.95);
                false
            }
            Node::Dock { id, tabs, active } => {
                let id = *id;
                let tabs = tabs.clone();
                let active = *active;
                let replacement = Node::Split {
                    edge,
                    split: 0.25,
                    first: Box::new(Node::Dock { id, tabs, active }),
                    second: Box::new(Node::dock(new)),
                };
                *self = replacement;
                true
            }
            Node::Fixed { .. } => false,
        }
    }

    /// Every dock in the tree, in paint order.
    pub fn docks(&self) -> Vec<DockId> {
        let mut out = Vec::new();
        self.collect(&mut out);
        out
    }

    fn collect(&self, out: &mut Vec<DockId>) {
        match self {
            Node::Split { first, second, .. } => {
                first.collect(out);
                second.collect(out);
            }
            Node::Dock { id, .. } => out.push(*id),
            Node::Fixed { .. } => {}
        }
    }

    pub fn find(&self, id: DockId) -> Option<&Node> {
        match self {
            Node::Split { first, second, .. } => first.find(id).or_else(|| second.find(id)),
            Node::Dock {
                id: found, tabs, ..
            } if *found == id || tabs.contains(&id) => Some(self),
            _ => None,
        }
    }

    pub fn activate(&mut self, id: DockId, tab: DockId) -> bool {
        if let Some(Node::Dock { id: owner, .. }) = self.find(tab) {
            if !matches!(self.find(id), Some(Node::Dock { id: target, .. }) if target == owner) {
                return false;
            }
        }
        match self {
            Node::Split { first, second, .. } => {
                first.activate(id, tab) || second.activate(id, tab)
            }
            Node::Dock {
                id: found,
                tabs,
                active,
            } => {
                if *found != id {
                    return false;
                }
                match tabs.iter().position(|t| *t == tab) {
                    Some(index) => {
                        *active = index;
                        true
                    }
                    None => {
                        tabs.push(tab);
                        *active = tabs.len() - 1;
                        true
                    }
                }
            }
            Node::Fixed { .. } => false,
        }
    }

    pub fn close(&mut self, id: DockId, tab: DockId) -> bool {
        match self {
            Node::Split { first, second, .. } => first.close(id, tab) || second.close(id, tab),
            Node::Dock {
                id: found,
                tabs,
                active,
            } => {
                if *found != id {
                    return false;
                }
                let Some(index) = tabs.iter().position(|t| *t == tab) else {
                    return false;
                };
                tabs.remove(index);
                *active = (*active).min(tabs.len().saturating_sub(1));
                true
            }
            Node::Fixed { .. } => false,
        }
    }
}

/// A dock after layout: where it is and how big it is.
#[derive(Clone, Copy, Debug)]
pub struct Placement {
    pub id: DockId,
    pub area: Rect,
}

/// Resolve the tree into absolute rectangles for a window of `size` pixels.
pub fn solve(root: &Node, area: Rect) -> Vec<Placement> {
    let mut out = Vec::new();
    solve_into(root, area, &mut out);
    out
}

fn solve_into(node: &Node, area: Rect, out: &mut Vec<Placement>) {
    match node {
        Node::Fixed { edge, size } => {
            let child = take(*edge, area, *size);
            solve_into_fixed(*edge, child, out);
        }
        Node::Dock { id, tabs, active } => out.push(Placement {
            id: tabs.get(*active).copied().unwrap_or(*id),
            area,
        }),
        Node::Split {
            edge,
            split,
            first,
            second,
        } => {
            let (a, b) = divide(*edge, area, *split);
            solve_into(first, a, out);
            solve_into(second, b, out);
        }
    }
}

/// Fixed nodes never dock content; they only reserve room.
fn solve_into_fixed(_edge: Edge, _area: Rect, _out: &mut Vec<Placement>) {}

fn take(edge: Edge, area: Rect, size: f32) -> Rect {
    match edge {
        Edge::Top => Rect::new(area.min[0], area.min[1], area.width(), size),
        Edge::Bottom => Rect::new(area.min[0], area.max[1] - size, area.width(), size),
        Edge::Left => Rect::new(area.min[0], area.min[1], size, area.height()),
        Edge::Right => Rect::new(area.max[0] - size, area.min[1], size, area.height()),
    }
}

fn divide(edge: Edge, area: Rect, split: f32) -> (Rect, Rect) {
    match edge {
        Edge::Left => {
            let width = area.width() * split;
            (
                Rect::new(area.min[0], area.min[1], width, area.height()),
                Rect::new(
                    area.min[0] + width,
                    area.min[1],
                    area.width() - width,
                    area.height(),
                ),
            )
        }
        Edge::Right => {
            let width = area.width() * split;
            (
                Rect::new(
                    area.min[0],
                    area.min[1],
                    area.width() - width,
                    area.height(),
                ),
                Rect::new(area.max[0] - width, area.min[1], width, area.height()),
            )
        }
        Edge::Top => {
            let height = area.height() * split;
            (
                Rect::new(area.min[0], area.min[1], area.width(), height),
                Rect::new(
                    area.min[0],
                    area.min[1] + height,
                    area.width(),
                    area.height() - height,
                ),
            )
        }
        Edge::Bottom => {
            let height = area.height() * split;
            (
                Rect::new(
                    area.min[0],
                    area.min[1],
                    area.width(),
                    area.height() - height,
                ),
                Rect::new(area.min[0], area.max[1] - height, area.width(), height),
            )
        }
    }
}

/// A grab handle produced by [`solve`], used for drag resizing.
#[derive(Clone, Copy, Debug)]
pub struct Splitter {
    pub area: Rect,
    pub edge: Edge,
    pub parent_area: Rect,
}

/// Collect every draggable handle in a solved layout.
pub fn splitters(root: &Node, area: Rect) -> Vec<Splitter> {
    let mut out = Vec::new();
    collect_splitters(root, area, &mut out);
    out
}

fn collect_splitters(node: &Node, area: Rect, out: &mut Vec<Splitter>) {
    match node {
        Node::Split {
            edge,
            split,
            first,
            second,
        } => {
            let (first_area, second_area) = divide(*edge, area, *split);

            // Create splitter handle between the two areas
            let splitter_area = match edge {
                Edge::Left => Rect::new(first_area.max[0] - 2.0, area.min[1], 4.0, area.height()),
                Edge::Right => Rect::new(second_area.min[0] - 2.0, area.min[1], 4.0, area.height()),
                Edge::Top => Rect::new(area.min[0], first_area.max[1] - 2.0, area.width(), 4.0),
                Edge::Bottom => Rect::new(area.min[0], second_area.min[1] - 2.0, area.width(), 4.0),
            };

            out.push(Splitter {
                area: splitter_area,
                edge: *edge,
                parent_area: area,
            });

            // Recursively collect splitters in children
            collect_splitters(first, first_area, out);
            collect_splitters(second, second_area, out);
        }
        _ => {}
    }
}

/// Default editor layout matching After Effects.
///
/// Left: Project. Right: Effect Controls over Composition. Bottom: Timeline.
pub fn editor_layout() -> Node {
    fn split(edge: Edge, ratio: f32, a: Node, b: Node) -> Node {
        Node::Split {
            edge,
            split: ratio,
            first: Box::new(a),
            second: Box::new(b),
        }
    }
    let right_panels = [
        (Panel::Info, 120.),
        (Panel::Preview, 143.),
        (Panel::Effects, 324.),
        (Panel::Character, 201.),
        (Panel::Align, 31.),
        (Panel::Audio, 29.),
        (Panel::Masks, 26.),
        (Panel::Media, 26.),
    ];
    fn stack(items: &[(Panel, f32)]) -> Node {
        if items.len() == 1 {
            return Node::dock(items[0].0.dock());
        }
        let total: f32 = items.iter().map(|p| p.1).sum();
        split(
            Edge::Top,
            items[0].1 / total,
            Node::dock(items[0].0.dock()),
            stack(&items[1..]),
        )
    }
    let project = Node::Dock {
        id: DockId(1),
        tabs: vec![DockId(1), DockId(17), DockId(2), DockId(11)],
        active: 0,
    };
    // Media has one workspace owner: keep it in the right dock until opened there.
    let project = match project {
        Node::Dock {
            id,
            mut tabs,
            active,
        } => {
            tabs.retain(|id| *id != DockId(17));
            Node::Dock { id, tabs, active }
        }
        _ => unreachable!(),
    };
    let left = split(
        Edge::Top,
        0.62,
        split(Edge::Left, 304. / 1294., project, Node::dock(DockId(3))),
        Node::dock(DockId(4)),
    );
    split(Edge::Right, 304. / 1600., left, stack(&right_panels))
}

/// Panel identity, matching the After Effects tab names.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Panel {
    Project = 1,
    EffectControls = 2,
    Composition = 3,
    Timeline = 4,
    Effects = 5,
    Masks = 6,
    Vector = 7,
    Audio = 8,
    Expression = 9,
    Nodes = 10,
    Properties = 11,
    Info = 12,
    Preview = 13,
    Character = 14,
    Paragraph = 15,
    Align = 16,
    Media = 17,
    RenderQueue = 18,
    Curves = 19,
    NodeProperties = 20,
    NodePreview = 21,
}

impl Panel {
    pub const ALL: [Self; 21] = [
        Self::Project,
        Self::EffectControls,
        Self::Composition,
        Self::Timeline,
        Self::Effects,
        Self::Masks,
        Self::Vector,
        Self::Audio,
        Self::Expression,
        Self::Nodes,
        Self::Properties,
        Self::Info,
        Self::Preview,
        Self::Character,
        Self::Paragraph,
        Self::Align,
        Self::Media,
        Self::RenderQueue,
        Self::Curves,
        Self::NodeProperties,
        Self::NodePreview,
    ];
    pub fn from_dock(id: DockId) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.dock() == id)
    }
    pub fn title_zh(self) -> &'static str {
        match self {
            Self::Project => "项目",
            Self::EffectControls => "效果控制",
            Self::Composition => "合成",
            Self::Timeline => "时间轴",
            Self::Effects => "效果和预设",
            Self::Masks => "蒙版",
            Self::Vector => "路径与钢笔",
            Self::Audio => "音频",
            Self::Expression => "表达式",
            Self::Nodes => "节点编辑器",
            Self::Properties => "属性",
            Self::Info => "信息",
            Self::Preview => "预览控制",
            Self::Character => "字符",
            Self::Paragraph => "段落",
            Self::Align => "对齐",
            Self::Media => "素材浏览器",
            Self::RenderQueue => "渲染队列",
            Self::Curves => "曲线编辑器",
            Self::NodeProperties => "节点属性",
            Self::NodePreview => "节点预览",
        }
    }
    pub fn dock(self) -> DockId {
        DockId(self as u32)
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Project => "Project",
            Self::EffectControls => "Effect Controls",
            Self::Composition => "Composition",
            Self::Timeline => "Timeline",
            Self::Effects => "Effects",
            Self::Masks => "Masks",
            Self::Vector => "Path",
            Self::Audio => "Audio",
            Self::Expression => "Expression",
            Self::Nodes => "Nodes",
            Self::Properties => "Properties",
            Self::Info => "Info",
            Self::Preview => "Preview",
            Self::Character => "Character",
            Self::Paragraph => "Paragraph",
            Self::Align => "Align",
            Self::Media => "Media Browser",
            Self::RenderQueue => "Render Queue",
            Self::Curves => "Graph Editor",
            Self::NodeProperties => "Node Properties",
            Self::NodePreview => "Node Preview",
        }
    }
}

/// Hit test a dock rect for a click.
pub fn hit(placements: &[Placement], point: [f32; 2]) -> Option<DockId> {
    placements
        .iter()
        .rev()
        .find(|p| p.area.contains(point))
        .map(|p| p.id)
}

/// Clamp a proposed split ratio so no dock can be collapsed or pushed off
/// screen. The minimum is expressed in pixels, so it holds at any window size.
pub fn clamp_split(edge: Edge, split: f32, area: Rect) -> f32 {
    let minimum = if edge.is_horizontal() {
        metrics::MIN_DOCK
    } else {
        metrics::MIN_TIMELINE
    };
    let extent = if edge.is_horizontal() {
        area.width()
    } else {
        area.height()
    };
    if extent <= 0.0 {
        return 0.5;
    }
    let floor = (minimum / extent).clamp(0.05, 0.45);
    split.clamp(floor, 1.0 - floor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_layout_keeps_the_timeline_under_project_and_viewer() {
        let placements = solve(&editor_layout(), Rect::new(0.0, 0.0, 1920.0, 1080.0));
        assert_eq!(placements.len(), 11);
        assert_eq!(
            placements.iter().map(|p| p.id).collect::<Vec<_>>(),
            vec![
                Panel::Project.dock(),
                Panel::Composition.dock(),
                Panel::Timeline.dock(),
                Panel::Info.dock(),
                Panel::Preview.dock(),
                Panel::Effects.dock(),
                Panel::Character.dock(),
                Panel::Align.dock(),
                Panel::Audio.dock(),
                Panel::Masks.dock(),
                Panel::Media.dock()
            ]
        );
    }

    #[test]
    fn docks_never_overlap_and_stay_inside_the_window() {
        let area = Rect::new(0.0, 0.0, 1600.0, 900.0);
        let placements = solve(&editor_layout(), area);
        for placement in &placements {
            assert!(area.contains_rect(placement.area), "{placement:?}");
        }
        for (i, a) in placements.iter().enumerate() {
            for b in placements.iter().skip(i + 1) {
                let overlap_x = (a.area.max[0] - b.area.min[0]).min(b.area.max[0] - a.area.min[0]);
                let overlap_y = (a.area.max[1] - b.area.min[1]).min(b.area.max[1] - a.area.min[1]);
                assert!(overlap_x <= 0.0 || overlap_y <= 0.0, "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn a_left_edge_split_puts_the_first_child_on_the_left() {
        let (a, b) = divide(Edge::Left, Rect::new(0.0, 0.0, 100.0, 50.0), 0.25);
        assert_eq!(a.min[0], 0.0);
        assert_eq!(a.width(), 25.0);
        assert_eq!(b.min[0], 25.0);
    }

    #[test]
    fn a_bottom_edge_split_puts_the_first_child_on_top() {
        let (a, b) = divide(Edge::Bottom, Rect::new(0.0, 0.0, 100.0, 100.0), 0.7);
        assert_eq!(a.height(), 30.0);
        assert_eq!(b.max[1], 100.0);
        assert_eq!(b.height(), 70.0);
    }

    #[test]
    fn splitting_a_dock_replaces_it_and_keeps_the_original_first() {
        let mut node = Node::dock(Panel::Project.dock());
        assert!(node.split(Edge::Right, Panel::Timeline.dock()));
        let placements = solve(&node, Rect::new(0.0, 0.0, 800.0, 600.0));
        assert_eq!(placements.len(), 2);
        assert_eq!(placements[0].id, Panel::Project.dock());
        assert_eq!(placements[1].id, Panel::Timeline.dock());
    }

    #[test]
    fn activating_an_unopened_tab_adds_it_to_that_dock_only() {
        let mut node = Node::Split {
            edge: Edge::Left,
            split: 0.5,
            first: Box::new(Node::dock(Panel::Project.dock())),
            second: Box::new(Node::dock(Panel::EffectControls.dock())),
        };
        assert!(node.activate(Panel::EffectControls.dock(), Panel::Effects.dock()));
        assert!(!node.activate(Panel::Project.dock(), Panel::Effects.dock()));
        let effects = node
            .find(Panel::EffectControls.dock())
            .map(|n| {
                matches!(n, Node::Dock { tabs, active, .. }
                if tabs.len() == 2 && tabs[*active] == Panel::Effects.dock())
            })
            .unwrap_or(false);
        assert!(effects);
    }

    #[test]
    fn closing_the_active_tab_falls_back_to_a_neighbour() {
        let mut node = Node::Dock {
            id: Panel::Effects.dock(),
            tabs: vec![Panel::Effects.dock(), Panel::Masks.dock()],
            active: 1,
        };
        assert!(node.close(Panel::Effects.dock(), Panel::Masks.dock()));
        match node {
            Node::Dock { tabs, active, .. } => {
                assert_eq!(tabs, vec![Panel::Effects.dock()]);
                assert_eq!(active, 0);
            }
            _ => panic!("expected a dock"),
        }
    }

    #[test]
    fn splits_clamp_so_a_dock_cannot_be_collapsed() {
        let area = Rect::new(0.0, 0.0, 400.0, 400.0);
        assert!(clamp_split(Edge::Right, 0.0, area) >= metrics::MIN_DOCK / 400.0);
        assert!(clamp_split(Edge::Right, 1.0, area) <= 1.0 - metrics::MIN_DOCK / 400.0);
        assert!((clamp_split(Edge::Right, 0.5, area) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn clamping_is_symmetric_so_neither_side_can_be_hidden() {
        let area = Rect::new(0.0, 0.0, 400.0, 400.0);
        assert_eq!(
            clamp_split(Edge::Bottom, 0.01, area) + clamp_split(Edge::Bottom, 0.99, area),
            1.0
        );
    }
}
