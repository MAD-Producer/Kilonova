//! Optional session-owned depth results. The live document stamp is read from
//! Session at both sampling and installation, never supplied by a poll caller.
use super::{Result, Session};
use crate::depth::{self, image::ImageInput, Backend, Estimate, Jobs, Key, SessionStatus};
use motion_core::Asset;
use motion_model::depth::{DepthGeometry, DepthModel, DepthSource, DepthTime};
use std::{path::PathBuf, sync::Arc};

pub(super) struct State {
    jobs: Jobs,
    binding: Option<Binding>,
    installed: Option<Arc<Estimate>>,
}
struct Binding {
    request: u64,
    key: Key,
    document: u64,
    revision: u64,
    frame: f64,
    composition: String,
    root: PathBuf,
    asset: Asset,
    decoded_budget: u64,
}
impl Binding {
    fn current(&self, session: &Session) -> bool {
        self.document == session.document_generation
            && self.revision == session.engine.revision()
            && self.frame == session.frame
            && self.composition == session.engine.project().composition_id
            && self.root == session.root
            && self.decoded_budget == session.effects.memory_policy().decoded_image_budget_bytes
            && session
                .engine
                .project()
                .assets
                .iter()
                .any(|asset| asset == &self.asset)
    }
}
impl Session {
    /// Opt in with a trusted native backend (for example depth::onnx::Onnx).
    /// The default Session creates no inference worker or runtime. Configure
    /// once per Session; cancel does not replace or restart a failed backend.
    pub fn configure_depth(&mut self, backend: impl Backend) -> Result<()> {
        self.check_thread()?;
        if self.depth.is_some() {
            return Err(
                depth::Error::Invalid("depth backend already configured for this session").into(),
            );
        }
        self.depth = Some(State {
            jobs: Jobs::start(backend)?,
            binding: None,
            installed: None,
        });
        Ok(())
    }

    /// Stage an actual registered static image on the depth worker. File IO,
    /// decode, crop, rotation and inference occur after its published start.
    /// Static images use source PTS 0/1; the composition frame stays in the live
    /// installation guard. Video needs a separate decoder-PTS contract.
    pub fn request_image_depth(
        &mut self,
        asset: u64,
        geometry: DepthGeometry,
        model: DepthModel,
    ) -> Result<u64> {
        self.check_thread()?;
        self.synchronize_depth()?;
        geometry.validate()?;
        model.validate()?;
        if !self.frame.is_finite() {
            return Err(depth::Error::Invalid("nonfinite session frame").into());
        }
        let asset = self
            .engine
            .project()
            .assets
            .iter()
            .find(|a| a.id == asset)
            .ok_or(depth::Error::Invalid(
                "registered depth image asset not found",
            ))?
            .clone();
        if geometry.source_size != [asset.width, asset.height] {
            return Err(depth::Error::Invalid(
                "depth geometry differs from registered image dimensions",
            )
            .into());
        }
        if asset.width > 4096 || asset.height > 4096 {
            return Err(depth::Error::Invalid(
                "depth image source requires explicit resize to at most 4096",
            )
            .into());
        }
        let key = Key {
            source: DepthSource {
                asset: asset.id,
                revision: self.engine.revision(),
                pts: DepthTime {
                    ticks: 0,
                    timescale: 1,
                },
            },
            geometry,
            model,
        };
        let depth = self
            .depth
            .as_mut()
            .ok_or(depth::Error::Invalid("depth backend not configured"))?;
        let request = depth.jobs.request_image(ImageInput {
            key: key.clone(),
            root: self.root.clone(),
            relative: asset.path.clone(),
            budget: self.effects.memory_policy().decoded_image_budget_bytes,
        })?;
        depth.binding = Some(Binding {
            request,
            key,
            document: self.document_generation,
            revision: self.engine.revision(),
            frame: self.frame,
            composition: self.engine.project().composition_id.clone(),
            root: self.root.clone(),
            asset,
            decoded_budget: self.effects.memory_policy().decoded_image_budget_bytes,
        });
        depth.installed = None;
        Ok(request)
    }

    /// Install or reuse only a result for the current independently checked
    /// document/revision/frame/source. A pending/canceled/stale result is None.
    /// The resource retains its declared relative/metric meaning; it is never
    /// implicitly bound as renderer hardware depth.
    pub fn poll_depth(&mut self) -> Result<Option<Arc<Estimate>>> {
        self.check_thread()?;
        self.synchronize_depth()?;
        let Some(state) = &mut self.depth else {
            return Ok(None);
        };
        let Some(binding) = &state.binding else {
            return Ok(None);
        };
        if let Some(estimate) = state.jobs.take_ready(&binding.key)? {
            state.installed = Some(Arc::new(estimate));
        }
        Ok(state.installed.clone())
    }

    pub fn cancel_depth(&mut self) -> Result<()> {
        self.check_thread()?;
        if let Some(state) = &mut self.depth {
            // Clear ownership even when cancellation finds a poisoned worker.
            state.binding = None;
            state.installed = None;
            state.jobs.cancel()?;
        }
        Ok(())
    }

    pub fn depth_status(&mut self) -> Result<Option<SessionStatus>> {
        self.check_thread()?;
        self.synchronize_depth()?;
        self.depth
            .as_ref()
            .map(|state| {
                Ok(SessionStatus {
                    request: state.binding.as_ref().map(|b| b.request),
                    installed: state.installed.is_some(),
                    counts: state.jobs.counts()?,
                    in_flight: state.jobs.in_flight()?,
                    queue_depth: state.jobs.queue_depth()?,
                })
            })
            .transpose()
    }

    pub(super) fn synchronize_depth(&mut self) -> Result<()> {
        if self
            .depth
            .as_ref()
            .and_then(|state| state.binding.as_ref())
            .is_some_and(|binding| !binding.current(self))
        {
            self.cancel_depth()?;
        }
        Ok(())
    }
}
