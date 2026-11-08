//! Worker-owned observations for measured capability selection (#75).
//!
//! A successful probe establishes an execution identity and elapsed wall time
//! for that exact prepared input. It is not a quality rank, a peak allocation
//! measurement, a warm-runtime promise, or a thermal stability certificate.
//! The private construction boundary prevents application labels or a cache hit
//! from being represented as a measured inference.

use super::{cache::Execution, Error, PreparedEstimate, Result};
use motion_model::depth::{DepthGeometry, DepthModel, DepthSource, PreparedDepthInput};
use std::{sync::Arc, time::Duration};

#[derive(Clone, Debug)]
pub struct Measurement {
    scope: Arc<()>,
    request: u64,
    execution: Execution,
    resolve: Duration,
    total: Duration,
}

impl Measurement {
    pub(super) fn new(
        scope: Arc<()>,
        request: u64,
        execution: Execution,
        resolve: Duration,
        total: Duration,
    ) -> Self {
        Self {
            scope,
            request,
            execution,
            resolve,
            total,
        }
    }

    pub fn request(&self) -> u64 {
        self.request
    }

    /// Actual backend/provider/runtime/procedure/numerical identity, resolved
    /// before inference and checked against the returned provider afterward.
    pub fn execution(&self) -> &Execution {
        &self.execution
    }

    /// Wall time spent resolving the actual execution environment. It is also
    /// included in `total`; it must not be added to `total` a second time.
    pub fn resolve(&self) -> Duration {
        self.resolve
    }

    /// Worker wall time including source preparation, identity resolution,
    /// tensor staging, actual inference and output ownership checks. Excludes
    /// queue wait, caller preparation, alignment and downstream installation.
    pub fn total(&self) -> Duration {
        self.total
    }

    /// Cloning one observation cannot turn it into a second independent run.
    /// Equal request numbers from separate workers are distinct observations.
    pub fn same_observation(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.scope, &other.scope) && self.request == other.request
    }
}

/// A source-bound observation, retaining no RGB image or depth output allocation.
#[derive(Clone, Debug)]
pub struct Observation {
    source: DepthSource,
    input: PreparedDepthInput,
    measurement: Measurement,
}
impl Observation {
    pub fn source(&self) -> &DepthSource {
        &self.source
    }
    pub fn input(&self) -> PreparedDepthInput {
        self.input
    }
    pub fn measurement(&self) -> &Measurement {
        &self.measurement
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Scope {
    model: DepthModel,
    geometry: DepthGeometry,
    execution: Execution,
}

/// A bounded latency profile for one exact model, geometry and execution.
/// Failed, canceled or reused work cannot be added. The caller chooses sample
/// coverage; a profile does not certify a representative video or quality tier.
pub struct Profile {
    capacity: usize,
    scope: Option<Scope>,
    observations: Vec<Observation>,
}
impl Profile {
    pub fn new(capacity: usize) -> Result<Self> {
        if !(1..=64).contains(&capacity) {
            return Err(Error::Invalid(
                "capability profile capacity must be in 1..64",
            ));
        }
        Ok(Self {
            capacity,
            scope: None,
            observations: Vec::with_capacity(capacity),
        })
    }

    /// Admission is atomic: a rejected observation cannot replace the profile's
    /// identity, discard existing observations, or count toward its sample size.
    pub fn record(&mut self, ready: &PreparedEstimate) -> Result<()> {
        let measurement = ready.measurement().ok_or(Error::Invalid(
            "capability profile requires an uncached worker measurement",
        ))?;
        if self.observations.len() == self.capacity {
            return Err(Error::Invalid("capability profile capacity exceeded"));
        }
        let descriptor = ready.estimate().resource.descriptor();
        let scope = Scope {
            model: descriptor.model.clone(),
            geometry: descriptor.geometry.clone(),
            execution: measurement.execution.clone(),
        };
        if self
            .scope
            .as_ref()
            .is_some_and(|existing| existing != &scope)
        {
            return Err(Error::Invalid(
                "capability profile model, geometry or execution changed",
            ));
        }
        if self
            .observations
            .iter()
            .any(|previous| previous.measurement.same_observation(measurement))
        {
            return Err(Error::Invalid(
                "capability observation was already recorded",
            ));
        }
        let observation = Observation {
            source: descriptor.source.clone(),
            input: ready.input(),
            measurement: measurement.clone(),
        };
        self.scope = Some(scope);
        self.observations.push(observation);
        Ok(())
    }

    pub fn observations(&self) -> &[Observation] {
        &self.observations
    }

    pub fn summary(&self) -> Result<Summary> {
        let scope = self
            .scope
            .clone()
            .ok_or(Error::Invalid("capability profile has no observations"))?;
        let mut totals: Vec<_> = self
            .observations
            .iter()
            .map(|s| s.measurement.total)
            .collect();
        totals.sort_unstable();
        // A nonempty scope is established only in the same atomic admission
        // that appends its first observation. Bounds remain explicit here.
        let fastest = *totals.first().ok_or(Error::State)?;
        let slowest = *totals.last().ok_or(Error::State)?;
        let median = *totals.get(totals.len() / 2).ok_or(Error::State)?;
        let resolve_slowest = self
            .observations
            .iter()
            .map(|s| s.measurement.resolve)
            .max()
            .ok_or(Error::State)?;
        Ok(Summary {
            scope,
            samples: totals.len(),
            fastest,
            median,
            slowest,
            resolve_slowest,
        })
    }
}

/// Observed order statistics only. Median is the upper middle observation for
/// an even count; no interpolation, warm-up removal or outlier trimming occurs.
#[derive(Clone, Debug)]
pub struct Summary {
    scope: Scope,
    samples: usize,
    fastest: Duration,
    median: Duration,
    slowest: Duration,
    resolve_slowest: Duration,
}
impl Summary {
    pub fn model(&self) -> &DepthModel {
        &self.scope.model
    }
    pub fn geometry(&self) -> &DepthGeometry {
        &self.scope.geometry
    }
    pub fn execution(&self) -> &Execution {
        &self.scope.execution
    }
    pub fn samples(&self) -> usize {
        self.samples
    }
    pub fn fastest(&self) -> Duration {
        self.fastest
    }
    pub fn median(&self) -> Duration {
        self.median
    }
    pub fn slowest(&self) -> Duration {
        self.slowest
    }
    pub fn resolve_slowest(&self) -> Duration {
        self.resolve_slowest
    }
}
