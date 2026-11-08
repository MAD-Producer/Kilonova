//! Worker-owned observations for measured capability selection (#75).
//!
//! A successful probe establishes an execution identity and elapsed wall time
//! for that exact prepared input. It is not a quality rank, a peak allocation
//! measurement, a warm-runtime promise, or a thermal stability certificate.
//! The private construction boundary prevents application labels or a cache hit
//! from being represented as a measured inference.

use super::cache::Execution;
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
