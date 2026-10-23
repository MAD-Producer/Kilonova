//! Native CPU image failures. Cancellation is a normal superseded request,
//! never inferred from an external decoder's display text.
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Allocation(#[from] std::collections::TryReserveError),
    #[error(transparent)]
    Image(#[from] image::ImageError),
    #[error(transparent)]
    Png(#[from] png::DecodingError),
    #[error(transparent)]
    Model(#[from] motion_model::Error),
    #[error("image decode superseded")]
    Cancelled,
    #[error("image decoder panicked: {0}")]
    WorkerPanic(String),
    #[error("image decode worker disconnected")]
    WorkerDisconnected,
    #[error("{0}")]
    Invalid(String),
    #[error("{message}")]
    Budget {
        rejection: crate::ResourceRejection,
        message: String,
    },
    /// Preserve the same original failure across repeated preview polls.
    #[error("{0}")]
    Shared(#[source] Arc<Error>),
}

impl Error {
    pub fn resource_rejection(&self) -> Option<&crate::ResourceRejection> {
        let mut error = self;
        loop {
            match error {
                Self::Budget { rejection, .. } => return Some(rejection),
                Self::Shared(cause) => error = cause,
                _ => return None,
            }
        }
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self::Invalid(message)
    }
}
impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self::Invalid(message.into())
    }
}
// Existing text-only export/application boundaries convert explicitly via `?`.
impl From<Error> for String {
    fn from(error: Error) -> Self {
        error.to_string()
    }
}
