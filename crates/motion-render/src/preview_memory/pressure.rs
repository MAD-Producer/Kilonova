//! Native adapter readings are advisory inputs to preview quality only.
use crate::{
    MemoryPressurePool, MemoryPressureReading, MemoryPressureSnapshot, MemoryPressureSource,
    MemoryPressureUnavailable,
};
use serde::Serialize;
use std::time::Duration;
use std::time::Instant;

const MIN_SAMPLE_INTERVAL: Duration = Duration::from_secs(1);
const LOWER_INTERVAL: Duration = Duration::from_secs(5);
const RECOVERY_INTERVAL: Duration = Duration::from_secs(30);
const RECOVERY_SAMPLES: u8 = 5;
const RECOVERY_FRAMES: u32 = 120;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewPressureStatus {
    #[default]
    Unknown,
    High,
    Intermediate,
    Low,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewPressureAction {
    None,
    /// First high observation of an episode; preserve every active source/front.
    EvictIdle,
    /// A bounded pressure floor, independent of typed allocation refusals.
    Quality {
        previous: u8,
        level: u8,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct PreviewPressureUpdate {
    pub sampled: bool,
    pub action: PreviewPressureAction,
    pub first_high: bool,
    pub persistent_high: bool,
}
impl Default for PreviewPressureUpdate {
    fn default() -> Self {
        Self {
            sampled: false,
            action: PreviewPressureAction::None,
            first_high: false,
            persistent_high: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewPressureInfo {
    pub scope: &'static str,
    pub status: PreviewPressureStatus,
    pub level: u8,
    pub source: Option<MemoryPressureSource>,
    pub worst_pool: Option<MemoryPressurePool>,
    pub unavailable: Option<MemoryPressureUnavailable>,
    pub consecutive_high_samples: u8,
    pub recovery_samples: u8,
    pub next_sample_in_ms: u64,
    pub next_quality_change_in_ms: u64,
}

#[derive(Debug, Default)]
pub(super) struct NativePressurePolicy {
    level: u8,
    status: PreviewPressureStatus,
    next_sample: Option<Duration>,
    last_native_sample: Option<Instant>,
    next_change: Duration,
    next_recovery: Duration,
    high: u8,
    low: u8,
    reclaimed: bool,
    source: Option<MemoryPressureSource>,
    pool: Option<MemoryPressurePool>,
    unavailable: Option<MemoryPressureUnavailable>,
}

impl NativePressurePolicy {
    pub(super) fn level(&self) -> u8 {
        self.level
    }
    pub(super) fn allows_prefetch(&self) -> bool {
        self.level == 0 && self.status != PreviewPressureStatus::High
    }

    pub(super) fn info(&self, now: Duration) -> PreviewPressureInfo {
        let millis = |time: Duration| {
            time.saturating_sub(now)
                .as_millis()
                .min(u128::from(u64::MAX)) as u64
        };
        PreviewPressureInfo {
            scope: "adapter_advisory_preview_only",
            status: self.status,
            level: self.level,
            source: self.source,
            worst_pool: self.pool,
            unavailable: self.unavailable,
            consecutive_high_samples: self.high,
            recovery_samples: self.low,
            next_sample_in_ms: self.next_sample.map_or(0, millis),
            next_quality_change_in_ms: millis(if self.status == PreviewPressureStatus::Low {
                self.next_recovery
            } else {
                self.next_change
            }),
        }
    }

    pub(super) fn poll(
        &mut self,
        now: Duration,
        successful_frames: u32,
        query: impl FnOnce() -> MemoryPressureSnapshot,
    ) -> PreviewPressureUpdate {
        if self.next_sample.is_some_and(|next| now < next) {
            return PreviewPressureUpdate::default();
        }
        let snapshot = query();
        // Use full Duration precision. Rounding observation times to milliseconds
        // can count the same 1s probe-cache entry twice at a timer boundary.
        let interval =
            MIN_SAMPLE_INTERVAL.max(Duration::from_millis(snapshot.minimum_query_interval_ms));
        self.next_sample = Some(now.saturating_add(interval));
        // Session and native-probe clocks are read at different instants. Even
        // a full policy interval can return the previous cached native sample.
        // Count only a distinct native sample, including at sub-ms boundaries.
        if let Some(sampled) = snapshot.sampled_at {
            if self
                .last_native_sample
                .is_some_and(|prior| sampled <= prior)
            {
                return PreviewPressureUpdate::default();
            }
            self.last_native_sample = Some(sampled);
        }
        let mut update = PreviewPressureUpdate {
            sampled: true,
            ..Default::default()
        };
        self.source = None;
        self.pool = None;
        self.unavailable = None;
        match snapshot.reading {
            MemoryPressureReading::Unavailable { reason, .. } => self.unknown(reason),
            MemoryPressureReading::Available { source, pools } => {
                // A pool budget is not free VRAM. Compare ratios independently;
                // never sum unlike DXGI/Vulkan/Metal pools or subtract our ledger.
                if pools.is_empty() || pools.iter().any(|pool| pool.budget_bytes == 0) {
                    self.unknown(MemoryPressureUnavailable::InvalidNativeResult);
                    return update;
                }
                let Some(worst) = pools.into_iter().max_by(|a, b| {
                    (u128::from(a.usage_bytes) * u128::from(b.budget_bytes))
                        .cmp(&(u128::from(b.usage_bytes) * u128::from(a.budget_bytes)))
                }) else {
                    self.unknown(MemoryPressureUnavailable::InvalidNativeResult);
                    return update;
                };
                self.source = Some(source);
                self.pool = Some(worst);
                let usage = u128::from(worst.usage_bytes) * 100;
                let budget = u128::from(worst.budget_bytes);
                if usage >= budget * 90 {
                    self.status = PreviewPressureStatus::High;
                    self.low = 0;
                    self.next_recovery = now.saturating_add(RECOVERY_INTERVAL);
                    let previous = self.high;
                    self.high = self.high.saturating_add(1).min(60);
                    update.first_high = previous == 0;
                    update.persistent_high = previous == 59;
                    if !self.reclaimed {
                        self.reclaimed = true;
                        update.action = PreviewPressureAction::EvictIdle;
                    } else if self.high >= 2
                        && now >= self.next_change
                        && self.level + 1 < super::PREVIEW_MEMORY_LEVELS
                    {
                        let previous = self.level;
                        self.level += 1;
                        self.next_change = now.saturating_add(LOWER_INTERVAL);
                        update.action = PreviewPressureAction::Quality {
                            previous,
                            level: self.level,
                        };
                    }
                } else if usage <= budget * 70 {
                    self.status = PreviewPressureStatus::Low;
                    self.high = 0;
                    self.reclaimed = false;
                    self.low = self.low.saturating_add(1).min(RECOVERY_SAMPLES);
                    if self.level > 0
                        && self.low >= RECOVERY_SAMPLES
                        && successful_frames >= RECOVERY_FRAMES
                        && now >= self.next_recovery
                    {
                        let previous = self.level;
                        self.level -= 1;
                        self.low = 0;
                        self.next_recovery = now.saturating_add(RECOVERY_INTERVAL);
                        update.action = PreviewPressureAction::Quality {
                            previous,
                            level: self.level,
                        };
                    }
                } else {
                    self.status = PreviewPressureStatus::Intermediate;
                    self.high = 0;
                    self.low = 0;
                    self.reclaimed = false;
                }
            }
        }
        update
    }

    fn unknown(&mut self, reason: MemoryPressureUnavailable) {
        self.status = PreviewPressureStatus::Unknown;
        self.unavailable = Some(reason);
        self.high = 0;
        self.low = 0;
        self.reclaimed = false;
        // Missing hardware information cannot authorize a quality increase.
    }
}

#[cfg(test)]
mod tests;
