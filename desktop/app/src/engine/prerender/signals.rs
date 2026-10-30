//! Short publication gate shared with dispatch; never holds business/GPU work.
use motion_desktop_media::movie::{prerender::worker, ExportControl};
use std::sync::Mutex;

enum Control {
    Cancel(ExportControl),
    Yield(worker::YieldControl),
}

#[derive(Default)]
struct State {
    generation: u64,
    next_ticket: u64,
    active: Option<(u64, Control)>,
    observation: Option<worker::Observation>,
    foreground_at: Option<std::time::Instant>,
    playing: bool,
}
#[derive(Default)]
pub(crate) struct Signals(Mutex<State>);

impl Signals {
    pub fn playing(&self) -> Result<bool, String> {
        Ok(self
            .0
            .lock()
            .map_err(|_| "prerender dispatch gate poisoned")?
            .playing)
    }
    /// Playback state travels outside the serial command queue. It neither
    /// edits the project nor admits work, and repeated publication is a no-op.
    pub fn set_playing(&self, playing: bool) -> Result<(), String> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| "prerender dispatch gate poisoned")?;
        if state.playing != playing {
            Self::interrupt_state(&mut state, true)?;
            state.playing = playing;
        }
        Ok(())
    }
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
        self.interrupt_with(false)
    }
    pub fn preempt(&self) -> Result<(), String> {
        self.interrupt_with(true)
    }
    fn interrupt_with(&self, yielding: bool) -> Result<(), String> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| "prerender dispatch gate poisoned")?;
        Self::interrupt_state(&mut state, yielding)
    }
    fn interrupt_state(state: &mut State, yielding: bool) -> Result<(), String> {
        state.generation = state
            .generation
            .checked_add(1)
            .ok_or("prerender dispatch generation exhausted")?;
        state.foreground_at = Some(std::time::Instant::now());
        if let Some((_, control)) = &state.active {
            match control {
                Control::Cancel(control) => {
                    control.cancel();
                }
                Control::Yield(control) if yielding => {
                    control.preempt();
                }
                Control::Yield(control) => {
                    control.control().cancel();
                }
            }
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
        self.publish_control(generation, Control::Cancel(control))
    }
    pub fn publish_yield(
        &self,
        generation: u64,
        control: worker::YieldControl,
    ) -> Result<Option<u64>, String> {
        self.publish_control(generation, Control::Yield(control))
    }
    fn publish_control(&self, generation: u64, control: Control) -> Result<Option<u64>, String> {
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
    fn playback_changes_invalidate_old_publication_but_repeated_state_does_not() {
        let gate = Signals::default();
        let old = gate.generation().unwrap();
        assert!(!gate.playing().unwrap());
        gate.set_playing(false).unwrap();
        assert_eq!(gate.generation().unwrap(), old);
        gate.set_playing(true).unwrap();
        assert!(gate.playing().unwrap());
        assert!(gate
            .publish_yield(old, worker::YieldControl::default())
            .unwrap()
            .is_none());
        let current = gate.generation().unwrap();
        gate.set_playing(true).unwrap();
        assert_eq!(gate.generation().unwrap(), current);
        let token = worker::YieldControl::default();
        let ticket = gate.publish_yield(current, token.clone()).unwrap().unwrap();
        gate.set_playing(false).unwrap();
        assert_eq!(
            token.control().check().unwrap_err(),
            "movie export cancelled"
        );
        gate.finish(ticket).unwrap();
    }
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

    #[test]
    #[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
    fn actual_dispatch_yields_completed_pixels_for_present_and_invalidates_them_for_edit() {
        use crate::engine::{Engine, UiFrame};
        use motion_desktop_media::movie::prerender::{
            worker::{Outcome, Worker, YieldControl},
            PrerenderPlan,
        };
        use std::time::{Duration, Instant};
        let root = tempfile::tempdir().unwrap();
        let mut project = motion_core::Project::new(32, 24, 30, 8).unwrap();
        project.layers.push(motion_core::Layer::solid(
            1,
            "Dispatch control",
            [32., 24.],
            [16., 12., 0.],
            [1., 0., 0., 1.],
        ));
        motion_core::storage::save(root.path(), &project).unwrap();
        let session = motion_host::session::Session::new(
            project,
            root.path().into(),
            motion_desktop_media::platform(),
        )
        .unwrap();
        let engine = Engine::start(
            root.path().into(),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let mut worker = Worker::start(3072).unwrap();
        fn drain(worker: &mut Worker) -> worker::Completion {
            let began = Instant::now();
            loop {
                if let Some(done) = worker.poll().unwrap() {
                    return done;
                }
                assert!(began.elapsed() < Duration::from_secs(30));
                std::thread::yield_now();
            }
        }
        worker
            .prepare(PrerenderPlan::capture(&session, 1, 2, 82, 3072, &Default::default()).unwrap())
            .unwrap()
            .unwrap();
        assert!(matches!(drain(&mut worker).outcome, Outcome::Prepared(_)));
        for editing in [false, true] {
            let token = YieldControl::default();
            let lease = engine
                .prerender_signals
                .publish_yield(
                    engine.prerender_signals.generation().unwrap(),
                    token.clone(),
                )
                .unwrap()
                .unwrap();
            worker
                .render_with_yield(1., token.clone())
                .unwrap()
                .unwrap();
            let began = Instant::now();
            while !token.completed() {
                assert!(began.elapsed() < Duration::from_secs(30));
                std::thread::yield_now();
            }
            if editing {
                engine
                    .edit(
                        serde_json::to_string(&motion_core::Command::SetColor {
                            object: 1,
                            value: [0., 1., 0., 1.],
                        })
                        .unwrap(),
                    )
                    .unwrap();
            } else {
                let ui = UiFrame {
                    paint: Default::default(),
                    atlas: None,
                    logical_size: [64.; 2],
                    composition: None,
                    inspection: Default::default(),
                    composition_image: None,
                    window: 0,
                    node_preview: None,
                };
                // No window is attached: this checks the real dispatch route,
                // independently of the separate successful native-window proof.
                assert!(engine
                    .present_async(ui)
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap()
                    .unwrap_err()
                    .contains("surface"));
            }
            match (editing, drain(&mut worker).outcome) {
                (false, Outcome::Frame(frame)) => {
                    assert_eq!(frame.image().pixels, [255, 0, 0, 255].repeat(32 * 24))
                }
                (true, Outcome::Cancelled) => {}
                _ => panic!("dispatch did not preserve the yield/invalidation distinction"),
            }
            assert_eq!(worker.snapshot().unwrap().retained_cpu_bytes, 0);
            engine.prerender_signals.finish(lease).unwrap();
        }
        worker.shutdown();
        let began = Instant::now();
        while !worker.stopped() {
            assert!(began.elapsed() < Duration::from_secs(30));
            std::thread::yield_now();
        }
        worker.shutdown();
    }
}
