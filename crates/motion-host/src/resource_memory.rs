//! Explicit diagnostic metadata; these values are not total process RAM.
use serde::Serialize;
use serde_json::Value;

/// Owned pixel vectors observed by one bounded cache query.
/// Capacity counts Vec backing storage, excluding allocator/container overhead.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PixelMemorySnapshot {
    pub known_payload_bytes: u64,
    pub known_capacity_bytes: u64,
    #[serde(flatten)]
    pub details: Value,
}
