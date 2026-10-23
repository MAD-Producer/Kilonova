//! Opt-in, bounded metadata accounting for instrumented queue submissions.
use crate::TextureResourceLedger;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

mod observation;
use observation::{Recovery, Site};

pub const MAX_SUBMISSION_SNAPSHOTS: usize = 8;

#[derive(Default)]
struct State {
    submitted: u64,
    completed: u64,
    skipped: u64,
    pending_untracked: usize,
    pending: BTreeMap<u64, TextureResourceLedger>,
    allocations: BTreeMap<String, (usize, Option<u64>)>,
    known_bytes: u64,
    peak_known_bytes: u64,
    unknown_count: usize,
}

#[derive(Default)]
pub(crate) struct SubmissionLedger {
    enabled: AtomicBool,
    state: Arc<Mutex<State>>,
}

/// Conservative capacities of snapshots attached to instrumented submissions.
/// No GPU handles are retained. Unknown/uninstrumented allocations are excluded.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmissionMemoryInfo {
    pub enabled: bool,
    pub submitted: u64,
    pub completed: u64,
    pub pending_submissions: usize,
    pub pending_untracked: usize,
    pub skipped_snapshots: u64,
    pub max_snapshots: usize,
    pub known_pending_bytes: u64,
    pub peak_known_pending_bytes: u64,
    /// Pending snapshot resources absent from the current ownership snapshot.
    pub known_retired_bytes: u64,
    pub unknown_capacity_count: usize,
}

/// One metadata-only observation of a renderer's tracked pending submissions.
/// This cannot identify raw submissions or measure physical GPU retirement.
#[derive(Clone, Debug)]
pub struct SubmissionMemorySnapshot {
    pub info: SubmissionMemoryInfo,
    pub pending_resources: TextureResourceLedger,
}

#[derive(Clone, Copy)]
struct Ticket {
    id: u64,
    tracked: bool,
}

impl SubmissionLedger {
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }
    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }
    fn begin(&self, resources: TextureResourceLedger) -> Ticket {
        let mut recovery = Recovery::new(Site::Begin, None);
        let mut state = recovery.lock(self.state.lock());
        state.submitted += 1;
        let id = state.submitted;
        recovery.submission = Some(id);
        let tracked = state.pending.len() < MAX_SUBMISSION_SNAPSHOTS;
        if tracked {
            for resource in resources.resources() {
                if let Some((count, _)) = state.allocations.get_mut(&resource.id) {
                    *count += 1;
                } else {
                    state
                        .allocations
                        .insert(resource.id.clone(), (1, resource.logical_bytes));
                    if let Some(bytes) = resource.logical_bytes {
                        state.known_bytes += bytes;
                    } else {
                        state.unknown_count += 1;
                    }
                }
            }
            state.peak_known_bytes = state.peak_known_bytes.max(state.known_bytes);
            state.pending.insert(id, resources);
        } else {
            state.skipped += 1;
            state.pending_untracked += 1;
        }
        Ticket { id, tracked }
    }
    fn complete(state: &Mutex<State>, ticket: Ticket) {
        let mut recovery = Recovery::new(Site::Complete, Some(ticket.id));
        let mut state = recovery.lock(state.lock());
        let resources = if ticket.tracked {
            state.pending.remove(&ticket.id)
        } else {
            None
        };
        if let Some(resources) = &resources {
            for resource in resources.resources() {
                let (count, bytes) = state
                    .allocations
                    .get_mut(&resource.id)
                    .expect("registered submission allocation");
                *count -= 1;
                if *count == 0 {
                    let bytes = *bytes;
                    state.allocations.remove(&resource.id);
                    if let Some(bytes) = bytes {
                        state.known_bytes -= bytes;
                    } else {
                        state.unknown_count -= 1;
                    }
                }
            }
        } else if !ticket.tracked {
            state.pending_untracked -= 1;
        }
        state.completed += 1;
        drop(state);
        drop(resources);
    }
    pub fn submit(
        &self,
        queue: &wgpu::Queue,
        buffer: wgpu::CommandBuffer,
        resources: TextureResourceLedger,
    ) -> wgpu::SubmissionIndex {
        let ticket = self.begin(resources);
        let index = queue.submit(Some(buffer));
        let state = self.state.clone();
        queue.on_submitted_work_done(move || Self::complete(&state, ticket));
        index
    }
    pub fn info(&self, current: &TextureResourceLedger) -> SubmissionMemoryInfo {
        self.observe(current, false).info
    }
    pub fn snapshot(&self, current: &TextureResourceLedger) -> SubmissionMemorySnapshot {
        self.observe(current, true)
    }
    fn observe(&self, current: &TextureResourceLedger, include_resources: bool) -> SubmissionMemorySnapshot {
        let mut recovery = Recovery::new(Site::Info, None);
        let state = recovery.lock(self.state.lock());
        let active = current
            .resources()
            .map(|r| r.id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        let retired = state
            .allocations
            .iter()
            .filter(|(id, _)| !active.contains(id.as_str()))
            .filter_map(|(_, (_, bytes))| *bytes)
            .sum();
        let info = SubmissionMemoryInfo {
            enabled: self.enabled(),
            submitted: state.submitted,
            completed: state.completed,
            pending_submissions: state.pending.len() + state.pending_untracked,
            pending_untracked: state.pending_untracked,
            skipped_snapshots: state.skipped,
            max_snapshots: MAX_SUBMISSION_SNAPSHOTS,
            known_pending_bytes: state.known_bytes,
            peak_known_pending_bytes: state.peak_known_bytes,
            known_retired_bytes: retired,
            unknown_capacity_count: state.unknown_count,
        };
        let mut pending_resources = TextureResourceLedger::default();
        if include_resources {
            for resources in state.pending.values() {
                pending_resources.merge(resources.clone());
            }
        }
        SubmissionMemorySnapshot { info, pending_resources }
    }
}

#[cfg(test)]
mod observation_tests;

#[cfg(test)]
mod frame_capacity_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
    fn shared_submissions_keep_replaced_resources_until_the_last_completion() {
        let mut renderer = pollster::block_on(crate::Renderer::headless()).unwrap();
        renderer.upload_image(7, 2, 2, &[255; 16]).unwrap();
        let owned = renderer.texture_resource_ledger();
        let tracker = SubmissionLedger::default();
        tracker.set_enabled(true);
        let first = tracker.begin(owned.clone());
        let second = tracker.begin(owned.clone());
        renderer.clear_assets();
        let current = renderer.texture_resource_ledger();
        let info = tracker.info(&current);
        assert_eq!(info.known_pending_bytes, owned.total_known_bytes());
        assert_eq!(info.known_retired_bytes, 16);
        assert_eq!(info.peak_known_pending_bytes, owned.total_known_bytes());
        assert_eq!(info.pending_submissions, 2);
        SubmissionLedger::complete(&tracker.state, second);
        assert_eq!(tracker.info(&current).known_retired_bytes, 16);
        tracker.set_enabled(false);
        assert_eq!(tracker.info(&current).known_retired_bytes, 16);
        SubmissionLedger::complete(&tracker.state, first);
        let info = tracker.info(&current);
        assert_eq!(info.known_pending_bytes, 0);
        assert_eq!(info.known_retired_bytes, 0);
        assert_eq!(info.completed, 2);
        assert_eq!(info.peak_known_pending_bytes, owned.total_known_bytes());
    }
    #[test]
    fn snapshot_limit_reports_gaps_instead_of_growing_or_forgetting_pending_work() {
        let tracker = SubmissionLedger::default();
        let tickets = (0..MAX_SUBMISSION_SNAPSHOTS + 3)
            .map(|_| tracker.begin(Default::default()))
            .collect::<Vec<_>>();
        let info = tracker.info(&Default::default());
        assert_eq!(info.pending_submissions, MAX_SUBMISSION_SNAPSHOTS + 3);
        assert_eq!(info.pending_untracked, 3);
        assert_eq!(info.skipped_snapshots, 3);
        for ticket in tickets.into_iter().rev() {
            SubmissionLedger::complete(&tracker.state, ticket);
        }
        let info = tracker.info(&Default::default());
        assert_eq!(info.pending_submissions, 0);
        assert_eq!(info.pending_untracked, 0);
        assert_eq!(info.completed, info.submitted);
    }
}
