//! Project organization is independent of layer references and filesystem paths.
//! An item without a placement belongs to the root; folders never own resources.
use crate::{ensure, Error, Project, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const MAX_FOLDERS: usize = 512;
pub const MAX_FOLDER_DEPTH: usize = 16;
pub const MAX_LIBRARY_ITEMS: usize =
    crate::MAX_LAYERS * 2 + crate::composition::MAX_COMPOSITIONS + 1 + 128;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectItem {
    Composition { id: String },
    Image { id: u64 },
    Video { id: u64 },
    Audio { id: u64 },
    Model { id: u64 },
    Font { id: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectFolder {
    pub id: u64,
    pub parent: Option<u64>,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemPlacement {
    pub item: ProjectItem,
    pub folder: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectLibrary {
    pub folders: Vec<ProjectFolder>,
    pub placements: Vec<ItemPlacement>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum LibraryAction {
    CreateFolder {
        id: u64,
        parent: Option<u64>,
        name: String,
    },
    RenameFolder {
        id: u64,
        name: String,
    },
    MoveFolder {
        id: u64,
        parent: Option<u64>,
    },
    /// Remove only the container. Its children and items move to its parent.
    RemoveFolder {
        id: u64,
    },
    MoveItems {
        items: Vec<ProjectItem>,
        folder: Option<u64>,
    },
}

/// Includes all registered resources, even when no layer references them.
/// Switching the active composition does not change an item's identity.
pub fn items(project: &Project) -> Vec<ProjectItem> {
    std::iter::once(ProjectItem::Composition {
        id: project.composition_id.clone(),
    })
    .chain(
        project
            .compositions
            .iter()
            .map(|c| ProjectItem::Composition { id: c.id.clone() }),
    )
    .chain(
        project
            .assets
            .iter()
            .map(|a| ProjectItem::Image { id: a.id }),
    )
    .chain(
        project
            .video_assets
            .iter()
            .map(|a| ProjectItem::Video { id: a.id }),
    )
    .chain(
        project
            .audio_assets
            .iter()
            .map(|a| ProjectItem::Audio { id: a.id }),
    )
    .chain(
        project
            .model_assets
            .iter()
            .map(|a| ProjectItem::Model { id: a.id }),
    )
    .chain(
        project
            .fonts
            .iter()
            .map(|a| ProjectItem::Font { id: a.id.clone() }),
    )
    .collect()
}

fn valid_name(name: &str) -> Result<()> {
    ensure(
        !name.trim().is_empty() && name.len() <= 256 && !name.chars().any(char::is_control),
        "project folder name must contain 1..256 bytes without control characters",
    )
}

impl ProjectLibrary {
    pub fn folder_of(&self, item: &ProjectItem) -> Option<u64> {
        self.placements
            .iter()
            .find(|p| &p.item == item)
            .map(|p| p.folder)
    }

    pub fn validate(&self, project: &Project) -> Result<()> {
        ensure(
            self.folders.len() <= MAX_FOLDERS,
            "project folder limit exceeded",
        )?;
        ensure(
            self.placements.len() <= MAX_LIBRARY_ITEMS,
            "project placement limit exceeded",
        )?;
        let mut parents = HashMap::with_capacity(self.folders.len());
        for folder in &self.folders {
            ensure(
                folder.id != 0 && parents.insert(folder.id, folder.parent).is_none(),
                "project folder IDs must be unique and nonzero",
            )?;
            valid_name(&folder.name)?;
        }
        for folder in &self.folders {
            let mut parent = folder.parent;
            let mut seen = HashSet::from([folder.id]);
            while let Some(id) = parent {
                ensure(seen.insert(id), "project folders cannot contain a cycle")?;
                ensure(
                    seen.len() <= MAX_FOLDER_DEPTH,
                    "project folder depth limit exceeded",
                )?;
                parent = *parents.get(&id).ok_or_else(|| {
                    Error::Invalid(format!("project folder={} missing parent={id}", folder.id))
                })?;
            }
        }
        let available: HashSet<_> = items(project).into_iter().collect();
        let mut assigned = HashSet::with_capacity(self.placements.len());
        for placement in &self.placements {
            ensure(
                parents.contains_key(&placement.folder),
                "project placement folder is missing",
            )?;
            ensure(
                available.contains(&placement.item),
                "project placement item is missing",
            )?;
            ensure(
                assigned.insert(&placement.item),
                "project item has multiple placements",
            )?;
        }
        Ok(())
    }

    /// Called only by the successful resource/composition removal operation.
    /// Validation never prunes corrupt references or hides a missing item.
    pub(crate) fn remove_item(&mut self, item: &ProjectItem) {
        self.placements.retain(|p| &p.item != item);
    }
}

pub fn apply(project: &mut Project, action: LibraryAction) -> Result<()> {
    let (operation, folder) = match &action {
        LibraryAction::CreateFolder { id, .. } => ("create_folder", Some(*id)),
        LibraryAction::RenameFolder { id, .. } => ("rename_folder", Some(*id)),
        LibraryAction::MoveFolder { id, .. } => ("move_folder", Some(*id)),
        LibraryAction::RemoveFolder { id } => ("remove_folder", Some(*id)),
        LibraryAction::MoveItems { folder, .. } => ("move_items", *folder),
    };
    apply_inner(project, action).map_err(|source| Error::ProjectLibrary {
        operation,
        folder,
        source: Box::new(source),
    })
}

fn apply_inner(project: &mut Project, action: LibraryAction) -> Result<()> {
    project.library.validate(project)?;
    let mut library = project.library.clone();
    match action {
        LibraryAction::CreateFolder { id, parent, name } => {
            ensure(
                library.folders.len() < MAX_FOLDERS,
                "project folder limit exceeded",
            )?;
            library.folders.push(ProjectFolder { id, parent, name });
        }
        LibraryAction::RenameFolder { id, name } => {
            library
                .folders
                .iter_mut()
                .find(|f| f.id == id)
                .ok_or_else(|| Error::Invalid("project folder no longer exists".into()))?
                .name = name;
        }
        LibraryAction::MoveFolder { id, parent } => {
            library
                .folders
                .iter_mut()
                .find(|f| f.id == id)
                .ok_or_else(|| Error::Invalid("project folder no longer exists".into()))?
                .parent = parent;
        }
        LibraryAction::RemoveFolder { id } => {
            let parent = library
                .folders
                .iter()
                .find(|f| f.id == id)
                .ok_or_else(|| Error::Invalid("project folder no longer exists".into()))?
                .parent;
            library.folders.retain(|f| f.id != id);
            for child in &mut library.folders {
                if child.parent == Some(id) {
                    child.parent = parent;
                }
            }
            for placement in &mut library.placements {
                if placement.folder == id {
                    // Root placements are omitted below, never represented as folder 0.
                    placement.folder = parent.unwrap_or(0);
                }
            }
            library.placements.retain(|p| p.folder != 0);
        }
        LibraryAction::MoveItems {
            items: targets,
            folder,
        } => {
            ensure(
                !targets.is_empty() && targets.len() <= MAX_LIBRARY_ITEMS,
                "project item move requires a bounded nonempty selection",
            )?;
            let mut unique = HashSet::with_capacity(targets.len());
            let available: HashSet<_> = items(project).into_iter().collect();
            for target in &targets {
                ensure(
                    unique.insert(target),
                    "project item move contains duplicates",
                )?;
                ensure(available.contains(target), "project item no longer exists")?;
            }
            // Check the destination even when all selected items are already there.
            ensure(
                folder.is_none_or(|id| library.folders.iter().any(|f| f.id == id)),
                "project destination folder no longer exists",
            )?;
            if let Some(folder) = folder {
                // Keep stored order stable, including no-op moves to the same folder.
                for target in targets {
                    if let Some(placement) =
                        library.placements.iter_mut().find(|p| p.item == target)
                    {
                        placement.folder = folder;
                    } else {
                        library.placements.push(ItemPlacement {
                            item: target,
                            folder,
                        });
                    }
                }
            } else {
                library.placements.retain(|p| !unique.contains(&p.item));
            }
        }
    }
    library.validate(project)?;
    project.library = library;
    Ok(())
}
