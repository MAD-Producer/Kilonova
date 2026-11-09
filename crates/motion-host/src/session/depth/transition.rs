//! Explicit Session ownership for a managed-image quality transition. This
//! samples aligned relative depth; it does not relabel a saved calibration or
//! implicitly install relative values as renderer hardware/camera depth.
use super::{Binding, Session, Source};
use crate::depth::{
    self,
    alignment::Limits,
    image::ImageInput,
    transition::{
        admission::{Admission, View},
        Activity, Choices, Controller, Policy,
    },
    Identity, PreparedEstimate,
};
use motion_model::depth::DepthModel;
use std::time::Duration;

pub(super) struct ImageTransition {
    controller: Controller,
    reference: PreparedEstimate,
    scope: Binding,
    choices: Choices,
    limits: Limits,
    admitted: Option<Admission>,
}

impl ImageTransition {
    pub(super) fn applies_to(&self, asset: u64) -> bool {
        self.choices.interactive.source.asset == asset
    }
    pub(super) fn usage(&self) -> (usize, usize, usize) {
        self.admitted.as_ref().map_or((0, 0, 0), |admitted| {
            let (scalar, image) = admitted.candidate_usage();
            (1, scalar, image)
        })
    }
}

fn transition_error(cause: depth::transition::Error) -> crate::Error {
    crate::Error::NativeSource {
        cause: Box::new(cause),
    }
}

impl Session {
    /// Freeze a selected, already qualified live admission without polling,
    /// inference, plane scans or source IO on the Session owner.
    pub(super) fn freeze_image_depth_transition(
        &self,
        composition: &str,
        object: u64,
        saved: &motion_model::depth::SavedDepthCalibration,
    ) -> crate::Result<Option<crate::depth_snapshot::Entry>> {
        let Some(state) = self.depth.as_ref() else {
            return Ok(None);
        };
        let Some(scope) = state.transition_export.as_ref().filter(|scope| {
            scope.composition == composition
                && scope.document_matches(self)
                && matches!(&scope.source, Source::Image(asset) if asset.id == saved.asset())
        }) else {
            return Ok(None);
        };
        let pending = || {
            crate::Error::from(format!(
            "frozen depth object={object} phase=capture_alignment: current qualified transition pending or absent"))
        };
        if !scope.current(self)? {
            return Err(pending());
        }
        let transition = state.transition.as_ref().ok_or_else(pending)?;
        if composition != transition.scope.composition || !transition.scope.current(self)? {
            return Err(pending());
        }
        let binding = state.binding.as_ref().ok_or_else(pending)?;
        if !binding.current(self)? {
            return Err(pending());
        }
        let (Source::Image(asset), Identity::Sample(live)) = (&binding.source, &binding.key) else {
            return Err(pending());
        };
        let view = self.engine.project().composition_view(composition)?;
        let layer = view
            .layers
            .iter()
            .find(|l| l.id == object)
            .ok_or_else(pending)?;
        if !matches!(&layer.content, motion_core::Content::Image { asset: id } if *id == asset.id) {
            return Err(pending());
        }
        let admission = transition.admitted.as_ref().ok_or_else(pending)?;
        let (estimate, input, alignment) = admission
            .freeze(&transition.controller, live, &transition.reference)?
            .ok_or_else(pending)?;
        alignment.calibration(&estimate, live, saved, input)?;
        Ok(Some(crate::depth_snapshot::Entry {
            object,
            asset: asset.clone(),
            key: live.clone(),
            input,
            calibration: saved.clone(),
            estimate,
            alignment: Some(alignment),
            request: binding.request,
            decoded_budget: binding.decoded_budget,
        }))
    }

    pub(super) fn attach_image_depth_transition(
        &mut self,
        scene: &mut motion_render::Scene,
        object: u64,
        saved: &motion_model::depth::SavedDepthCalibration,
    ) -> crate::Result<Option<bool>> {
        let applies = self
            .depth
            .as_ref()
            .and_then(|state| state.transition.as_ref())
            .is_some_and(|transition| transition.applies_to(saved.asset()));
        if !applies {
            return Ok(None);
        }
        let scope = &self
            .depth
            .as_ref()
            .and_then(|state| state.transition.as_ref())
            .ok_or(depth::Error::Invalid("image transition disappeared"))?
            .scope;
        if scene.composition_id != scope.composition {
            return Ok(Some(false));
        }
        let source_object = scene.source_object(object);
        let project = self
            .engine
            .project()
            .composition_view(&scene.composition_id)?;
        let image_layer = project.layers.iter().any(|layer| layer.id == source_object
            && matches!(&layer.content, motion_core::Content::Image { asset } if *asset == saved.asset()));
        // An equal numeric asset ID or time cannot authorize using an image
        // transition for a video, another composition or a different layer kind.
        if !image_layer {
            return Ok(None);
        }
        let Some(binding) = self.depth.as_ref().and_then(|state| state.binding.as_ref()) else {
            return Ok(Some(false));
        };
        // Capture the independently current Session request, not the candidate's
        // own metadata. The view rechecks scope and instance proof below.
        if !binding.current(self)? {
            return Ok(Some(false));
        }
        let Identity::Sample(live) = &binding.key else {
            return Ok(Some(false));
        };
        let current = live.source.clone();
        let Some(view) = self.image_depth_transition()? else {
            return Ok(Some(false));
        };
        let mapped = view.calibration(saved)?;
        scene.attach_aligned_calibrated_depth(
            object,
            &current,
            view.reference_resource(),
            view.candidate_resource(),
            view.input(),
            &mapped,
            view.image().cloned(),
        )?;
        Ok(Some(true))
    }

    /// Opt into two explicitly qualified configurations after installing an
    /// actual managed-image reference with request_image_depth/poll_depth.
    /// Resolution and backend names are never interpreted as quality evidence.
    /// Sampling must subsequently use image_depth_transition, not poll_depth:
    /// returning the candidate's raw Estimate would discard its alignment proof.
    pub fn begin_image_depth_transition(
        &mut self,
        settled: DepthModel,
        policy: Policy,
        limits: Limits,
    ) -> crate::Result<()> {
        self.check_thread()?;
        self.synchronize_depth()?;
        settled.validate()?;
        limits
            .validate()
            .map_err(|cause| depth::Error::Alignment(cause.into()))?;
        let controller = Controller::new(policy).map_err(transition_error)?;
        let state = self
            .depth
            .as_ref()
            .ok_or(depth::Error::Invalid("depth backend not configured"))?;
        if state.transition.is_some() {
            return Err(depth::Error::Invalid("image depth transition already configured").into());
        }
        let scope = state
            .binding
            .as_ref()
            .ok_or(depth::Error::Invalid("image depth reference is absent"))?;
        let reference = state
            .prepared
            .as_ref()
            .ok_or(depth::Error::Invalid("image depth reference is pending"))?;
        if !matches!(scope.source, Source::Image(_)) || !scope.current(self)? {
            return Err(depth::Error::Invalid("current managed image reference required").into());
        }
        let Identity::Sample(interactive) = &scope.key else {
            return Err(
                depth::Error::Invalid("image transition cannot use a video reference").into(),
            );
        };
        if reference.estimate().resource.descriptor().model != interactive.model {
            return Err(depth::Error::Invalid(
                "image transition reference differs from live binding",
            )
            .into());
        }
        let mut settled_key = interactive.clone();
        settled_key.model = settled;
        let transition = ImageTransition {
            controller,
            reference: reference.clone(),
            scope: scope.clone(),
            choices: Choices {
                interactive: interactive.clone(),
                settled: settled_key,
            },
            limits,
            admitted: None,
        };
        let state = self.depth
            .as_mut()
            .ok_or(depth::Error::Invalid("depth owner disappeared"))?;
        state.transition_export = Some(transition.scope.clone());
        state.transition = Some(transition);
        Ok(())
    }

    /// Hysteresis produces at most one new managed-image request per transition.
    /// The original Session source/document binding is checked independently;
    /// all IO, decode, inference and alignment occur on its existing depth worker.
    pub fn request_image_depth_transition(
        &mut self,
        activity: Activity,
        now: Duration,
    ) -> crate::Result<Option<u64>> {
        self.check_thread()?;
        self.synchronize_depth()?;
        let Some(scope) = self
            .depth
            .as_ref()
            .and_then(|s| s.transition.as_ref())
            .map(|t| t.scope.clone())
        else {
            return Err(depth::Error::Invalid("image depth transition not configured").into());
        };
        if !scope.current(self)? {
            self.cancel_depth()?;
            return Ok(None);
        }
        let state = self
            .depth
            .as_mut()
            .ok_or(depth::Error::Invalid("depth owner disappeared"))?;
        let transition = state
            .transition
            .as_mut()
            .ok_or(depth::Error::Invalid("image transition disappeared"))?;
        let Some(ticket) = transition
            .controller
            .observe(transition.choices.clone(), activity, now)
            .map_err(transition_error)?
        else {
            return Ok(None);
        };
        // A new generation revokes old sampling even if submission fails. Do
        // not implicitly retry a failed submission during ordinary polling.
        transition.admitted = None;
        let Source::Image(asset) = &scope.source else {
            return Err(depth::Error::Invalid("image transition source is not an image").into());
        };
        let request = transition
            .controller
            .request_aligned_image(
                &ticket,
                &mut state.jobs,
                ImageInput {
                    key: ticket.key().clone(),
                    root: scope.root.clone(),
                    relative: asset.path.clone(),
                    budget: scope.decoded_budget,
                },
                &transition.reference,
                transition.limits,
            )
            .map_err(transition_error)?;
        if let Some(request) = request {
            let mut binding = scope;
            binding.request = request;
            binding.key = Identity::Sample(ticket.key().clone());
            state.binding = Some(binding);
        }
        Ok(request)
    }

    /// Admit only a completed proof from this Session's current worker/request.
    /// Normal pending/superseded work is false. Capacity rejection preserves the
    /// original reference and never silently admits the candidate's raw values.
    pub fn poll_image_depth_transition(&mut self) -> crate::Result<bool> {
        self.check_thread()?;
        self.synchronize_depth()?;
        let Some(state) = self.depth.as_mut() else {
            return Ok(false);
        };
        let Some(transition) = state.transition.as_mut() else {
            return Ok(false);
        };
        let Some(binding) = state.binding.as_ref() else {
            return Ok(false);
        };
        let Identity::Sample(live) = &binding.key else {
            return Ok(false);
        };
        let Some(ticket) = transition.controller.current() else {
            return Ok(false);
        };
        if let Some(admitted) = transition
            .controller
            .take_aligned_ready(ticket, &mut state.jobs, live)
            .map_err(transition_error)?
        {
            let required_bytes =
                state.retained.bytes + state.retained.image_bytes + admitted.candidate_bytes();
            let byte_limit = motion_model::depth::MAX_DEPTH_RESOURCE_BYTES;
            let required_samples = state.retained.entries.len() + 1;
            let sample_limit = motion_model::MAX_LAYERS;
            if required_bytes > byte_limit || required_samples > sample_limit {
                state.retained.rejections = state.retained.rejections.saturating_add(1);
                let cause = depth::Error::RetentionCapacity {
                    request: binding.request,
                    asset: live.source.asset,
                    required_bytes,
                    byte_limit,
                    required_samples,
                    sample_limit,
                };
                log::error!("operation=depth_transition_session_admit request={} source_id={} cause={cause}", binding.request, live.source.asset);
                return Err(cause.into());
            }
            transition.admitted = Some(admitted);
        }
        Ok(transition.admitted.is_some())
    }

    /// The borrow prevents Session mutation while aligned samples are used.
    /// Every call rechecks the independently live Session binding before the
    /// admission checks its controller, generation and exact resource instances.
    pub fn image_depth_transition(&mut self) -> crate::Result<Option<View<'_>>> {
        if !self.poll_image_depth_transition()? {
            return Ok(None);
        }
        let Some(state) = self.depth.as_ref() else {
            return Ok(None);
        };
        let Some(binding) = state.binding.as_ref() else {
            return Ok(None);
        };
        if !binding.current(self)? {
            return Ok(None);
        }
        let Identity::Sample(live) = &binding.key else {
            return Ok(None);
        };
        let Some(transition) = state.transition.as_ref() else {
            return Ok(None);
        };
        let Some(admitted) = transition.admitted.as_ref() else {
            return Ok(None);
        };
        admitted
            .view(&transition.controller, live)
            .map_err(|cause| depth::Error::Alignment(cause.into()).into())
    }
}
