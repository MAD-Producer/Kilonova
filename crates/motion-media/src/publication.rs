//! Publish paired cache files with rollback of the previous pair.
use crate::Result;
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NONCE: AtomicU64 = AtomicU64::new(1);

struct Entry {
    destination: PathBuf,
    backup: Option<PathBuf>,
    published: bool,
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
                    return Err(format!(
                        "create cache recovery directory {}: {error}",
                        directory.display()
                    ))
                }
            }
        }
        Err("cache recovery directory collision limit exceeded".into())
    }

    fn publish(&mut self, source: &Path, destination: &Path) -> Result<()> {
        let backup = match fs::symlink_metadata(destination) {
            Ok(metadata) => {
                if !metadata.file_type().is_file() {
                    return Err(format!(
                        "cache destination is not a regular file: {}",
                        destination.display()
                    ));
                }
                let backup = self.directory.join(format!("old-{}", self.entries.len()));
                fs::rename(destination, &backup)
                    .map_err(|e| format!("backup cache {}: {e}", destination.display()))?;
                Some(backup)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(format!("inspect cache {}: {error}", destination.display())),
        };
        self.entries.push(Entry {
            destination: destination.into(),
            backup,
            published: false,
        });
        fs::rename(source, destination)
            .map_err(|e| format!("publish cache {}: {e}", destination.display()))?;
        self.entries
            .last_mut()
            .expect("entry just inserted")
            .published = true;
        Ok(())
    }

    fn rollback(&mut self) -> Option<String> {
        self.finished = true;
        let mut errors = Vec::new();
        for entry in self.entries.iter().rev() {
            if entry.published {
                if let Err(error) = fs::remove_file(&entry.destination) {
                    if error.kind() != io::ErrorKind::NotFound {
                        errors.push(format!(
                            "remove new cache {}: {error}",
                            entry.destination.display()
                        ));
                    }
                }
            }
            if let Some(backup) = &entry.backup {
                if let Err(error) = fs::rename(backup, &entry.destination) {
                    errors.push(format!(
                        "restore cache {} from {}: {error}",
                        entry.destination.display(),
                        backup.display()
                    ));
                }
            }
        }
        if errors.is_empty() {
            None
        } else {
            self.preserve = true;
            let error = format!(
                "cache rollback failed; recovery directory={}: {}",
                self.directory.display(),
                errors.join("; ")
            );
            self.rollback_error = Some(error.chars().take(2048).collect());
            Some(error)
        }
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
            if let Err(error) = fs::remove_file(&path) {
                if error.kind() != io::ErrorKind::NotFound {
                    log::warn!(
                        "cache backup cleanup failed: path={} reason={error}",
                        path.display()
                    );
                }
            }
        }
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
}

// An unwind between the two file moves still restores the pair while the
// caller holds its publication/cancellation lock. Observation is deferred.
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
        if first.1 == second.1 {
            return Err("cache pair destinations must differ".into());
        }
        if self.cache.is_some() {
            return Err("cache publication cleanup already owns a transaction".into());
        }
        let parent = first.1.parent().ok_or("cache parent missing")?;
        let publication = self.cache.insert(Publication::new(parent)?);
        let attempt = Attempt(publication);
        let publication = &mut *attempt.0;
        let result = publication
            .publish(first.0, first.1)
            .and_then(|()| publication.publish(second.0, second.1));
        match result {
            Ok(()) => {
                publication.finished = true;
                Ok(())
            }
            Err(error) => match publication.rollback() {
                None => Err(error),
                Some(rollback) => Err(format!("{error}; {rollback}")),
            },
        }
    }

    pub(crate) fn rollback_files(
        &mut self,
        primary: impl std::fmt::Display,
        paths: &[&Path],
    ) -> String {
        let mut reason = primary.to_string();
        let mut warning = String::new();
        for path in paths.iter().rev() {
            if let Err(error) = fs::remove_file(path) {
                if error.kind() != io::ErrorKind::NotFound {
                    let detail = format!("rollback remove {}: {error}", path.display());
                    reason.push_str("; ");
                    reason.push_str(&detail);
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
        reason
    }
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
        assert!(error.contains("audio.wave"), "{error}");
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
            .contains("not a regular file"));
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
        assert!(error.starts_with("save failed; rollback remove "));
        assert!(error.contains("published.wav"));
        assert!(!error.contains("already-removed"));
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
        assert!(error.contains(&recovery.display().to_string()));
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
fn rollback_files(primary: impl std::fmt::Display, paths: &[&Path]) -> String {
    PublicationCleanup::default().rollback_files(primary, paths)
}

#[cfg(test)]
mod cleanup_tests;
