//! Error identity survives worker replies; presentation may format it as text.
use std::{fmt, sync::Arc, time::Duration};

/// New native color failures carry nested, typed preparation/IO causes. Keep
/// those causes in the command's own record, alongside its real request ID;
/// the deferred phase record is additional evidence, not a replacement.
pub(super) fn command_cause(error: &motion_host::Error) -> String {
    if !matches!(
        error,
        motion_host::Error::ColorPreparation(_)
            | motion_host::Error::NativeColor(_)
            | motion_host::Error::Render(motion_render::RenderError::AuthoredScene(_))
    ) {
        return crate::fallback_observation::error_cause(error);
    }
    struct Chain<'a>(&'a dyn std::error::Error);
    impl fmt::Display for Chain<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let mut cause = Some(self.0);
            for index in 0..8 {
                let Some(current) = cause else {
                    break;
                };
                write!(
                    f,
                    "{}{}",
                    if index == 0 { "" } else { ": " },
                    crate::fallback_observation::short(current)
                )?;
                cause = current.source();
            }
            Ok(())
        }
    }
    crate::fallback_observation::error_cause(Chain(error))
}

#[derive(Clone, Debug)]
pub(crate) enum Error {
    Host(Arc<motion_host::Error>),
    Surface(Arc<wgpu::CreateSurfaceError>),
    QueueFull {
        capacity: usize,
    },
    Timeout {
        request: u64,
        operation: &'static str,
        waited: Duration,
    },
    ReplyDisconnected {
        operation: Arc<str>,
        cause: Box<Error>,
    },
    ShuttingDown,
    Stopped,
    Message(Arc<str>),
}

impl Error {
    pub(crate) fn host(&self) -> Option<&motion_host::Error> {
        match self {
            Self::Host(error) => Some(error),
            Self::ReplyDisconnected { cause, .. } => cause.host(),
            _ => None,
        }
    }

    pub(crate) fn composition_detail(&self) -> Option<&motion_core::composition::CompositionError> {
        self.host().and_then(motion_host::Error::composition_detail)
    }

    pub(crate) fn envelope(&self) -> serde_json::Value {
        serde_json::json!({"ok": false, "error": self.to_string(),
            "error_detail": self.composition_detail()})
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Host(error) => fmt::Display::fmt(error, f),
            Self::Surface(error) => fmt::Display::fmt(error, f),
            Self::QueueFull { capacity } => write!(f, "engine command queue is full (capacity {capacity}); command was not queued"),
            Self::Timeout { operation, .. } => write!(f, "{operation} timed out; its result is still pending; check project state before retrying"),
            Self::ReplyDisconnected { operation, cause } => write!(f, "{operation} response disconnected: {cause}"),
            Self::ShuttingDown => f.write_str("engine shutdown requested; new commands are not accepted"),
            Self::Stopped => f.write_str("engine session has stopped; reopen the project"),
            Self::Message(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Host(error) => Some(error.as_ref()),
            Self::Surface(error) => Some(error.as_ref()),
            Self::ReplyDisconnected { cause, .. } => Some(cause.as_ref()),
            _ => None,
        }
    }
}

// Shared native failures compare by identity, never by their display text.
impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Host(a), Self::Host(b)) => Arc::ptr_eq(a, b),
            (Self::Surface(a), Self::Surface(b)) => Arc::ptr_eq(a, b),
            (Self::QueueFull { capacity: a }, Self::QueueFull { capacity: b }) => a == b,
            (
                Self::Timeout {
                    request: a,
                    operation: ao,
                    waited: aw,
                },
                Self::Timeout {
                    request: b,
                    operation: bo,
                    waited: bw,
                },
            ) => a == b && ao == bo && aw == bw,
            (Self::Message(a), Self::Message(b)) => a == b,
            (
                Self::ReplyDisconnected {
                    operation: a,
                    cause: ac,
                },
                Self::ReplyDisconnected {
                    operation: b,
                    cause: bc,
                },
            ) => a == b && ac == bc,
            (Self::ShuttingDown, Self::ShuttingDown) | (Self::Stopped, Self::Stopped) => true,
            _ => false,
        }
    }
}
impl Eq for Error {}

// Existing JSON diagnostics expose display text. Native matching happens before
// this serialization boundary; structured replies use `envelope` explicitly.
impl serde::Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl From<motion_host::Error> for Error {
    fn from(error: motion_host::Error) -> Self {
        Self::Host(Arc::new(error))
    }
}
impl From<wgpu::CreateSurfaceError> for Error {
    fn from(error: wgpu::CreateSurfaceError) -> Self {
        Self::Surface(Arc::new(error))
    }
}
impl From<motion_ui::style::Error> for Error {
    fn from(error: motion_ui::style::Error) -> Self {
        motion_host::Error::from_source(error).into()
    }
}
impl From<motion_ui::text::FontError> for Error {
    fn from(error: motion_ui::text::FontError) -> Self {
        motion_host::Error::from_source(error).into()
    }
}
impl From<String> for Error {
    fn from(error: String) -> Self {
        Self::Message(error.into())
    }
}
impl From<&str> for Error {
    fn from(error: &str) -> Self {
        Self::Message(error.into())
    }
}
impl From<Error> for String {
    fn from(error: Error) -> Self {
        error.to_string()
    }
}

#[cfg(test)]
mod tests;
