//! Primary font loading keeps actual read/parse causes, including rejected candidates.
use super::shaping::ShapeError;
use std::{
    fmt,
    io::Read,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontPhase {
    Open,
    Read,
    Parse,
}

#[derive(Debug)]
pub enum FontCause {
    Io(std::io::Error),
    Parse(ab_glyph::InvalidFont),
}

#[derive(Debug)]
pub enum FontError {
    Shaping(ShapeError),
    Candidate {
        phase: FontPhase,
        family: String,
        face_index: u32,
        path: Option<PathBuf>,
        cause: FontCause,
    },
    NoUsableFont {
        requested: Option<String>,
        prefer_chinese: bool,
        attempted: usize,
        first_failure: Option<Box<FontError>>,
    },
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Shaping(cause) => cause.fmt(f),
            Self::Candidate {
                phase,
                family,
                face_index,
                path,
                cause,
            } => {
                write!(f, "UI font phase={phase:?} face_index={face_index} cause=")?;
                match cause {
                    FontCause::Io(error) => error.fmt(f),
                    FontCause::Parse(error) => error.fmt(f),
                }?;
                write!(f, " family={family:?}")?;
                if let Some(path) = path {
                    write!(f, " path={}", path.display())?;
                }
                Ok(())
            }
            Self::NoUsableFont {
                requested,
                prefer_chinese,
                attempted,
                first_failure,
            } => {
                f.write_str("no usable system font was found")?;
                if let Some(cause) = first_failure {
                    write!(f, "; first failure: {cause}")?;
                }
                write!(f, " requested={requested:?} prefer_chinese={prefer_chinese} attempted={attempted}")
            }
        }
    }
}
impl std::error::Error for FontError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Shaping(cause) => Some(cause),
            Self::Candidate {
                cause: FontCause::Io(cause),
                ..
            } => Some(cause),
            Self::Candidate {
                cause: FontCause::Parse(cause),
                ..
            } => Some(cause),
            Self::NoUsableFont { first_failure, .. } => {
                first_failure.as_ref().map(|cause| cause.as_ref() as _)
            }
        }
    }
}
impl From<ShapeError> for FontError {
    fn from(cause: ShapeError) -> Self {
        Self::Shaping(cause)
    }
}

pub(super) fn candidate(face: &fontdb::FaceInfo) -> Result<ab_glyph::FontArc, FontError> {
    let path = match &face.source {
        fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _) => Some(path.clone()),
        fontdb::Source::Binary(_) => None,
    };
    let error = |phase, cause| FontError::Candidate {
        phase,
        family: face
            .families
            .first()
            .map_or("unknown", |value| value.0.as_str())
            .chars()
            .take(96)
            .collect(),
        face_index: face.index,
        path: path.clone(),
        cause,
    };
    // Preserve shared/memory sources even if their original path has moved.
    // A plain File needs owned bytes for FontVec anyway; read it directly so
    // fontdb's Option-only mapping boundary cannot discard the native IO cause.
    let bytes = match &face.source {
        fontdb::Source::Binary(data) | fontdb::Source::SharedFile(_, data) => {
            data.as_ref().as_ref().to_vec()
        }
        fontdb::Source::File(path) => {
            let mut file = std::fs::File::open(path)
                .map_err(|cause| error(FontPhase::Open, FontCause::Io(cause)))?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .map_err(|cause| error(FontPhase::Read, FontCause::Io(cause)))?;
            bytes
        }
    };
    ab_glyph::FontVec::try_from_vec_and_index(bytes, face.index)
        .map(ab_glyph::FontArc::new)
        .map_err(|cause| error(FontPhase::Parse, FontCause::Parse(cause)))
}

/// Consecutive degraded selection calls for this immutable catalog, not frames
/// or individual candidates. A selection without a rejected source resets it.
#[derive(Default)]
pub(super) struct SelectionFailures(AtomicUsize);
impl SelectionFailures {
    pub fn success(&self) {
        self.0.store(0, Ordering::Relaxed);
    }
    pub fn failure(&self, error: &FontError, rejected: usize, elapsed: Duration) {
        let count = self
            .0
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                Some(n.saturating_add(1))
            })
            .unwrap_or_else(|n| n);
        if count != 0 && count != 59 {
            return;
        }
        struct Bounded(String, usize);
        impl fmt::Write for Bounded {
            fn write_str(&mut self, value: &str) -> fmt::Result {
                for ch in value.chars().take(self.1) {
                    self.0.push(if ch.is_control() { ' ' } else { ch });
                    self.1 -= 1;
                }
                Ok(())
            }
        }
        let mut reason = Bounded(String::new(), 512);
        let _ = fmt::write(&mut reason, format_args!("{error}"));
        if count == 0 {
            log::debug!("UI font selection degradation count=1 rejected_candidates={rejected} elapsed_ms={} reason={}", elapsed.as_millis(), reason.0);
        } else {
            log::error!("persistent UI font selection degradation count=60 rejected_candidates={rejected} elapsed_ms={} reason={}", elapsed.as_millis(), reason.0);
        }
    }
}

#[cfg(test)]
mod tests;
