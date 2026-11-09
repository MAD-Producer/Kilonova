use super::Scene;
use motion_model::depth::{
    DepthResource, DepthResourceIdentity, DepthSource, DepthSurface, PreparedDepthInput,
    RelativeDepthCalibration, SavedDepthCalibration, MAX_DEPTH_RESOURCE_BYTES,
};
use motion_model::{ensure, Result};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(super) struct BoundSurface {
    pub calibration: SavedDepthCalibration,
    source: DepthSource,
    input: PreparedDepthInput,
    resource: DepthResourceIdentity,
    reference_resource: Option<DepthResourceIdentity>,
    binding: Option<RelativeDepthCalibration>,
    mesh: Arc<DepthSurface>,
    image: Option<crate::DepthImage>,
}
impl Scene {
    pub(super) fn reset_depth(&mut self) {
        // A previous nested instance may have renderer aliases. Cache only the
        // authored IDs, then require a fresh current-source admission each sample.
        self.depth_surfaces = std::mem::take(&mut self.depth_surfaces)
            .into_iter()
            .map(|(id, surface)| (self.source_object(id), surface))
            .collect();
        for surface in self.depth_surfaces.values_mut() {
            surface.image = None;
        }
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
        calibration.validate_binding(resource, input)?;
        let binding = if calibration.is_physical() { None }
            else { Some(calibration.rebind(resource, input)?) };
        self.attach_bound_calibrated_depth(object, current, resource, input, binding.as_ref(), None)
    }

    /// An inference owner supplies a qualified mapped calibration, retaining
    /// its original reference and candidate instances. Recheck the authored
    /// calibration/source here; this renderer does not qualify affine fit or
    /// model accuracy. It never relabels the candidate as the reference model.
    pub fn attach_aligned_calibrated_depth(
        &mut self,
        object: u64,
        current: &DepthSource,
        reference: &DepthResource,
        candidate: &DepthResource,
        input: PreparedDepthInput,
        binding: &RelativeDepthCalibration,
        image: Option<crate::DepthImage>,
    ) -> Result<()> {
        let saved = self.depth_required.get(&object).ok_or_else(|| {
            motion_model::Error::Invalid(format!(
                "depth surface object={object}: no sampled render calibration"
            ))
        })?;
        let original = saved.rebind(reference, input)?;
        let first = reference.descriptor();
        let second = candidate.descriptor();
        ensure(
            current.same_sample(&first.source)
                && first.source.same_sample(&second.source)
                && first.geometry == second.geometry
                && first.size == second.size
                && first.measure == second.measure,
            "aligned depth render reference and candidate source, grid or meaning differ",
        )?;
        ensure(
            original.curve() == binding.curve()
                && original.extrapolation() == binding.extrapolation()
                && original.anchors().map(|anchor| anchor.camera_z_meters)
                    == binding.anchors().map(|anchor| anchor.camera_z_meters),
            "aligned depth render changed authored metric observations",
        )?;
        if let Some(image) = &image {
            ensure(
                image.size() == second.geometry.source_size,
                "aligned depth image dimensions differ from source geometry",
            )?;
        }
        self.attach_bound_calibrated_depth(
            object,
            current,
            candidate,
            input,
            Some(binding),
            Some(reference.cache_identity()),
        )?;
        if let Some(surface) = self.depth_surfaces.get_mut(&object) {
            surface.image = image;
        }
        Ok(())
    }

    /// Attach a physical candidate already qualified by the inference owner.
    /// This renderer rechecks saved-reference identity and physical meaning; it
    /// does not establish model accuracy or repeat the owner's held-out proof.
    /// Candidate values retain their own declared units, so conversion is done
    /// exactly once by camera-Z surface construction, without an affine remap.
    pub fn attach_aligned_physical_depth(
        &mut self,
        object: u64,
        current: &DepthSource,
        reference: &DepthResource,
        candidate: &DepthResource,
        input: PreparedDepthInput,
        image: Option<crate::DepthImage>,
    ) -> Result<()> {
        use motion_model::depth::{DepthAxis, DepthMeasure};
        let saved = self.depth_required.get(&object).ok_or_else(|| {
            motion_model::Error::Invalid(format!(
                "depth surface object={object}: no sampled physical placement"))
        })?;
        ensure(saved.is_physical(), "aligned physical depth requires a physical placement")?;
        saved.validate_binding(reference, input)?;
        let first = reference.descriptor();
        let second = candidate.descriptor();
        ensure(current.same_sample(&first.source)
            && first.source.same_sample(&second.source)
            && first.geometry == second.geometry && first.size == second.size,
            "aligned physical depth source or grid differs")?;
        ensure(matches!((first.measure, second.measure),
            (DepthMeasure::Metric { axis: DepthAxis::CameraZ, .. },
             DepthMeasure::Metric { axis: DepthAxis::CameraZ, .. })
            | (DepthMeasure::InverseMetric { axis: DepthAxis::CameraZ, .. },
               DepthMeasure::InverseMetric { axis: DepthAxis::CameraZ, .. })),
            "aligned physical depth representation or axis differs")?;
        if let Some(image) = &image {
            ensure(image.size() == second.geometry.source_size,
                "aligned physical depth image dimensions differ from source geometry")?;
        }
        self.attach_bound_calibrated_depth(object, current, candidate, input,
            None, Some(reference.cache_identity()))?;
        if let Some(surface) = self.depth_surfaces.get_mut(&object) {
            surface.image = image;
        }
        Ok(())
    }

    fn attach_bound_calibrated_depth(
        &mut self,
        object: u64,
        current: &DepthSource,
        resource: &DepthResource,
        input: PreparedDepthInput,
        binding: Option<&RelativeDepthCalibration>,
        reference_resource: Option<DepthResourceIdentity>,
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
        let depth = binding.map(|binding| binding.bind(resource)).transpose()?;
        let cached = self.depth_surfaces.get(&object).is_some_and(|surface| {
            surface.calibration == *calibration
                && surface.source.same_sample(current)
                && surface.input == input
                && surface.resource == resource.cache_identity()
                && surface.reference_resource == reference_resource
                && surface.binding.as_ref() == binding
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
            let budget = MAX_DEPTH_RESOURCE_BYTES.saturating_sub(retained);
            let mesh = Arc::new(match depth {
                Some(depth) => DepthSurface::with_byte_budget(&depth, reference,
                    calibration.pixels_per_metre(), budget)?,
                None => DepthSurface::from_camera_z(resource, reference,
                    calibration.pixels_per_metre(), budget)?,
            });
            self.depth_surfaces.insert(
                object,
                BoundSurface {
                    calibration: calibration.clone(),
                    source: current.clone(),
                    input,
                    resource: resource.cache_identity(),
                    reference_resource,
                    binding: binding.cloned(),
                    mesh,
                    image: None,
                },
            );
        }
        if let Some(surface) = self.depth_surfaces.get_mut(&object) {
            surface.image = None;
        }
        self.depth_pending.remove(&object);
        Ok(())
    }
    /// Pair depth with the exact original image decoded by its admission worker.
    /// This keeps color and geometry consistent after file changes/GPU eviction.
    pub fn attach_calibrated_depth_image(
        &mut self,
        object: u64,
        current: &DepthSource,
        resource: &DepthResource,
        input: PreparedDepthInput,
        image: crate::DepthImage,
    ) -> Result<()> {
        ensure(
            image.size() == resource.descriptor().geometry.source_size,
            "owned depth image dimensions differ from source geometry",
        )?;
        self.attach_calibrated_depth(object, current, resource, input)?;
        if let Some(surface) = self.depth_surfaces.get_mut(&object) {
            surface.image = Some(image);
        }
        Ok(())
    }
    pub(crate) fn calibrated_source_images(
        &self,
    ) -> impl Iterator<Item = (u64, &crate::DepthImage)> {
        self.layers.iter().filter_map(|layer| {
            if self.depth_pending.contains(&layer.id)
                || !self.depth_required.contains_key(&layer.id)
            {
                return None;
            }
            let surface = self.depth_surfaces.get(&layer.id)?;
            Some((layer.asset?, surface.image.as_ref()?))
        })
    }
    pub(crate) fn calibrated_video_images(
        &self,
    ) -> impl Iterator<Item = (u64, &crate::DepthImage)> {
        self.layers.iter().filter_map(|layer| {
            if !self.has_owned_video_depth(layer.id) {
                return None;
            }
            Some((
                layer.id,
                self.depth_surfaces.get(&layer.id)?.image.as_ref()?,
            ))
        })
    }
    pub(crate) fn has_owned_video_depth(&self, object: u64) -> bool {
        self.depth_required.contains_key(&object)
            && !self.depth_pending.contains(&object)
            && self
                .layers
                .iter()
                .any(|layer| layer.id == object && layer.video.is_some())
            && self
                .depth_surfaces
                .get(&object)
                .is_some_and(|surface| surface.image.is_some())
    }
    /// Original video layers whose colors still need a decoder. A calibrated
    /// video admission owns its exact display pixels and must not decode again.
    /// video_layers() intentionally keeps its complete source/capability meaning.
    pub fn video_decode_layers(&self) -> Vec<&crate::DrawLayer> {
        let mut layers: Vec<_> = self
            .layers
            .iter()
            .filter(|layer| layer.video.is_some() && !self.has_owned_video_depth(layer.id))
            .collect();
        for node in &self.nested {
            layers.extend(node.scene.video_decode_layers());
        }
        layers
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
