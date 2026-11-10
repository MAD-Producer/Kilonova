//! Native adapter samples, separate from managed buffers and process peaks.
use super::{Error, Result};
use motion_render::{MemoryPressureReading, MemoryPressureSnapshot, MemoryPressureSource};
use serde::{Deserialize, Serialize};

/// Two advisory native observations made by the owned inference process.
/// Neither the larger observation nor their sum is a device-memory peak.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterMemorySamples {
    pub pid: u32,
    pub before: MemoryPressureSnapshot,
    pub after: MemoryPressureSnapshot,
}

impl AdapterMemorySamples {
    pub(crate) fn validate(&self, pid: u32, backend: &str) -> Result<()> {
        if self.pid == 0 || self.pid != pid {
            return Err(Error::Invalid(
                "adapter memory sample process differs from inference",
            ));
        }
        for sample in [&self.before, &self.after] {
            if sample.version != 1
                || sample.backend != backend
                || sample.minimum_query_interval_ms != 1000
                || sample.is_physical_capacity
                || sample.is_frame_peak
            {
                return Err(Error::Invalid(
                    "adapter memory sample identity or semantics differ",
                ));
            }
            match &sample.reading {
                MemoryPressureReading::Available { source, pools } => {
                    let correct_source = matches!(
                        (backend, source),
                        ("Dx12", MemoryPressureSource::DxgiLocalSegment)
                            | ("Vulkan", MemoryPressureSource::VulkanDeviceLocalHeaps)
                            | ("Metal", MemoryPressureSource::MetalWorkingSet)
                    );
                    if !correct_source
                        || pools.is_empty()
                        || pools.len() > 16
                        || pools.iter().any(|p| p.index >= 16)
                        || pools.windows(2).any(|p| p[0].index >= p[1].index)
                        || (*source != MemoryPressureSource::VulkanDeviceLocalHeaps
                            && (pools.len() != 1 || pools[0].index != 0))
                    {
                        return Err(Error::Invalid(
                            "adapter memory native pools differ or exceed bounds",
                        ));
                    }
                }
                MemoryPressureReading::Unavailable { cause, .. } => {
                    if cause.as_ref().is_some_and(|c| c.chars().count() > 512) {
                        return Err(Error::Invalid("adapter memory failure cause exceeds bound"));
                    }
                }
            }
        }
        Ok(())
    }
}
