//! Short publication gate shared with dispatch; never holds business/GPU work.
use motion_desktop_media::movie::{prerender::worker, ExportControl};
use std::sync::Mutex;

#[derive(Default)]
struct State {
    generation: u64,
    next_ticket: u64,
    active: Option<(u64, ExportControl)>,
    observation: Option<worker::Observation>,
    foreground_at: Option<std::time::Instant>,
}
#[derive(Default)]
pub(crate) struct Signals(Mutex<State>);

impl Signals {
    pub fn generation(&self) -> Result<u64, String> {
        Ok(self
            .0
            .lock()
            .map_err(|_| "prerender dispatch gate poisoned")?
            .generation)
    }
    /// Run before foreground enqueue. Even a rejected enqueue may preempt
    /// optional background work; it cannot lose a cancellation to a new token.
    pub fn interrupt(&self) -> Result<(), String> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| "prerender dispatch gate poisoned")?;
        state.generation = state
            .generation
            .checked_add(1)
            .ok_or("prerender dispatch generation exhausted")?;
        state.foreground_at = Some(std::time::Instant::now());
        if let Some((_, control)) = &state.active {
            control.cancel();
        }
        Ok(())
    }
    pub fn quiet_for(&self) -> Result<std::time::Duration, String> {
        Ok(self
            .0
            .lock()
            .map_err(|_| "prerender dispatch gate poisoned")?
            .foreground_at
            .map_or(std::time::Duration::MAX, |at| at.elapsed()))
    }
    /// Capture generation before metadata capture. A foreground enqueue during
    /// capture invalidates this publication, even if its handler already ended.
    pub fn publish(&self, generation: u64, control: ExportControl) -> Result<Option<u64>, String> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| "prerender dispatch gate poisoned")?;
        if state.generation != generation {
            return Ok(None);
        }
        if state.active.is_some() {
            return Err("prerender control still awaiting acknowledgment".into());
        }
        let ticket = state
            .next_ticket
            .checked_add(1)
            .ok_or("prerender publication ticket exhausted")?;
        state.next_ticket = ticket;
        state.active = Some((ticket, control));
        Ok(Some(ticket))
    }
    pub fn finish(&self, ticket: u64) -> Result<(), String> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| "prerender dispatch gate poisoned")?;
        if !state.active.as_ref().is_some_and(|(id, _)| *id == ticket) {
            return Err("prerender publication acknowledgment mismatch".into());
        }
        state.active = None;
        Ok(())
    }
    pub fn observe(&self, observation: worker::Observation) -> Result<(), String> {
        self.0
            .lock()
            .map_err(|_| "prerender dispatch gate poisoned")?
            .observation = Some(observation);
        Ok(())
    }
    pub fn snapshot(&self) -> Result<Option<worker::Snapshot>, String> {
        let observation = self
            .0
            .lock()
            .map_err(|_| "prerender dispatch gate poisoned")?
            .observation
            .clone();
        observation.map(|o| o.snapshot()).transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn foreground_before_publication_prevents_a_lost_cancellation() {
        let gate = Signals::default();
        let generation = gate.generation().unwrap();
        gate.interrupt().unwrap();
        assert!(gate
            .publish(generation, ExportControl::default())
            .unwrap()
            .is_none());
    }
    #[test]
    fn foreground_after_publication_cancels_and_preserves_the_slot_until_ack() {
        let gate = Signals::default();
        let control = ExportControl::default();
        let ticket = gate
            .publish(gate.generation().unwrap(), control.clone())
            .unwrap()
            .unwrap();
        gate.interrupt().unwrap();
        assert_eq!(control.check().unwrap_err(), "movie export cancelled");
        assert!(gate
            .publish(gate.generation().unwrap(), ExportControl::default())
            .is_err());
        assert!(gate.finish(ticket + 1).is_err());
        gate.finish(ticket).unwrap();
        let next = gate
            .publish(gate.generation().unwrap(), ExportControl::default())
            .unwrap()
            .unwrap();
        assert_ne!(ticket, next);
    }
}
