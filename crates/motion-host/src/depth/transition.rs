//! Pure admission policy for two explicitly resolved depth configurations.
//!
//! A caller must qualify the models/backends before constructing the choices.
//! This module neither infers quality from resolution nor performs inference,
//! alignment, caching or installation. Check `is_current` again after any work
//! and independently supply the live key at the installation boundary.

use super::{Input, Jobs, Key, PreparedEstimate};
pub mod admission;
use motion_model::depth::DepthResource;
use std::{
    sync::{Arc, Condvar, Mutex, Weak},
    time::Duration,
};

pub const VERSION: u32 = 1;

/// Asymmetric hysteresis: interaction immediately requests the interactive
/// configuration; settling requires both continuous idle and minimum residence.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub idle_delay: Duration,
    pub minimum_interactive_residence: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    Interactive,
    Idle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Interactive,
    Settled,
}

/// Exact model/hash/processing identities already resolved by a capability
/// policy. Both choices must describe the same source sample and geometry.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Choices {
    pub interactive: Key,
    pub settled: Key,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid depth transition policy: {0}")]
    Policy(&'static str),
    #[error("invalid depth transition choice: {0}")]
    Choice(#[source] super::Error),
    #[error("depth transition choices describe different source samples or geometry")]
    DifferentSample,
    #[error("depth transition clock moved backwards")]
    Clock,
    #[error("depth transition generation exhausted")]
    Generation,
    #[error("depth transition input differs from its current ticket")]
    Input,
    #[error("depth transition job failed: {0}")]
    Job(#[source] super::Error),
}

type Result<T> = std::result::Result<T, Error>;

/// Opaque ownership and generation prevent A -> B -> A or another controller's
/// result from being admitted merely because the public keys match again.
#[derive(Clone, Debug)]
pub struct Ticket {
    owner: Arc<()>,
    generation: u64,
    phase: Phase,
    key: Key,
}

impl Ticket {
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn key(&self) -> &Key {
        &self.key
    }
}

pub struct Controller {
    owner: Arc<()>,
    policy: Policy,
    generation: u64,
    last_clock: Option<Duration>,
    idle_since: Option<Duration>,
    interactive_since: Option<Duration>,
    choices: Option<Choices>,
    current: Option<Ticket>,
    submission: Option<Submission>,
}

// Weak ownership prevents worker address reuse without retaining its ready
// scalar planes or thread. Only the last explicit submission is represented.
struct Submission {
    worker: Weak<(Mutex<super::State>, Condvar)>,
    request: u64,
    generation: u64,
}

impl Choices {
    pub(crate) fn validate(&self) -> Result<()> {
        self.interactive.validate().map_err(Error::Choice)?;
        self.settled.validate().map_err(Error::Choice)?;
        if !self.interactive.source.same_sample(&self.settled.source)
            || self.interactive.geometry != self.settled.geometry
        {
            return Err(Error::DifferentSample);
        }
        Ok(())
    }

    /// Advancing PTS does not by itself change the qualified configurations or
    /// residence timer. Seeking still supplies Interactive or calls invalidate.
    fn same_configuration(&self, other: &Self) -> bool {
        self.interactive.source.asset == other.interactive.source.asset
            && self.interactive.source.revision == other.interactive.source.revision
            && self.interactive.geometry == other.interactive.geometry
            && self.interactive.model == other.interactive.model
            && self.settled.model == other.settled.model
    }
}

impl Controller {
    pub fn new(policy: Policy) -> Result<Self> {
        if policy.idle_delay.is_zero() || policy.minimum_interactive_residence.is_zero() {
            return Err(Error::Policy(
                "positive idle and residence durations required",
            ));
        }
        Ok(Self {
            owner: Arc::new(()),
            policy,
            generation: 0,
            last_clock: None,
            idle_since: None,
            interactive_since: None,
            choices: None,
            current: None,
            submission: None,
        })
    }

    fn check_clock(&self, now: Duration) -> Result<()> {
        if self.last_clock.is_some_and(|last| now < last) {
            return Err(Error::Clock);
        }
        Ok(())
    }

    /// Called on an explicit source/seek/cancel boundary even if no replacement
    /// is requested. It invalidates outstanding work, including identical keys.
    /// A later observe starts a fresh interactive residence and idle interval.
    pub fn invalidate(&mut self, now: Duration) -> Result<()> {
        self.check_clock(now)?;
        let generation = self.generation.checked_add(1).ok_or(Error::Generation)?;
        self.generation = generation;
        self.last_clock = Some(now);
        self.idle_since = None;
        self.interactive_since = None;
        self.choices = None;
        self.current = None;
        self.submission = None;
        Ok(())
    }

    /// Return a new request only when its exact key or phase changes. Duplicate
    /// polling performs no inference and emits no logs. Time is a caller's
    /// monotonic elapsed clock; no wall-clock sleep or implicit timeout is used.
    /// All validation precedes mutation, so malformed input retains prior state.
    pub fn observe(
        &mut self,
        choices: Choices,
        activity: Activity,
        now: Duration,
    ) -> Result<Option<Ticket>> {
        choices.validate()?;
        self.check_clock(now)?;
        let reset = self
            .choices
            .as_ref()
            .is_none_or(|previous| !previous.same_configuration(&choices));
        let mut interactive_since = if reset {
            now
        } else {
            self.interactive_since.unwrap_or(now)
        };
        // Returning from settled starts a new residence. Repeated interactive
        // frames reset idle time without indefinitely extending residence.
        if activity == Activity::Interactive
            && self
                .current
                .as_ref()
                .is_some_and(|t| t.phase == Phase::Settled)
        {
            interactive_since = now;
        }
        let idle_since = match activity {
            Activity::Interactive => None,
            Activity::Idle => Some(if reset {
                now
            } else {
                self.idle_since.unwrap_or(now)
            }),
        };
        let phase = if activity == Activity::Idle
            && now - idle_since.unwrap_or(now) >= self.policy.idle_delay
            && now - interactive_since >= self.policy.minimum_interactive_residence
        {
            Phase::Settled
        } else {
            Phase::Interactive
        };
        let key = match phase {
            Phase::Interactive => &choices.interactive,
            Phase::Settled => &choices.settled,
        };
        let changed = reset
            || self
                .current
                .as_ref()
                .is_none_or(|ticket| ticket.phase != phase || !ticket.key.same_sample(key));
        let next = if changed {
            Some(Ticket {
                owner: Arc::clone(&self.owner),
                generation: self.generation.checked_add(1).ok_or(Error::Generation)?,
                phase,
                key: key.clone(),
            })
        } else {
            None
        };
        self.last_clock = Some(now);
        self.idle_since = idle_since;
        self.interactive_since = Some(interactive_since);
        self.choices = Some(choices);
        if let Some(ticket) = &next {
            self.generation = ticket.generation;
            self.current = Some(ticket.clone());
            self.submission = None;
        }
        Ok(next)
    }

    pub fn current(&self) -> Option<&Ticket> {
        self.current.as_ref()
    }

    fn owns_current(&self, ticket: &Ticket) -> bool {
        self.current.as_ref().is_some_and(|current| {
            Arc::ptr_eq(&self.owner, &ticket.owner)
                && ticket.generation == current.generation
                && ticket.phase == current.phase
                && ticket.key.same_sample(&current.key)
        })
    }

    fn job_error(&self, operation: &'static str, error: super::Error) -> Error {
        // Jobs methods have returned and released their observation lock before
        // logging. Keep the original error, bounding only the log context.
        let cause: String = error.to_string().chars().take(512).collect();
        log::error!(
            "operation={operation} generation={} cause={cause}",
            self.generation
        );
        Error::Job(error)
    }

    /// Explicit submission to the existing single worker. A caller submits each
    /// newly returned observe ticket once. Stale/foreign tickets do not touch
    /// Jobs; a new actual request retains Jobs' cancellation and start records.
    pub fn request(
        &mut self,
        ticket: &Ticket,
        jobs: &mut Jobs,
        input: Input,
    ) -> Result<Option<u64>> {
        if !self.owns_current(ticket) {
            return Ok(None);
        }
        if !ticket.key.same_sample(input.key()) {
            log::error!("operation=depth_transition_request generation={} source_id={} cause=input_differs_from_ticket",
                ticket.generation, ticket.key.source.asset);
            return Err(Error::Input);
        }
        let request = jobs
            .request(input)
            .map_err(|error| self.job_error("depth_transition_request", error))?;
        self.submission = Some(Submission {
            worker: Arc::downgrade(&jobs.shared),
            request,
            generation: ticket.generation,
        });
        Ok(Some(request))
    }

    /// Infer and align on the existing worker before publishing a completion.
    /// Retain the exact reference planes and prepared pixel fingerprint; no
    /// fitting, source IO or extra worker is introduced on the caller thread.
    /// This is explicit admission, not automatic Session installation.
    pub fn request_aligned(
        &mut self,
        ticket: &Ticket,
        jobs: &mut Jobs,
        input: Input,
        reference: &PreparedEstimate,
        limits: super::alignment::Limits,
    ) -> Result<Option<u64>> {
        if !self.owns_current(ticket) {
            return Ok(None);
        }
        if !ticket.key.same_sample(input.key()) {
            log::error!("operation=depth_transition_request_aligned generation={} source_id={} cause=input_differs_from_ticket",
                ticket.generation, ticket.key.source.asset);
            return Err(Error::Input);
        }
        let request = jobs
            .submit_with_alignment(
                super::Work::Prepared(input),
                Some(admission::Pending::new(ticket, reference, limits)),
            )
            .map_err(|error| self.job_error("depth_transition_request_aligned", error))?;
        self.submission = Some(Submission {
            worker: Arc::downgrade(&jobs.shared),
            request,
            generation: ticket.generation,
        });
        Ok(Some(request))
    }

    /// Session-only managed-source staging. The reference was retained from an
    /// actual worker completion; decoding and fitting stay on that same worker.
    pub(crate) fn request_aligned_image(
        &mut self,
        ticket: &Ticket,
        jobs: &mut Jobs,
        input: super::image::ImageInput,
        reference: &PreparedEstimate,
        limits: super::alignment::Limits,
    ) -> Result<Option<u64>> {
        if !self.owns_current(ticket) {
            return Ok(None);
        }
        if !ticket.key.same_sample(&input.key) {
            return Err(Error::Input);
        }
        let request = jobs
            .submit_with_alignment(
                super::Work::Image(input),
                Some(admission::Pending::new(ticket, reference, limits)),
            )
            .map_err(|cause| self.job_error("depth_transition_request_image", cause))?;
        self.submission = Some(Submission {
            worker: Arc::downgrade(&jobs.shared),
            request,
            generation: ticket.generation,
        });
        Ok(Some(request))
    }

    /// Revoke tickets before canceling the worker, even when no replacement is
    /// requested. A cancellation failure cannot make the old ticket valid again.
    pub fn invalidate_jobs(&mut self, jobs: &mut Jobs, now: Duration) -> Result<()> {
        self.invalidate(now)?;
        jobs.cancel()
            .map_err(|error| self.job_error("depth_transition_cancel", error))
    }

    /// Stale polling cannot consume another generation's ready result. Preserve
    /// actual provider/costs and the worker-owned prepared RGB fingerprint.
    /// This does not align or install a result: callers must recheck is_current
    /// after any subsequent alignment, using the independent live key again.
    pub fn take_ready(
        &self,
        ticket: &Ticket,
        jobs: &mut Jobs,
        live: &Key,
    ) -> Result<Option<PreparedEstimate>> {
        if !self.matches_submission(ticket, jobs, live)? {
            return Ok(None);
        }
        let ready = jobs
            .take_prepared_ready(live)
            .map_err(|error| self.job_error("depth_transition_take", error))?;
        Ok(ready.filter(|ready| self.is_current(ticket, live, &ready.estimate().resource)))
    }

    /// Read only a completed alignment from this controller's actual request.
    /// No fit occurs here. Stale/foreign/unbound polling cannot consume a newer
    /// completion; callers must still obtain a view using independently read
    /// live identity immediately before using its samples.
    pub fn take_aligned_ready(
        &self,
        ticket: &Ticket,
        jobs: &mut Jobs,
        live: &Key,
    ) -> Result<Option<admission::Admission>> {
        if !self.matches_submission(ticket, jobs, live)? {
            return Ok(None);
        }
        match jobs
            .take_identity(&super::Identity::Sample(live.clone()))
            .map_err(|error| self.job_error("depth_transition_take_aligned", error))?
        {
            None => Ok(None),
            Some(super::Ready::Aligned(admitted)) => {
                if admitted
                    .view(self, live)
                    .map_err(|cause| {
                        self.job_error(
                            "depth_transition_take_aligned",
                            super::Error::Alignment(cause.into()),
                        )
                    })?
                    .is_none()
                {
                    return Ok(None);
                }
                Ok(Some(admitted))
            }
            Some(super::Ready::Raw(_)) => Err(self.job_error(
                "depth_transition_take_aligned",
                super::Error::Invalid("raw completion requires its prepared result API"),
            )),
        }
    }

    fn matches_submission(&self, ticket: &Ticket, jobs: &Jobs, live: &Key) -> Result<bool> {
        if !self.owns_current(ticket) || !ticket.key.same_sample(live) {
            return Ok(false);
        }
        let Some(submission) = &self.submission else {
            return Ok(false);
        };
        if submission.generation != ticket.generation
            || !Weak::ptr_eq(&submission.worker, &Arc::downgrade(&jobs.shared))
        {
            return Ok(false);
        }
        // &mut Jobs excludes another caller's submission between this check
        // and take_prepared_ready. The worker never changes State.latest.
        // Drop the observation guard (or poison) before reporting an error.
        let latest = match jobs.shared.0.lock() {
            Ok(state) => state.latest,
            Err(poison) => {
                drop(poison);
                return Err(self.job_error("depth_transition_take", super::Error::State));
            }
        };
        if latest != submission.request {
            return Ok(false);
        }
        Ok(true)
    }

    /// Identity gate only, not permission to install unaligned depth. The live
    /// key must be obtained independently after the work, not copied from the
    /// ticket. Old/canceled/foreign work is a normal rejection, not a fault.
    pub fn is_current(&self, ticket: &Ticket, live: &Key, result: &DepthResource) -> bool {
        let descriptor = result.descriptor();
        self.owns_current(ticket)
            && ticket.key.same_sample(live)
            && descriptor.source.same_sample(&live.source)
            && descriptor.geometry == live.geometry
            && descriptor.model == live.model
    }
}
