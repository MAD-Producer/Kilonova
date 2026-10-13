//! Named desktop layouts: only UI ownership and geometry are saved here.
use super::*;
use motion_ui::dock::{Node, Panel};

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    #[default]
    Default,
    Animation,
    Nodes,
    Drawing,
    All,
}
impl Preset {
    pub const ALL: [Self; 5] = [
        Self::Default,
        Self::Animation,
        Self::Nodes,
        Self::Drawing,
        Self::All,
    ];
    pub fn title(self, locale: crate::i18n::Locale) -> &'static str {
        match (self, locale == crate::i18n::Locale::Zh) {
            (Self::Default, true) => "默认",
            (Self::Animation, true) => "动画",
            (Self::Nodes, true) => "节点",
            (Self::Drawing, true) => "绘制",
            (Self::All, true) => "所有面板",
            (Self::Default, false) => "Default",
            (Self::Animation, false) => "Animation",
            (Self::Nodes, false) => "Nodes",
            (Self::Drawing, false) => "Drawing",
            (Self::All, false) => "All panels",
        }
    }
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct SavedLayout {
    root: Node,
    floating: Vec<DockId>,
    hidden: Vec<DockId>,
    #[serde(deserialize_with = "geometry")]
    window_geometry: std::collections::BTreeMap<u32, WindowGeometry>,
}
fn geometry<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<std::collections::BTreeMap<u32, WindowGeometry>, D::Error> {
    use serde::Deserialize;
    std::collections::BTreeMap::<String, WindowGeometry>::deserialize(d)?
        .into_iter()
        .map(|(id, g)| Ok((id.parse().map_err(serde::de::Error::custom)?, g)))
        .collect()
}
impl SavedLayout {
    pub(super) fn capture(layout: &motion_ui::workspace::Workspace) -> Self {
        Self {
            root: layout.root.clone(),
            floating: layout.floating.clone(),
            hidden: layout.hidden.clone(),
            window_geometry: layout.window_geometry.clone(),
        }
    }
    fn restore(&self) -> Option<motion_ui::workspace::Workspace> {
        // Reuse the framework deserializer and its geometry checks; no gesture survives a switch.
        serde_json::from_value(serde_json::to_value(self).expect("layout serializes")).ok()
    }
    pub fn valid(&self) -> bool {
        self.restore().is_some_and(|layout| {
            layout.valid()
                && layout
                    .owned_panels()
                    .iter()
                    .all(|id| Panel::from_dock(*id).is_some())
        })
    }
}
fn split(edge: Edge, ratio: f32, a: Node, b: Node) -> Node {
    Node::Split {
        edge,
        split: ratio,
        first: Box::new(a),
        second: Box::new(b),
    }
}
fn tabs(panels: &[Panel]) -> Node {
    let ids: Vec<_> = panels.iter().map(|p| p.dock()).collect();
    Node::Dock {
        id: ids[0],
        tabs: ids,
        active: 0,
    }
}
fn layout(preset: Preset) -> motion_ui::workspace::Workspace {
    let mut layout = motion_ui::workspace::Workspace::default();
    layout.root = match preset {
        Preset::Default => dock::editor_layout(),
        Preset::Animation => {
            let mut root = dock::editor_layout();
            if let Node::Split { first, .. } = &mut root {
                if let Node::Split { split, .. } = first.as_mut() {
                    *split = 0.535;
                }
            }
            root
        }
        Preset::Nodes => split(
            Edge::Bottom,
            0.27,
            split(
                Edge::Right,
                0.32,
                Node::dock(Panel::Nodes.dock()),
                split(
                    Edge::Bottom,
                    0.43,
                    Node::dock(Panel::Composition.dock()),
                    tabs(&[
                        Panel::EffectControls,
                        Panel::NodeProperties,
                        Panel::NodePreview,
                    ]),
                ),
            ),
            Node::dock(Panel::Timeline.dock()),
        ),
        Preset::Drawing => split(
            Edge::Bottom,
            0.25,
            split(
                Edge::Left,
                304. / 1600.,
                tabs(&[Panel::Vector, Panel::Masks]),
                split(
                    Edge::Right,
                    286. / 1292.,
                    Node::dock(Panel::Composition.dock()),
                    Node::dock(Panel::Properties.dock()),
                ),
            ),
            Node::dock(Panel::Timeline.dock()),
        ),
        Preset::All => split(
            Edge::Bottom,
            0.35,
            split(
                Edge::Right,
                0.25,
                split(
                    Edge::Left,
                    0.25,
                    tabs(&[Panel::Project, Panel::Media, Panel::EffectControls]),
                    tabs(&[Panel::Composition, Panel::Nodes, Panel::NodePreview]),
                ),
                tabs(&[
                    Panel::Properties,
                    Panel::NodeProperties,
                    Panel::Effects,
                    Panel::Vector,
                    Panel::Masks,
                    Panel::Expression,
                ]),
            ),
            tabs(&[
                Panel::Timeline,
                Panel::RenderQueue,
                Panel::Character,
                Panel::Paragraph,
                Panel::Align,
                Panel::Audio,
                Panel::Preview,
                Panel::Info,
                Panel::Curves,
            ]),
        ),
    };
    let ids = layout.owned_panels();
    layout.hidden = Panel::ALL
        .into_iter()
        .map(|p| p.dock())
        .filter(|id| !ids.contains(id))
        .collect();
    layout
}
impl Workspace {
    pub fn save_named(&mut self, name: String) {
        self.named_layouts
            .insert(name, SavedLayout::capture(&self.layout));
    }

    pub fn design_layout(preset: Preset) -> Node {
        layout(preset).root
    }
    pub fn select_preset(&mut self, preset: Preset) {
        self.preset_layouts
            .insert(self.preset, SavedLayout::capture(&self.layout));
        self.layout = self
            .preset_layouts
            .get(&preset)
            .filter(|s| s.valid())
            .and_then(SavedLayout::restore)
            .unwrap_or_else(|| layout(preset));
        self.preset = preset;
        self.design_revision = 2;
    }
    pub fn reset_preset(&mut self) {
        self.layout = layout(self.preset);
        self.design_revision = 2;
        self.preset_layouts.remove(&self.preset);
    }
    pub fn migrate_design(&mut self) {
        if self.design_revision == 1 {
            // Preserve custom workspaces. Only upgrade the former Nodes default,
            // which lacks a Composition panel, and retain it as a named layout.
            if let Some(old) = self.preset_layouts.get(&Preset::Nodes).cloned() {
                if old.root.find(Panel::Composition.dock()).is_none() {
                    self.named_layouts
                        .entry("Nodes (previous)".into())
                        .or_insert(old);
                    self.preset_layouts.remove(&Preset::Nodes);
                }
            }
            if self.preset == Preset::Nodes
                && self.layout.root.find(Panel::Composition.dock()).is_none()
            {
                self.named_layouts
                    .entry("Nodes (previous)".into())
                    .or_insert_with(|| SavedLayout::capture(&self.layout));
                self.layout = layout(Preset::Nodes);
            }
            self.design_revision = 2;
        }
        if self.design_revision == 0 {
            self.legacy_layout = Some(SavedLayout::capture(&self.layout));
            self.layout = layout(Preset::Default);
            self.preset = Preset::Default;
            self.design_revision = 2;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn presets_remember_docks_and_floating_geometry_without_duplicate_owners() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("workspace.json");
        let mut workspace = Workspace::default();
        workspace.redock(Panel::Nodes.dock());
        workspace.float(Panel::Nodes.dock());
        workspace.window_geometry.insert(
            10,
            WindowGeometry {
                size: [900., 640.],
                position: Some([40, 60]),
            },
        );
        for preset in [
            Preset::Animation,
            Preset::Nodes,
            Preset::Drawing,
            Preset::All,
        ] {
            workspace.select_preset(preset);
            assert!(workspace.valid());
        }
        workspace.select_preset(Preset::Default);
        assert!(workspace.floating.contains(&Panel::Nodes.dock()));
        assert_eq!(workspace.window_geometry[&10].size, [900., 640.]);
        workspace.save(&path).unwrap();
        let mut loaded = Workspace::load(&path);
        loaded.select_preset(Preset::Nodes);
        loaded.select_preset(Preset::Default);
        assert!(loaded.floating.contains(&Panel::Nodes.dock()));
        assert!(loaded.valid());
    }
    #[test]
    fn legacy_layout_is_preserved_while_adopting_the_design() {
        let mut workspace = Workspace::default();
        workspace.design_revision = 0;
        workspace.root = Node::Dock {
            id: DockId(1),
            tabs: vec![DockId(1), DockId(2), DockId(3), DockId(4)],
            active: 0,
        };
        let before = serde_json::to_value(&workspace.root).unwrap();
        workspace.migrate_design();
        assert_eq!(
            serde_json::to_value(&workspace.legacy_layout.as_ref().unwrap().root).unwrap(),
            before
        );
        assert!(workspace.valid());
        let placements = dock::solve(&workspace.root, Rect::new(0., 0., 1600., 894.));
        let props = placements
            .iter()
            .find(|p| p.id == Panel::Properties.dock())
            .unwrap();
        assert!(props.area.min[0] > 1200.);
    }
}
