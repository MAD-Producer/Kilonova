//! One admission stage for explicitly requested, already measured configurations.
//!
//! This compares worker latency for one exact model and oriented source geometry.
//! It does not establish quality tiers, memory fitness or sustained performance,
//! and must not by itself authorize automatic product configuration changes.

use super::{Profile, Summary};
use crate::depth::{cache::Execution, Error, Result};
use motion_model::depth::{DepthGeometry, DepthModel};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Request {
    pub model: DepthModel,
    pub geometry: DepthGeometry,
    /// None explicitly permits comparison of all measured backends. Some is an
    /// exact backend name; an unavailable name never silently selects another.
    pub backend: Option<String>,
    pub maximum_worker_latency: Duration,
    pub minimum_samples: usize,
}

impl Request {
    fn validate(&self) -> Result<()> {
        self.model.validate()?;
        self.geometry.validate()?;
        if self.model.sha256 == [0; 32] {
            return Err(Error::Invalid(
                "latency selection requires an exact learned-model digest",
            ));
        }
        if self.maximum_worker_latency.is_zero() || !(1..=64).contains(&self.minimum_samples) {
            return Err(Error::Invalid(
                "latency selection requires a positive budget and 1..64 samples",
            ));
        }
        if self
            .backend
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.len() > 128 || s.chars().any(char::is_control))
        {
            return Err(Error::Invalid(
                "latency selection backend identity is unbounded",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    Selected,
    /// No measured configuration met this request. No inference, synthetic
    /// result, backend fallback or precomputed resource is produced here.
    PrecomputedRequired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Admission {
    Eligible,
    DifferentModel,
    DifferentGeometry,
    DifferentBackend,
    InsufficientSamples,
    OverLatencyBudget,
}

#[derive(Clone, Debug)]
pub struct Candidate {
    summary: Summary,
    admission: Admission,
}

impl Candidate {
    pub fn summary(&self) -> &Summary {
        &self.summary
    }
    pub fn admission(&self) -> Admission {
        self.admission
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
    pub fn disposition(&self) -> Disposition {
        if self.selected.is_some() {
            Disposition::Selected
        } else {
            Disposition::PrecomputedRequired
        }
    }
    /// Includes actual model/hash/processing and backend/provider/runtime/plan,
    /// rather than echoing a requested provider or inferring quality from it.
    pub fn resolved(&self) -> Option<&Summary> {
        self.selected
            .and_then(|i| self.candidates.get(i))
            .map(|c| &c.summary)
    }
}

fn same_inputs(first: &Profile, other: &Profile) -> bool {
    first.observations().len() == other.observations().len()
        && first
            .observations()
            .iter()
            .zip(other.observations())
            .all(|(a, b)| a.source().same_sample(b.source()) && a.input() == b.input())
}

/// Compare at most eight explicit profiles, retaining each candidate's reason.
/// Comparable profiles must contain exactly the same ordered source/PTS and RGB
/// fingerprint cohort. Empty profiles and repeated actual executions are errors,
/// not zero-cost or additional evidence. No warm-up/outlier is discarded.
///
/// Select the smallest observed maximum, then upper median; exact ties preserve
/// caller order. This bounds only the observed sample set, never future latency.
/// The function performs no IO, inference, installation, retries or logging.
pub fn select(requested: Request, profiles: &[&Profile]) -> Result<Decision> {
    requested.validate()?;
    if profiles.len() > 8 {
        return Err(Error::Invalid(
            "latency selection accepts at most eight profiles",
        ));
    }
    let mut candidates: Vec<Candidate> = Vec::with_capacity(profiles.len());
    let mut selected: Option<usize> = None;
    let mut cohort: Option<&Profile> = None;
    let mut executions: Vec<&Execution> = Vec::with_capacity(profiles.len());
    for profile in profiles {
        let summary = profile.summary()?;
        let comparable =
            summary.model() == &requested.model && summary.geometry() == &requested.geometry;
        if comparable {
            if executions.contains(&summary.execution()) {
                return Err(Error::Invalid(
                    "latency selection repeats an actual execution",
                ));
            }
            // References must point into the input profiles, not this local
            // summary, which is moved into the result below.
            let execution = profile
                .observations()
                .first()
                .ok_or(Error::State)?
                .measurement()
                .execution();
            executions.push(execution);
            if let Some(first) = cohort {
                if !same_inputs(first, profile) {
                    return Err(Error::Invalid(
                        "latency profiles require the same ordered source and RGB cohort",
                    ));
                }
            } else {
                cohort = Some(profile);
            }
        }
        let admission = if summary.model() != &requested.model {
            Admission::DifferentModel
        } else if summary.geometry() != &requested.geometry {
            Admission::DifferentGeometry
        } else if requested
            .backend
            .as_ref()
            .is_some_and(|backend| backend != &summary.execution().backend)
        {
            Admission::DifferentBackend
        } else if summary.samples() < requested.minimum_samples {
            Admission::InsufficientSamples
        } else if summary.slowest() > requested.maximum_worker_latency {
            Admission::OverLatencyBudget
        } else {
            Admission::Eligible
        };
        if admission == Admission::Eligible
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
        candidates.push(Candidate { summary, admission });
    }
    Ok(Decision {
        requested,
        candidates,
        selected,
    })
}
