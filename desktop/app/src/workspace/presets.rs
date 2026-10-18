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
        let mut layout: motion_ui::workspace::Workspace =
            match serde_json::from_value(serde_json::to_value(self).expect("layout serializes")) {
                Ok(layout) => layout,
                Err(error) => {
                    static FAILURES: crate::fallback_observation::Counter = crate::fallback_observation::Counter::new();
                    FAILURES.failure("workspace.preset.restore", format_args!("reason={error} fallback=no_saved_layout"));
                    return None;
                }
            };
        layout.close(Panel::Masks.dock());
        layout.close(Panel::Expression.dock());
        Some(layout)
    }
    pub fn valid(&self) -> bool {
        let Some(layout) = self.restore() else { return false; };
        let valid = layout.valid()
            && layout.owned_panels().iter().all(|id| Panel::from_dock(*id).is_some());
        if !valid {
            // Lifetime, fixed process site: healthy presets must not conceal
            // a different malformed saved layout in the same selection menu.
            static FAILURES: crate::fallback_observation::Counter = crate::fallback_observation::Counter::new();
            FAILURES.failure("workspace.preset.validate", format_args!("reason=invalid_panel_ownership fallback=preset_default"));
        }
        valid
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
                Node::dock(Panel::Vector.dock()),
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
        self.restore_panel();
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
        self.restore_panel();
        self.layout = layout(self.preset);
        self.design_revision = 2;
        self.preset_layouts.remove(&self.preset);
    }
    pub fn migrate_design(&mut self) {
        self.layout.close(Panel::Masks.dock());
        self.layout.close(Panel::Expression.dock());
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
    fn legacy_mask_panels_are_hidden_without_losing_properties_or_other_layout_ownership() {
        for floating in [false, true] {
            let folder = tempfile::tempdir().unwrap();
            let path = folder.path().join("legacy-masks.json");
            let mut workspace = Workspace::default();
            workspace.layout.redock(Panel::Masks.dock());
            workspace.layout.redock(Panel::Vector.dock());
            if floating {
                assert!(workspace.layout.float(Panel::Masks.dock()));
            }
            workspace
                .preset_layouts
                .insert(Preset::Drawing, SavedLayout::capture(&workspace.layout));
            workspace.save(&path).unwrap();
            let mut restored = Workspace::load(&path);
            for preset in [Preset::Default, Preset::Drawing, Preset::All] {
                restored.select_preset(preset);
                assert!(restored.valid());
                assert!(restored.root.find(Panel::Masks.dock()).is_none());
                assert!(!restored.floating.contains(&Panel::Masks.dock()));
                assert!(restored.root.find(Panel::Properties.dock()).is_some());
                assert!(restored.root.find(Panel::Vector.dock()).is_some());
            }
            restored.redock(Panel::Masks.dock());
            assert!(restored.root.find(Panel::Masks.dock()).is_none());
            assert!(restored.root.find(Panel::Properties.dock()).is_some());
            assert!(restored.valid());
        }
        assert!(!Panel::available().any(|panel| panel == Panel::Masks));
        assert_eq!(Panel::from_dock(Panel::Masks.dock()), Some(Panel::Masks));
    }
    #[test]
    fn legacy_expression_layouts_redirect_to_properties_without_touching_project_data() {
        for floating in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("legacy-expression-layout.json");
            let mut workspace = Workspace::default();
            workspace.layout.redock(Panel::Expression.dock());
            if floating {
                assert!(workspace.layout.float(Panel::Expression.dock()));
            }
            workspace
                .preset_layouts
                .insert(Preset::All, SavedLayout::capture(&workspace.layout));
            workspace.save(&path).unwrap();
            let mut loaded = Workspace::load(&path);
            for preset in Preset::ALL {
                loaded.select_preset(preset);
                assert!(loaded.valid());
                assert!(loaded.root.find(Panel::Expression.dock()).is_none());
                assert!(!loaded.floating.contains(&Panel::Expression.dock()));
            }
            loaded.redock(Panel::Expression.dock());
            assert!(loaded.root.find(Panel::Expression.dock()).is_none());
            assert!(loaded.root.find(Panel::Properties.dock()).is_some());
        }
        assert!(!Panel::available().any(|p| p == Panel::Expression));
        assert_eq!(
            Panel::from_dock(Panel::Expression.dock()),
            Some(Panel::Expression)
        );
    }
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

#[cfg(test)]
mod restore_observation_tests {
    use super::*;
    #[test]
    fn invalid_saved_geometry_is_reported_and_does_not_replace_the_layout() {
        const CHILD: &str = "MOTION_SAVED_LAYOUT_FALLBACK_CHILD";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = std::path::PathBuf::from(root);
            let _logger = crate::logging::init(root.clone()).unwrap();
            let normal = SavedLayout::capture(&motion_ui::workspace::Workspace::default());
            let mut bad = normal.clone();
            bad.window_geometry.insert(2,WindowGeometry { size:[-1.,200.],position:None });
            for _ in 0..120 { assert!(bad.restore().is_none()); assert!(normal.restore().is_some()); }
            let text = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
            let records = text.lines().filter(|l| l.contains("site=workspace.preset.restore ")).collect::<Vec<_>>();
            assert_eq!(records.len(),2);
            assert!(records[1].contains("ERROR") && records[1].contains("invalid floating window size"));
            assert_eq!(bad.window_geometry[&2].size,[-1.,200.]);
            let mut invalid = normal.clone();
            invalid.root = Node::Dock { id:DockId(1),tabs:vec![DockId(1),DockId(1)],active:0 };
            assert!(invalid.restore().is_some());
            let mut workspace = Workspace::default();
            for _ in 0..120 {
                workspace.select_preset(Preset::Default);
                workspace.preset_layouts.insert(Preset::Nodes,invalid.clone());
                workspace.select_preset(Preset::Nodes);
                assert!(workspace.valid());
                assert!(normal.valid());
                assert_eq!(serde_json::to_value(&workspace.layout.root).unwrap(),serde_json::to_value(layout(Preset::Nodes).root).unwrap());
            }
            let text = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
            let records = text.lines().filter(|l| l.contains("site=workspace.preset.validate ")).collect::<Vec<_>>();
            assert_eq!(records.len(),2);
            assert!(records[1].contains("ERROR") && records[1].contains("reason=invalid_panel_ownership"));
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","workspace::presets::restore_observation_tests::invalid_saved_geometry_is_reported_and_does_not_replace_the_layout"]).env_remove("MOTION_LOG").env(CHILD,temp.path()).output().unwrap();
        assert!(output.status.success(),"{}\n{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stderr).contains("site=workspace.preset.restore"));
    }
}
