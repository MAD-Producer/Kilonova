//! Combined logical texture capacity at an explicit observation boundary.
use crate::{SubmissionMemorySnapshot, TextureResourceLedger};
use serde::Serialize;

/// Current ownership unioned with instrumented pending submission identities.
/// Not a frame peak, a reservation, or a driver-residency measurement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameTextureCapacity {
    pub known_owned_bytes: u64,
    pub known_retired_bytes: u64,
    pub known_combined_bytes: u64,
    pub unknown_capacity_count: usize,
    pub active_renderers: usize,
    pub all_active_renderers_tracked: bool,
    pub pending_untracked: usize,
}

impl FrameTextureCapacity {
    /// Snapshots from distinct queues are observed sequentially, not atomically.
    /// Shared allocations across owners and queues count once by identity.
    pub fn observe<'a>(
        owned: &TextureResourceLedger,
        submissions: impl IntoIterator<Item = &'a SubmissionMemorySnapshot>,
    ) -> Self {
        let mut combined = owned.clone();
        let mut active_renderers = 0;
        let mut all_tracked = true;
        let mut pending_untracked = 0usize;
        for snapshot in submissions {
            active_renderers += 1;
            all_tracked &= snapshot.info.enabled;
            pending_untracked = pending_untracked.saturating_add(snapshot.info.pending_untracked);
            combined.merge(snapshot.pending_resources.clone());
        }
        let known_owned_bytes = owned.total_known_bytes();
        let known_combined_bytes = combined.total_known_bytes();
        Self {
            known_owned_bytes,
            known_retired_bytes: known_combined_bytes - known_owned_bytes,
            known_combined_bytes,
            unknown_capacity_count: combined.unknown_capacity_count(),
            active_renderers,
            all_active_renderers_tracked: active_renderers > 0 && all_tracked,
            pending_untracked,
        }
    }
}
