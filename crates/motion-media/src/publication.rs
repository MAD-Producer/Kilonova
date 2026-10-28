//! Publish complete cache directories with rollback of the previous snapshot.
#[cfg(test)]
mod atomic_directory_tests;
use crate::{Error, Result};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NONCE: AtomicU64 = AtomicU64::new(1);

/// File publication is shared; the edit decides whether a clip is created.
#[derive(Clone, Copy)]
pub(crate) enum ImportTarget {
    Layer,
    Replace(u64),
    Library(Option<u64>),
}
impl ImportTarget {
    pub fn replacement(self) -> Option<u64> {
        match self { Self::Replace(id) => Some(id), _ => None }
    }
    pub fn creates_layer(self) -> bool { matches!(self, Self::Layer) }
    pub fn operation(self) -> &'static str {
        match self {
            Self::Layer => "import_media",
            Self::Replace(_) => "replace_media",
            Self::Library(_) => "register_media",
        }
    }
}

struct Entry {
    destination: PathBuf,
    backup: Option<PathBuf>,
    published: bool,
    directory: bool,
}
struct Publication {
    directory: PathBuf,
    entries: Vec<Entry>,
    finished: bool,
    preserve: bool,
    rollback_error: Option<String>,
}
impl Publication {
    fn new(parent: &Path) -> Result<Self> {
        for _ in 0..128 {
            let nonce = NONCE.fetch_add(1, Ordering::Relaxed);
            let directory = parent.join(format!(".cache-publish-{}-{nonce}", std::process::id()));
            match fs::create_dir(&directory) {
                Ok(()) => {
                    return Ok(Self {
                        directory,
                        entries: Vec::new(),
                        finished: false,
                        preserve: false,
                        rollback_error: None,
                    })
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(Error::from(error).context(format!(
                        "create cache recovery directory {}", directory.display()
                    )))
                }
            }
        }
        Err("cache recovery directory collision limit exceeded".into())
    }

    fn publish(&mut self, source: &Path, destination: &Path) -> Result<()> {
        let source_type = fs::symlink_metadata(source)
            .map_err(|e| Error::from(e).context(format!("inspect staged cache for {}", destination.display())))?
            .file_type();
        let directory = source_type.is_dir();
        if !directory && !source_type.is_file() {
            return Err(format!("cache source is not a regular file or directory: {}", source.display()).into());
        }
        let backup = match fs::symlink_metadata(destination) {
            Ok(metadata) => {
                if if directory { !metadata.file_type().is_dir() } else { !metadata.file_type().is_file() } {
                    return Err(format!(
                        "cache destination has an unexpected file type: {}",
                        destination.display()
                    ).into());
                }
                let backup = self.directory.join(format!("old-{}", self.entries.len()));
                fs::rename(destination, &backup)
                    .map_err(|e| Error::from(e).context(format!("backup cache {}", destination.display())))?;
                Some(backup)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(Error::from(error).context(format!("inspect cache {}", destination.display()))),
        };
        self.entries.push(Entry {
            destination: destination.into(),
            backup,
            published: false,
            directory,
        });
        fs::rename(source, destination)
            .map_err(|e| Error::from(e).context(format!("publish cache {}", destination.display())))?;
        self.entries
            .last_mut()
            .expect("entry just inserted")
            .published = true;
        Ok(())
    }

    fn rollback(&mut self) -> Option<Error> {
        self.finished = true;
        let mut errors = Vec::new();
        for (index, entry) in self.entries.iter().enumerate().rev() {
            if entry.published {
                // Withdraw a published directory atomically; dispose of its
                // potentially large contents only after business guards drop.
                let removed = if entry.directory {
                    match fs::symlink_metadata(&entry.destination) {
                        Ok(metadata) if metadata.file_type().is_dir() =>
                            fs::rename(&entry.destination, self.directory.join(format!("discard-{index}"))),
                        Ok(_) => Err(io::Error::other("published cache directory changed file type")),
                        Err(error) => Err(error),
                    }
                } else { fs::remove_file(&entry.destination) };
                if let Err(error) = removed {
                    if error.kind() != io::ErrorKind::NotFound {
                        errors.push(Error::from(error).context(format!(
                            "remove new cache {}", entry.destination.display()
                        )));
                        // The live path may now belong to another writer.
                        // Preserve our backup rather than replace that object.
                        continue;
                    }
                }
            }
            if let Some(backup) = &entry.backup {
                let restored = match fs::symlink_metadata(&entry.destination) {
                    Ok(_) => Err(io::Error::new(io::ErrorKind::AlreadyExists,
                        "cache destination occupied during rollback")),
                    Err(error) if error.kind() == io::ErrorKind::NotFound =>
                        fs::rename(backup, &entry.destination),
                    Err(error) => Err(error),
                };
                if let Err(error) = restored {
                    errors.push(Error::from(error).context(format!(
                        "restore cache {} from {}",
                        entry.destination.display(),
                        backup.display()
                    )));
                }
            }
        }
        let mut errors = errors.into_iter();
        let primary = errors.next()?;
        self.preserve = true;
        let error = primary.rollback(errors.collect()).context(format!(
            "cache rollback failed; recovery directory={}", self.directory.display()
        ));
        self.rollback_error = Some(error.to_string().chars().take(2048).collect());
        Some(error)
    }
}
impl Drop for Publication {
    fn drop(&mut self) {
        if !self.finished {
            self.rollback();
        }
        if let Some(error) = &self.rollback_error {
            log::error!("{error}");
        }
        if self.preserve {
            return;
        }
        for index in 0..2 {
            let path = self.directory.join(format!("old-{index}"));
            let removed = if self.entries.get(index).is_some_and(|entry| entry.directory) {
                fs::remove_dir_all(&path)
            } else { fs::remove_file(&path) };
            if let Err(error) = removed {
                if error.kind() != io::ErrorKind::NotFound {
                    log::warn!(
                        "cache backup cleanup failed: path={} reason={error}",
                        path.display()
                    );
                }
            }
            cleanup_stage(&self.directory.join(format!("discard-{index}")));
        }
        cleanup_stage(&self.directory.join("new-cache"));
        if let Err(error) = fs::remove_dir(&self.directory) {
            log::warn!(
                "cache recovery directory cleanup failed: path={} reason={error}",
                self.directory.display()
            );
        }
    }
}

/// Declare before business guards. Publication and rollback stay synchronous;
/// only backup disposal and cleanup observations wait until those guards drop.
#[derive(Default)]
pub(crate) struct PublicationCleanup {
    cache: Option<Publication>,
    rollback_warning: Option<String>,
    published_cache: bool,
}

// Restore a withdrawn snapshot before the caller's publication/cancellation
// lock is released. Large directory disposal and observation are deferred.
struct Attempt<'a>(&'a mut Publication);
impl Drop for Attempt<'_> {
    fn drop(&mut self) {
        if !self.0.finished {
            self.0.rollback();
        }
    }
}

impl PublicationCleanup {
    pub(crate) fn publish_cache_pair(
        &mut self,
        first: (&Path, &Path),
        second: (&Path, &Path),
    ) -> Result<()> {
        self.publish_cache_pair_at_checkpoint(first, second, || {})
            .map_err(|error| error.context(format!("publish cache pair {}", first.1.display())))
    }

    fn publish_cache_pair_at_checkpoint(
        &mut self,
        first: (&Path, &Path),
        second: (&Path, &Path),
        checkpoint: impl FnOnce(),
    ) -> Result<()> {
        if first.1 == second.1 {
            return Err("cache pair destinations must differ".into());
        }
        if self.cache.is_some() {
            return Err("cache publication cleanup already owns a transaction".into());
        }
        let cache_directory = first.1.parent().ok_or("cache parent missing")?;
        if second.1.parent() != Some(cache_directory) {
            return Err("cache pair members must belong to one directory".into());
        }
        let parent = cache_directory.parent().ok_or("cache directory parent missing")?;
        let publication = self.cache.insert(Publication::new(parent)?);
        let attempt = Attempt(publication);
        let publication = &mut *attempt.0;
        let result = (|| {
            let staged = publication.directory.join("new-cache");
            fs::create_dir(&staged).map_err(Error::from)?;
            stage_cache_member(first, &staged)?;
            checkpoint();
            stage_cache_member(second, &staged)?;
            validate_cache_directory(cache_directory, first.1, second.1)?;
            publication.publish(&staged, cache_directory)
        })();
        match result {
            Ok(()) => {
                publication.finished = true;
                self.published_cache = true;
                Ok(())
            }
            Err(error) => match publication.rollback() {
                None => Err(error),
                Some(rollback) => Err(error.rollback(vec![rollback])),
            },
        }
    }

    pub(crate) fn rollback_published_cache(&mut self, primary: impl Into<Error>) -> Error {
        let primary = primary.into();
        if std::mem::take(&mut self.published_cache) {
            if let Some(rollback) = self.cache.as_mut().and_then(Publication::rollback) {
                return primary.rollback(vec![rollback]);
            }
        }
        primary
    }

    pub(crate) fn rollback_files(
        &mut self,
        primary: impl Into<Error>,
        paths: &[&Path],
    ) -> Error {
        let primary = primary.into();
        let mut secondary = Vec::new();
        let mut warning = String::new();
        for path in paths.iter().rev() {
            if let Err(error) = fs::remove_file(path) {
                if error.kind() != io::ErrorKind::NotFound {
                    let error = Error::from(error).context(format!("rollback remove {}", path.display()));
                    let detail = error.to_string();
                    secondary.push(error);
                    // Each import calls rollback once and returns immediately;
                    // current audio/video transactions contain at most five files.
                    if !warning.is_empty() {
                        warning.push_str("; ");
                    }
                    warning.extend(
                        detail
                            .chars()
                            .take(2048usize.saturating_sub(warning.chars().count())),
                    );
                }
            }
        }
        if !warning.is_empty() {
            self.rollback_warning = Some(warning.chars().take(2048).collect());
        }
        primary.rollback(secondary)
    }
}

fn stage_cache_member((source, destination): (&Path, &Path), staged: &Path) -> Result<()> {
    match fs::symlink_metadata(destination) {
        Ok(metadata) if !metadata.file_type().is_file() =>
            return Err(format!("cache destination is not a regular file: {}", destination.display()).into()),
        Ok(_) => {},
        Err(error) if error.kind() == io::ErrorKind::NotFound => {},
        Err(error) => return Err(Error::from(error).context(format!("inspect cache {}", destination.display()))),
    }
    let source_type = fs::symlink_metadata(source)
        .map_err(|error| Error::from(error).context(format!("stage cache {} from {}", destination.display(), source.display())))?
        .file_type();
    if !source_type.is_file() {
        return Err(format!("cache source is not a regular file: {}", source.display()).into());
    }
    let name = destination.file_name().ok_or("cache member filename missing")?;
    fs::rename(source, staged.join(name))
        .map_err(|error| Error::from(error).context(format!("stage cache {} from {}", destination.display(), source.display())))
}

fn validate_cache_directory(directory: &Path, first: &Path, second: &Path) -> Result<()> {
    match fs::symlink_metadata(directory) {
        Ok(metadata) if !metadata.file_type().is_dir() =>
            return Err(format!("cache destination is not a directory: {}", directory.display()).into()),
        Ok(_) => {},
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(Error::from(error).context(format!("inspect cache directory {}", directory.display()))),
    }
    for entry in fs::read_dir(directory).map_err(Error::from)? {
        let entry = entry.map_err(Error::from)?;
        if (Some(entry.file_name().as_os_str()) != first.file_name()
            && Some(entry.file_name().as_os_str()) != second.file_name())
            || !entry.file_type().map_err(Error::from)?.is_file() {
            return Err(format!("unexpected member in cache directory: {}", entry.path().display()).into());
        }
    }
    Ok(())
}

impl Drop for PublicationCleanup {
    fn drop(&mut self) {
        if let Some(warning) = &self.rollback_warning {
            log::warn!("{warning}");
        }
    }
}

#[cfg(test)]
fn publish_cache_pair(first: (&Path, &Path), second: (&Path, &Path)) -> Result<()> {
    PublicationCleanup::default().publish_cache_pair(first, second)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn second_file_failure_restores_both_previous_cache_files() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("audio.pcm");
        let second = directory.path().join("audio.wave");
        let staged = directory.path().join("new.pcm");
        fs::write(&first, b"old PCM").unwrap();
        fs::write(&second, b"old waveform").unwrap();
        fs::write(&staged, b"new PCM").unwrap();
        let missing = directory.path().join("missing.wave");
        let error = publish_cache_pair((&staged, &first), (&missing, &second)).unwrap_err();
        assert!(matches!(error.primary(), Error::Io(cause) if cause.kind() == io::ErrorKind::NotFound));
        assert!(error.to_string().contains("audio.wave"), "{error}");
        assert_eq!(fs::read(first).unwrap(), b"old PCM");
        assert_eq!(fs::read(second).unwrap(), b"old waveform");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    }

    #[test]
    fn second_file_failure_removes_a_new_first_file_without_deleting_foreign_directories() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("video.pts");
        let second = directory.path().join("video.png");
        let staged = directory.path().join("new.pts");
        let image = directory.path().join("new.png");
        fs::write(&staged, b"timestamps").unwrap();
        fs::write(&image, b"thumbnail").unwrap();
        fs::create_dir(&second).unwrap();
        assert!(publish_cache_pair((&staged, &first), (&image, &second))
            .unwrap_err()
            .to_string().contains("not a regular file"));
        assert!(!first.exists());
        assert!(second.is_dir());
        assert_eq!(fs::read(image).unwrap(), b"thumbnail");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    }

    #[test]
    fn rollback_cleanup_keeps_the_primary_error_and_reports_remaining_paths() {
        let directory = tempfile::tempdir().unwrap();
        let blocked = directory.path().join("published.wav");
        fs::create_dir(&blocked).unwrap();
        let missing = directory.path().join("already-removed");
        let error = rollback_files("save failed", &[&blocked, &missing]);
        let Error::Rollback { primary, secondary, .. } = &error else { panic!("rollback causes lost: {error:?}"); };
        assert!(matches!(primary.as_ref(), Error::Message(message) if message == "save failed"));
        assert_eq!(secondary.len(), 1);
        assert!(matches!(secondary[0].primary(), Error::Io(_)));
        assert!(error.to_string().starts_with("save failed; rollback remove "));
        assert!(error.to_string().contains("published.wav"));
        assert!(!error.to_string().contains("already-removed"));
        assert!(blocked.is_dir());
    }

    #[test]
    fn unwind_restores_the_old_cache_pair() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("audio.pcm");
        let source = directory.path().join("new.pcm");
        fs::write(&target, b"old PCM").unwrap();
        fs::write(&source, b"new PCM").unwrap();
        let caught = std::panic::catch_unwind(|| {
            let mut transaction = Publication::new(directory.path()).unwrap();
            transaction.publish(&source, &target).unwrap();
            panic!("injected between cache publications");
        });
        assert!(caught.is_err());
        assert_eq!(fs::read(target).unwrap(), b"old PCM");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn failed_rollback_keeps_the_old_bytes_in_a_named_recovery_directory() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("video.pts");
        let source = directory.path().join("new.pts");
        fs::write(&target, b"old timestamps").unwrap();
        fs::write(&source, b"new timestamps").unwrap();
        let mut transaction = Publication::new(directory.path()).unwrap();
        transaction.publish(&source, &target).unwrap();
        let recovery = transaction.directory.clone();
        // A concurrent filesystem change prevents rollback from replacing this path.
        fs::remove_file(&target).unwrap();
        fs::create_dir(&target).unwrap();
        let error = transaction.rollback().unwrap();
        assert!(error.to_string().contains(&recovery.display().to_string()));
        drop(transaction);
        assert_eq!(fs::read(recovery.join("old-0")).unwrap(), b"old timestamps");
        assert!(target.is_dir());
    }

    #[test]
    fn successful_pair_replaces_both_files_and_removes_backups() {
        let directory = tempfile::tempdir().unwrap();
        let targets = [directory.path().join("a"), directory.path().join("b")];
        let sources = [
            directory.path().join("new-a"),
            directory.path().join("new-b"),
        ];
        for index in 0..2 {
            fs::write(&targets[index], b"old").unwrap();
            fs::write(&sources[index], format!("new-{index}")).unwrap();
        }
        publish_cache_pair((&sources[0], &targets[0]), (&sources[1], &targets[1])).unwrap();
        for index in 0..2 {
            assert_eq!(
                fs::read(&targets[index]).unwrap(),
                format!("new-{index}").as_bytes()
            );
        }
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    }
}

pub(crate) fn cleanup_stage(path: &Path) {
    if let Err(error) = fs::remove_dir_all(path) {
        if error.kind() != io::ErrorKind::NotFound {
            log::warn!(
                "media stage cleanup failed: path={} reason={error}",
                path.display()
            );
        }
    }
}

#[cfg(test)]
fn rollback_files(primary: impl Into<Error>, paths: &[&Path]) -> Error {
    PublicationCleanup::default().rollback_files(primary, paths)
}

#[cfg(test)]
mod cleanup_tests;
