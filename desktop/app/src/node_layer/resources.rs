//! Project snapshots hold metadata; the portable graph format still holds PNGs.
use super::*;
use motion_core::{
    storage::{self, NodeResourceStage},
    NodeImageReference, NodeResource,
};
#[derive(Debug)]
pub enum SourceError {
    Model(motion_core::Error),
    Graph(motion_graph::Error),
    Json(serde_json::Error),
    Image {
        path: String,
        cause: Box<motion_graph::Error>,
    },
}
impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Model(e) => std::fmt::Display::fmt(e, f),
            Self::Graph(e) => std::fmt::Display::fmt(e, f),
            Self::Json(e) => std::fmt::Display::fmt(e, f),
            Self::Image { path, cause } => write!(
                f,
                "node source image phase=decode path={path} cause={cause}"
            ),
        }
    }
}
impl std::error::Error for SourceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Model(e) => e,
            Self::Graph(e) => e,
            Self::Json(e) => e,
            Self::Image { cause, .. } => cause.as_ref(),
        })
    }
}
impl From<motion_core::Error> for SourceError {
    fn from(error: motion_core::Error) -> Self {
        Self::Model(error)
    }
}
impl From<motion_graph::Error> for SourceError {
    fn from(error: motion_graph::Error) -> Self {
        Self::Graph(error)
    }
}
impl From<serde_json::Error> for SourceError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

fn image_reference(image: &motion_graph::GraphImage) -> NodeImageReference {
    NodeImageReference {
        width: image.width(),
        height: image.height(),
        resource: NodeResource {
            path: format!("assets/node-{}.png", image.id()),
            bytes: image.png().len() as u64,
            sha256: image.id().into(),
        },
    }
}
pub fn source_document(document: &GraphDocument) -> Result<Value, String> {
    document
        .validate_metadata_budget()
        .map_err(|e| e.to_string())?;
    document.validate_images().map_err(|e| e.to_string())?;
    let mut metadata = document.clone();
    metadata.images.clear();
    let mut value = serde_json::to_value(metadata).map_err(|e| e.to_string())?;
    if !document.images.is_empty() {
        value["images"] = serde_json::to_value(
            document
                .images
                .iter()
                .map(|(id, image)| (id, image_reference(image)))
                .collect::<std::collections::BTreeMap<_, _>>(),
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(value)
}
pub(super) fn runtime_resource(bytes: &[u8]) -> NodeResource {
    let sha256 = storage::resource_sha256(bytes);
    NodeResource {
        path: format!("assets/node-{sha256}.msfx"),
        bytes: bytes.len() as u64,
        sha256,
    }
}
pub(super) fn stage_images(
    root: &std::path::Path,
    document: &GraphDocument,
    existing: Option<&Value>,
) -> motion_core::Result<NodeResourceStage> {
    let mut staged = NodeResourceStage::new(root)?;
    let previous = existing
        .map(motion_core::node_resources::source_resources)
        .transpose()?
        .unwrap_or_default();
    for image in document.images.values() {
        let resource = image_reference(image).resource;
        if !previous.contains(&resource) {
            staged.stage(&resource, image.png())?;
        }
    }
    Ok(staged)
}
pub(super) fn commit_owned(
    session: &mut motion_host::Session,
    staged: NodeResourceStage,
    commands: Vec<motion_core::Command>,
) -> motion_host::Result<Value> {
    let revision = session.engine.revision();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        session.apply_command_batch(commands)
    }));
    // Sampling/snapshot can fail or unwind after the model actually commits.
    // Undo/redo still owns these files in that case; preserve the primary cause.
    if session.engine.revision() != revision {
        staged.keep_resources();
    }
    match result {
        Ok(result) => result,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}
/// CPU/file worker entry point; callers must not hydrate images in GUI paint.
pub fn load_document(source: &Value, root: &std::path::Path) -> Result<GraphDocument, SourceError> {
    motion_core::node_resources::source_resources(source)?;
    let mut metadata = source["document"].clone();
    let images = metadata
        .as_object_mut()
        .ok_or_else(|| motion_core::Error::Invalid("node source document missing".into()))?
        .remove("images");
    let mut document: GraphDocument = serde_json::from_value(metadata)?;
    if let Some(images) = images {
        let images: std::collections::BTreeMap<String, NodeImageReference> =
            serde_json::from_value(images)?;
        for (id, reference) in images {
            let bytes = storage::read_node_resource(root, &reference.resource)?;
            let image =
                motion_graph::GraphImage::from_png(bytes).map_err(|cause| SourceError::Image {
                    path: reference.resource.path.clone(),
                    cause: Box::new(cause),
                })?;
            if image.width() != reference.width
                || image.height() != reference.height
                || image.id() != id
            {
                return Err(motion_core::Error::Invalid(format!(
                    "node source image metadata does not match its pixels: path={}",
                    reference.resource.path
                ))
                .into());
            }
            document.images.insert(id, image);
        }
    }
    document.validate_images()?;
    document.validate_metadata_budget()?;
    if !(document.minimum_format_version()..=motion_graph::IMAGE_ASSETS_FORMAT_VERSION)
        .contains(&document.format_version)
    {
        return Err(
            motion_core::Error::Invalid("unsupported project node graph version".into()).into(),
        );
    }
    Ok(document)
}
