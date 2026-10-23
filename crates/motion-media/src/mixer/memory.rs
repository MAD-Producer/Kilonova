use super::*;
use crate::memory::{CpuBufferAllocation, CpuMemoryInfo};
use serde_json::json;

impl AudioMixer {
    /// Explicit owned-buffer metadata. No file reads/seeks and no mixing.
    /// Tables shared by source Arc identity count once within this mixer.
    pub fn memory_info(&self) -> CpuMemoryInfo {
        let mut resources = vec![
            CpuBufferAllocation::vector(
                "input-bytes".into(),
                "audio_pcm_input_scratch",
                &self.bytes,
            ),
            CpuBufferAllocation::vector(
                "resample-samples".into(),
                "audio_resample_scratch",
                &self.samples,
            ),
        ];
        resources.retain(|r| r.capacity_bytes > 0);
        let mut identities = HashMap::new();
        for source in &self.sources {
            let next_table = identities.len() + 1;
            let index = *identities
                .entry(Arc::as_ptr(&source.weights))
                .or_insert_with(|| {
                    resources.push(CpuBufferAllocation::vector(
                        format!("sinc-table-{next_table}"),
                        "audio_sinc_table",
                        source.weights.as_ref(),
                    ));
                    resources.len() - 1
                });
            resources[index]
                .references
                .push(json!({"source":source.id}));
        }
        CpuMemoryInfo::new(
            "audio_mixer_owned_scratch_and_shared_tables",
            resources,
            json!({"version":1,"identityScope":"one_mixer_snapshot",
                "sourceCount":self.sources.len(),"uniqueResamplingTables":identities.len(),
                "unknownBytes":null,"fileBackedPcmResidentBytes":null,
                "excludes":["disk_pcm_file_size","OS_file_cache","project_voices_and_container_storage",
                    "external_allocations","allocator_and_Arc_overhead"]}),
        )
    }
}

#[cfg(test)]
mod tests;
