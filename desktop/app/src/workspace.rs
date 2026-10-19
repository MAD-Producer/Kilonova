//! Application preferences wrap the reusable UI workspace; no editor state is stored here.
mod manager;
mod presets;
mod maximize;
pub(crate) mod preferences;
pub use manager::{Manager, ManagerEvent};
pub use motion_ui::workspace::{Change, WindowGeometry};
use motion_ui::{
    dock::{self, DockId, Edge},
    input::{Input, Rect},
};
pub use presets::{Preset, SavedLayout};
use std::path::Path;
fn visible_column() -> bool {
    true
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct RecentProject {
    pub root: std::path::PathBuf,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Workspace {
    /// Temporary display projection; the owned layout remains complete.
    #[serde(skip)]
    maximized: Option<DockId>,
    #[serde(default)]
    pub locale: crate::i18n::Locale,
    #[serde(default)]
    pub preset: Preset,
    #[serde(default)]
    pub recent_projects: Vec<RecentProject>,
    #[serde(default)]
    design_revision: u32,
    #[serde(default)]
    preset_layouts: std::collections::BTreeMap<Preset, SavedLayout>,
    #[serde(default)]
    legacy_layout: Option<SavedLayout>,
    #[serde(default)]
    pub named_layouts: std::collections::BTreeMap<String, SavedLayout>,
    #[serde(default)]
    pub style_path: Option<std::path::PathBuf>,
    #[serde(default)]
    pub(crate) preview_quality: preferences::PreviewQuality,
    #[serde(default)]
    pub favorites: std::collections::BTreeSet<String>,
    #[serde(default)]
    pub send_diagnostics: bool,
    #[serde(default)]
    pub node_view: crate::node_editor::ViewPreferences,
    #[serde(default)]
    pub node_recents: crate::node_editor::RecentNodes,
    #[serde(default = "visible_column")]
    pub timeline_column_blend: bool,
    #[serde(default = "visible_column")]
    pub timeline_column_parent: bool,
    #[serde(default = "visible_column")]
    pub timeline_column_matte: bool,
    #[serde(flatten)]
    layout: motion_ui::workspace::Workspace,
}
impl Default for Workspace {
    fn default() -> Self {
        Self {
            maximized: None,
            locale: Default::default(),
            preset: Default::default(),
            recent_projects: vec![],
            design_revision: 2,
            preset_layouts: Default::default(),
            legacy_layout: None,
            named_layouts: Default::default(),
            favorites: Default::default(),
            send_diagnostics: false,
            node_view: Default::default(),
            node_recents: Default::default(),
            timeline_column_blend: true,
            timeline_column_parent: true,
            timeline_column_matte: true,
            style_path: None,
            preview_quality: Default::default(),
            layout: Default::default(),
        }
    }
}
impl std::ops::Deref for Workspace {
    type Target = motion_ui::workspace::Workspace;
    fn deref(&self) -> &Self::Target {
        &self.layout
    }
}
impl std::ops::DerefMut for Workspace {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.layout
    }
}
impl Workspace {
    pub fn column_visibility(&self) -> crate::panels::ColumnVisibility {
        crate::panels::ColumnVisibility {
            blend: self.timeline_column_blend,
            parent: self.timeline_column_parent,
            matte: self.timeline_column_matte,
        }
    }
    pub fn set_column_visibility(&mut self, visibility: crate::panels::ColumnVisibility) {
        self.timeline_column_blend = visibility.blend;
        self.timeline_column_parent = visibility.parent;
        self.timeline_column_matte = visibility.matte;
    }
    pub fn valid(&self) -> bool {
        self.layout.valid()
            && self
                .owned_panels()
                .iter()
                .all(|id| dock::Panel::from_dock(*id).is_some())
            && self
                .window_geometry
                .keys()
                .all(|id| dock::Panel::from_dock(DockId(*id)).is_some())
            && (1..=4).all(|id| self.owned_panels().contains(&DockId(id)))
    }
    pub fn load(path: &Path) -> Self {
        static FAILURES: crate::fallback_observation::Counter = crate::fallback_observation::Counter::new();
        let began = std::time::Instant::now();
        let result = (|| {
            let bytes = std::fs::read(path).map_err(|e| (if e.kind() == std::io::ErrorKind::NotFound { "missing" } else { "read" }, e.to_string()))?;
            if bytes.len() > 64 * 1024 { return Err(("size", format!("bytes={} limit=65536", bytes.len()))); }
            let mut workspace: Self = serde_json::from_slice(&bytes).map_err(|e| ("json", e.to_string()))?;
            if !workspace.valid() { return Err(("validation", "invalid application workspace ownership".into())); }
            workspace.migrate_design();
            Ok(workspace)
        })();
        match result {
            Ok(workspace) => { FAILURES.success(); workspace }
            Err(("missing", _)) => {
                FAILURES.success(); Self::default()
            }
            Err((phase, reason)) => {
                FAILURES.failure("workspace.load", format_args!("phase={phase} elapsed_ms={} path={} reason={reason} fallback=default", began.elapsed().as_millis(), crate::fallback_observation::short(path.display())));
                Self::default()
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        use std::{io::Write, sync::atomic::{AtomicU64, Ordering}};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        let phase = |phase, cause: std::io::Error| format!("save application settings phase={phase} path={} cause={cause}", path.display());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| phase("create_directory", e))?;
        }
        let mut name = std::ffi::OsString::from(".");
        name.push(path.file_name().ok_or("application settings path has no file name")?);
        name.push(format!(".{}-{}.tmp", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        let temporary = path.with_file_name(name);
        let mut owned = crate::owned_file::OwnedFile::create(&temporary).map_err(|e| phase("create_temporary", e))?;
        owned.file().write_all(&bytes).map_err(|e| phase("write", e))?;
        owned.file().sync_all().map_err(|e| phase("sync_file", e))?;
        owned.close();
        std::fs::rename(&temporary, path).map_err(|e| phase("publish", e))?;
        owned.keep();
        Ok(())
    }
    pub fn redock(&mut self, panel: DockId) {
        self.restore_panel();
        let panel = if matches!(panel, id if id == dock::Panel::Masks.dock() || id == dock::Panel::Expression.dock())
        {
            dock::Panel::Properties.dock()
        } else {
            panel
        };
        if self.layout.root.find(panel).is_some() {
            return;
        }
        let targets = if matches!(panel.0, 10 | 19 | 21) {
            vec![DockId(3)]
        } else if panel.0 > 4 {
            vec![DockId(11), DockId(2)]
        } else if panel.0 == 2 {
            vec![DockId(11)]
        } else {
            vec![]
        };
        for target in targets {
            if self.layout.root.find(target).is_some() {
                self.layout.redock_to(panel, target, None);
                return;
            }
        }
        let target = dock::solve(&self.layout.root, Rect::new(0., 0., 1000., 800.))
            .first()
            .map(|p| p.id);
        let edge = match panel.0 {
            1 => Edge::Left,
            2 => Edge::Right,
            4 => Edge::Bottom,
            _ => Edge::Left,
        };
        if let Some(target) = target {
            self.layout.redock_to(panel, target, Some(edge));
        } else {
            self.layout.redock(panel);
        }
    }
    pub fn interact(&mut self, input: &Input, size: [f32; 2]) -> Option<Change> {
        // The projected dock has no splitters or other docks to drag into.
        if self.maximized_panel().is_some() {
            return None;
        }
        self.layout.interact_with_header_actions(
            input,
            Rect::new(
                0.,
                crate::panels::CHROME_HEIGHT,
                size[0],
                (size[1] - crate::panels::CHROME_HEIGHT - 20.).max(0.),
            ),
            0.,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn node_view_preferences_save_reopen_and_accept_legacy_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspace.json");
        let mut workspace = Workspace::default();
        workspace.node_view = crate::node_editor::ViewPreferences { grid:false, minimap:true };
        workspace.save(&path).unwrap();
        let reopened = Workspace::load(&path);
        assert!(reopened.valid());
        assert_eq!(reopened.node_view, workspace.node_view);
        let mut legacy: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        legacy.as_object_mut().unwrap().remove("node_view");
        std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
        assert_eq!(Workspace::load(&path).node_view, crate::node_editor::ViewPreferences::default());
    }
    #[test]
    fn timeline_column_visibility_defaults_visible_for_legacy_json_and_round_trips() {
        let mut legacy = serde_json::to_value(Workspace::default()).unwrap();
        for key in [
            "timeline_column_blend",
            "timeline_column_parent",
            "timeline_column_matte",
        ] {
            legacy.as_object_mut().unwrap().remove(key);
        }
        let parsed: Workspace = serde_json::from_value(legacy).unwrap();
        assert_eq!(
            parsed.column_visibility(),
            crate::panels::ColumnVisibility::default()
        );
        let mut explicit = serde_json::to_value(Workspace::default()).unwrap();
        explicit["timeline_column_blend"] = serde_json::Value::Bool(false);
        explicit["timeline_column_matte"] = serde_json::Value::Bool(false);
        let parsed: Workspace = serde_json::from_value(explicit).unwrap();
        assert_eq!(
            parsed.column_visibility(),
            crate::panels::ColumnVisibility {
                blend: false,
                parent: true,
                matte: false,
            }
        );
        let mut workspace = Workspace::default();
        workspace.set_column_visibility(crate::panels::ColumnVisibility {
            blend: false,
            parent: true,
            matte: false,
        });
        let json = serde_json::to_string(&workspace).unwrap();
        let loaded: Workspace = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.column_visibility(), workspace.column_visibility());
    }
    #[test]
    fn all_registered_panels_keep_unique_ownership_through_close_float_and_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspace.json");
        let mut workspace = Workspace::default();
        for panel in dock::Panel::available() {
            let id = panel.dock();
            workspace.redock(id);
            assert!(workspace.float(id));
            assert!(workspace.valid());
            workspace.save(&path).unwrap();
            workspace = Workspace::load(&path);
            assert!(workspace.floating.contains(&id));
            workspace.redock(id);
            assert!(workspace.close(id));
            assert!(workspace.valid());
            workspace.save(&path).unwrap();
            workspace = Workspace::load(&path);
            assert!(workspace.hidden.contains(&id));
            workspace.redock(id);
            assert!(workspace.valid());
            assert!(!workspace.hidden.contains(&id));
        }
    }
    #[test]
    fn floating_geometry_round_trips_without_accepting_duplicate_or_unknown_panels() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspace.json");
        let mut workspace = Workspace::default();
        workspace.window_geometry.insert(
            19,
            WindowGeometry {
                size: [800., 500.],
                position: Some([-1200, 40]),
            },
        );
        workspace.save(&path).unwrap();
        let loaded = Workspace::load(&path);
        assert_eq!(loaded.window_geometry[&19].position, Some([-1200, 40]));
        workspace.floating.push(DockId(1));
        assert!(!workspace.valid());
        workspace.floating = vec![DockId(999)];
        assert!(!workspace.valid());
    }
    #[test]
    fn optional_nodes_tab_survives_save_float_and_redock() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("nodes-layout.json");
        let mut workspace = Workspace::default();
        assert!(workspace.valid());
        workspace.redock(dock::Panel::Nodes.dock());
        assert!(workspace.valid());
        assert!(workspace.root.find(DockId(10)).is_some());
        workspace.save(&path).unwrap();
        let mut loaded = Workspace::load(&path);
        assert!(loaded.root.find(DockId(10)).is_some());
        assert!(loaded.float(DockId(10)));
        assert!(loaded.valid());
        loaded.save(&path).unwrap();
        assert!(Workspace::load(&path).floating.contains(&DockId(10)));
        loaded.redock(DockId(10));
        assert!(loaded.valid());
        let owner = loaded.root.find(DockId(3)).unwrap();
        assert!(matches!(owner,motion_ui::dock::Node::Dock{tabs,..} if tabs.contains(&DockId(10))));
    }
    #[test]
    fn every_panel_keeps_one_owner_after_dock_float_and_close() {
        let mut w = Workspace::default();
        assert!(w.valid());
        assert!(w.move_panel(DockId(2), DockId(1), None));
        assert!(w.valid());
        assert!(w.float(DockId(3)));
        assert!(w.valid());
        assert!(!w.float(DockId(3)));
        w.redock(DockId(3));
        assert!(w.valid());
        assert_eq!(w.floating.len(), 0);
        for p in 1..=4 {
            assert!(w.float(DockId(p)));
        }
        assert!(w.valid());
        for p in 1..=4 {
            w.redock(DockId(p));
            assert!(w.valid());
        }
    }
    #[test]
    fn layout_round_trips_and_invalid_preferences_reset() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("layout.json");
        let mut w = Workspace::default();
        w.float(DockId(2));
        w.save(&path).unwrap();
        let loaded = Workspace::load(&path);
        assert_eq!(loaded.floating, vec![DockId(2)]);
        assert!(loaded.valid());
        std::fs::write(&path, b"{}").unwrap();
        assert!(Workspace::load(&path).valid());
    }
}
