//! Bounded owner-side eligibility and cancellation for local prerender work.
//!
//! This module does not render or reserve GPU memory. The consumer must construct
//! authoritative content/output keys, obtain shared allocation admission before
//! starting work, and install only a matching completed ticket. No capability is
//! enabled until that actual render/cache integration is accepted.
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

pub const MAX_WINDOW: usize = 8;
pub mod static_intervals;

/// Content includes all immutable project/source/plugin/model dependencies;
/// output includes format/color/quality/observer interpretation. UI chrome is
/// excluded. The caller must never substitute a project revision for these keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Context {
    pub document_generation: u64,
    pub device_generation: u64,
    pub content: [u8; 32],
    pub output: [u8; 32],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameKey {
    context: Context,
    frame_bits: u64,
}
impl FrameKey {
    pub fn context(self) -> Context {
        self.context
    }
    pub fn frame(self) -> f64 {
        f64::from_bits(self.frame_bits)
    }
    fn new(context: Context, frame: f64) -> Self {
        Self {
            context,
            frame_bits: if frame == 0. {
                0f64.to_bits()
            } else {
                frame.to_bits()
            },
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Backward,
}
#[derive(Clone, Copy, Debug)]
pub struct Demand {
    pub context: Context,
    pub frame: f64,
    pub total_frames: u32,
    pub direction: Direction,
    pub playing: bool,
    pub editing: bool,
    pub foreground_pending: bool,
    pub pressure: bool,
    /// Monotonic owner clock, injected independently of playback frame time.
    pub now: Duration,
}
#[derive(Clone, Copy, Debug)]
pub struct Policy {
    pub enabled: bool,
    pub window: usize,
    pub idle_delay: Duration,
    pub max_seek_frames_per_second: f64,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            enabled: false,
            window: 3,
            idle_delay: Duration::from_millis(200),
            max_seek_frames_per_second: 120.,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hold {
    NoDemand,
    Disabled,
    Foreground,
    Editing,
    Pressure,
    FastSeek,
    IdleDelay,
    Budget,
    Busy,
    Empty,
    Ready,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidPolicy,
    InvalidDemand,
    ClockReversed,
    TicketExhausted,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "prerender scheduler: {self:?}")
    }
}
impl std::error::Error for Error {}

/// Saturating lifetime counters of this scheduler, preserved by invalidation.
#[derive(Clone, Copy, Debug, Default)]
pub struct Counts {
    pub started: u64,
    pub completed: u64,
    pub canceled: u64,
    pub stale: u64,
    pub budget_holds: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct Status {
    pub enabled: bool,
    pub window: usize,
    pub hold: Hold,
    pub pending: usize,
    pub in_flight: Option<FrameKey>,
    pub counts: Counts,
}
#[derive(Clone, Debug)]
pub struct Ticket {
    id: u64,
    key: FrameKey,
    cancel: Arc<AtomicBool>,
}
impl Ticket {
    pub fn key(&self) -> FrameKey {
        self.key
    }
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn canceled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }
}

pub struct Scheduler {
    policy: Policy,
    window_limit: usize,
    demand: Option<Demand>,
    last_motion: Duration,
    fast_seek: bool,
    pending: VecDeque<FrameKey>,
    completed: Vec<FrameKey>,
    flight: Option<Ticket>,
    next_ticket: u64,
    hold: Hold,
    counts: Counts,
}
impl Scheduler {
    pub fn new(policy: Policy) -> Result<Self, Error> {
        validate_policy(policy)?;
        Ok(Self {
            policy,
            window_limit: MAX_WINDOW,
            demand: None,
            last_motion: Duration::ZERO,
            fast_seek: false,
            pending: VecDeque::new(),
            completed: Vec::new(),
            flight: None,
            next_ticket: 0,
            hold: Hold::NoDemand,
            counts: Counts::default(),
        })
    }
    pub fn set_policy(&mut self, policy: Policy) -> Result<(), Error> {
        validate_policy(policy)?;
        self.cancel_flight();
        self.pending.clear();
        self.completed.clear();
        self.policy = policy;
        if let Some(demand) = self.demand {
            self.update(demand)?;
        } else {
            self.hold = if policy.enabled {
                Hold::NoDemand
            } else {
                Hold::Disabled
            };
        }
        Ok(())
    }
    /// An advisory work limit, independent of policy, content identity and cache
    /// quota. Retain already admitted/completed frames; never alias their keys.
    /// Foreground holds, cancellation and explicit policy invalidation still win.
    pub fn set_window_limit(&mut self, limit: usize) -> Result<(), Error> {
        if limit > MAX_WINDOW {
            return Err(Error::InvalidPolicy);
        }
        if self.window_limit != limit {
            self.window_limit = limit;
            self.pending.clear();
            if let Some(demand) = self.demand {
                self.refresh(demand);
            }
        }
        Ok(())
    }
    pub fn update(&mut self, demand: Demand) -> Result<(), Error> {
        self.update_inner(demand, false)
    }
    /// Continuous playback may retain an admitted frame inside the new window,
    /// including the frame now due. Explicit seeks retain the conservative API
    /// above. Context changes, reverse motion, holds and fast motion still cancel.
    pub fn advance_playback(&mut self, demand: Demand) -> Result<(), Error> {
        self.update_inner(demand, true)
    }
    fn update_inner(&mut self, demand: Demand, playback: bool) -> Result<(), Error> {
        if !demand.frame.is_finite() || demand.frame < 0. || demand.total_frames == 0 {
            self.invalidate();
            return Err(Error::InvalidDemand);
        }
        if self.demand.is_some_and(|old| demand.now < old.now) {
            self.invalidate();
            return Err(Error::ClockReversed);
        }
        let moved = self.demand.is_none_or(|old| {
            old.context != demand.context
                || old.frame != demand.frame
                || old.total_frames != demand.total_frames
                || old.direction != demand.direction
        });
        if moved {
            self.fast_seek = self.demand.is_some_and(|old| {
                old.context == demand.context
                    && old.frame != demand.frame
                    && (demand.now == old.now
                        || (demand.frame - old.frame).abs() / (demand.now - old.now).as_secs_f64()
                            > self.policy.max_seek_frames_per_second)
            });
            self.last_motion = demand.now;
            let continuous = playback
                && !self.fast_seek
                && self.demand.is_some_and(|old| {
                    old.playing
                        && demand.playing
                        && old.context == demand.context
                        && old.total_frames == demand.total_frames
                        && old.direction == demand.direction
                        && match demand.direction {
                            Direction::Forward => demand.frame >= old.frame,
                            Direction::Backward => demand.frame <= old.frame,
                        }
                });
            if !continuous
                || self
                    .flight
                    .as_ref()
                    .is_some_and(|t| !in_window(t.key, demand, self.policy.window))
            {
                self.cancel_flight();
            }
            self.pending.clear();
            if continuous {
                self.completed
                    .retain(|key| in_window(*key, demand, self.policy.window));
            } else {
                self.completed.clear();
            }
        }
        if demand.now.saturating_sub(self.last_motion) >= self.policy.idle_delay {
            self.fast_seek = false;
        }
        self.demand = Some(demand);
        self.refresh(demand);
        Ok(())
    }
    // Only called with owner-held, already validated demand.
    fn refresh(&mut self, demand: Demand) {
        let window = self.policy.window.min(self.window_limit);
        self.hold = if !self.policy.enabled || window == 0 {
            Hold::Disabled
        } else if demand.foreground_pending {
            Hold::Foreground
        } else if demand.editing {
            Hold::Editing
        } else if demand.pressure {
            Hold::Pressure
        } else if self.fast_seek {
            Hold::FastSeek
        } else if !demand.playing
            && demand.now.saturating_sub(self.last_motion) < self.policy.idle_delay
        {
            Hold::IdleDelay
        } else {
            Hold::Ready
        };
        if self.hold != Hold::Ready {
            self.cancel_flight();
            self.pending.clear();
            return;
        }
        if demand.frame >= f64::from(demand.total_frames) {
            self.cancel_flight();
            self.pending.clear();
            self.hold = Hold::Empty;
            return;
        }
        for step in 1..=window {
            let frame = demand.frame
                + if demand.direction == Direction::Forward {
                    step as f64
                } else {
                    -(step as f64)
                };
            if frame < 0. || frame >= f64::from(demand.total_frames) {
                break;
            }
            let key = FrameKey::new(demand.context, frame);
            if !self.pending.contains(&key)
                && !self.completed.contains(&key)
                && !self.flight.as_ref().is_some_and(|t| t.key == key)
            {
                self.pending.push_back(key);
            }
        }
        if self.flight.is_some() {
            self.hold = Hold::Busy;
        } else if self.pending.is_empty() {
            self.hold = Hold::Empty;
        }
    }
    /// Inspect the exact eligible destination before reserving real storage.
    /// This does not consume a ticket, count a start or bypass any hold.
    pub fn next_key(&self) -> Option<FrameKey> {
        if self.flight.is_some() || !matches!(self.hold, Hold::Ready | Hold::Budget) {
            return None;
        }
        self.pending.front().copied()
    }
    /// Admission must represent an actual shared-budget reservation owned by
    /// the consumer, not merely a guessed free-byte count. This scheduler never
    /// acquires a GPU resource or creates another unbounded worker queue.
    pub fn take_next(&mut self, admitted: bool) -> Result<Option<Ticket>, Error> {
        // Cancellation is cooperative. Keep owning the slot until the worker
        // acknowledges completion, so repeated seeks cannot spawn more work.
        if self.flight.is_some() {
            return Ok(None);
        }
        if !matches!(self.hold, Hold::Ready | Hold::Budget) {
            return Ok(None);
        }
        if self.pending.is_empty() {
            self.hold = Hold::Empty;
            return Ok(None);
        }
        if !admitted {
            self.hold = Hold::Budget;
            self.counts.budget_holds = self.counts.budget_holds.saturating_add(1);
            return Ok(None);
        }
        let id = self
            .next_ticket
            .checked_add(1)
            .ok_or(Error::TicketExhausted)?;
        let Some(key) = self.pending.pop_front() else {
            self.hold = Hold::Empty;
            return Ok(None);
        };
        let ticket = Ticket {
            id,
            key,
            cancel: Arc::new(AtomicBool::new(false)),
        };
        self.next_ticket = id;
        self.flight = Some(ticket.clone());
        self.hold = Hold::Busy;
        self.counts.started = self.counts.started.saturating_add(1);
        Ok(Some(ticket))
    }
    /// Return an installable key only for the one current, noncanceled flight.
    /// A stale completion cannot clear or replace a newer flight.
    pub fn finish(&mut self, ticket: &Ticket) -> Option<FrameKey> {
        if !self.flight.as_ref().is_some_and(|current| {
            current.id == ticket.id
                && current.key == ticket.key
                && Arc::ptr_eq(&current.cancel, &ticket.cancel)
        }) {
            self.counts.stale = self.counts.stale.saturating_add(1);
            return None;
        }
        self.flight = None;
        let accepted = !ticket.canceled();
        if accepted {
            self.counts.completed = self.counts.completed.saturating_add(1);
            self.completed.push(ticket.key);
        } else {
            self.counts.stale = self.counts.stale.saturating_add(1);
        }
        if let Some(demand) = self.demand {
            self.refresh(demand);
        }
        accepted.then_some(ticket.key)
    }
    pub fn invalidate(&mut self) {
        self.cancel_flight();
        self.pending.clear();
        self.completed.clear();
        self.demand = None;
        self.fast_seek = false;
        self.hold = Hold::NoDemand;
    }
    pub fn status(&self) -> Status {
        Status {
            enabled: self.policy.enabled,
            window: self.policy.window.min(self.window_limit),
            hold: self.hold,
            pending: self.pending.len(),
            in_flight: self.flight.as_ref().map(|t| t.key),
            counts: self.counts,
        }
    }
    fn cancel_flight(&mut self) {
        if let Some(ticket) = &self.flight {
            if !ticket.cancel.swap(true, Ordering::AcqRel) {
                self.counts.canceled = self.counts.canceled.saturating_add(1);
            }
        }
    }
}
fn in_window(key: FrameKey, demand: Demand, window: usize) -> bool {
    let distance = match demand.direction {
        Direction::Forward => key.frame() - demand.frame,
        Direction::Backward => demand.frame - key.frame(),
    };
    key.context() == demand.context
        && distance >= 0.
        && distance <= window as f64
        && distance.fract() == 0.
}
impl Drop for Scheduler {
    fn drop(&mut self) {
        self.cancel_flight();
    }
}
fn validate_policy(policy: Policy) -> Result<(), Error> {
    if policy.window > MAX_WINDOW
        || !policy.max_seek_frames_per_second.is_finite()
        || policy.max_seek_frames_per_second <= 0.
    {
        Err(Error::InvalidPolicy)
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "prerender/tests.rs"]
mod tests;
