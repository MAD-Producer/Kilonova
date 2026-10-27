//! Publish already-synced project bytes without acknowledging an unsynced rename.
use crate::{Error, Result};
#[cfg(not(windows))]
use std::fs;
use std::{
    io,
    path::{Path, PathBuf},
};

pub(super) struct ProjectPublication {
    directory: PathBuf,
    #[cfg(unix)]
    handle: fs::File,
}

fn failure(phase: &'static str, path: &Path, published: bool, source: io::Error) -> Error {
    Error::Storage {
        phase,
        path: path.into(),
        published,
        source,
    }
}

impl ProjectPublication {
    pub(super) fn prepare(root: &Path) -> Result<Self> {
        // Resolve once before opening the temporary file. Windows gets a native
        // extended-length path; publication never depends on a later cwd change.
        let directory = root
            .canonicalize()
            .map_err(|e| failure("resolve_directory", root, false, e))?;
        #[cfg(unix)]
        let handle = fs::File::open(&directory)
            .map_err(|e| failure("open_directory", &directory, false, e))?;
        Ok(Self {
            directory,
            #[cfg(unix)]
            handle,
        })
    }

    pub(super) fn directory(&self) -> &Path {
        &self.directory
    }

    pub(super) fn publish(&self, temporary: &Path) -> Result<()> {
        let destination = self.directory.join("project.json");
        replace(temporary, &destination).map_err(|e| failure("publish", &destination, false, e))?;
        #[cfg(test)]
        super::crash_tests::reached(super::crash_tests::Phase::Published);
        // fsync(file) does not make its containing directory entry durable.
        // https://man7.org/linux/man-pages/man2/fsync.2.html
        #[cfg(unix)]
        self.handle
            .sync_all()
            .map_err(|e| failure("sync_directory", &self.directory, true, e))?;
        Ok(())
    }
}

#[cfg(not(windows))]
pub(super) fn publish_preset(temporary: &Path, destination: &Path) -> Result<()> {
    replace(temporary, destination).map_err(|e| failure("preset_publish", destination, false, e))?;
    let parent = destination.parent().ok_or_else(|| failure("preset_sync_directory", destination, true,
        io::Error::new(io::ErrorKind::InvalidInput, "preset parent missing")))?;
    fs::File::open(parent).and_then(|file| file.sync_all())
        .map_err(|e| failure("preset_sync_directory", parent, true, e))?;
    Ok(())
}
#[cfg(windows)]
pub(super) fn publish_preset(temporary: &Path, destination: &Path) -> Result<()> {
    replace(temporary, destination).map_err(|e| failure("preset_publish", destination, false, e))
}

#[cfg(not(windows))]
fn replace(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }
    fn wide(path: &Path) -> io::Result<Vec<u16>> {
        let mut value: Vec<_> = path.as_os_str().encode_wide().collect();
        if value.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "NUL in project publication path",
            ));
        }
        value.push(0);
        Ok(value)
    }
    let source = wide(source)?;
    let destination = wide(destination)?;
    // Same-directory replacement, with no cross-volume copy or reboot fallback.
    // https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw
    const MOVEFILE_REPLACE_EXISTING: u32 = 1;
    const MOVEFILE_WRITE_THROUGH: u32 = 8;
    // SAFETY: both UTF-16 arrays are NUL terminated, contain no interior NUL,
    // and remain live for the synchronous system call. No handles are retained.
    let moved = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
