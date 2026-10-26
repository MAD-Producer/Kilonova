//! Successful pipelines from one preceding registry generation. Program IDs
//! may change; code, vertex/blend specialization and target format must match.
use super::*;
use motion_effects::shader::CompiledShader;
use std::sync::Arc;

const MAX_PREVIOUS_PIPELINES: usize = 128;
const MAX_PREVIOUS_WGSL_BYTES: usize = 8 << 20;

#[cfg(test)]
mod tests;

pub(super) struct Pipeline {
    pub(super) pipeline: wgpu::RenderPipeline,
    // A private strong reference prevents mutation through Arc::get_mut.
    shader: Arc<CompiledShader>,
    used: u64,
}
struct PreviousPipeline {
    pipeline: wgpu::RenderPipeline,
    wgsl: Arc<str>,
    format: u32,
    sprite: bool,
    additive: bool,
}
#[derive(Default)]
pub(super) struct PreviousPipelines {
    entries: Vec<PreviousPipeline>,
    wgsl_bytes: usize,
    clock: u64,
    reuses: u64,
}
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectPipelineCacheInfo {
    pub current_entries: usize,
    pub previous_entries: usize,
    /// UTF-8 bytes retained for comparison, excluding containers and driver code.
    pub previous_wgsl_bytes: usize,
    pub previous_entry_limit: usize,
    pub previous_wgsl_byte_limit: usize,
    pub reuses: u64,
}

fn same_code(a: &CompiledShader, b: &CompiledShader) -> bool {
    a.wgsl == b.wgsl && a.sprite == b.sprite && a.additive == b.additive
}

impl GpuState {
    pub fn pipeline_cache_info(&self) -> EffectPipelineCacheInfo {
        EffectPipelineCacheInfo {
            current_entries: self.pipelines.len(),
            previous_entries: self.previous_pipelines.entries.len(),
            previous_wgsl_bytes: self.previous_pipelines.wgsl_bytes,
            previous_entry_limit: MAX_PREVIOUS_PIPELINES,
            previous_wgsl_byte_limit: MAX_PREVIOUS_WGSL_BYTES,
            reuses: self.previous_pipelines.reuses,
        }
    }

    pub(super) fn retire_pipelines(&mut self) {
        // Multiple registry updates before preparation still share the last
        // prepared generation; an empty generation must not erase its proof.
        if self.pipelines.is_empty() {
            return;
        }
        // Replace, never append generations. No package/resources/old registry
        // or admission leases are kept by this pipeline-only cache.
        self.previous_pipelines.entries.clear();
        self.previous_pipelines.wgsl_bytes = 0;
        let mut entries: Vec<_> = self.pipelines.drain().collect();
        entries.sort_unstable_by_key(|(_, entry)| std::cmp::Reverse(entry.used));
        for ((_, format), entry) in entries {
            if self.previous_pipelines.entries.len() == MAX_PREVIOUS_PIPELINES {
                break;
            }
            let bytes = entry.shader.wgsl.len();
            if bytes > MAX_PREVIOUS_WGSL_BYTES - self.previous_pipelines.wgsl_bytes {
                continue;
            }
            self.previous_pipelines.entries.push(PreviousPipeline {
                pipeline: entry.pipeline,
                wgsl: Arc::from(entry.shader.wgsl.as_str()),
                format,
                sprite: entry.shader.sprite,
                additive: entry.shader.additive,
            });
            self.previous_pipelines.wgsl_bytes += bytes;
        }
    }

    pub(super) fn reuse_pipeline(
        &mut self,
        key: (u32, u32),
        shader: &Arc<CompiledShader>,
        force_compile: bool,
    ) {
        self.previous_pipelines.clock = self.previous_pipelines.clock.saturating_add(1);
        let used = self.previous_pipelines.clock;
        if force_compile {
            self.pipelines.remove(&key);
            return;
        }
        if let Some(entry) = self.pipelines.get_mut(&key) {
            // The healthy frame path compares only private Arc identities.
            if Arc::ptr_eq(&entry.shader, shader) || same_code(&entry.shader, shader) {
                entry.shader = shader.clone();
                entry.used = used;
                return;
            }
        }
        self.pipelines.remove(&key);
        if let Some(entry) = self.previous_pipelines.entries.iter().find(|entry| {
            entry.format == key.1
                && entry.sprite == shader.sprite
                && entry.additive == shader.additive
                && entry.wgsl.as_ref() == shader.wgsl
        }) {
            self.pipelines.insert(
                key,
                Pipeline {
                    pipeline: entry.pipeline.clone(),
                    shader: shader.clone(),
                    used,
                },
            );
            self.previous_pipelines.reuses = self.previous_pipelines.reuses.saturating_add(1);
        }
    }

    pub(super) fn insert_pipeline(
        &mut self,
        key: (u32, u32),
        shader: &Arc<CompiledShader>,
        pipeline: wgpu::RenderPipeline,
    ) {
        self.pipelines.insert(
            key,
            Pipeline {
                pipeline,
                shader: shader.clone(),
                used: self.previous_pipelines.clock,
            },
        );
    }
}
