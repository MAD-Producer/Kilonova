//! Managed original publication. Own Prepared outside any Session/business guard:
//! rollback, including its unwind observation, must run after that guard exits.
use super::{decode, project::Attachment, source};
use motion_core::{
    media_assets::{self, AssetKind, AssetRef},
    storage, Engine, Project, TrackingAsset,
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Attachment(#[from] super::project::Error),
    #[error(transparent)]
    Model(#[from] motion_core::Error),
    #[error("tracking import phase={phase} path={} cause={cause}", .path.display())]
    Io {
        phase: &'static str,
        path: PathBuf,
        #[source]
        cause: std::io::Error,
    },
    #[error("tracking import: {0}")]
    Invalid(&'static str),
    #[error("tracking import cancelled")]
    Cancelled,
    #[error("tracking import superseded by a project edit")]
    Superseded,
    #[error("{primary}; original-file rollback also failed: {secondary}")]
    Cleanup {
        #[source]
        primary: Box<Error>,
        secondary: Box<Error>,
    },
}
fn io(phase: &'static str, path: &Path, cause: std::io::Error) -> Error {
    Error::Io {
        phase,
        path: path.into(),
        cause,
    }
}
fn require(value: bool, cause: &'static str) -> Result<(), Error> {
    if value {
        Ok(())
    } else {
        Err(Error::Invalid(cause))
    }
}
fn cancelled(check: &mut impl FnMut() -> bool) -> Result<(), Error> {
    if check() {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}

struct OwnedOriginal {
    path: Option<PathBuf>,
    root: PathBuf,
    // Once save is attempted an unwind may follow publication. Retain the file
    // unless a normal error proves publication did not occur; never break reopen.
    rollback: bool,
}
impl OwnedOriginal {
    fn cleanup(&mut self) -> Result<(), Error> {
        let Some(path) = self.path.take() else {
            return Ok(());
        };
        if !self.rollback {
            return Ok(());
        }
        let resolved = match path.canonicalize() {
            Ok(path) => path,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(io("rollback_resolve", &path, e)),
        };
        require(
            resolved == path && resolved.starts_with(&self.root),
            "rollback refused a changed original-file path",
        )?;
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io("rollback_original", &path, e)),
        }
    }
    fn finish<T>(&mut self, result: Result<T, Error>) -> Result<T, Error> {
        match result {
            Ok(value) => {
                self.path = None;
                Ok(value)
            }
            Err(primary) => match self.cleanup() {
                Ok(()) => Err(primary),
                Err(secondary) => Err(Error::Cleanup {
                    primary: Box::new(primary),
                    secondary: Box::new(secondary),
                }),
            },
        }
    }
}
impl Drop for OwnedOriginal {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup() {
            // The enclosing operation owns this outside with_session.
            log::error!("tracking import unwind rollback failed phase=rollback cause={error}");
        }
    }
}

/// Captures author state, never GPU/session objects. Rejected/stale preparations
/// remove only their exclusively created original file; verified reused files survive.
pub struct Prepared {
    root: PathBuf,
    expected: Project,
    revision: u64,
    attachment: Attachment,
    owned: OwnedOriginal,
}
impl Prepared {
    pub fn read(
        root: &Path,
        project: &Project,
        revision: u64,
        document: &Path,
        selected: AssetRef,
        mut check: impl FnMut() -> bool,
    ) -> Result<Self, Error> {
        cancelled(&mut check)?;
        project.validate()?;
        require(
            !document.as_os_str().is_empty()
                && document.as_os_str().to_string_lossy().len() <= 4096,
            "original document path must be bounded and nonempty",
        )?;
        let root = root.canonicalize().map_err(|e| io("root", root, e))?;
        let mut file = File::open(document).map_err(|e| io("open_document", document, e))?;
        let before = file
            .metadata()
            .map_err(|e| io("document_metadata", document, e))?;
        let limit = motion_core::tracking_assets::MAX_TRACKING_ASSET_BYTES;
        require(
            before.is_file() && before.len() > 0 && before.len() <= limit,
            "original document exceeds file budget",
        )?;
        let mut original = Vec::new();
        let mut buffer = [0; storage::TRANSFER_BUFFER_BYTES];
        loop {
            cancelled(&mut check)?;
            let n = match file.read(&mut buffer) {
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(io("read_document", document, e)),
            };
            if n == 0 {
                break;
            }
            require(
                (original.len() as u64).saturating_add(n as u64) <= limit,
                "original document grew beyond budget",
            )?;
            original.extend_from_slice(&buffer[..n]);
        }
        let after = file
            .metadata()
            .map_err(|e| io("document_metadata_after", document, e))?;
        require(
            before.len() == original.len() as u64
                && after.len() == before.len()
                && before
                    .modified()
                    .map_err(|e| io("document_modified", document, e))?
                    == after
                        .modified()
                        .map_err(|e| io("document_modified_after", document, e))?,
            "original document changed during read",
        )?;
        let solve = decode::from_slice(&original).map_err(super::project::Error::Decode)?;
        let media = media_assets::lookup(project, selected)
            .ok_or(Error::Invalid("selected original source is missing"))?;
        require(
            matches!(selected.kind, AssetKind::Image | AssetKind::Video),
            "tracking plate must be image or video",
        )?;
        let path = root
            .join(media.path())
            .canonicalize()
            .map_err(|e| io("resolve_source", Path::new(media.path()), e))?;
        require(
            path.starts_with(&root),
            "original source resolves outside project",
        )?;
        let mut plate = File::open(&path).map_err(|e| io("open_source", &path, e))?;
        let before = plate
            .metadata()
            .map_err(|e| io("source_metadata", &path, e))?;
        require(
            before.is_file() && before.len() > 0 && before.len() <= storage::MAX_MEDIA_ASSET,
            "original source exceeds file budget",
        )?;
        let proof =
            source::verify(&solve, &mut plate, before.len(), &mut check).map_err(|cause| {
                if matches!(cause, source::Error::Cancelled) {
                    Error::Cancelled
                } else {
                    Error::Attachment(super::project::Error::Source(cause))
                }
            })?;
        let after = plate
            .metadata()
            .map_err(|e| io("source_metadata_after", &path, e))?;
        require(
            before.len() == after.len()
                && before
                    .modified()
                    .map_err(|e| io("source_modified", &path, e))?
                    == after
                        .modified()
                        .map_err(|e| io("source_modified_after", &path, e))?,
            "original source changed during verification",
        )?;
        drop(solve); // Do not retain two decoded sample arrays during preparation.
        let id = project
            .assets
            .iter()
            .map(|a| a.id)
            .chain(project.audio_assets.iter().map(|a| a.id))
            .chain(project.video_assets.iter().map(|a| a.id))
            .chain(project.model_assets.iter().map(|a| a.id))
            .chain(project.tracking_assets.iter().map(|a| a.id))
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(Error::Invalid("resource IDs exhausted"))?;
        let attachment = Attachment::prepare(project, id, selected, &original, proof)?;
        cancelled(&mut check)?;
        for relative in ["assets", "assets/tracking"] {
            let path = root.join(relative);
            match fs::create_dir(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(io("create_directory", &path, e)),
            }
            require(
                path.canonicalize()
                    .map_err(|e| io("resolve_directory", &path, e))?
                    == path
                    && path.is_dir(),
                "tracking directory must be a direct managed directory",
            )?;
        }
        let path = root.join(&attachment.descriptor().path);
        let mut owned = OwnedOriginal {
            path: None,
            root: root.clone(),
            rollback: true,
        };
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut output) => {
                owned.path = Some(path.clone());
                let write = (|| {
                    output
                        .write_all(attachment.original())
                        .map_err(|e| io("write_original", &path, e))?;
                    output
                        .sync_all()
                        .map_err(|e| io("sync_original", &path, e))?;
                    Ok(())
                })();
                drop(output);
                if let Err(error) = write {
                    return owned.finish(Err(error));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                storage::open_tracking_resource(&root, attachment.descriptor())?;
            }
            Err(e) => return Err(io("create_original", &path, e)),
        }
        // The original must become durable before project.json can reference it.
        // File::sync_all does not persist newly created directory entries on Unix.
        #[cfg(unix)]
        for directory in [
            root.join("assets/tracking"),
            root.join("assets"),
            root.clone(),
        ] {
            let result = File::open(&directory)
                .and_then(|directory| directory.sync_all())
                .map_err(|cause| io("sync_original_directory", &directory, cause));
            if let Err(error) = result {
                return owned.finish(Err(error));
            }
        }
        Ok(Self {
            root,
            expected: project.clone(),
            revision,
            attachment,
            owned,
        })
    }
    pub fn descriptor(&self) -> &TrackingAsset {
        self.attachment.descriptor()
    }
    pub fn track_count(&self) -> usize {
        self.attachment.solve().tracks.len()
    }
    pub fn observation_count(&self) -> usize {
        self.attachment
            .solve()
            .tracks
            .iter()
            .map(|t| t.samples.len())
            .sum()
    }

    pub fn commit(&mut self, engine: &mut Engine) -> Result<(), Error> {
        self.commit_using(engine, storage::save)
    }
    fn commit_using(
        &mut self,
        engine: &mut Engine,
        persist: impl FnOnce(&Path, &Project) -> motion_core::Result<()>,
    ) -> Result<(), Error> {
        if engine.revision() != self.revision || engine.project() != &self.expected {
            return Err(Error::Superseded);
        }
        require(
            !engine.gesture_active(),
            "finish the active gesture before tracking import",
        )?;
        let mut acknowledgement = None;
        engine.apply_batch_with_prepare(vec![self.attachment.registration()], |candidate| {
            self.owned.rollback = false;
            match persist(&self.root, candidate) {
                Ok(()) => Ok(()),
                Err(
                    error @ motion_core::Error::Storage {
                        published: true, ..
                    },
                ) => {
                    // Bytes are already published: commit matching history/model,
                    // retain the source, and propagate the durability failure below.
                    acknowledgement = Some(error);
                    Ok(())
                }
                Err(motion_core::Error::Io(source)) => {
                    self.owned.rollback = true;
                    // Generic core IO errors otherwise lose the save phase/path
                    // at this boundary. Keep the original OS error as the cause.
                    Err(motion_core::Error::Storage {
                        phase: "save_tracking_project",
                        path: self.root.join("project.json"),
                        published: false,
                        source,
                    })
                }
                Err(error) => {
                    self.owned.rollback = true;
                    Err(error)
                }
            }
        })?;
        self.owned.rollback = false;
        if let Some(error) = acknowledgement {
            return Err(error.into());
        }
        Ok(())
    }
    /// Call outside the session guard, including after failed commit/snapshot.
    pub fn finish<T>(mut self, result: Result<T, Error>) -> Result<T, Error> {
        self.owned.finish(result)
    }
    /// Must run after leaving the Session guard. Published or uncertain saves
    /// retain the original; a failure before publication releases owned bytes.
    pub fn cleanup_uncommitted(&mut self) -> Result<(), Error> {
        self.owned.cleanup()
    }
}

#[cfg(test)]
#[path = "import_tests.rs"]
mod tests;
