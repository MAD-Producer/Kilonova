//! Typed failures within media operations. Task-status strings are a wire boundary.
//! Concrete sources are shared through `Arc` so decoder state can retain and
//! return the same native failure without flattening non-Clone errors to text.
//! `std::error::Error::source` exposes that shared cause; consumers can inspect
//! the variant or downcast the source to `Arc<io::Error>`, `Arc<JsonError>`, etc.
use std::sync::Arc;

#[derive(Clone, Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Unclassified text from existing validation and host-supplied capabilities.
    #[error("{0}")]
    Message(String),
    #[error("invalid TTF/OTF font: {0}")]
    Font(String),
    #[error("{0}")]
    Io(#[source] Arc<std::io::Error>),
    #[error("{0}")]
    Json(#[source] Arc<serde_json::Error>),
    #[error("{0}")]
    Image(#[source] Arc<image::ImageError>),
    #[error("{0}")]
    Model(#[source] Arc<motion_model::Error>),
    #[error("{0}")]
    Clock(#[source] Arc<std::time::SystemTimeError>),
    /// Native adapters keep their error object without making media depend on
    /// any particular codec, windowing library or GPU implementation.
    #[error("{cause}")]
    Backend {
        backend: &'static str,
        #[source]
        cause: Arc<dyn std::error::Error + Send + Sync>,
    },
    #[error("{message}")]
    Decode {
        phase: &'static str,
        message: String,
        #[source]
        cause: Arc<symphonia::core::errors::Error>,
    },
    #[error("{message}")]
    Context {
        message: String,
        #[source]
        cause: Arc<Error>,
    },
    #[error("{message}")]
    Rollback {
        message: String,
        #[source]
        primary: Arc<Error>,
        secondary: Vec<Error>,
    },
    #[error("{operation} failed: {reason}")]
    Panic {
        operation: &'static str,
        reason: String,
    },
}

impl Error {
    pub fn backend(
        backend: &'static str,
        cause: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Backend {
            backend,
            cause: Arc::new(cause),
        }
    }
    pub(crate) fn decode(phase: &'static str, cause: symphonia::core::errors::Error) -> Self {
        let message = format!("audio {phase} failed: {cause}");
        Self::Decode {
            phase,
            message,
            cause: Arc::new(cause),
        }
    }

    pub(crate) fn symphonia(phase: &'static str, cause: symphonia::core::errors::Error) -> Self {
        Self::Decode {
            phase,
            message: cause.to_string(),
            cause: Arc::new(cause),
        }
    }

    /// Keep the original typed failure while adding an operation/path diagnostic.
    pub fn context(self, context: String) -> Self {
        let message = format!("{context}: {self}");
        Self::Context {
            message,
            cause: Arc::new(self),
        }
    }

    pub(crate) fn rollback(self, secondary: Vec<Error>) -> Self {
        if secondary.is_empty() {
            return self;
        }
        let mut message = self.to_string();
        for error in &secondary {
            message.push_str(&format!("; {error}"));
        }
        Self::Rollback {
            message,
            primary: Arc::new(self),
            secondary,
        }
    }

    /// Classify the original failure even when contextual/rollback text was added.
    pub fn primary(&self) -> &Self {
        let mut current = self;
        loop {
            current = match current {
                Self::Context { cause, .. } => cause.as_ref(),
                Self::Rollback { primary, .. } => primary.as_ref(),
                _ => return current,
            };
        }
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}
impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self::Message(message.into())
    }
}
impl From<Error> for String {
    fn from(error: Error) -> Self {
        error.to_string()
    }
}

macro_rules! source_conversion {
    ($source:ty, $variant:ident) => {
        impl From<$source> for Error {
            fn from(error: $source) -> Self {
                Self::$variant(Arc::new(error))
            }
        }
    };
}
source_conversion!(std::io::Error, Io);
source_conversion!(serde_json::Error, Json);
source_conversion!(image::ImageError, Image);
source_conversion!(motion_model::Error, Model);
source_conversion!(std::time::SystemTimeError, Clock);

impl From<symphonia::core::errors::Error> for Error {
    fn from(error: symphonia::core::errors::Error) -> Self {
        Self::symphonia("probe", error)
    }
}
