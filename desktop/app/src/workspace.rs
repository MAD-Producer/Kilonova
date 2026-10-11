//! Application preferences wrap the reusable UI workspace; no editor state is stored here.
pub use motion_ui::workspace::Change;
use motion_ui::{
    dock::{self, DockId, Edge},
    input::{Input, Rect},
};
use std::path::Path;
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Workspace {
    #[serde(default)]
    pub locale: crate::i18n::Locale,
    #[serde(flatten)]
    layout: motion_ui::workspace::Workspace,
}
impl Default for Workspace {
    fn default() -> Self {
        Self {
            locale: Default::default(),
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
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .filter(|bytes| bytes.len() <= 64 * 1024)
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .filter(|w| {
                w.layout.valid()
                    && w.owned_panels().iter().all(|id| (1..=4).contains(&id.0))
                    && (1..=4).all(|id| w.owned_panels().contains(&DockId(id)))
            })
            .unwrap_or_default()
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
    pub fn redock(&mut self, panel: DockId) {
        if self.layout.root.find(panel).is_some() {
            return;
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
        self.layout.interact(
            input,
            Rect::new(
                0.,
                crate::panels::CHROME_HEIGHT,
                size[0],
                (size[1] - crate::panels::CHROME_HEIGHT - 22.).max(0.),
            ),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
