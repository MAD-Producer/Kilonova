//! Cleanup only files exclusively created by this operation; close before removal.
use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

pub(crate) struct OwnedFile {
    path: Option<PathBuf>,
    file: Option<File>,
}
impl OwnedFile {
    pub(crate) fn create(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new().write(true).create_new(true).open(path)?;
        Ok(Self {
            path: Some(path.to_owned()),
            file: Some(file),
        })
    }
    pub(crate) fn file(&mut self) -> &mut File {
        self.file.as_mut().expect("owned file must be open")
    }
    pub(crate) fn close(&mut self) {
        self.file.take();
    }
    pub(crate) fn keep(mut self) {
        self.close();
        self.path.take();
    }
}
impl Drop for OwnedFile {
    fn drop(&mut self) {
        self.close();
        if let Some(path) = self.path.take() {
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => log::warn!(
                    "temporary file cleanup failed: path={} reason={error}",
                    path.display()
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn exclusive_creation_never_claims_an_existing_file() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("foreign.tmp");
        fs::write(&path, b"foreign bytes").unwrap();
        assert!(OwnedFile::create(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"foreign bytes");
    }
    #[test]
    fn failed_write_scope_and_unwind_close_and_remove_only_the_owned_file() {
        let root = tempfile::tempdir().unwrap();
        for unwind in [false, true] {
            let path = root.path().join(format!("owned-{unwind}.tmp"));
            if unwind {
                assert!(std::panic::catch_unwind(|| {
                    let mut file = OwnedFile::create(&path).unwrap();
                    file.file().write_all(b"partial bytes").unwrap();
                    panic!("injected operation panic");
                })
                .is_err());
            } else {
                let result: io::Result<()> = (|| {
                    let mut file = OwnedFile::create(&path)?;
                    file.file().write_all(b"partial bytes")?;
                    Err(io::Error::other("injected operation failure"))
                })();
                assert!(result.is_err());
            }
            assert!(!path.exists());
        }
    }
    #[test]
    fn kept_file_survives_and_a_missing_temporary_is_already_cleaned() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("kept.bin");
        let mut file = OwnedFile::create(&path).unwrap();
        file.file().write_all(b"complete bytes").unwrap();
        file.keep();
        assert_eq!(fs::read(&path).unwrap(), b"complete bytes");
        let path = root.path().join("removed.tmp");
        let mut file = OwnedFile::create(&path).unwrap();
        file.close();
        fs::remove_file(&path).unwrap();
        drop(file);
    }
}
