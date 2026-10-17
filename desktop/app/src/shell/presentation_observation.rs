//! Evidence for action-induced deferral; never changes the presentation policy.
use std::{collections::HashMap, time::Instant};

#[derive(Default)]
pub(crate) struct ActionDeferrals(HashMap<u64, Episode>);
struct Episode {
    count: usize,
    began: Instant,
}
impl ActionDeferrals {
    pub(crate) fn deferred(
        &mut self,
        window: u64,
        phase: &str,
        actions: usize,
        editing: bool,
        worker_queue_depth: usize,
    ) {
        let episode = self.0.entry(window).or_insert_with(|| Episode {
            count: 0,
            began: Instant::now(),
        });
        episode.count = episode.count.saturating_add(1);
        if episode.count == 1 {
            log::debug!("presentation deferred site=shell.present.actions window={window} count=1 phase={phase} queued_actions={actions} editing={editing} worker_queue_depth={worker_queue_depth}");
        } else if episode.count == 60 {
            log::error!("persistent presentation deferral site=shell.present.actions window={window} count=60 phase={phase} elapsed_ms={} queued_actions={actions} editing={editing} worker_queue_depth={worker_queue_depth}", episode.began.elapsed().as_millis());
        }
    }

    /// Submission ends this action-defer episode, not proof of a visible frame.
    pub(crate) fn submitted(&mut self, window: u64) {
        if let Some(episode) = self.0.remove(&window) {
            log::debug!("presentation submitted after action deferral window={window} count={} elapsed_ms={}", episode.count, episode.began.elapsed().as_millis());
        }
    }

    pub(crate) fn closed(&mut self, window: u64) {
        self.0.remove(&window);
    }
}

#[cfg(test)]
mod tests;
