//! A bounded UI projection. Do not deserialize layer animations while painting.
use super::*;
use motion_core::project_library::{ProjectFolder, ProjectItem, ProjectLibrary};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum RowKey {
    Folder(u64),
    Item(ProjectItem),
}
#[derive(Clone)]
pub(super) struct Entry {
    pub key: RowKey,
    pub name: String,
    pub folder: Option<u64>,
}
pub(super) struct Catalogue {
    pub source: crate::layer_clipboard::KeyframeSource,
    pub library: ProjectLibrary,
    pub entries: Vec<Entry>,
}
impl Catalogue {
    pub fn read(state: &Value) -> Result<Self, String> {
        let p = &state["project"];
        let library: ProjectLibrary =
            serde_json::from_value(p["library"].clone()).map_err(|error| {
                format!(
                    "project library metadata phase=decode: {}",
                    crate::fallback_observation::short(error)
                )
            })?;
        let mut entries = Vec::new();
        let id = p["composition_id"]
            .as_str()
            .ok_or("project composition id missing")?;
        entries.push(Entry {
            key: RowKey::Item(ProjectItem::Composition { id: id.into() }),
            name: p["name"].as_str().unwrap_or(id).into(),
            folder: None,
        });
        for c in p["compositions"].as_array().into_iter().flatten() {
            let id = c["id"].as_str().ok_or("library composition id missing")?;
            entries.push(Entry {
                key: RowKey::Item(ProjectItem::Composition { id: id.into() }),
                name: c["name"].as_str().unwrap_or(id).into(),
                folder: None,
            });
        }
        for (field, kind) in [
            ("assets", 0),
            ("video_assets", 1),
            ("audio_assets", 2),
            ("model_assets", 3),
        ] {
            for asset in p[field].as_array().into_iter().flatten() {
                let id = asset["id"].as_u64().ok_or("library asset id missing")?;
                let item = match kind {
                    0 => ProjectItem::Image { id },
                    1 => ProjectItem::Video { id },
                    2 => ProjectItem::Audio { id },
                    _ => ProjectItem::Model { id },
                };
                let path = asset["path"].as_str().unwrap_or("");
                let name = path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned();
                entries.push(Entry {
                    key: RowKey::Item(item),
                    name,
                    folder: None,
                });
            }
        }
        for font in p["fonts"].as_array().into_iter().flatten() {
            let id = font["id"].as_str().ok_or("library font id missing")?;
            entries.push(Entry {
                key: RowKey::Item(ProjectItem::Font { id: id.into() }),
                name: font["name"].as_str().unwrap_or(id).into(),
                folder: None,
            });
        }
        for entry in &mut entries {
            if let RowKey::Item(item) = &entry.key {
                entry.folder = library.folder_of(item);
            }
        }
        let mut folders: Vec<_> = library
            .folders
            .iter()
            .map(|f| Entry {
                key: RowKey::Folder(f.id),
                name: f.name.clone(),
                folder: f.parent,
            })
            .collect();
        folders.extend(entries);
        Ok(Self {
            source: crate::layer_clipboard::KeyframeSource::of(state),
            library,
            entries: folders,
        })
    }
    pub fn folder(&self, id: u64) -> Option<&ProjectFolder> {
        self.library.folders.iter().find(|f| f.id == id)
    }
    pub fn visible(&self, view: &ViewPreferences, query: &str) -> Vec<Entry> {
        let query = query.to_lowercase();
        self.entries
            .iter()
            .filter(|entry| {
                (entry.folder == view.folder || view.search_all && !query.is_empty())
                    && (entry.name.to_lowercase().contains(&query)
                        || entry.name == "Untitled Project"
                            && [Locale::Zh, Locale::En].iter().any(|locale| {
                                locale.t(T::Untitled).to_lowercase().contains(&query)
                            }))
            })
            .cloned()
            .collect()
    }
    pub fn breadcrumb(&self, folder: Option<u64>, locale: Locale) -> String {
        let mut names = Vec::new();
        let mut next = folder;
        for _ in 0..motion_core::project_library::MAX_FOLDER_DEPTH {
            let Some(folder) = next.and_then(|id| self.folder(id)) else {
                break;
            };
            names.push(folder.name.as_str());
            next = folder.parent;
        }
        names.push(word(locale, "项目", "Project"));
        names.reverse();
        names.join(" / ")
    }
    pub fn allows_move(&self, key: &RowKey, folder: Option<u64>) -> bool {
        let RowKey::Folder(id) = key else { return true };
        let mut next = folder;
        for _ in 0..=motion_core::project_library::MAX_FOLDER_DEPTH {
            let Some(parent) = next else { return true };
            if parent == *id {
                return false;
            }
            next = self.folder(parent).and_then(|f| f.parent);
        }
        false
    }
}
impl RowKey {
    pub fn media(&self) -> Option<motion_core::media_assets::AssetRef> {
        use motion_core::media_assets::{AssetKind, AssetRef};
        Some(match self {
            Self::Item(ProjectItem::Image { id }) => AssetRef {
                kind: AssetKind::Image,
                id: *id,
            },
            Self::Item(ProjectItem::Video { id }) => AssetRef {
                kind: AssetKind::Video,
                id: *id,
            },
            Self::Item(ProjectItem::Audio { id }) => AssetRef {
                kind: AssetKind::Audio,
                id: *id,
            },
            _ => return None,
        })
    }
    pub fn caption(&self, locale: Locale) -> &'static str {
        match self {
            Self::Folder(_) => word(locale, "文件夹", "Folder"),
            Self::Item(ProjectItem::Composition { .. }) => locale.t(T::Composition),
            Self::Item(ProjectItem::Image { .. }) => word(locale, "图片", "Image"),
            Self::Item(ProjectItem::Video { .. }) => word(locale, "视频", "Video"),
            Self::Item(ProjectItem::Audio { .. }) => word(locale, "音频", "Audio"),
            Self::Item(ProjectItem::Model { .. }) => word(locale, "模型", "Model"),
            Self::Item(ProjectItem::Font { .. }) => word(locale, "字体", "Font"),
        }
    }
    pub fn icon(&self) -> motion_ui::icons::Icon {
        use motion_ui::icons::Icon;
        match self {
            Self::Folder(_) => Icon::Folder,
            Self::Item(ProjectItem::Composition { .. }) => Icon::Composition,
            Self::Item(ProjectItem::Video { .. }) => Icon::Video,
            Self::Item(ProjectItem::Audio { .. }) => Icon::Audio,
            _ => Icon::Image,
        }
    }
}
