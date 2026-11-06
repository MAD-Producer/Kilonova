//! Pure admission policy for two explicitly resolved depth configurations.
//!
//! A caller must qualify the models/backends before constructing the choices.
//! This module neither infers quality from resolution nor performs inference,
//! alignment, caching or installation. Check `is_current` again after any work
//! and independently supply the live key at the installation boundary.

use super::Key;
use motion_model::depth::DepthResource;
use std::{sync::Arc, time::Duration};

pub const VERSION: u32 = 1;

/// Asymmetric hysteresis: interaction immediately requests the interactive
/// configuration; settling requires both continuous idle and minimum residence.
#[derive(Clone, Copy, Debug)]
pub struct Policy {
    pub idle_delay: Duration,
    pub minimum_interactive_residence: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    Interactive,
    Idle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Interactive,
    Settled,
}

/// Exact model/hash/processing identities already resolved by a capability
/// policy. Both choices must describe the same source sample and geometry.
#[derive(Clone, Debug)]
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
}

impl Choices {
    fn validate(&self) -> Result<()> {
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
        }
        Ok(next)
    }

    pub fn current(&self) -> Option<&Ticket> {
        self.current.as_ref()
    }

    /// Identity gate only, not permission to install unaligned depth. The live
    /// key must be obtained independently after the work, not copied from the
    /// ticket. Old/canceled/foreign work is a normal rejection, not a fault.
    pub fn is_current(&self, ticket: &Ticket, live: &Key, result: &DepthResource) -> bool {
        let Some(current) = &self.current else {
            return false;
        };
        let descriptor = result.descriptor();
        Arc::ptr_eq(&self.owner, &ticket.owner)
            && ticket.generation == current.generation
            && ticket.phase == current.phase
            && ticket.key.same_sample(&current.key)
            && ticket.key.same_sample(live)
            && descriptor.source.same_sample(&live.source)
            && descriptor.geometry == live.geometry
            && descriptor.model == live.model
    }
}
