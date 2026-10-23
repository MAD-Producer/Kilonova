use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    DocumentVersion,
    NodeVersion,
    InvalidValue,
    UnknownNode,
    UnsupportedNode,
    DuplicateNode,
    UnknownPort,
    TypeMismatch,
    DuplicateInput,
    MissingInput,
    Cycle,
    MissingOutput,
    MultipleOutputs,
    UnknownParameter,
    DuplicateParameter,
    Budget,
    Manifest,
    Shader,
    Io,
    Json,
    Zip,
    Publication,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub code: ErrorCode,
    pub node: Option<String>,
    pub port: Option<String>,
    pub message: String,
}
impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.code)?;
        if let Some(node) = &self.node {
            write!(f, " at node {node}")?;
        }
        if let Some(port) = &self.port {
            write!(f, ".{port}")?;
        }
        write!(f, ": {}", self.message)
    }
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(Diagnostic),
    #[error("{diagnostic}")]
    Effect {
        diagnostic: Diagnostic,
        #[source]
        cause: Box<motion_effects::Error>,
    },
    #[error("graph I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("graph JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("effect ZIP: {0}")]
    Zip(#[from] zip::result::ZipError),
}
pub type Result<T> = std::result::Result<T, Error>;
impl Error {
    pub fn diagnostic(&self) -> Diagnostic {
        match self {
            Self::Invalid(d) | Self::Effect { diagnostic: d, .. } => d.clone(),
            Self::Io(e) => Diagnostic {
                code: ErrorCode::Io,
                node: None,
                port: None,
                message: e.to_string(),
            },
            Self::Json(e) => Diagnostic {
                code: ErrorCode::Json,
                node: None,
                port: None,
                message: e.to_string(),
            },
            Self::Zip(e) => Diagnostic {
                code: ErrorCode::Zip,
                node: None,
                port: None,
                message: e.to_string(),
            },
        }
    }
    /// Preserve the existing diagnostic while retaining the native loader or
    /// compiler cause. The declared Box is exposed by Error::source().
    pub(crate) fn effect(code: ErrorCode, prefix: &'static str, cause: motion_effects::Error) -> Self {
        Self::Effect {
            diagnostic: Diagnostic {
                code,
                node: None,
                port: None,
                message: format!("{prefix}{cause}"),
            },
            cause: Box::new(cause),
        }
    }
    pub(crate) fn invalid(
        code: ErrorCode,
        node: Option<&str>,
        port: Option<&str>,
        message: impl Into<String>,
    ) -> Self {
        Self::Invalid(Diagnostic {
            code,
            node: node.map(str::to_owned),
            port: port.map(str::to_owned),
            message: message.into(),
        })
    }
}
