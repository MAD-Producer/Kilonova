//! Project and session lifecycle.
use super::stamp;
use crate::platform::Platform;
use crate::session::Result;
use motion_core::Project;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

/// Explicit desktop choice; legacy hosts retain their existing save policy.
#[derive(Clone, Copy)]
pub enum PreviousProject {
    Save,
    Discard,
}

// Lives outside with_session: cleanup and its fallback observation must happen
// after that function releases the session registry lock, including unwinding.
#[derive(Default)]
struct PendingProjectDirectory(Option<(PathBuf, PathBuf)>);
impl PendingProjectDirectory {
    fn own(&mut self, base: PathBuf, path: PathBuf) {
        self.0 = Some((base, path));
    }
    fn commit(&mut self) {
        self.0 = None;
    }
    fn cleanup(&mut self) -> Result<()> {
        let Some((base, path)) = self.0.take() else { return Ok(()); };
        let resolved = match path.canonicalize() {
            Ok(resolved) => resolved,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(cause) => return Err(crate::Error::ProjectCleanup { operation: "resolve", path, cause }),
        };
        // Only this transaction's direct child of the canonical library is owned.
        // A substituted symlink/junction must not redirect recursive deletion.
        if resolved != path || resolved.parent() != Some(base.as_path()) {
            return Err(format!("phase=project_cleanup refused changed directory={} resolved={} library={}",
                path.display(), resolved.display(), base.display()).into());
        }
        match std::fs::remove_dir_all(&resolved) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(cause) => Err(crate::Error::ProjectCleanup { operation: "directory", path: resolved, cause }),
        }
    }
    fn finish<T>(mut self, result: Result<T>) -> Result<T> {
        match result {
            Ok(value) => { self.commit(); Ok(value) }
            Err(primary) => match self.cleanup() {
                Ok(()) => Err(primary),
                Err(secondary) => Err(crate::Error::Cleanup { primary: Box::new(primary), secondary: Box::new(secondary) }),
            },
        }
    }
}
impl Drop for PendingProjectDirectory {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup() {
            log::error!("project rollback during unwind failed: {error}");
        }
    }
}

fn save_previous(s: &crate::session::Session, choice: Option<PreviousProject>) -> Result<()> {
    let save = match choice {
        Some(PreviousProject::Save) => true,
        Some(PreviousProject::Discard) => false,
        None => match motion_core::storage::validate_assets(&s.root, s.engine.project()) {
            Ok(()) => true,
            Err(error) => {
                log::warn!("skip saving previous project: root={} reason={error}", s.root.display());
                false
            }
        },
    };
    if save {
        motion_core::storage::save(&s.root, s.engine.project()).map_err(crate::Error::from)?;
    }
    Ok(())
}

/// Directory names the project browser is allowed to open.
fn project_name_allowed(name: &str, current: &str) -> bool {
    name == current
        || name == "default"
        || ["project-", "import-"].iter().any(|prefix| {
            name.strip_prefix(*prefix).is_some_and(|suffix| {
                !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit())
            })
        })
}

pub fn create(root: PathBuf, project_text: &str, platform: Arc<dyn Platform>) -> Result<i64> {
    std::fs::create_dir_all(&root).map_err(crate::Error::from)?;
    let project = if project_text.is_empty() {
        if root.join("project.json").exists() {
            motion_core::storage::load(&root).map_err(crate::Error::from)?
        } else {
            Project::new(1080, 1920, 30, 180).map_err(crate::Error::from)?
        }
    } else {
        serde_json::from_str(project_text).map_err(crate::Error::from)?
    };
    crate::session::open(root, project, platform).map_err(|error| {
        crate::session::set_creation_error(error.to_string());
        error
    })
}

pub fn template(kind: i32) -> Result<Value> {
    if kind != 0 {
        return Err("unknown project template".into());
    }
    serde_json::to_value(Project::demo()).map_err(crate::Error::from)
}

pub fn state(id: i64) -> Result<Value> {
    crate::session::with_session(id, |s| Ok(s.snapshot()))
}

pub fn save(id: i64) -> Result<Value> {
    crate::session::with_session(id, |s| {
        motion_core::storage::save(&s.root, s.engine.project()).map_err(crate::Error::from)?;
        Ok(s.snapshot())
    })
}

pub fn new_project(id: i64, text: &str) -> Result<Value> {
    new_project_inner(id, text, None)
}

pub fn new_project_with_choice(id: i64, text: &str, choice: PreviousProject) -> Result<Value> {
    new_project_inner(id, text, Some(choice))
}

fn new_project_inner(id: i64, text: &str, choice: Option<PreviousProject>) -> Result<Value> {
    let project: Project = serde_json::from_str(text).map_err(crate::Error::from)?;
    if !project.assets.is_empty()
        || !project.audio_assets.is_empty()
        || !project.video_assets.is_empty()
    {
        return Err("new project must have no external assets".into());
    }
    let engine = motion_core::Engine::new(project).map_err(crate::Error::from)?;
    let mut pending = PendingProjectDirectory::default();
    let result = crate::session::with_session(id, |s| {
        let parent = s.root.parent().ok_or("project parent is missing")?
            .canonicalize().map_err(crate::Error::from)?;
        let destination = parent.join(format!("project-{}", stamp()?));
        save_previous(s, choice)?;
        std::fs::create_dir(&destination).map_err(crate::Error::from)?;
        pending.own(parent, destination.clone());
        motion_core::storage::save(&destination, engine.project()).map_err(crate::Error::from)?;
        s.replace_project(engine, destination)?;
        pending.commit();
        Ok(s.snapshot())
    });
    pending.finish(result)
}

pub fn open_project(id: i64, name: &str) -> Result<Value> {
    crate::session::with_session(id, |s| {
        let current = s
            .root
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("invalid active project directory")?;
        if !project_name_allowed(name, current) || name.contains('/') || name.contains('\\') {
            return Err("invalid project directory".into());
        }
        let parent = s
            .root
            .parent()
            .ok_or("project parent is missing")?
            .canonicalize()
            .map_err(crate::Error::from)?;
        let destination = parent
            .join(name)
            .canonicalize()
            .map_err(crate::Error::from)?;
        if destination.parent() != Some(parent.as_path()) {
            return Err("project resolves outside its library".into());
        }
        let project = motion_core::storage::load(&destination).map_err(crate::Error::from)?;
        let engine = motion_core::Engine::new(project).map_err(crate::Error::from)?;
        save_previous(s, None)?;
        s.replace_project(engine, destination)?;
        Ok(s.snapshot())
    })
}

pub fn replace(id: i64, text: &str) -> Result<Value> {
    let project: Project = serde_json::from_str(text).map_err(crate::Error::from)?;
    crate::session::with_session(id, |s| {
        motion_core::storage::validate_assets(&s.root, &project).map_err(crate::Error::from)?;
        let engine = motion_core::Engine::new(project).map_err(crate::Error::from)?;
        s.replace_project(engine, s.root.clone())?;
        Ok(s.snapshot())
    })
}

/// Open an explicitly selected desktop project directory. Loading and asset
/// validation finish before the current project is saved or replaced.
pub fn open_directory(id: i64, directory: &std::path::Path) -> Result<Value> {
    open_directory_with_choice(id, directory, PreviousProject::Save)
}

pub fn open_directory_with_choice(id: i64, directory: &std::path::Path, choice: PreviousProject) -> Result<Value> {
    let destination = directory.canonicalize().map_err(crate::Error::from)?;
    let project = motion_core::storage::load(&destination).map_err(crate::Error::from)?;
    let engine = motion_core::Engine::new(project).map_err(crate::Error::from)?;
    crate::session::with_session(id, |s| {
        save_previous(s, Some(choice))?;
        s.replace_project(engine, destination)?;
        Ok(s.snapshot())
    })
}

pub fn import_project(id: i64, file: &std::path::Path) -> Result<Value> {
    import_project_with_choice(id, file, PreviousProject::Save)
}

pub fn import_project_with_choice(id: i64, file: &std::path::Path, choice: PreviousProject) -> Result<Value> {
    let mut pending = PendingProjectDirectory::default();
    let result = crate::session::with_session(id, |s| {
        let parent = s.root.parent().ok_or("project parent is missing")?
            .canonicalize().map_err(crate::Error::from)?;
        let destination = parent.join(format!("import-{}", stamp()?));
        let project =
            motion_core::storage::import_package(file, &destination).map_err(crate::Error::from)?;
        pending.own(parent, destination.clone());
        let engine = motion_core::Engine::new(project).map_err(crate::Error::from)?;
        save_previous(s, Some(choice))?;
        s.replace_project(engine, destination)?;
        pending.commit();
        Ok(s.snapshot())
    });
    pending.finish(result)
}

#[cfg(test)]
#[path = "project_cleanup_tests.rs"]
mod cleanup_tests;
