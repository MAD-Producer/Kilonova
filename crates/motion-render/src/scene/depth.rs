use super::Scene;
use motion_model::depth::{
    DepthResource, DepthSource, DepthSurface, PreparedDepthInput, SavedDepthCalibration,
    MAX_DEPTH_RESOURCE_BYTES,
};
use motion_model::{ensure, Result};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(super) struct BoundSurface {
    pub calibration: SavedDepthCalibration,
    source: DepthSource,
    input: PreparedDepthInput,
    mesh: Arc<DepthSurface>,
}
impl Scene {
    pub(super) fn reset_depth(&mut self) {
        // A previous nested instance may have renderer aliases. Cache only the
        // authored IDs, then require a fresh current-source admission each sample.
        self.depth_surfaces = std::mem::take(&mut self.depth_surfaces)
            .into_iter()
            .map(|(id, surface)| (self.source_object(id), surface))
            .collect();
        self.depth_required.clear();
        self.depth_pending.clear();
        self.shared_rasters.clear();
    }
    pub fn calibrated_depth_layers(&self) -> impl Iterator<Item = (u64, &SavedDepthCalibration)> {
        self.depth_required
            .iter()
            .map(|(&id, calibration)| (id, calibration))
    }
    pub fn has_calibrated_depth(&self) -> bool {
        !self.depth_required.is_empty()
            || self.nested.iter().any(|n| n.scene.has_calibrated_depth())
    }
    /// The caller supplies independently guarded current source identity, rather
    /// than admitting a resource by copying its own revision into the guard.
    pub fn attach_calibrated_depth(
        &mut self,
        object: u64,
        current: &DepthSource,
        resource: &DepthResource,
        input: PreparedDepthInput,
    ) -> Result<()> {
        let calibration = self.depth_required.get(&object).ok_or_else(|| {
            motion_model::Error::Invalid(format!(
                "depth surface object={object}: no sampled render calibration"
            ))
        })?;
        ensure(
            current.same_sample(&resource.descriptor().source),
            &format!(
                "depth surface object={object} source={} phase=admit: current source differs",
                current.asset
            ),
        )?;
        let binding = calibration.rebind(resource, input)?;
        let depth = binding.bind(resource)?;
        let cached = self.depth_surfaces.get(&object).is_some_and(|surface| {
            surface.calibration == *calibration
                && surface.source.same_sample(current)
                && surface.input == input
        });
        if !cached {
            let reference = calibration.reference().ok_or_else(|| {
                motion_model::Error::Invalid(format!(
                    "depth surface object={object}: reference camera is missing"
                ))
            })?;
            let retained: usize = self
                .depth_surfaces
                .iter()
                .filter(|(id, _)| **id != object)
                .map(|(_, s)| s.mesh.bytes())
                .sum();
            let mesh = Arc::new(DepthSurface::with_byte_budget(
                &depth,
                reference,
                calibration.pixels_per_metre(),
                MAX_DEPTH_RESOURCE_BYTES.saturating_sub(retained),
            )?);
            self.depth_surfaces.insert(
                object,
                BoundSurface {
                    calibration: calibration.clone(),
                    source: current.clone(),
                    input,
                    mesh,
                },
            );
        }
        self.depth_pending.remove(&object);
        Ok(())
    }
    pub(crate) fn depth_surface(&self, object: u64) -> Option<Arc<DepthSurface>> {
        if self.depth_pending.contains(&object) {
            return None;
        }
        self.depth_surfaces.get(&object).map(|s| s.mesh.clone())
    }
    pub(crate) fn validate_depth_ready(&self) -> Result<()> {
        for (&object, calibration) in &self.depth_required {
            ensure(self.depth_surface(object).is_some(), &format!(
                "depth surface object={object} source={} phase=render: current calibrated result pending", calibration.asset()))?;
        }
        for child in &self.nested {
            child.scene.validate_depth_ready()?;
        }
        Ok(())
    }
}
