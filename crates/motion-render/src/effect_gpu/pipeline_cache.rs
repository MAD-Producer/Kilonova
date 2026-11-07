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

// Snapshot only immutable shader proof and native handles. Large source copies
// belong on the preparation worker, never the renderer owner. The WGSL bound
// describes proof text; it is not a bound on driver allocations or GLSL metadata.
enum ShaderProof {
    Live(Arc<CompiledShader>),
    Retired(Arc<str>),
}
pub(crate) struct KnownPipeline {
    pipeline: wgpu::RenderPipeline,
    proof: ShaderProof,
    format: u32,
    sprite: bool,
    additive: bool,
}
impl KnownPipeline {
    fn wgsl(&self) -> &str {
        match &self.proof {
            ShaderProof::Live(shader) => &shader.wgsl,
            ShaderProof::Retired(wgsl) => wgsl,
        }
    }
    pub(crate) fn matches(&self, shader: &CompiledShader, format: u32) -> bool {
        self.format == format
            && self.sprite == shader.sprite
            && self.additive == shader.additive
            && self.wgsl() == shader.wgsl
    }
    pub(crate) fn prepared(&self) -> crate::pipeline_preparation::PreparedPipeline {
        crate::pipeline_preparation::PreparedPipeline {
            pipeline: self.pipeline.clone(),
            wgsl: match &self.proof {
                ShaderProof::Live(shader) => Arc::from(shader.wgsl.as_str()),
                ShaderProof::Retired(wgsl) => wgsl.clone(),
            },
            format: self.format,
            sprite: self.sprite,
            additive: self.additive,
        }
    }
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
    /// Native creations on the renderer owner, excluding preparation workers.
    /// Lifetime counter; registry changes do not reset it.
    pub foreground_builds: u64,
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
    /// At most one bounded bank per queued/running preparation job. Reuse still
    /// requires complete code, blend, vertex specialization and format equality.
    pub(crate) fn preparation_snapshot(&self) -> Vec<KnownPipeline> {
        let mut entries: Vec<KnownPipeline> = Vec::new();
        let mut bytes = 0;
        for ((_, format), entry) in &self.pipelines {
            let shader = &entry.shader;
            if entries.len() == MAX_PREVIOUS_PIPELINES {
                break;
            }
            if shader.wgsl.len() > MAX_PREVIOUS_WGSL_BYTES - bytes
                || entries.iter().any(|entry| entry.matches(shader, *format))
            {
                continue;
            }
            bytes += shader.wgsl.len();
            entries.push(KnownPipeline {
                pipeline: entry.pipeline.clone(),
                proof: ShaderProof::Live(shader.clone()),
                format: *format,
                sprite: shader.sprite,
                additive: shader.additive,
            });
        }
        for entry in &self.previous_pipelines.entries {
            if entries.len() == MAX_PREVIOUS_PIPELINES {
                break;
            }
            if entry.wgsl.len() > MAX_PREVIOUS_WGSL_BYTES - bytes
                || entries.iter().any(|known| {
                    known.format == entry.format
                        && known.sprite == entry.sprite
                        && known.additive == entry.additive
                        && known.wgsl() == entry.wgsl.as_ref()
                })
            {
                continue;
            }
            bytes += entry.wgsl.len();
            entries.push(KnownPipeline {
                pipeline: entry.pipeline.clone(),
                proof: ShaderProof::Retired(entry.wgsl.clone()),
                format: entry.format,
                sprite: entry.sprite,
                additive: entry.additive,
            });
        }
        entries
    }
    /// Owner-approved work, with the same complete code/specialization proof as
    /// retired live pipelines. Replaces one bounded bank, never appends epochs.
    pub(crate) fn install_prepared(
        &mut self,
        batch: &mut crate::PreparedEffectPipelines,
    ) -> Result<(), RenderError> {
        let bytes = batch
            .entries
            .iter()
            .try_fold(0usize, |sum, entry| sum.checked_add(entry.wgsl.len()))
            .ok_or("prepared pipeline byte count overflow")?;
        if batch.entries.len() > MAX_PREVIOUS_PIPELINES || bytes > MAX_PREVIOUS_WGSL_BYTES {
            return Err("prepared pipelines exceed the bounded reuse bank".into());
        }
        let entries = std::mem::take(&mut batch.entries)
            .into_iter()
            .map(|entry| PreviousPipeline {
                pipeline: entry.pipeline,
                wgsl: entry.wgsl,
                format: entry.format,
                sprite: entry.sprite,
                additive: entry.additive,
            })
            .collect();
        self.previous_pipelines.entries = entries;
        self.previous_pipelines.wgsl_bytes = bytes;
        Ok(())
    }
    pub fn pipeline_cache_info(&self) -> EffectPipelineCacheInfo {
        EffectPipelineCacheInfo {
            foreground_builds: self.pipeline_builds,
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
