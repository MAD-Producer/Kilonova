//! Explicit multi-criterion admission, independent of execution backend names.
//!
//! External quality, whole-memory and sustained/thermal reports are trusted
//! caller inputs, not measurements performed or authenticated by this module.
//! Their record digests identify evidence to audit; a nonzero digest alone does
//! not certify its contents. The host must verify the referenced reports before
//! supplying them. This stage never performs inference or installs a result.

use super::{Profile, Summary};
use crate::depth::{cache::Execution, Error, Result};
use motion_model::depth::{DepthGeometry, DepthModel};
use sha2::{Digest, Sha256};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Fast,
    Standard,
    High,
}

/// Explicit application limits for this request. Tiers carry no built-in model,
/// resolution, backend or accuracy assumptions, and no default budgets.
#[derive(Clone, Debug)]
pub struct Request {
    pub tier: Tier,
    pub domain: String,
    /// Exact hardware/environment record, including relevant runtime settings.
    pub environment_sha256: [u8; 32],
    pub geometry: DepthGeometry,
    pub model: Option<DepthModel>,
    pub backend: Option<String>,
    pub minimum_samples: usize,
    pub maximum_worker_latency: Duration,
    /// Whole-process host peak and whole-device allocation observations,
    /// respectively. Managed inference buffers alone are insufficient.
    pub maximum_host_peak_bytes: u64,
    pub maximum_device_peak_bytes: u64,
    pub maximum_quality_rms: f64,
    pub maximum_quality_error: f64,
    pub maximum_transition_jump: f64,
    pub minimum_sustained_duration: Duration,
    pub maximum_temperature_celsius: f64,
}

/// Normalized independent-reference residuals and additional transition jump,
/// under the protocol in `record_sha256`. Not ONNX/WGSL equivalence errors.
#[derive(Clone, Debug)]
pub struct Quality {
    pub record_sha256: [u8; 32],
    pub independent_reference_sha256: [u8; 32],
    pub samples: usize,
    pub normalized_rms: f64,
    pub normalized_maximum: f64,
    pub maximum_additional_transition_jump: f64,
}

#[derive(Clone, Debug)]
pub struct Memory {
    pub record_sha256: [u8; 32],
    pub host_peak_bytes: u64,
    pub device_peak_bytes: u64,
    pub weight_bytes: u64,
    pub activation_peak_bytes: u64,
}

/// Complete sustained run; no warm-up or slower observation may be removed.
/// The record must describe the requested environment and measurement method.
#[derive(Clone, Debug)]
pub struct Sustained {
    pub record_sha256: [u8; 32],
    pub samples: usize,
    pub failed: usize,
    pub canceled: usize,
    pub duration: Duration,
    pub maximum_worker_latency: Duration,
    pub maximum_temperature_celsius: f64,
}

/// Exact binding prevents reuse of a report for another learned model, plan,
/// precision, geometry, input cohort or hardware environment. Missing sections
/// remain unknown and yield individual refusal reasons, never zero usage.
#[derive(Clone, Debug)]
pub struct Qualification {
    pub model: DepthModel,
    pub geometry: DepthGeometry,
    pub execution: Execution,
    pub cohort_sha256: [u8; 32],
    pub environment_sha256: [u8; 32],
    pub domain: String,
    pub quality: Option<Quality>,
    pub memory: Option<Memory>,
    pub sustained: Option<Sustained>,
}

pub struct Input<'a> {
    pub profile: &'a Profile,
    pub qualification: Option<&'a Qualification>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    DifferentModel,
    DifferentGeometry,
    DifferentBackend,
    DifferentEnvironment,
    DifferentDomain,
    InsufficientSamples,
    OverLatencyBudget,
    MissingQualification,
    MissingQuality,
    MissingMemory,
    MissingSustained,
    InsufficientQualitySamples,
    OverQualityRms,
    OverQualityError,
    OverTransitionJump,
    OverHostMemory,
    OverDeviceMemory,
    InsufficientSustainedSamples,
    IncompleteSustainedRun,
    InsufficientSustainedDuration,
    OverSustainedLatency,
    OverTemperature,
}

#[derive(Clone, Debug)]
pub struct Candidate {
    summary: Summary,
    qualification: Option<Qualification>,
    refusals: Vec<Refusal>,
}
impl Candidate {
    pub fn summary(&self) -> &Summary {
        &self.summary
    }
    pub fn refusals(&self) -> &[Refusal] {
        &self.refusals
    }
    /// Preserve supplied record identities and observed values for auditing
    /// after the caller releases its inputs. This is not authentication.
    pub fn qualification(&self) -> Option<&Qualification> {
        self.qualification.as_ref()
    }
}

#[derive(Clone, Debug)]
pub struct Decision {
    requested: Request,
    candidates: Vec<Candidate>,
    selected: Option<usize>,
}
impl Decision {
    pub fn requested(&self) -> &Request {
        &self.requested
    }
    pub fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }
    pub fn disposition(&self) -> super::latency::Disposition {
        if self.selected.is_some() {
            super::latency::Disposition::Selected
        } else {
            super::latency::Disposition::PrecomputedRequired
        }
    }
    pub fn resolved(&self) -> Option<&Summary> {
        self.selected.map(|i| &self.candidates[i].summary)
    }
}

fn label(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}
fn nonnegative(value: f64) -> bool {
    value.is_finite() && value >= 0.
}
fn nonzero(value: [u8; 32]) -> bool {
    value != [0; 32]
}

impl Request {
    fn validate(&self) -> Result<()> {
        self.geometry.validate()?;
        if let Some(model) = &self.model {
            model.validate()?;
            if !nonzero(model.sha256) {
                return Err(Error::Invalid("capability request requires an exact model"));
            }
        }
        if !label(&self.domain)
            || !nonzero(self.environment_sha256)
            || self.backend.as_ref().is_some_and(|s| !label(s))
            || !(1..=64).contains(&self.minimum_samples)
            || self.maximum_worker_latency.is_zero()
            || self.maximum_host_peak_bytes == 0
            || self.maximum_device_peak_bytes == 0
            || !nonnegative(self.maximum_quality_rms)
            || !nonnegative(self.maximum_quality_error)
            || self.maximum_quality_error < self.maximum_quality_rms
            || !nonnegative(self.maximum_transition_jump)
            || self.minimum_sustained_duration.is_zero()
            || !self.maximum_temperature_celsius.is_finite()
            || self.maximum_temperature_celsius < -273.15
        {
            return Err(Error::Invalid("invalid explicit capability limits"));
        }
        Ok(())
    }
}

/// Versioned, ordered source/RGB cohort identity; no model/backend labels enter
/// this digest. Equivalent rational PTS has the same canonical representation.
pub fn cohort_sha256(profile: &Profile) -> Result<[u8; 32]> {
    profile.summary()?;
    let mut hash = Sha256::new();
    hash.update(b"motion-depth-capability-cohort-v1\0");
    for observation in profile.observations() {
        let source = observation.source();
        let (mut a, mut b) = (
            source.pts.ticks.unsigned_abs(),
            u64::from(source.pts.timescale),
        );
        while b != 0 {
            (a, b) = (b, a % b);
        }
        let divisor = i128::from(a);
        let normalized = (
            source.asset,
            source.revision,
            i128::from(source.pts.ticks) / divisor,
            i128::from(source.pts.timescale) / divisor,
            observation.input(),
        );
        let encoded = serde_json::to_vec(&normalized)?;
        hash.update((encoded.len() as u64).to_le_bytes());
        hash.update(encoded);
    }
    Ok(hash.finalize().into())
}

impl Qualification {
    fn validate(&self, profile: &Profile, summary: &Summary) -> Result<()> {
        if self.model != *summary.model()
            || self.geometry != *summary.geometry()
            || self.execution != *summary.execution()
            || self.cohort_sha256 != cohort_sha256(profile)?
            || !nonzero(self.environment_sha256)
            || !label(&self.domain)
        {
            return Err(Error::Invalid(
                "qualification differs from measured configuration or cohort",
            ));
        }
        if let Some(q) = &self.quality {
            if !nonzero(q.record_sha256)
                || !nonzero(q.independent_reference_sha256)
                || q.samples != summary.samples()
                || !nonnegative(q.normalized_rms)
                || !nonnegative(q.normalized_maximum)
                || q.normalized_maximum < q.normalized_rms
                || !nonnegative(q.maximum_additional_transition_jump)
            {
                return Err(Error::Invalid("invalid independent quality report"));
            }
        }
        if let Some(m) = &self.memory {
            if !nonzero(m.record_sha256)
                || m.host_peak_bytes == 0
                || m.weight_bytes == 0
                || m.activation_peak_bytes == 0
                || summary
                    .retained_gpu_buffer_capacity()
                    .is_some_and(|managed| m.device_peak_bytes < managed)
            {
                return Err(Error::Invalid("invalid whole-memory report"));
            }
        }
        if let Some(s) = &self.sustained {
            if !nonzero(s.record_sha256)
                || s.samples == 0
                || s.duration.is_zero()
                || s.maximum_worker_latency.is_zero()
                || s.duration < s.maximum_worker_latency
                || s.failed
                    .checked_add(s.canceled)
                    .is_none_or(|n| n > s.samples)
                || !s.maximum_temperature_celsius.is_finite()
                || s.maximum_temperature_celsius < -273.15
            {
                return Err(Error::Invalid("invalid sustained thermal report"));
            }
        }
        Ok(())
    }
}

/// At most eight explicit candidates. All comparable configurations must use
/// the same ordered source/RGB cohort even across different model profiles.
/// A duplicate actual model/execution is not additional evidence. Preserve all
/// refusal reasons. Among fully admitted choices minimize observed worst-case
/// latency, then upper median, retaining caller order on exact ties. Empty or
/// unqualified candidates require an explicitly supplied precomputed resource;
/// this function does not manufacture one or silently downgrade a request.
pub fn select(requested: Request, inputs: &[Input<'_>]) -> Result<Decision> {
    requested.validate()?;
    if inputs.len() > 8 {
        return Err(Error::Invalid(
            "capability policy accepts at most eight candidates",
        ));
    }
    let mut candidates: Vec<Candidate> = Vec::with_capacity(inputs.len());
    let mut selected: Option<usize> = None;
    let mut cohort = None;
    for input in inputs {
        let summary = input.profile.summary()?;
        if candidates.iter().any(|c| {
            c.summary.model() == summary.model()
                && c.summary.execution() == summary.execution()
                && c.summary.geometry() == summary.geometry()
        }) {
            return Err(Error::Invalid(
                "capability policy repeats an actual configuration",
            ));
        }
        if summary.geometry() == &requested.geometry {
            let digest = cohort_sha256(input.profile)?;
            if cohort.is_some_and(|existing| existing != digest) {
                return Err(Error::Invalid(
                    "capability candidates require one ordered source/RGB cohort",
                ));
            }
            cohort = Some(digest);
        }
        let mut refusals = Vec::new();
        let mut reject = |condition, reason| {
            if condition {
                refusals.push(reason);
            }
        };
        reject(
            requested
                .model
                .as_ref()
                .is_some_and(|m| m != summary.model()),
            Refusal::DifferentModel,
        );
        reject(
            summary.geometry() != &requested.geometry,
            Refusal::DifferentGeometry,
        );
        reject(
            requested
                .backend
                .as_ref()
                .is_some_and(|b| b != &summary.execution().backend),
            Refusal::DifferentBackend,
        );
        reject(
            summary.samples() < requested.minimum_samples,
            Refusal::InsufficientSamples,
        );
        reject(
            summary.slowest() > requested.maximum_worker_latency,
            Refusal::OverLatencyBudget,
        );
        if let Some(q) = input.qualification {
            q.validate(input.profile, &summary)?;
            reject(
                q.environment_sha256 != requested.environment_sha256,
                Refusal::DifferentEnvironment,
            );
            reject(q.domain != requested.domain, Refusal::DifferentDomain);
            if let Some(v) = &q.quality {
                reject(
                    v.samples < requested.minimum_samples,
                    Refusal::InsufficientQualitySamples,
                );
                reject(
                    v.normalized_rms > requested.maximum_quality_rms,
                    Refusal::OverQualityRms,
                );
                reject(
                    v.normalized_maximum > requested.maximum_quality_error,
                    Refusal::OverQualityError,
                );
                reject(
                    v.maximum_additional_transition_jump > requested.maximum_transition_jump,
                    Refusal::OverTransitionJump,
                );
            } else {
                reject(true, Refusal::MissingQuality);
            }
            if let Some(v) = &q.memory {
                reject(
                    v.host_peak_bytes > requested.maximum_host_peak_bytes,
                    Refusal::OverHostMemory,
                );
                reject(
                    v.device_peak_bytes > requested.maximum_device_peak_bytes,
                    Refusal::OverDeviceMemory,
                );
            } else {
                reject(true, Refusal::MissingMemory);
            }
            if let Some(v) = &q.sustained {
                reject(
                    v.samples < requested.minimum_samples,
                    Refusal::InsufficientSustainedSamples,
                );
                reject(
                    v.failed != 0 || v.canceled != 0,
                    Refusal::IncompleteSustainedRun,
                );
                reject(
                    v.duration < requested.minimum_sustained_duration,
                    Refusal::InsufficientSustainedDuration,
                );
                reject(
                    v.maximum_worker_latency > requested.maximum_worker_latency,
                    Refusal::OverSustainedLatency,
                );
                reject(
                    v.maximum_temperature_celsius > requested.maximum_temperature_celsius,
                    Refusal::OverTemperature,
                );
            } else {
                reject(true, Refusal::MissingSustained);
            }
        } else {
            reject(true, Refusal::MissingQualification);
        }
        if refusals.is_empty()
            && selected.is_none_or(|i| {
                (summary.slowest(), summary.median())
                    < (
                        candidates[i].summary.slowest(),
                        candidates[i].summary.median(),
                    )
            })
        {
            selected = Some(candidates.len());
        }
        candidates.push(Candidate {
            summary,
            qualification: input.qualification.cloned(),
            refusals,
        });
    }
    Ok(Decision {
        requested,
        candidates,
        selected,
    })
}
