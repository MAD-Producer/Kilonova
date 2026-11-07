//! Instance-bound aligned result admission. This is not Session installation,
//! a persistent cache format, a quality selector or an export policy.
use super::{Controller, Ticket};
use crate::depth::{
    alignment::{Aligned, Alignment, Limits, Rejection, Statistics},
    Costs, Key, PreparedEstimate,
};
use motion_model::depth::{DepthDescriptor, DepthResource, PreparedDepthInput};
use std::time::{Duration, Instant};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("depth transition prepared input pixels differ")]
    Pixels,
    #[error("depth transition alignment rejected: {0}")]
    Alignment(#[from] Rejection),
}

/// Retains exactly one reference resource and one prepared candidate. Neither
/// raw scalar plane is rewritten or relabeled. Dropping it releases both; no
/// global cache, worker or second output plane is created.
pub struct Admission {
    ticket: Ticket,
    reference: DepthResource,
    candidate: PreparedEstimate,
    alignment: Alignment,
}

/// One immutable reference instance per queued/running aligned request. This is
/// private so only a controller-validated submission can create worker work.
pub(crate) struct Pending {
    ticket: Ticket,
    reference: DepthResource,
    input: PreparedDepthInput,
    limits: Limits,
}

impl Pending {
    pub(super) fn new(ticket: &Ticket, reference: &PreparedEstimate, limits: Limits) -> Self {
        Self {
            ticket: ticket.clone(),
            reference: reference.estimate().resource.clone(),
            input: reference.input(),
            limits,
        }
    }

    pub(crate) fn generation(&self) -> u64 {
        self.ticket.generation()
    }

    pub(crate) fn complete(
        self,
        candidate: PreparedEstimate,
        canceled: impl FnMut() -> bool,
    ) -> Result<Admission, Error> {
        Admission::fit(
            self.ticket,
            self.reference,
            self.input,
            candidate,
            self.limits,
            canceled,
        )
    }
}

impl Admission {
    /// Call after taking a prepared candidate from the current actual Jobs.
    /// The caller supplies independently read live identity and qualified limits.
    /// Stale work is a normal miss; incompatible pixels and failed fits remain
    /// explicit errors. Alignment cancellation retains its original typed cause.
    pub fn prepare(
        controller: &Controller,
        ticket: &Ticket,
        live: &Key,
        reference: &PreparedEstimate,
        candidate: PreparedEstimate,
        limits: Limits,
        canceled: impl FnMut() -> bool,
    ) -> Result<Option<Self>, Error> {
        if !controller.is_current(ticket, live, &candidate.estimate().resource) {
            return Ok(None);
        }
        Pending::new(ticket, reference, limits)
            .complete(candidate, canceled)
            .map(Some)
    }

    fn fit(
        ticket: Ticket,
        reference: DepthResource,
        reference_input: PreparedDepthInput,
        mut candidate: PreparedEstimate,
        limits: Limits,
        canceled: impl FnMut() -> bool,
    ) -> Result<Self, Error> {
        if reference_input != candidate.input() {
            log::error!("operation=depth_transition_align generation={} source_id={} cause=prepared_pixels_differ",
                ticket.generation(), ticket.key().source.asset);
            return Err(Error::Pixels);
        }
        log::info!(
            "start depth_transition_align generation={} source_id={} processing={}",
            ticket.generation(),
            ticket.key().source.asset,
            ticket.key().model.processing
        );
        // Info is buffered by the production sink. Publish it before entering
        // alignment or cancellation polling, rather than only during teardown.
        log::logger().flush();
        let began = Instant::now();
        let alignment =
            Alignment::fit(&reference, &candidate.estimate().resource, limits, canceled);
        log::debug!(
            "done depth_transition_align generation={} source_id={} elapsed_ms={}",
            ticket.generation(),
            ticket.key().source.asset,
            began.elapsed().as_millis()
        );
        let alignment = alignment.map_err(|cause| {
            // This pure calculation holds no business/observation lock. Expected
            // cancellation is not a fault; an unsafe quality transition is an
            // explicit refusal, rather than permission to install raw output.
            if !matches!(cause, Rejection::Canceled) {
                log::warn!("operation=depth_transition_align generation={} source_id={} elapsed_ms={} cause={cause}",
                    ticket.generation(), ticket.key().source.asset, began.elapsed().as_millis());
            }
            Error::Alignment(cause)
        })?;
        candidate.estimate.costs.alignment = began.elapsed();
        Ok(Self {
            ticket,
            reference,
            candidate,
            alignment,
        })
    }

    pub(crate) fn finish_worker_costs(&mut self, total: Duration) {
        self.candidate.estimate.costs.total = total;
    }

    pub fn statistics(&self) -> Statistics {
        self.alignment.statistics()
    }

    /// Obtain live identity again after the work. A -> B -> A, foreign owners,
    /// seeks and explicit invalidation reject the retained proof even when its
    /// public metadata still matches. The view borrows the controller, preventing
    /// controller mutation while its admitted samples are in use on this thread.
    pub fn view<'a>(
        &'a self,
        controller: &'a Controller,
        live: &Key,
    ) -> Result<Option<View<'a>>, Rejection> {
        if !controller.is_current(&self.ticket, live, &self.candidate.estimate().resource) {
            return Ok(None);
        }
        Ok(Some(View {
            _controller: controller,
            aligned: self
                .alignment
                .bind(&self.reference, &self.candidate.estimate().resource)?,
            candidate: &self.candidate,
        }))
    }
}

/// Explicit aligned sampling with original provider, costs and model metadata.
/// This does not manufacture a new model/resource identity or change raw values.
pub struct View<'a> {
    _controller: &'a Controller,
    aligned: Aligned<'a>,
    candidate: &'a PreparedEstimate,
}
impl View<'_> {
    pub fn sample(&self, index: usize) -> Result<Option<f64>, Rejection> {
        self.aligned.sample(index)
    }
    pub fn descriptor(&self) -> &DepthDescriptor {
        self.aligned.resource().descriptor()
    }
    pub fn provider(&self) -> &str {
        &self.candidate.estimate().provider
    }
    pub fn costs(&self) -> &Costs {
        &self.candidate.estimate().costs
    }
    pub fn input(&self) -> PreparedDepthInput {
        self.candidate.input()
    }
}
