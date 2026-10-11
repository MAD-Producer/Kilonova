//! Bounded preview policy. It never changes a project or a formal export.
use crate::{image_resources::Resolution, ResourceRejection};
use serde::Serialize;

pub const PREVIEW_MEMORY_LEVELS: u8 = 4;
pub const PREVIEW_MEMORY_COOLDOWN_MS: u64 = 30_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewMemoryQuality {
    pub level: u8,
    pub image_edge: Option<u32>,
    pub video_edge: Option<u32>,
    pub target_divisor: u32,
}
impl PreviewMemoryQuality {
    pub fn image_resolution(self) -> Resolution {
        self.image_edge
            .map_or(Resolution::Full, Resolution::Preview)
    }
    pub fn dimensions(self, width: u32, height: u32) -> (u32, u32) {
        (
            width.div_ceil(self.target_divisor).max(1),
            height.div_ceil(self.target_divisor).max(1),
        )
    }
}

#[derive(Debug, Default)]
pub struct PreviewMemoryPolicy {
    level: u8,
    blocked: bool,
    failures: u8,
    successes: u32,
    next_probe_ms: u64,
    recovery: Option<u8>,
    pub refusal_count: u64,
    pub last_rejection: Option<ResourceRejection>,
}
impl PreviewMemoryPolicy {
    pub fn quality(&self, high: bool) -> PreviewMemoryQuality {
        let edge = 2048 >> self.level;
        PreviewMemoryQuality {
            level: self.level,
            image_edge: if high && self.level == 0 {
                None
            } else {
                Some(edge)
            },
            video_edge: if self.level == 0 { None } else { Some(edge) },
            target_divisor: 1 << self.level,
        }
    }
    pub fn blocked(&self) -> bool {
        self.blocked
    }
    pub fn failures(&self) -> u8 {
        self.failures
    }
    pub fn next_probe_ms(&self) -> u64 {
        self.next_probe_ms
    }
    /// One request after cooldown probes the lowest level, never restarts at full.
    pub fn begin(&mut self, now_ms: u64) -> bool {
        if self.blocked {
            if now_ms < self.next_probe_ms {
                return false;
            }
            self.blocked = false;
        } else if self.level > 0 && self.successes >= 120 && now_ms >= self.next_probe_ms {
            self.recovery = Some(self.level);
            self.level -= 1;
            self.successes = 0;
        }
        true
    }
    /// Return whether a strictly smaller candidate (or known working level) exists.
    pub fn refused(&mut self, rejection: ResourceRejection, now_ms: u64) -> bool {
        self.last_rejection = Some(rejection);
        self.refusal_count += 1;
        self.failures = self.failures.saturating_add(1).min(PREVIEW_MEMORY_LEVELS);
        self.successes = 0;
        self.next_probe_ms = now_ms.saturating_add(PREVIEW_MEMORY_COOLDOWN_MS);
        if let Some(working) = self.recovery.take() {
            self.level = working;
            return true;
        }
        if self.level + 1 < PREVIEW_MEMORY_LEVELS {
            self.level += 1;
            true
        } else {
            self.blocked = true;
            false
        }
    }
    pub fn succeeded(&mut self) {
        self.failures = 0;
        self.blocked = false;
        self.recovery = None;
        self.successes = self.successes.saturating_add(1);
        if self.level == 0 {
            self.last_rejection = None;
        }
    }
    /// An edit can remove the pressure; allow one probe at the retained level.
    pub fn changed(&mut self) {
        self.blocked = false;
        self.failures = 0;
        self.successes = 0;
        self.recovery = None;
    }
}
