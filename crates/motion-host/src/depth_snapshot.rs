//! Immutable, source-checked depth ownership for an independent export task.
//! Inference workers, GPU handles and a live Session never enter a snapshot.
use crate::{Result, Session};
use motion_render::Scene;
use std::path::Path;
mod wire;

#[cfg(feature = "ai-depth")]
use crate::depth::{image::ImageInput, Cancellation, Estimate, Key};
#[cfg(feature = "ai-depth")]
use motion_model::depth::{PreparedDepthInput, SavedDepthCalibration, MAX_DEPTH_RESOURCE_BYTES};
#[cfg(feature = "ai-depth")]
use std::sync::Arc;

#[cfg(feature = "ai-depth")]
#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub object: u64,
    pub asset: motion_core::Asset,
    pub key: Key,
    pub input: PreparedDepthInput,
    pub calibration: SavedDepthCalibration,
    pub estimate: Arc<Estimate>,
    pub request: u64,
    pub decoded_budget: u64,
}

/// Creating this object requires current admissions, independently of a saved
/// calibration. Cloning it retains scalar ownership without copying planes.
#[derive(Clone, Debug, Default)]
pub struct DepthSnapshot {
    #[cfg(feature = "ai-depth")]
    pub(crate) composition: String,
    #[cfg(feature = "ai-depth")]
    pub(crate) entries: Vec<Entry>,
}

/// Admission after actual frozen-source verification. Only the verifier can
/// construct this guard; attaching raw retained metadata is not an export path.
pub struct VerifiedDepthSnapshot<'a> {
    snapshot: &'a DepthSnapshot,
}
impl VerifiedDepthSnapshot<'_> {
    pub fn attach(&self, scene: &mut Scene) -> Result<()> {
        self.snapshot.attach(scene)
    }
    pub fn layer_count(&self) -> usize {
        self.snapshot.layer_count()
    }
}

impl DepthSnapshot {
    /// Metadata-only capture on the Session owner. Poll its depth worker first;
    /// pending/stale data is refused instead of invoking inference or blocking.
    pub fn capture(session: &Session) -> Result<Self> {
        session.check_thread()?;
        #[cfg(feature = "ai-depth")]
        {
            session.capture_depth_snapshot()
        }
        #[cfg(not(feature = "ai-depth"))]
        {
            for id in session.engine.project().reachable_compositions()? {
                if session
                    .engine
                    .project()
                    .composition_view(&id)?
                    .layers
                    .iter()
                    .any(|layer| layer.three_d && (layer.depth_calibration.is_some() || layer.video_depth_calibration.is_some()))
                {
                    return Err("frozen calibrated depth requires the ai-depth feature".into());
                }
            }
            Ok(Self::default())
        }
    }

    pub fn layer_count(&self) -> usize {
        #[cfg(feature = "ai-depth")]
        {
            self.entries.len()
        }
        #[cfg(not(feature = "ai-depth"))]
        {
            0
        }
    }

    #[cfg(feature = "ai-depth")]
    pub(crate) fn validate(&self) -> Result<()> {
        if self.composition.len() > 256 || self.composition.chars().any(char::is_control) {
            return Err("invalid bounded frozen depth composition".into());
        }
        if self.entries.len() > motion_model::MAX_LAYERS {
            return Err("frozen depth layer count exceeds limit".into());
        }
        let mut identities = Vec::new();
        let mut bytes = 0usize;
        let mut objects = std::collections::HashSet::new();
        for entry in &self.entries {
            if entry.asset.path.len() > 1024
                || entry.asset.path.is_empty()
                || !Path::new(&entry.asset.path)
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_)))
                || [entry.asset.width, entry.asset.height] != entry.key.geometry.source_size
                || entry.estimate.provider.is_empty()
                || entry.estimate.provider.len() > 256
                || entry.estimate.provider.chars().any(char::is_control)
            {
                return Err("invalid bounded frozen depth source metadata".into());
            }
            if !objects.insert(entry.object) {
                return Err("frozen depth contains duplicate layer IDs".into());
            }
            if entry.asset.id != entry.key.source.asset {
                return Err("frozen depth asset differs from original request".into());
            }
            let descriptor = entry.estimate.resource.descriptor();
            if entry.key.source != descriptor.source
                || entry.key.geometry != descriptor.geometry
                || entry.key.model != descriptor.model
            {
                return Err("frozen depth resource differs from original request".into());
            }
            entry
                .calibration
                .rebind(&entry.estimate.resource, entry.input)?;
            let resource = &entry.estimate.resource;
            let identity = resource.cache_identity();
            if !identities.contains(&identity) {
                bytes = bytes
                    .checked_add(
                        resource.values().len() * 4
                            + resource.validity().map_or(0, |v| v.len())
                            + resource.confidence().map_or(0, |v| v.len() * 4),
                    )
                    .ok_or("frozen depth byte count overflow")?;
                if bytes > MAX_DEPTH_RESOURCE_BYTES {
                    return Err("frozen depth retained planes exceed 192 MiB".into());
                }
                identities.push(identity);
            }
        }
        Ok(())
    }

    /// Check that the exporter draws exactly the captured source/calibration
    /// mapping. A valid RGB fingerprint alone does not authorize relinked assets.
    pub fn validate_project(&self, project: &motion_core::Project) -> Result<()> {
        #[cfg(feature = "ai-depth")]
        self.validate()?;
        #[cfg(feature = "ai-depth")]
        let mut count = 0;
        #[cfg(not(feature = "ai-depth"))]
        let count = 0;
        for id in project.reachable_compositions()? {
            let view = project.composition_view(&id)?;
            for layer in view
                .layers
                .iter()
                .filter(|l| l.three_d && (l.depth_calibration.is_some() || l.video_depth_calibration.is_some()))
            {
                #[cfg(feature = "ai-depth")]
                {
                    count += 1;
                    let entry = self
                        .entries
                        .iter()
                        .find(|e| e.object == layer.id)
                        .ok_or("frozen depth project mapping has no admitted resource")?;
                    if id != self.composition
                        || !matches!(layer.content, motion_core::Content::Image { asset } if asset == entry.asset.id)
                        || !project.assets.contains(&entry.asset)
                        || layer.depth_calibration.as_ref() != Some(&entry.calibration)
                    {
                        return Err(
                            "frozen depth project source or calibration mapping changed".into()
                        );
                    }
                }
                #[cfg(not(feature = "ai-depth"))]
                return Err(format!(
                    "frozen calibrated depth object={} requires the ai-depth feature",
                    layer.id
                )
                .into());
            }
        }
        if count != self.layer_count() {
            return Err("frozen depth project layer set changed".into());
        }
        Ok(())
    }

    /// Verify the actual *frozen copy*, after source copying and before drawing.
    /// Invoke on the export worker, outside Session/registry locks. Source IO,
    /// decode and fingerprinting are bounded by the original staging policies.
    /// This does not run inference and never changes an installed live result.
    pub fn verify_sources(
        &self,
        root: &Path,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<VerifiedDepthSnapshot<'_>> {
        check()?;
        #[cfg(feature = "ai-depth")]
        {
            self.validate()?;
            let mut verified: Vec<&Entry> = Vec::new();
            for entry in &self.entries {
                if verified.iter().any(|prior| {
                    prior.key.same_sample(&entry.key)
                        && prior.input == entry.input
                        && prior.asset == entry.asset
                }) {
                    continue;
                }
                check()?;
                let began = std::time::Instant::now();
                log::info!("start frozen_depth operation=verify_source source_request={} object={} source_id={} phase=decode_frozen_rgb path={:?}",
                    entry.request, entry.object, entry.asset.id, entry.asset.path);
                log::logger().flush();
                let input = ImageInput {
                    key: entry.key.clone(), root: root.into(),
                    relative: entry.asset.path.clone(), budget: entry.decoded_budget,
                }.decode(&Cancellation::default()).map_err(|cause| crate::Error::with_context(
                    format!("frozen depth source_request={} object={} source_id={} phase=decode_frozen_rgb", entry.request, entry.object, entry.asset.id), cause))?;
                check()?;
                let actual = input.fingerprint(&Cancellation::default())?;
                if actual != entry.input {
                    return Err(format!("frozen depth source_request={} object={} source_id={} phase=verify_frozen_rgb: prepared RGB fingerprint changed",
                        entry.request, entry.object, entry.asset.id).into());
                }
                log::debug!("done frozen_depth operation=verify_source source_request={} source_id={} elapsed_ms={}",
                    entry.request, entry.asset.id, began.elapsed().as_millis());
                verified.push(entry);
            }
        }
        #[cfg(not(feature = "ai-depth"))]
        let _ = root;
        check()?;
        Ok(VerifiedDepthSnapshot { snapshot: self })
    }

    /// Static original-image results can be used throughout this frozen
    /// document's clock. This never relaxes live worker installation guards.
    /// All layers are prepared on a clone so refusal leaves the caller intact.
    fn attach(&self, scene: &mut Scene) -> Result<()> {
        if !scene.has_calibrated_depth() {
            return Ok(());
        }
        #[cfg(feature = "ai-depth")]
        {
            if scene.composition_id != self.composition
                || scene
                    .nested
                    .iter()
                    .any(|node| node.scene.has_calibrated_depth())
            {
                return Err("frozen depth phase=attach: nested or foreign composition depth is not supported".into());
            }
            let required: Vec<_> = scene
                .calibrated_depth_layers()
                .map(|(id, c)| (id, c.clone()))
                .collect();
            let mut prepared = scene.clone();
            for (object, calibration) in required {
                let original = scene.source_object(object);
                let entry = self
                    .entries
                    .iter()
                    .find(|e| e.object == original)
                    .ok_or_else(|| {
                        crate::Error::from(format!(
                            "frozen depth object={object} phase=attach: source pending or absent"
                        ))
                    })?;
                if entry.calibration != calibration {
                    return Err(format!(
                        "frozen depth object={object} phase=attach: saved calibration changed"
                    )
                    .into());
                }
                prepared.attach_calibrated_depth(
                    object,
                    &entry.key.source,
                    &entry.estimate.resource,
                    entry.input,
                )?;
            }
            *scene = prepared;
            Ok(())
        }
        #[cfg(not(feature = "ai-depth"))]
        {
            Err("frozen calibrated depth requires the ai-depth feature".into())
        }
    }
}
