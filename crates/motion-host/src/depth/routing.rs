//! Two explicitly resolved model/backend bindings in the existing serial Jobs.
//!
//! Resolution and backend names establish no quality ranking. The caller must
//! qualify each configuration before assigning it to a transition choice. This
//! router does not create workers, retry a failure, or fall back to its other
//! binding. Identical model identities must be resolved to one backend first.

use super::{Backend, Cancellation, Error, Estimate, Input, Result};
use motion_model::depth::DepthModel;

pub struct Routed<A, B> {
    first_model: DepthModel,
    first: A,
    second_model: DepthModel,
    second: B,
}

impl<A: Backend, B: Backend> Routed<A, B> {
    /// Construction validates only the two bounded identities. It neither
    /// probes hardware nor loads models on the caller's thread.
    pub fn new(
        first_model: DepthModel,
        first: A,
        second_model: DepthModel,
        second: B,
    ) -> Result<Self> {
        first_model.validate()?;
        second_model.validate()?;
        if first_model == second_model {
            return Err(Error::Invalid(
                "depth routes require distinct resolved model identities",
            ));
        }
        Ok(Self {
            first_model,
            first,
            second_model,
            second,
        })
    }
}

impl<A: Backend, B: Backend> Backend for Routed<A, B> {
    fn infer(&mut self, input: &Input, cancellation: &Cancellation) -> Result<Estimate> {
        cancellation.check()?;
        // Preserve the worker-owned input and its exact hash/profile identity.
        // Jobs publishes in-flight state and its Info start before entering us;
        // the selected backend retains its actual provider, costs and failures.
        if input.key().model == self.first_model {
            self.first.infer(input, cancellation)
        } else if input.key().model == self.second_model {
            self.second.infer(input, cancellation)
        } else {
            Err(Error::Invalid(
                "no depth route for the exact requested model identity",
            ))
        }
    }
}
