//! Application memory policy, not an adapter memory-capacity query.
use serde::{Deserialize, Serialize};

pub const DEFAULT_SOURCE_TEXTURE_BUDGET: u64 = 128 * 1024 * 1024;
pub const MAX_SOURCE_TEXTURE_BUDGET: u64 = 2 * 1024 * 1024 * 1024;
pub const MAX_SCRATCH_BUDGET: u64 = 384 * 1024 * 1024;

/// Per-session allocation limits; these are not measured GPU capacity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryPolicy {
    pub source_texture_budget_bytes: u64,
    pub scratch_budget_bytes: u64,
}
impl Default for MemoryPolicy {
    fn default() -> Self {
        Self {
            source_texture_budget_bytes: DEFAULT_SOURCE_TEXTURE_BUDGET,
            scratch_budget_bytes: motion_effects::SCRATCH_BUDGET_FLOOR,
        }
    }
}
impl MemoryPolicy {
    pub fn validate(self) -> Result<(), String> {
        if !(4..=MAX_SOURCE_TEXTURE_BUDGET).contains(&self.source_texture_budget_bytes) {
            return Err("source texture budget must be 4 bytes..2 GiB".into());
        }
        if !(motion_effects::SCRATCH_BUDGET_FLOOR..=MAX_SCRATCH_BUDGET)
            .contains(&self.scratch_budget_bytes)
        {
            return Err("scratch budget must be 64..384 MiB".into());
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
