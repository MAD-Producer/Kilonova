//! Stylesheet failures retain the provider's error until the presentation boundary.
use std::{
    fmt,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Open,
    Read,
    Decode,
    DecodeOverride,
    EncodeBase,
    Compile,
}

#[derive(Debug)]
pub enum Cause {
    Io(std::io::Error),
    Json(serde_json::Error),
    Invalid(String),
    TooLarge { limit: usize },
}

#[derive(Debug)]
pub struct Error {
    pub path: Option<PathBuf>,
    pub phase: Phase,
    pub cause: Cause,
}

impl Error {
    pub(super) fn new(phase: Phase, cause: Cause) -> Self {
        Self {
            path: None,
            phase,
            cause,
        }
    }

    pub(super) fn at(mut self, path: &Path) -> Self {
        self.path = Some(path.to_owned());
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "stylesheet phase={:?}: ", self.phase)?;
        match &self.cause {
            Cause::Io(error) => error.fmt(f),
            Cause::Json(error) => error.fmt(f),
            Cause::Invalid(reason) => f.write_str(reason),
            Cause::TooLarge { limit } => write!(f, "stylesheet exceeds {limit} bytes"),
        }?;
        // Log callers bound this display; a long file path must not hide the cause.
        if let Some(path) = &self.path {
            write!(f, " path={}", path.display())?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.cause {
            Cause::Io(error) => Some(error),
            Cause::Json(error) => Some(error),
            Cause::Invalid(_) | Cause::TooLarge { .. } => None,
        }
    }
}
