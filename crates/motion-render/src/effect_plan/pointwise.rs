//! Opt-in native prototype. Original effect admission remains authoritative;
//! replacing passes never relaxes existing budget or dimension checks.
use super::*;
use motion_effects::pointwise::{
    Eligibility, PointwiseChain, PointwiseStage, PortableFusionMetrics, StageNamespace,
};
use serde::Serialize;
use std::collections::BTreeMap;

const MAX_COMPILED_GROUPS: usize = 64;
const MAX_REPORTED_GROUPS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FusionRetainReason {
    UnprovedImplementation,
    RuntimeBoundary,
    SingleStage,
    ParameterOrStageLimit,
    CacheLimit,
    QuantizedAlphaBoundary,
    DownstreamBoundary,
}

#[derive(Clone, Debug, Serialize)]
pub struct FusionGroupReport {
    pub object: u64,
    pub effects: Vec<u64>,
    pub program: u32,
    pub structure_key: String,
    pub original_passes: usize,
    pub fused_passes: usize,
    pub portable: PortableFusionMetrics,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct FusionFrameReport {
    pub enabled: bool,
    pub original_passes: usize,
    pub final_passes: usize,
    /// Logical RGBA reads+writes; excludes caches, compression and bus traffic.
    pub original_texture_traffic_bytes: u64,
    pub final_texture_traffic_bytes: u64,
    pub groups: Vec<FusionGroupReport>,
    pub omitted_groups: usize,
    pub retained: BTreeMap<FusionRetainReason, u64>,
    pub portable_compiles: u64,
    pub cache_entries: usize,
}

enum Compilation {
    Ready {
        program: u32,
        metrics: PortableFusionMetrics,
    },
    Failed(Arc<RenderError>),
}

#[derive(Default)]
pub(super) struct FusionState {
    enabled: bool,
    compiled: BTreeMap<String, Compilation>,
    compiles: u64,
    report: FusionFrameReport,
}
impl FusionState {
    pub(super) fn reset_programs(&mut self) {
        // Native program IDs are invalid after a registry replacement.
        // Lifetime compile counters stay monotonic, just like native counters.
        self.compiled.clear();
        self.report = FusionFrameReport::default();
    }
}

struct Candidate {
    stage: PointwiseStage,
    namespace: StageNamespace,
}

fn traffic(passes: &[EffectPass], precision: ChannelPrecision) -> u64 {
    // Image-only draws read input and original source and write one attachment.
    // Utility conversions also expose two bindings, so this is a declared upper
    // bound, not measured physical memory bandwidth.
    let bpp = motion_effects::pointwise::canonical_boundary(precision)
        .storage
        .bytes_per_pixel();
    passes.iter().fold(0u64, |sum, p| {
        sum.saturating_add(u64::from(p.width) * u64::from(p.height) * bpp * 3)
    })
}

impl PlanBuilder {
    /// The initial proof family is deliberately opt-in. Ordinary users keep
    /// their current execution policy until native acceptance is complete.
    pub fn set_pointwise_fusion(&mut self, enabled: bool) {
        if self.fusion.enabled != enabled {
            self.preview_inputs = None;
            self.fusion.enabled = enabled;
        }
    }

    pub fn pointwise_fusion_report(&self) -> &FusionFrameReport {
        &self.fusion.report
    }

    fn pointwise_candidate(
        &self,
        scene: &Scene,
        at: usize,
        end: usize,
    ) -> Result<Option<Candidate>, RenderError> {
        if at + 3 > end {
            return Ok(None);
        }
        let [a, b, c] = &self.frame.passes[at..at + 3] else {
            return Ok(None);
        };
        let Some(effect_index) = self.pass_clocks[at] else {
            return Ok(None);
        };
        if self.pass_clocks[at + 1..at + 3] != [Some(effect_index); 2] {
            return Ok(None);
        }
        let e = &scene.effects[effect_index];
        let Some(resolved) = &self.resolved[effect_index] else {
            return Ok(None);
        };
        let Eligibility::Proven(proof) = &resolved.pointwise else {
            return Ok(None);
        };
        let scale = a.uniform.output_mode[3];
        let edge = match proof.edge_mode() {
            EdgeMode::Transparent => 0.,
            EdgeMode::Clamp => 1.,
            _ => return Ok(None),
        };
        let same_rect = |p: &EffectPass| {
            p.width == a.width
                && p.height == a.height
                && p.uniform.size == a.uniform.size
                && p.uniform.region == a.uniform.region
                && p.uniform.input_region == a.uniform.region
                && p.uniform.source_region == a.uniform.region
                && p.uniform.output_mode[3] == scale
                && !p.sprite
                && p.sprite_count == 0
                && p.resource_input == 0
                && p.lut == -1
        };
        if !e.enabled
            || e.image_input.is_some()
            || e.scene.is_some()
            || !scale.is_finite()
            || scale <= 0.
            || !same_rect(a)
            || !same_rect(b)
            || !same_rect(c)
            || a.program != 1
            || a.input != 0
            || a.source != 0
            || a.output != 1
            || b.input != 1
            || b.source != 1
            || b.output != 2
            || c.program != 1
            || c.input != 2
            || c.source != 2
            || c.output != 0
            || resolved.programs.as_slice() != [b.program]
            || a.uniform.mode != [0., 0., 0., 0.]
            || a.uniform.output_mode != [1., 1., 1., scale]
            || b.uniform.mode != [edge, 1., 1., 0.]
            || b.uniform.output_mode[..2] != [1., 1.]
            || c.uniform.mode != [edge, 1., 1., 0.]
            || c.uniform.output_mode != [0., 0., 1., scale]
        {
            return Ok(None);
        }
        let Some(program) = self.programs.get(b.program as usize) else {
            return Ok(None);
        };
        if !program.resources.is_empty() || !proof.matches_compiled(&program.shader)? {
            return Ok(None);
        }
        let Some(opacity) = proof
            .parameters()
            .iter()
            .position(|p| p.id == "effect_opacity")
        else {
            return Ok(None);
        };
        if b.uniform.output_mode[2] != b.uniform.params[opacity][0] / 100. {
            return Ok(None);
        }
        let namespace = StageNamespace {
            object: e.layer,
            effect: e.instance,
        };
        Ok(Some(Candidate {
            stage: PointwiseStage::new(
                proof.clone(),
                namespace,
                &b.uniform.params[..proof.parameters().len()],
            )?,
            namespace,
        }))
    }

    fn fused_program(
        &mut self,
        chain: &PointwiseChain,
    ) -> Result<Option<(u32, PortableFusionMetrics)>, RenderError> {
        let key = chain.structure_key();
        if let Some(cached) = self.fusion.compiled.get(&key) {
            return match cached {
                Compilation::Ready { program, metrics } => {
                    if let Some(cause) = self.program_errors.get(program) {
                        return Err(RenderError::Invalid(cause.clone()));
                    }
                    Ok(Some((*program, metrics.clone())))
                }
                Compilation::Failed(error) => Err(RenderError::Shared(error.clone())),
            };
        }
        if self.fusion.compiled.len() >= MAX_COMPILED_GROUPS {
            return Ok(None);
        }
        log::info!(
            "start operation=pointwise_compile structure={} stages={} cells={}",
            key,
            chain.instructions().len(),
            chain.bindings().len()
        );
        self.fusion.compiles = self.fusion.compiles.saturating_add(1);
        match chain.compile() {
            Ok(fused) => {
                let id = self.programs.len() as u32;
                log::debug!(
                    "done operation=pointwise_compile structure={} elapsed_ns={} wgsl_bytes={}",
                    key,
                    fused.metrics.portable_compile_nanos,
                    fused.metrics.compiled_wgsl_bytes
                );
                self.programs.push(EffectProgram {
                    key: key.clone(),
                    shader: Arc::new(fused.compiled),
                    package: None,
                    resources: vec![],
                });
                self.fusion.compiled.insert(
                    key,
                    Compilation::Ready {
                        program: id,
                        metrics: fused.metrics.clone(),
                    },
                );
                Ok(Some((id, fused.metrics)))
            }
            Err(error) => {
                log::error!(
                    "operation=pointwise_compile structure={} cause={error}",
                    key
                );
                let error = Arc::new(RenderError::from(error));
                self.fusion
                    .compiled
                    .insert(key, Compilation::Failed(error.clone()));
                Err(RenderError::Shared(error))
            }
        }
    }

    pub(super) fn fuse_pointwise(&mut self, scene: &Scene) -> Result<(), RenderError> {
        let mut report = FusionFrameReport {
            enabled: self.fusion.enabled,
            original_passes: self.frame.passes.len(),
            original_texture_traffic_bytes: traffic(&self.frame.passes, self.precision),
            portable_compiles: self.fusion.compiles,
            cache_entries: self.fusion.compiled.len(),
            ..Default::default()
        };
        if !self.fusion.enabled {
            report.final_passes = report.original_passes;
            report.final_texture_traffic_bytes = report.original_texture_traffic_bytes;
            self.fusion.report = report;
            return Ok(());
        }
        let mut passes = Vec::with_capacity(self.frame.passes.len());
        let mut clocks = Vec::with_capacity(self.pass_clocks.len());
        for (index, _) in scene.effects.iter().enumerate().filter(|(_, e)| e.enabled) {
            if self.resolved[index]
                .as_ref()
                .is_some_and(|r| matches!(r.pointwise, Eligibility::Retain(_)))
            {
                *report
                    .retained
                    .entry(FusionRetainReason::UnprovedImplementation)
                    .or_default() += 1;
            }
        }
        for draw_index in 0..self.frame.draws.len() {
            let draw = self.frame.draws[draw_index];
            self.frame.draws[draw_index].pass_start = passes.len();
            // An integer storage rounding difference can be magnified when tiny alpha
            // is lifted later. Retain the whole draw until this is proved,
            // including any groups preceding the alpha-changing operation.
            let quantized_alpha = matches!(
                self.precision,
                ChannelPrecision::Unorm8 | ChannelPrecision::Unorm16
            ) && self.pass_clocks[draw.pass_start..draw.pass_end]
                .iter()
                .flatten()
                .any(|&i| {
                    let e = &scene.effects[i];
                    e.effect == "invert"
                        && e.param_ids
                            .iter()
                            .position(|p| p == "channel")
                            .is_some_and(|slot| e.values[slot][0] == 5.)
                });
            if quantized_alpha {
                *report
                    .retained
                    .entry(FusionRetainReason::QuantizedAlphaBoundary)
                    .or_default() += 1;
                passes.extend_from_slice(&self.frame.passes[draw.pass_start..draw.pass_end]);
                clocks.extend_from_slice(&self.pass_clocks[draw.pass_start..draw.pass_end]);
                self.frame.draws[draw_index].pass_end = passes.len();
                continue;
            }
            let mut at = draw.pass_start;
            while at < draw.pass_end {
                let Some(first) = self.pointwise_candidate(scene, at, draw.pass_end)? else {
                    let p = &self.frame.passes[at];
                    if p.program == 1
                        && p.input == 0
                        && p.output == 1
                        && self.pass_clocks[at].is_some_and(|i| {
                            self.resolved[i]
                                .as_ref()
                                .is_some_and(|r| matches!(r.pointwise, Eligibility::Proven(_)))
                        })
                    {
                        *report
                            .retained
                            .entry(FusionRetainReason::RuntimeBoundary)
                            .or_default() += 1;
                    }
                    passes.push(self.frame.passes[at]);
                    clocks.push(self.pass_clocks[at]);
                    at += 1;
                    continue;
                };
                let mut candidates = vec![first];
                let mut end = at + 3;
                let mut chain = None;
                while end < draw.pass_end {
                    let Some(next) = self.pointwise_candidate(scene, end, draw.pass_end)? else {
                        break;
                    };
                    if self.frame.passes[end].uniform.region != self.frame.passes[at].uniform.region
                        || self.frame.passes[end].uniform.size != self.frame.passes[at].uniform.size
                        || self.frame.passes[end].uniform.output_mode[3]
                            != self.frame.passes[at].uniform.output_mode[3]
                    {
                        break;
                    }
                    candidates.push(next);
                    let stages: Vec<_> = candidates.iter().map(|c| c.stage.clone()).collect();
                    match PointwiseChain::new(self.precision, &stages) {
                        Ok(next_chain) => {
                            chain = Some(next_chain);
                            end += 3;
                        }
                        Err(_) => {
                            candidates.pop();
                            *report
                                .retained
                                .entry(FusionRetainReason::ParameterOrStageLimit)
                                .or_default() += 1;
                            break;
                        }
                    }
                }
                let Some(chain) = chain else {
                    *report
                        .retained
                        .entry(FusionRetainReason::SingleStage)
                        .or_default() += 1;
                    passes.extend_from_slice(&self.frame.passes[at..at + 3]);
                    clocks.extend_from_slice(&self.pass_clocks[at..at + 3]);
                    at += 3;
                    continue;
                };
                // A later unproved/nonlinear consumer can amplify even a local
                // permitted rounding difference. The initial prototype only
                // replaces a proved tail, never an upstream partial group.
                if end != draw.pass_end {
                    *report
                        .retained
                        .entry(FusionRetainReason::DownstreamBoundary)
                        .or_default() += 1;
                    passes.extend_from_slice(&self.frame.passes[at..end]);
                    clocks.extend_from_slice(&self.pass_clocks[at..end]);
                    at = end;
                    continue;
                }
                let Some((program, portable)) = self.fused_program(&chain)? else {
                    *report
                        .retained
                        .entry(FusionRetainReason::CacheLimit)
                        .or_default() += 1;
                    passes.extend_from_slice(&self.frame.passes[at..end]);
                    clocks.extend_from_slice(&self.pass_clocks[at..end]);
                    at = end;
                    continue;
                };
                // Keep a separate canonical value while writing slot 0. The
                // lifetime allocator must place these in different attachments.
                let mut copy = self.frame.passes[at];
                // The existing raster utility with unit modulation copies a
                // canonical pixel without unpremultiplying tiny alpha values.
                copy.program = 0;
                copy.output = 4;
                copy.uniform.output_mode = [0., 0., 1., copy.uniform.output_mode[3]];
                copy.uniform.params = [[0.; 4]; 32];
                copy.uniform.params[0] = [1.; 4];
                let mut fused = copy;
                fused.program = program;
                fused.input = 4;
                fused.source = 4;
                fused.output = 0;
                fused.uniform.params = *chain.uniforms();
                passes.extend([copy, fused]);
                clocks.extend([None, None]);
                reserve_scratch(&mut self.frame.scratch_sizes, 4, copy.width, copy.height);
                self.frame.slots |= 1 << 4;
                if report.groups.len() < MAX_REPORTED_GROUPS {
                    report.groups.push(FusionGroupReport {
                        object: draw.layer,
                        effects: candidates.iter().map(|c| c.namespace.effect).collect(),
                        program,
                        structure_key: chain.structure_key(),
                        original_passes: end - at,
                        fused_passes: 2,
                        portable,
                    });
                } else {
                    report.omitted_groups += 1;
                }
                at = end;
            }
            self.frame.draws[draw_index].pass_end = passes.len();
        }
        report.final_passes = passes.len();
        report.final_texture_traffic_bytes = traffic(&passes, self.precision);
        report.portable_compiles = self.fusion.compiles;
        report.cache_entries = self.fusion.compiled.len();
        self.frame.passes = passes;
        self.pass_clocks = clocks;
        self.fusion.report = report;
        Ok(())
    }
}
