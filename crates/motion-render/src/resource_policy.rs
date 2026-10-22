//! Application memory policy, not an adapter memory-capacity query.
use serde::{Deserialize, Serialize};

pub const MEMORY_POLICY_VERSION: u32 = 4;
pub const DEFAULT_SOURCE_TEXTURE_BUDGET: u64 = 128 * 1024 * 1024;
pub const MAX_SOURCE_TEXTURE_BUDGET: u64 = 2 * 1024 * 1024 * 1024;
pub const MAX_SCRATCH_BUDGET: u64 = 384 * 1024 * 1024;
/// Two legacy 16-megapixel RGBA targets. This is an application allowance,
/// separate from source/scratch limits and from physical GPU residency.
pub const MAX_PREVIEW_TARGET_BUDGET: u64 = 128 * 1024 * 1024;

fn default_preview_target_budget() -> u64 {
    MAX_PREVIEW_TARGET_BUDGET
}

/// Per-session allocation limits; these are not measured GPU capacity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryPolicy {
    /// Session-wide logical capacity for enrolled native allocations, including
    /// preview, editor and frozen exports. The Session applies this to its shared
    /// AllocationBudget; standalone renderers use the budget supplied at creation.
    /// This is not a physical VRAM capacity or a per-frame residency measurement.
    pub session_allocation_budget_bytes: u64,
    pub source_texture_budget_bytes: u64,
    pub scratch_budget_bytes: u64,
    /// Combined capacity of the retained front and the next candidate.
    /// Older callers that omit this field retain the previous allowance.
    #[serde(default = "default_preview_target_budget")]
    pub preview_target_budget_bytes: u64,
}
impl Default for MemoryPolicy {
    fn default() -> Self {
        Self {
            session_allocation_budget_bytes: crate::allocation_budget::DEFAULT_RENDER_ALLOCATION_BUDGET,
            source_texture_budget_bytes: DEFAULT_SOURCE_TEXTURE_BUDGET,
            scratch_budget_bytes: motion_effects::SCRATCH_BUDGET_FLOOR,
            preview_target_budget_bytes: default_preview_target_budget(),
        }
    }
}
impl MemoryPolicy {
    pub fn validate(self) -> Result<(), String> {
        if !(1..=crate::allocation_budget::MAX_RENDER_ALLOCATION_BUDGET)
            .contains(&self.session_allocation_budget_bytes)
        {
            return Err("session allocation budget must be 1 byte..4 GiB".into());
        }
        if !(4..=MAX_SOURCE_TEXTURE_BUDGET).contains(&self.source_texture_budget_bytes) {
            return Err("source texture budget must be 4 bytes..2 GiB".into());
        }
        if !(motion_effects::SCRATCH_BUDGET_FLOOR..=MAX_SCRATCH_BUDGET)
            .contains(&self.scratch_budget_bytes)
        {
            return Err("scratch budget must be 64..384 MiB".into());
        }
        if !(8..=MAX_PREVIEW_TARGET_BUDGET).contains(&self.preview_target_budget_bytes) {
            return Err("preview target budget must be 8 bytes..128 MiB".into());
        }
        Ok(())
    }
}
pub fn scratch_budget(total_mem: u64, guarded: bool) -> u64 {
    const GIB: u64 = 1024 * 1024 * 1024;
    if guarded || total_mem == 0 {
        return motion_effects::SCRATCH_BUDGET_FLOOR;
    }
    if total_mem <= 3 * GIB {
        96 * 1024 * 1024
    } else if total_mem <= 6 * GIB {
        192 * 1024 * 1024
    } else {
        MAX_SCRATCH_BUDGET
    }
}
