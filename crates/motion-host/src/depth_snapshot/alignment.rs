//! Frozen choice and an instance-bound affine proof. Capturing a live proof is
//! metadata-only; reconstructed planes must be qualified again on the exporter.
use crate::{
    depth::{
        alignment::{Alignment, Limits},
        transition::{Choices, Phase, Policy},
        Estimate, Key,
    },
    Result,
};
use motion_model::depth::{PreparedDepthInput, RelativeDepthCalibration, SavedDepthCalibration};
use std::{sync::Arc, time::Instant};
#[cfg(test)]
mod tests;

/// Current wire policy only. No fallback or implicit model upgrade on export.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Spec {
    pub alignment_version: u32,
    pub transition_version: u32,
    pub generation: u64,
    pub phase: Phase,
    pub choices: Choices,
    pub policy: Policy,
    pub limits: Limits,
    pub coefficients: [f64; 2],
}

impl Spec {
    pub(crate) fn validate(&self, key: &Key) -> Result<()> {
        if self.alignment_version != crate::depth::alignment::VERSION
            || self.transition_version != crate::depth::transition::VERSION
        {
            return Err("unsupported frozen depth alignment or transition policy version".into());
        }
        if self.generation == 0
            || self.policy.idle_delay.is_zero()
            || self.policy.minimum_interactive_residence.is_zero()
            || !self.coefficients.iter().all(|value| value.is_finite())
            || self.coefficients[0] <= 0.
        {
            return Err(
                "invalid frozen depth transition generation, policy or coefficients".into(),
            );
        }
        self.choices
            .validate()
            .map_err(|cause| crate::Error::NativeSource {
                cause: Box::new(cause),
            })?;
        self.limits
            .validate()
            .map_err(|cause| crate::Error::NativeSource {
                cause: Box::new(cause),
            })?;
        let selected = match self.phase {
            Phase::Interactive => &self.choices.interactive,
            Phase::Settled => &self.choices.settled,
        };
        if selected != key {
            return Err(
                "frozen depth selected phase differs from the actual candidate request".into(),
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Frozen {
    reference: Arc<Estimate>,
    proof: Alignment,
    spec: Spec,
}

impl Frozen {
    /// Called only after actual live controller/worker admission. Cloning an
    /// immutable reference/proof does not scan planes, fit, decode or do IO.
    pub(crate) fn from_live(
        reference: Arc<Estimate>,
        candidate: &Estimate,
        proof: &Alignment,
        spec: Spec,
        key: &Key,
    ) -> Result<Self> {
        let value = Self {
            reference,
            proof: proof.clone(),
            spec,
        };
        value.validate(candidate, key)?;
        Ok(value)
    }

    /// Wire metadata and content digests must be verified before this call.
    /// A serialized coefficient pair is never accepted as an instance proof.
    pub(crate) fn restore(
        reference: Arc<Estimate>,
        candidate: &Estimate,
        spec: Spec,
        key: &Key,
        request: u64,
        object: u64,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<Self> {
        check()?;
        spec.validate(key)?;
        let began = Instant::now();
        let phase = if matches!(reference.resource.descriptor().measure,
            motion_model::depth::DepthMeasure::Relative { .. }) {
            "fit_reconstructed_planes"
        } else { "verify_physical_units" };
        log::info!("start frozen_depth operation=restore_alignment source_request={request} object={object} source_id={} generation={} phase={phase}", key.source.asset, spec.generation);
        log::logger().flush();
        let mut canceled = None;
        let fitted = Alignment::for_transition(
            &reference.resource,
            &candidate.resource,
            spec.limits,
            || match check() {
                Ok(()) => false,
                Err(cause) => {
                    canceled = Some(cause);
                    true
                }
            },
        );
        // Preserve the original caller cancellation; do not replace it with a
        // generic alignment error or continue an independent export silently.
        if let Some(cause) = canceled {
            return Err(cause);
        }
        let proof = fitted.map_err(|cause| {
            log::error!("operation=frozen_depth_restore_alignment source_request={request} object={object} source_id={} elapsed_ms={} cause={cause}", key.source.asset, began.elapsed().as_millis());
            crate::Error::NativeSource { cause: Box::new(cause) }
        })?;
        check()?;
        let value = Self {
            reference,
            proof,
            spec,
        };
        value.validate(candidate, key)?;
        log::debug!("done frozen_depth operation=restore_alignment source_request={request} object={object} source_id={} elapsed_ms={}", key.source.asset, began.elapsed().as_millis());
        Ok(value)
    }

    fn validate(&self, candidate: &Estimate, key: &Key) -> Result<()> {
        self.spec.validate(key)?;
        if self.proof.coefficients() != self.spec.coefficients {
            return Err("frozen depth coefficients differ from the qualified planes".into());
        }
        let descriptor = candidate.resource.descriptor();
        let reference = self.reference.resource.descriptor();
        if self.reference.provider.is_empty()
            || self.reference.provider.len() > 256
            || self.reference.provider.chars().any(char::is_control)
        {
            return Err("invalid bounded frozen alignment reference provider".into());
        }
        let interactive = &self.spec.choices.interactive;
        if interactive.source != reference.source
            || interactive.geometry != reference.geometry
            || interactive.model != reference.model
        {
            return Err("frozen alignment reference differs from the interactive request".into());
        }
        if key.source != descriptor.source
            || key.geometry != descriptor.geometry
            || key.model != descriptor.model
        {
            return Err("frozen aligned resource differs from the actual selected request".into());
        }
        self.proof
            .bind(&self.reference.resource, &candidate.resource)
            .map_err(|cause| crate::Error::NativeSource {
                cause: Box::new(cause),
            })?;
        Ok(())
    }

    /// Metadata-only admission of an authored placement against the immutable
    /// original reference and the current candidate's existing instance proof.
    pub(crate) fn validate_placement(
        &self, candidate: &Estimate, key: &Key,
        saved: &SavedDepthCalibration, input: PreparedDepthInput,
    ) -> Result<()> {
        if saved.is_physical() {
            self.validate(candidate, key)?;
            saved.validate_binding(&self.reference.resource, input)?;
        } else {
            self.calibration(candidate, key, saved, input)?;
        }
        Ok(())
    }

    pub(crate) fn calibration(
        &self,
        candidate: &Estimate,
        key: &Key,
        saved: &SavedDepthCalibration,
        input: PreparedDepthInput,
    ) -> Result<RelativeDepthCalibration> {
        self.validate(candidate, key)?;
        let original = saved.rebind(&self.reference.resource, input)?;
        let [scale, offset] = self.proof.coefficients();
        let anchors = original.anchors().map(|mut anchor| {
            anchor.value = (anchor.value - offset) / scale;
            anchor
        });
        Ok(RelativeDepthCalibration::new(
            candidate.resource.descriptor().clone(),
            anchors,
            original.curve(),
            original.extrapolation(),
        )?)
    }

    pub(crate) fn reference(&self) -> &Arc<Estimate> {
        &self.reference
    }

    pub(crate) fn spec(&self) -> &Spec {
        &self.spec
    }
}
