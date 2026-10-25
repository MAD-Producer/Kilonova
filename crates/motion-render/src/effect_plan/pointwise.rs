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
const MAX_RETIRED_CODE_BYTES: usize = 8 << 20;

#[cfg(test)]
mod cache_tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FusionRetainReason {
    UnprovedImplementation,
    RuntimeBoundary,
    SingleStage,
    ParameterOrStageLimit,
    CacheLimit,
    AlphaBoundary,
    QuantizedTransferBoundary,
    DownstreamBoundary,
    ExternalConsumerBoundary,
    CompositingBoundary,
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
    /// Successful portable-code reuse after program IDs were invalidated.
    pub portable_reuses: u64,
    pub cache_entries: usize,
    pub retired_cache_entries: usize,
    /// Retained WGSL/GLSL/reflection UTF-8 bytes, excluding container overhead.
    pub retired_code_bytes: usize,
    pub retired_code_byte_limit: usize,
}

enum Compilation {
    Ready {
        program: Option<u32>,
        // Successful private compilation proof; a public program cannot mutate it.
        shader: Arc<shader::CompiledShader>,
        metrics: PortableFusionMetrics,
    },
    Failed(Arc<RenderError>),
}

#[derive(Default)]
pub(super) struct FusionState {
    enabled: bool,
    compiled: BTreeMap<String, Compilation>,
    compiles: u64,
    reuses: u64,
    report: FusionFrameReport,
    #[cfg(all(debug_assertions, feature = "gpu-tests"))]
    pub(super) invalid_source_once: bool,
}
impl FusionState {
    pub(super) fn reset_programs(&mut self) {
        // IDs are registry-local; immutable successful code remains reusable.
        // Failed compilations are retried only after this explicit invalidation.
        let mut retained = 0;
        self.compiled.retain(|_, entry| match entry {
            Compilation::Ready {
                program, shader, ..
            } => {
                let bytes = code_bytes(shader);
                if bytes > MAX_RETIRED_CODE_BYTES - retained {
                    return false;
                }
                retained += bytes;
                *program = None;
                true
            }
            Compilation::Failed(_) => false,
        });
        self.report = FusionFrameReport::default();
    }

    fn cache_report(&self, report: &mut FusionFrameReport) {
        report.portable_compiles = self.compiles;
        report.portable_reuses = self.reuses;
        report.cache_entries = self.compiled.len();
        report.retired_code_byte_limit = MAX_RETIRED_CODE_BYTES;
        for entry in self.compiled.values() {
            if let Compilation::Ready {
                program: None,
                shader,
                ..
            } = entry
            {
                report.retired_cache_entries += 1;
                report.retired_code_bytes += code_bytes(shader);
            }
        }
    }
}

fn code_bytes(shader: &shader::CompiledShader) -> usize {
    shader.wgsl.len()
        + shader.glsl.vertex.len()
        + shader.glsl.fragment.len()
        + shader.glsl.blocks.keys().map(String::len).sum::<usize>()
        + shader.glsl.textures.keys().map(String::len).sum::<usize>()
}

struct Candidate {
    stage: PointwiseStage,
    namespace: StageNamespace,
    scene_effect: usize,
}

fn copy_mapping(
    mapping: &mut [std::ops::Range<usize>],
    original: std::ops::Range<usize>,
    start: usize,
) {
    for (offset, value) in mapping[original].iter_mut().enumerate() {
        *value = start + offset..start + offset + 1;
    }
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
            scene_effect: effect_index,
        }))
    }

    fn fused_program(
        &mut self,
        chain: &PointwiseChain,
    ) -> Result<Option<(u32, PortableFusionMetrics)>, RenderError> {
        let key = chain.structure_key();
        #[cfg(all(debug_assertions, feature = "gpu-tests"))]
        if self.fusion.invalid_source_once {
            // Acceptance faults must reach the actual parser even on a hit.
            self.fusion.compiled.remove(&key);
        }
        if let Some(cached) = self.fusion.compiled.get_mut(&key) {
            return match cached {
                Compilation::Ready {
                    program,
                    shader,
                    metrics,
                } => {
                    let id = *program.get_or_insert_with(|| {
                        let id = self.programs.len() as u32;
                        self.programs.push(EffectProgram {
                            key: key.clone(),
                            shader: shader.clone(),
                            package: None,
                            resources: vec![],
                        });
                        self.fusion.reuses = self.fusion.reuses.saturating_add(1);
                        id
                    });
                    // The successful private proof, rather than a caller's
                    // replacement of the public program, authorizes this hit.
                    let slot = &mut self.programs[id as usize];
                    if !Arc::ptr_eq(&slot.shader, shader) {
                        *slot = EffectProgram {
                            key: key.clone(),
                            shader: shader.clone(),
                            package: None,
                            resources: vec![],
                        };
                    }
                    if let Some(cause) = self.program_errors.get(&id) {
                        return Err(RenderError::Invalid(cause.clone()));
                    }
                    Ok(Some((id, metrics.clone())))
                }
                Compilation::Failed(error) => Err(RenderError::Shared(error.clone())),
            };
        }
        if self.fusion.compiled.len() >= MAX_COMPILED_GROUPS {
            // Retired groups must not permanently exhaust the live-group limit.
            let retired = self.fusion.compiled.iter().find_map(|(key, entry)| {
                matches!(entry, Compilation::Ready { program: None, .. }).then(|| key.clone())
            });
            if let Some(retired) = retired {
                self.fusion.compiled.remove(&retired);
            } else {
                return Ok(None);
            }
        }
        let observation = crate::pointwise_observation::CompilePhase::start(
            "pointwise_compile",
            &key,
            chain.instructions().first().map(|i| i.namespace.object),
            chain.instructions().iter().map(|i| i.namespace.effect),
            None,
            None,
        );
        self.fusion.compiles = self.fusion.compiles.saturating_add(1);
        match self.compile_pointwise(chain) {
            Ok(fused) => {
                let id = self.programs.len() as u32;
                observation.done(fused.metrics.compiled_wgsl_bytes);
                let shader = Arc::new(fused.compiled);
                self.programs.push(EffectProgram {
                    key: key.clone(),
                    shader: shader.clone(),
                    package: None,
                    resources: vec![],
                });
                self.fusion.compiled.insert(
                    key,
                    Compilation::Ready {
                        program: Some(id),
                        shader,
                        metrics: fused.metrics.clone(),
                    },
                );
                Ok(Some((id, fused.metrics)))
            }
            Err(error) => {
                observation.failed(&error.to_string());
                let error = Arc::new(RenderError::from(error));
                self.fusion
                    .compiled
                    .insert(key, Compilation::Failed(error.clone()));
                Err(RenderError::Shared(error))
            }
        }
    }

    fn compile_pointwise(
        &mut self,
        chain: &PointwiseChain,
    ) -> motion_effects::Result<motion_effects::pointwise::FusedShader> {
        #[cfg(all(debug_assertions, feature = "gpu-tests"))]
        if std::mem::take(&mut self.fusion.invalid_source_once) {
            // Explicit acceptance fault: execute the real portable parser with
            // corrupted generated source, rather than returning a mocked error.
            let source = chain.generated_source()? + "\nfn pointwise_diagnostic_invalid( {";
            motion_effects::shader::compile(&source, "main_fx")?;
        }
        chain.compile()
    }

    #[cfg(all(debug_assertions, feature = "gpu-tests"))]
    pub(crate) fn debug_inject_pointwise_portable_failure(&mut self) {
        self.fusion.invalid_source_once = true;
    }

    pub(super) fn fuse_pointwise(&mut self, scene: &Scene) -> Result<(), RenderError> {
        let mut report = FusionFrameReport {
            enabled: self.fusion.enabled,
            original_passes: self.frame.passes.len(),
            original_texture_traffic_bytes: traffic(&self.frame.passes, self.precision),
            ..Default::default()
        };
        if !self.fusion.enabled {
            report.final_passes = report.original_passes;
            report.final_texture_traffic_bytes = report.original_texture_traffic_bytes;
            self.fusion.cache_report(&mut report);
            self.fusion.report = report;
            return Ok(());
        }
        // Complete local def/use validation precedes optimization and final
        // physical placement. Semantic slot names alone cannot establish that
        // eliminating intermediate images preserves all of their consumers.
        let dependencies = scratch::logical_graph(&self.frame, None)?.grouping_dependencies();
        // A proved tail is local to one atomic draw. An arbitrary image-input
        // consumer or track matte can amplify its tolerated rounding error.
        // Source-stage readers bypass these effected outputs entirely.
        let externally_read = self
            .effect_execution
            .as_ref()
            .map(|e| e.externally_read().to_vec())
            .unwrap_or_else(|| vec![false; self.frame.draws.len()]);
        // Transfer blends may amplify source or backdrop errors. Adjustment
        // effects consume the accumulated backdrop without an explicit resource
        // edge. Neither consumer is covered by the initial pointwise proof.
        let compositing_boundary = self
            .frame
            .compositing
            .iter()
            .any(|c| c.visible && (c.blend != 0 || c.space != 0))
            || self
                .frame
                .draws
                .iter()
                .any(|d| d.words[31] == 2. && d.pass_start < d.pass_end);
        let mut passes = Vec::with_capacity(self.frame.passes.len());
        let mut clocks = Vec::with_capacity(self.pass_clocks.len());
        let mut mapping = vec![0..0; self.frame.passes.len()];
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
            if externally_read[draw_index] || compositing_boundary {
                let reason = if externally_read[draw_index] {
                    FusionRetainReason::ExternalConsumerBoundary
                } else {
                    FusionRetainReason::CompositingBoundary
                };
                *report.retained.entry(reason).or_default() += 1;
                copy_mapping(&mut mapping, draw.pass_start..draw.pass_end, passes.len());
                passes.extend_from_slice(&self.frame.passes[draw.pass_start..draw.pass_end]);
                clocks.extend_from_slice(&self.pass_clocks[draw.pass_start..draw.pass_end]);
                self.frame.draws[draw_index].pass_end = passes.len();
                continue;
            }
            // Native subnormal-alpha probes exceed the declared pixel bounds
            // in float modes too. Until the full alpha-lifting path is proved,
            // preserve every real store, including groups before that operation.
            let changes_alpha = self.pass_clocks[draw.pass_start..draw.pass_end]
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
            if changes_alpha {
                *report
                    .retained
                    .entry(FusionRetainReason::AlphaBoundary)
                    .or_default() += 1;
                copy_mapping(&mut mapping, draw.pass_start..draw.pass_end, passes.len());
                passes.extend_from_slice(&self.frame.passes[draw.pass_start..draw.pass_end]);
                clocks.extend_from_slice(&self.pass_clocks[draw.pass_start..draw.pass_end]);
                self.frame.draws[draw_index].pass_end = passes.len();
                continue;
            }
            // A one-code difference at a dark UNORM16 canonical store can be
            // amplified by a following inversion and transfer conversion. The
            // dense native counterexample exceeds the unchanged 8/65535 bound.
            // Preserve the whole draw's real stores for this operation order;
            // a smaller passing fixture does not prove a safe numeric domain.
            let mut saw_tint = false;
            let quantized_transfer = self.precision == ChannelPrecision::Unorm16
                && self.pass_clocks[draw.pass_start..draw.pass_end]
                    .iter()
                    .flatten()
                    .any(|&i| {
                        let Some(resolved) = self.resolved[i].as_ref() else {
                            return false;
                        };
                        let Eligibility::Proven(proof) = &resolved.pointwise else {
                            return false;
                        };
                        match proof.op() {
                            motion_effects::pointwise::PointwiseOp::Tint => {
                                saw_tint = true;
                                false
                            }
                            motion_effects::pointwise::PointwiseOp::Invert => saw_tint,
                        }
                    });
            if quantized_transfer {
                *report
                    .retained
                    .entry(FusionRetainReason::QuantizedTransferBoundary)
                    .or_default() += 1;
                copy_mapping(&mut mapping, draw.pass_start..draw.pass_end, passes.len());
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
                    copy_mapping(&mut mapping, at..at + 1, passes.len());
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
                    let members: Vec<_> = candidates
                        .iter()
                        .map(|c| c.scene_effect)
                        .chain(std::iter::once(next.scene_effect))
                        .collect();
                    if !dependencies.is_closed_group(at..end + 3)
                        || !self
                            .effect_execution
                            .as_ref()
                            .is_some_and(|e| e.allows_group(&members))
                    {
                        *report
                            .retained
                            .entry(FusionRetainReason::ExternalConsumerBoundary)
                            .or_default() += 1;
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
                    copy_mapping(&mut mapping, at..at + 3, passes.len());
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
                    copy_mapping(&mut mapping, at..end, passes.len());
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
                    copy_mapping(&mut mapping, at..end, passes.len());
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
                mapping[at..end].fill(passes.len()..passes.len() + 2);
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
        self.fusion.cache_report(&mut report);
        self.frame.passes = passes;
        self.pass_clocks = clocks;
        if let Some(execution) = &mut self.effect_execution {
            execution.lower_tasks(&mapping, &self.frame.draws)?;
        }
        self.fusion.report = report;
        Ok(())
    }
}
