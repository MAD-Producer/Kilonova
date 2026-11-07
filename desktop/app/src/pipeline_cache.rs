//! The OS-account data directory is application configuration, never a project
//! or plugin path. No shared-temp fallback. All filesystem/permission work runs
//! inside the existing observable cache job on the session's bounded worker.
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

pub(crate) fn storage() -> Option<motion_host::session::DriverPipelineCacheStorage> {
    let base = user_cache_base()?;
    let root = base.join("MotionStudio").join("driver-pipeline-cache-v1");
    // SAFETY: this location comes only from the trusted process's OS-account
    // configuration, never project/import data. The worker validates a private
    // leaf and rejects linked files before each operation. The account must
    // protect its configured data directory and parent chain from other users.
    Some(unsafe { motion_host::session::DriverPipelineCacheStorage::new(root, Arc::new(prepare)) })
}
fn user_cache_base() -> Option<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME").map(|base| PathBuf::from(base).join("Library/Caches"));
    #[cfg(all(unix, not(target_os = "macos")))]
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|base| PathBuf::from(base).join(".cache")));
    #[cfg(not(any(windows, unix)))]
    let base: Option<PathBuf> = None;
    base.filter(|base| base.is_absolute())
}
fn refused(reason: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, reason)
}

#[cfg(test)]
pub(crate) fn prepare_test_storage(path: &Path) -> io::Result<()> { prepare(path) }

fn prepare(path: &Path) -> io::Result<()> {
    let root = path
        .parent()
        .ok_or_else(|| refused("pipeline cache has no parent"))?;
    let parent = root
        .parent()
        .ok_or_else(|| refused("pipeline cache has no account directory"))?;
    fs::create_dir_all(parent)?;
    reject_link(&fs::symlink_metadata(parent)?)?;
    #[cfg(windows)]
    windows::private_directory(root)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt};
        match fs::DirBuilder::new().mode(0o700).create(root) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        let metadata = fs::symlink_metadata(root)?;
        reject_link(&metadata)?;
        // SAFETY: POSIX geteuid has no parameters or ownership obligations.
        if !metadata.is_dir()
            || metadata.uid() != unsafe { geteuid() }
            || metadata.mode() & 0o077 != 0
        {
            return Err(refused(
                "pipeline cache directory must be account-owned with mode 0700",
            ));
        }
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            reject_link(&metadata)?;
            if !metadata.is_file() {
                return Err(refused("pipeline cache is not a regular file"));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if metadata.uid() != unsafe { geteuid() }
                    || metadata.nlink() != 1
                    || metadata.mode() & 0o022 != 0
                {
                    return Err(refused("pipeline cache file is not private account data"));
                }
            }
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
fn reject_link(metadata: &fs::Metadata) -> io::Result<()> {
    if metadata.file_type().is_symlink() {
        return Err(refused("pipeline cache must not follow a symbolic link"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(refused("pipeline cache must not follow a reparse point"));
        }
    }
    Ok(())
}
#[cfg(unix)]
extern "C" {
    fn geteuid() -> u32;
}
#[cfg(test)]
mod tests;
#[cfg(windows)]
mod windows;
