/// Native causes from CPU path processing and triangle generation.
#[derive(Debug, thiserror::Error)]
pub enum VectorGeometryError {
    #[error("{0}")]
    Model(#[from] motion_model::Error),
    #[error("vector {phase}: {cause:?}")]
    Tessellation {
        phase: &'static str,
        #[source]
        cause: lyon_tessellation::TessellationError,
    },
    #[error("{0}")]
    Invalid(String),
}

impl From<String> for VectorGeometryError {
    fn from(message: String) -> Self {
        Self::Invalid(message)
    }
}

impl From<&str> for VectorGeometryError {
    fn from(message: &str) -> Self {
        Self::Invalid(message.to_owned())
    }
}
