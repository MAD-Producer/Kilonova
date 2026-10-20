//! Editable application preferences; project documents never contain these fields.
use super::Workspace;
use crate::{i18n::Locale, node_editor::ViewPreferences, panels::ColumnVisibility};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PreviewQuality {
    #[default]
    Auto,
    High,
    Balanced,
    Economy,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preference_candidate_retains_layout_and_unrelated_settings_and_legacy_defaults() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("settings.json");
        let mut workspace = Workspace::default();
        workspace.favorites.insert("test.favorite".into());
        workspace.send_diagnostics = true;
        let original = serde_json::to_value(&workspace).unwrap();
        let mut values = workspace.preferences();
        values.preview = PreviewQuality::Economy;
        values.node_view.grid = false;
        values.columns.parent = false;
        values.locale = Locale::Zh;
        workspace.save_preferences(&path, &values).unwrap();
        assert_eq!(
            serde_json::to_value(&workspace).unwrap(),
            original,
            "candidate save must not mutate live settings"
        );
        let reopened = Workspace::load(&path);
        assert_eq!(reopened.preferences(), values);
        assert_eq!(reopened.favorites, workspace.favorites);
        assert!(reopened.send_diagnostics && reopened.valid());
        let mut expected = original.clone();
        let mut actual = serde_json::to_value(&reopened).unwrap();
        for key in [
            "preview_quality",
            "locale",
            "node_view",
            "timeline_column_parent",
        ] {
            expected.as_object_mut().unwrap().remove(key);
            actual.as_object_mut().unwrap().remove(key);
        }
        assert_eq!(expected, actual);
        let mut legacy = original;
        legacy.as_object_mut().unwrap().remove("preview_quality");
        std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
        assert_eq!(Workspace::load(&path).preview_quality, PreviewQuality::Auto);
        legacy["preview_quality"] = "unsupported_quality".into();
        let invalid = serde_json::to_vec(&legacy).unwrap();
        std::fs::write(&path, &invalid).unwrap();
        assert_eq!(Workspace::load(&path).preview_quality, PreviewQuality::Auto);
        assert_eq!(
            std::fs::read(&path).unwrap(),
            invalid,
            "invalid input is preserved for diagnosis"
        );
    }

    #[test]
    fn failed_publication_preserves_destination_and_removes_only_owned_temporary() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("settings.json");
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("keep"), b"original").unwrap();
        let error = Workspace::default().save(&path).unwrap_err();
        assert!(error.contains("phase=publish"), "{error}");
        assert_eq!(std::fs::read(path.join("keep")).unwrap(), b"original");
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 1);
        std::fs::remove_file(path.join("keep")).unwrap();
        std::fs::remove_dir(&path).unwrap();
        let mut workspace = Workspace::default();
        workspace.save(&path).unwrap();
        workspace.preview_quality = PreviewQuality::Balanced;
        workspace.save(&path).unwrap();
        assert_eq!(
            Workspace::load(&path).preview_quality,
            PreviewQuality::Balanced
        );
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn locked_existing_settings_remain_byte_identical_when_replace_is_denied() {
        use std::os::windows::fs::OpenOptionsExt;
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("settings.json");
        let mut workspace = Workspace::default();
        workspace.save(&path).unwrap();
        let before = std::fs::read(&path).unwrap();
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        workspace.preview_quality = PreviewQuality::Economy;
        let error = workspace.save(&path).unwrap_err();
        assert!(error.contains("phase=publish"), "{error}");
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 1);
        drop(lock);
        workspace.save(&path).unwrap();
        assert_eq!(
            Workspace::load(&path).preview_quality,
            PreviewQuality::Economy
        );
    }
}
impl PreviewQuality {
    pub(crate) fn id(self) -> i32 {
        self as i32
    }
    pub(crate) fn from_id(id: i32) -> Result<Self, String> {
        match id {
            0 => Ok(Self::Auto),
            1 => Ok(Self::High),
            2 => Ok(Self::Balanced),
            3 => Ok(Self::Economy),
            _ => Err("invalid application preview quality".into()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct Preferences {
    pub locale: Locale,
    pub preview: PreviewQuality,
    pub node_view: ViewPreferences,
    pub columns: ColumnVisibility,
    pub style_path: Option<PathBuf>,
    pub shortcuts: crate::shortcuts::bindings::Bindings,
}
impl Workspace {
    pub(crate) fn preferences(&self) -> Preferences {
        Preferences {
            locale: self.locale,
            preview: self.preview_quality,
            node_view: self.node_view,
            columns: self.column_visibility(),
            style_path: self.style_path.clone(),
            shortcuts: self.shortcut_bindings.clone(),
        }
    }
    pub(crate) fn set_preferences(&mut self, preferences: &Preferences) {
        self.locale = preferences.locale;
        self.preview_quality = preferences.preview;
        self.node_view = preferences.node_view;
        self.set_column_visibility(preferences.columns);
        self.style_path.clone_from(&preferences.style_path);
        self.shortcut_bindings.clone_from(&preferences.shortcuts);
    }
    pub(crate) fn save_preferences(
        &self,
        path: &Path,
        preferences: &Preferences,
    ) -> Result<(), String> {
        // Save a candidate without changing the live dock, its drag state or settings.
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        let mut candidate: Workspace = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        candidate.set_preferences(preferences);
        candidate.save(path)
    }
}
