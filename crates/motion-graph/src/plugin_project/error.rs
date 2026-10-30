use serde::Serialize;
use std::{fmt, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Read,
    Write,
    Structure,
    Bindings,
    Compile,
}

/// Display is safe for the default log: no WGSL snippets, full source, or user
/// labels. Original native causes remain available through Error::source().
pub enum Error {
    Invalid {
        phase: Phase,
        field: &'static str,
        message: &'static str,
    },
    Io {
        phase: Phase,
        path: PathBuf,
        cause: std::io::Error,
    },
    Json {
        phase: Phase,
        line: usize,
        column: usize,
        cause: serde_json::Error,
    },
    Graph {
        phase: Phase,
        cause: Box<crate::Error>,
    },
    Effect {
        phase: Phase,
        cause: Box<motion_effects::Error>,
    },
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Rust's Result-returning main uses Debug for termination. Keep that
        // channel safe too; the native source remains available explicitly.
        fmt::Display::fmt(self, f)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        // Expose the native error, not Box<Error>, so downcasting preserves
        // diagnostics even when that native error has no further source.
        match self {
            Self::Invalid { .. } => None,
            Self::Io { cause, .. } => Some(cause),
            Self::Json { cause, .. } => Some(cause),
            Self::Graph { cause, .. } => Some(cause.as_ref()),
            Self::Effect { cause, .. } => Some(cause.as_ref()),
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "plugin project phase={:?}", self.phase())?;
        match self {
            Self::Invalid { field, message, .. } => write!(f, " field={field}: {message}"),
            Self::Io { path, cause, .. } => {
                write!(f, " path={}: {}", limited(path.display()), limited(cause))
            }
            Self::Json {
                line,
                column,
                cause,
                ..
            } => write!(
                f,
                " JSON line={line} column={column} category={:?}",
                cause.classify()
            ),
            Self::Graph { cause, .. } => match cause.as_ref() {
                crate::Error::Effect { diagnostic, cause } => write!(
                    f,
                    " code={:?} node={} field={}: {}",
                    diagnostic.code,
                    limited(diagnostic.node.as_deref().unwrap_or("-")),
                    limited(diagnostic.port.as_deref().unwrap_or("-")),
                    EffectSummary(cause)
                ),
                crate::Error::Json(cause) => write!(
                    f,
                    " graph JSON line={} column={} category={:?}",
                    cause.line(),
                    cause.column(),
                    cause.classify()
                ),
                cause => write!(f, ": {}", limited(cause)),
            },
            Self::Effect { cause, .. } => write!(f, ": {}", EffectSummary(cause)),
        }
    }
}

struct EffectSummary<'a>(&'a motion_effects::Error);
impl fmt::Display for EffectSummary<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            // `message` contains emit_to_string(source), including authored
            // lines. The native parser cause identifies the fault without it.
            motion_effects::Error::ShaderParse { cause, .. } => {
                write!(f, "phase=wgsl_parse: {}", limited(cause))
            }
            cause => write!(f, "{}", limited(cause)),
        }
    }
}
// Bound formatting while it is written; do not allocate a complete diagnostic
// or path first. Control characters cannot inject additional log records.
fn limited(value: impl fmt::Display) -> impl fmt::Display {
    struct Limited<T>(T);
    impl<T: fmt::Display> fmt::Display for Limited<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            struct Buffer {
                value: String,
                remaining: usize,
            }
            impl fmt::Write for Buffer {
                fn write_str(&mut self, text: &str) -> fmt::Result {
                    for ch in text.chars().take(self.remaining) {
                        self.value.push(if ch.is_control() { ' ' } else { ch });
                        self.remaining -= 1;
                    }
                    Ok(())
                }
            }
            let mut buffer = Buffer {
                value: String::new(),
                remaining: 384,
            };
            fmt::write(&mut buffer, format_args!("{}", self.0))?;
            f.write_str(&buffer.value)
        }
    }
    Limited(value)
}
pub type Result<T> = std::result::Result<T, Error>;
impl Error {
    pub(super) fn graph(phase: Phase, cause: crate::Error) -> Self {
        Self::Graph {
            phase,
            cause: Box::new(cause),
        }
    }
    pub(super) fn effect(phase: Phase, cause: motion_effects::Error) -> Self {
        Self::Effect {
            phase,
            cause: Box::new(cause),
        }
    }
    pub(super) fn json(phase: Phase, cause: serde_json::Error) -> Self {
        Self::Json {
            phase,
            line: cause.line(),
            column: cause.column(),
            cause,
        }
    }
    pub fn phase(&self) -> Phase {
        match self {
            Self::Invalid { phase, .. }
            | Self::Io { phase, .. }
            | Self::Json { phase, .. }
            | Self::Graph { phase, .. }
            | Self::Effect { phase, .. } => *phase,
        }
    }
}
pub(super) fn ensure(
    ok: bool,
    phase: Phase,
    field: &'static str,
    message: &'static str,
) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(Error::Invalid {
            phase,
            field,
            message,
        })
    }
}
