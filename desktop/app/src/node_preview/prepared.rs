//! An owned compiled graph and its bounded decoded resources cannot be paired
//! independently. Interactive callers construct this on the CPU compiler worker.
use super::*;
pub struct PreparedGraph {
    pub(super) compiled: motion_graph::CompiledGraph,
    pub(super) pixels: Vec<image::RgbaImage>,
}
impl std::ops::Deref for PreparedGraph {
    type Target = motion_graph::CompiledGraph;
    fn deref(&self) -> &Self::Target {
        &self.compiled
    }
}
impl PreparedGraph {
    // Existing GPU tests can corrupt candidate data without exposing mutable
    // access to the production compiled/resource pairing.
    #[cfg(test)]
    pub(crate) fn source_for_test(&mut self) -> &mut motion_graph::CompiledGraph {
        &mut self.compiled
    }
    pub fn new(compiled: motion_graph::CompiledGraph) -> Result<Self, RenderError> {
        let pixels = decode_resources(&compiled)?;
        Ok(Self { compiled, pixels })
    }
}
pub(super) fn decode_resources(
    compiled: &motion_graph::CompiledGraph,
) -> Result<Vec<image::RgbaImage>, RenderError> {
    let definition = compiled
        .manifest
        .effects
        .first()
        .ok_or("node preview has no effect")?;
    if definition.resources.len() > 4 {
        return Err("node preview has more than four resources".into());
    }
    let mut decoded = 0;
    let mut pixels = Vec::new();
    for path in &definition.resources {
        let encoded = compiled
            .resource(path)
            .ok_or("node preview resource is missing")?;
        let image = motion_render::image_resources::decode_png_with_budget(
            encoded,
            motion_effects::DEFAULT_DECODED_IMAGE_BUDGET - decoded,
        )
        .map_err(|cause| {
            let cause = RenderError::from(cause);
            RenderError::Context {
                message: format!("node preview phase=resource_decode path={path}: {cause}"),
                cause: Box::new(cause),
            }
        })?;
        decoded += u64::from(image.width()) * u64::from(image.height()) * 4;
        pixels.push(image);
    }
    Ok(pixels)
}
/// Direct acceptance/test callers may submit a compiled source. Production live
/// editing uses the Prepared variant exclusively; this fallback retains the
/// existing serial GPU validation protocol for explicit native acceptance calls.
pub enum InstallGraph {
    Prepared(PreparedGraph),
    Source(motion_graph::CompiledGraph),
}
impl From<PreparedGraph> for InstallGraph {
    fn from(value: PreparedGraph) -> Self {
        Self::Prepared(value)
    }
}
impl From<motion_graph::CompiledGraph> for InstallGraph {
    fn from(value: motion_graph::CompiledGraph) -> Self {
        Self::Source(value)
    }
}
impl InstallGraph {
    pub fn prepare(self) -> Result<PreparedGraph, RenderError> {
        match self {
            Self::Prepared(graph) => Ok(graph),
            Self::Source(graph) => PreparedGraph::new(graph),
        }
    }
}
