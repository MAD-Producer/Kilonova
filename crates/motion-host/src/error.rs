//! Native host errors. Text is retained only at providers which still expose
//! an untyped result; known native errors must use their source variant.
//! Context/cleanup sources retain their declared Box<Error> wrapper, matching
//! the media API's explicit Arc source wrappers. Inspect native variants inside.
use motion_core::composition::CompositionError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[cfg(feature = "ai-depth")]
    #[error(transparent)]
    Depth(#[from] crate::depth::Error),
    #[error(transparent)]
    Model(#[from] motion_core::Error),
    #[error(transparent)]
    Scene(#[from] motion_scene::Error),
    #[error(transparent)]
    Render(#[from] motion_render::RenderError),
    #[error(transparent)]
    ImageResource(#[from] motion_render::image_resources::Error),
    #[error(transparent)]
    Media(#[from] motion_media::Error),
    #[error(transparent)]
    Effects(#[from] motion_effects::Error),
    #[error(transparent)]
    Clock(#[from] std::time::SystemTimeError),
    #[error(transparent)]
    GpuPoll(#[from] wgpu::PollError),
    #[error("{context}: {cause}")]
    Context {
        context: String,
        #[source]
        cause: Box<Error>,
    },
    #[error("phase=project_cleanup {operation}={} cause={cause}", .path.display())]
    ProjectCleanup {
        operation: &'static str,
        path: std::path::PathBuf,
        #[source]
        cause: std::io::Error,
    },
    #[error("{primary}; cleanup also failed: {secondary}")]
    Cleanup {
        #[source]
        primary: Box<Error>,
        secondary: Box<Error>,
    },
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Image(#[from] image::ImageError),
    #[error(transparent)]
    Composition(#[from] CompositionError),
    #[error("{detail}")]
    CompositionContext {
        detail: CompositionError,
        #[source]
        cause: Box<Error>,
    },
    #[error(transparent)]
    Frozen(#[from] crate::ops::media::frozen::Error),
    /// Host validation messages and remaining explicitly untyped providers.
    /// This variant never infers an identity from a message prefix.
    #[error("{0}")]
    Message(String),
}

impl Error {
    pub fn with_context(context: impl Into<String>, cause: impl Into<Error>) -> Self {
        Self::Context {
            context: context.into(),
            cause: Box::new(cause.into()),
        }
    }

    pub fn composition_detail(&self) -> Option<&CompositionError> {
        match self {
            Self::Composition(detail) | Self::CompositionContext { detail, .. } => Some(detail),
            Self::Model(motion_core::Error::Composition(detail)) => Some(detail),
            Self::Render(error) => render_composition_detail(error),
            Self::ImageResource(error) => image_composition_detail(error),
            Self::Media(error) => media_composition_detail(error),
            Self::Frozen(crate::ops::media::frozen::Error::Session(cause)) => {
                cause.composition_detail()
            }
            Self::Frozen(crate::ops::media::frozen::Error::Model(
                motion_core::Error::Composition(detail),
            )) => Some(detail),
            Self::Frozen(crate::ops::media::frozen::Error::Media(error)) => {
                media_composition_detail(error)
            }
            Self::Context { cause, .. } => cause.composition_detail(),
            Self::Cleanup { primary, .. } => primary.composition_detail(),
            _ => None,
        }
    }
}

fn render_composition_detail(error: &motion_render::RenderError) -> Option<&CompositionError> {
    use motion_render::{composition_plan::CompositionPlanError, RenderError};
    let mut current = error;
    loop {
        match current {
            RenderError::Model(motion_core::Error::Composition(detail)) => return Some(detail),
            RenderError::ImageResource(error) => return image_composition_detail(error),
            RenderError::VectorGeometry(crate_error) => return match crate_error {
                motion_render::VectorGeometryError::Model(motion_core::Error::Composition(detail)) => Some(detail),
                _ => None,
            },
            RenderError::Context { cause, .. } | RenderError::Diagnostic { cause, .. } => {
                current = cause;
            }
            RenderError::Shared(cause) => current = cause,
            RenderError::CompositionPlan(CompositionPlanError::Encoding(cause)) => current = cause,
            _ => return None,
        }
    }
}

fn image_composition_detail(error: &motion_render::image_resources::Error) -> Option<&CompositionError> {
    use motion_render::image_resources::Error;
    let mut current = error;
    loop {
        match current {
            Error::Model(motion_core::Error::Composition(detail)) => return Some(detail),
            Error::Shared(cause) => current = cause,
            _ => return None,
        }
    }
}

fn media_composition_detail(error: &motion_media::Error) -> Option<&CompositionError> {
    match error.primary() {
        motion_media::Error::Model(model) => match model.as_ref() {
            motion_core::Error::Composition(detail) => Some(detail),
            _ => None,
        },
        _ => None,
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
/// Text-only application boundaries remain source-compatible through `?`.
impl From<Error> for String {
    fn from(error: Error) -> Self {
        error.to_string()
    }
}
