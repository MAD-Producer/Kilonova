//! Native CPU image failures. Cancellation is a normal superseded request,
//! never inferred from an external decoder's display text.
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
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
    /// Preserve the same original failure across repeated preview polls.
    #[error("{0}")]
    Shared(#[source] Arc<Error>),
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
