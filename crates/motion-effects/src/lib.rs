//! Portable effect descriptions and shader compilation, without editor/GPU dependencies.
pub mod builtin;
pub mod image_contract;
mod package;
pub mod memory;
mod scene;
mod schema;
pub use scene::*;
mod native_ui;
pub use native_ui::*;
pub mod shader;
pub use package::{package_directory, package_directory_with_limits, package_source_plan,
    SourceFilePlan, SourcePackagePlan, EffectPackage, InstalledPlugin, Registry,
    PackageReadLimits, DEFAULT_DECODED_IMAGE_BUDGET, MAX_DECODED_IMAGE_BUDGET};
pub use schema::*;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("effect PNG resources exceed configured decoded image budget: package={package} resource={resource} required_bytes={required_bytes} budget_bytes={budget_bytes}; increase the decoded image allowance or reduce package resources")]
    DecodedResourceBudget {
        package: String,
        resource: String,
        required_bytes: u64,
        budget_bytes: u64,
    },
    #[error("{0}")]
    Invalid(String),
    #[error("WGSL parse (phase=wgsl_parse): {message}")]
    ShaderParse {
        message: String,
        #[source]
        cause: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("{context}: {cause}")]
    ShaderValidation {
        context: &'static str,
        #[source]
        cause: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("GLSL ES 300: phase={phase} stage={stage:?}: {cause}")]
    ShaderGlsl {
        phase: &'static str,
        stage: naga::ShaderStage,
        #[source]
        cause: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
}
pub type Result<T> = std::result::Result<T, Error>;
pub(crate) fn ensure(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(Error::Invalid(message.into()))
    }
}
