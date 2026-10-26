//! Backend annotations for producers already owned by the semantic frame.
//! This is the only boundary that reads native drafts. No pass/uniform table,
//! mutable frame, scene or compiler is retained by the logical lowerer.
use super::*;

pub(super) struct NativeAnnotations {
    pub nodes: Vec<ExecutionNode>,
    pub bindings: Vec<EffectOperationBinding>,
    pub semantic_origins: Vec<usize>,
    pub mapping: Vec<Option<usize>>,
    pub effects: Vec<Option<usize>>,
    pub draw_ranges: Vec<Range<usize>>,
    pub pass_count: usize,
}

impl NativeAnnotations {
    pub fn admit(
        semantic: &preparation::SemanticFrame,
        frame: &EffectFramePlan,
        clocks: &[Option<usize>],
        programs: &[EffectProgram],
        scene: &Scene,
        codes: &mut CodeIdentities,
    ) -> Result<Self, RenderError> {
        if frame.draws.len() > motion_model::MAX_LAYERS
            || scene.effects.len()
                > motion_model::MAX_LAYERS * motion_effects::MAX_EFFECTS_PER_LAYER
            || frame.draws.len() != scene.layers.len()
            || clocks.len() != frame.passes.len()
        {
            return Err("effect IR draft origins do not match the scene".into());
        }
        if scene.composition_id.is_empty()
            || scene.composition_id.len() > 128
            || scene.composition_id.trim().is_empty()
            || scene.composition_id.chars().any(char::is_control)
        {
            return Err(RenderError::ExecutionPlan(
                ExecutionPlanError::InvalidOrigin {
                    node: ExecutionNodeId(0),
                },
            ));
        }
        semantic.validate(frame, clocks)?;
        if semantic.composition != scene.composition_id
            || semantic.layers.iter().enumerate().any(|(draw, layer)| {
                layer.matte.map(|matte| matte.authored) != scene.layers[draw].track_matte
            })
            || semantic.nodes.iter().any(|node| {
                scene.layers[node.draw].id != node.layer
                    || node.effect.is_some_and(|i| {
                        scene.effects.get(i).is_none_or(|e| {
                            e.layer != node.layer || Some(e.instance) != node.instance
                        })
                    })
            })
        {
            return Err("sampled scene changed the owned semantic operation origins".into());
        }
        let mut result = Self {
            nodes: Vec::new(),
            bindings: Vec::new(),
            semantic_origins: Vec::new(),
            mapping: vec![None; semantic.nodes.len()],
            effects: vec![None; scene.effects.len()],
            draw_ranges: frame
                .draws
                .iter()
                .map(|d| d.pass_start..d.pass_end)
                .collect(),
            pass_count: frame.passes.len(),
        };
        for (node, value) in semantic.nodes.iter().enumerate() {
            let admission = value
                .admission
                .as_ref()
                .ok_or("semantic operation is pending")?;
            if admission.alias {
                continue;
            }
            let id = result.bindings.len();
            let passes = if value.input.is_none() {
                result.draw_ranges[value.draw].start..result.draw_ranges[value.draw].start
            } else {
                admission.passes.clone()
            };
            let mut binding = EffectOperationBinding {
                node: ExecutionNodeId(id as u32),
                draw: value.draw,
                scene_effect: value.effect,
                original_passes: passes.clone(),
                output_region: admission.region,
                resource_tokens: vec![],
                lookup_tables: vec![],
                instance_data: vec![],
                imported_image: None,
            };
            let mut code = Sha256::new();
            code.update(b"motion-effect-operation-v1");
            let mut shader_bytes = 0_u64;
            let mut loops = 0_u64;
            let mut samples = Some(0_u64);
            for pass in &frame.passes[passes] {
                for (token, tokens, absent) in [
                    (pass.resource_input, &mut binding.resource_tokens, 0),
                    (pass.lut, &mut binding.lookup_tables, -1),
                ] {
                    if token != absent && !tokens.contains(&token) {
                        tokens.push(token);
                    }
                }
                if pass.instance_data != 0 && !binding.instance_data.contains(&pass.instance_data) {
                    binding.instance_data.push(pass.instance_data);
                }
                let program = programs.get(pass.program as usize).ok_or_else(|| {
                    RenderError::Invalid(format!(
                        "layer={} phase=effect_ir: absent program={}",
                        value.layer, pass.program
                    ))
                })?;
                let identity = codes.get(pass.program, program);
                code.update(identity.digest);
                shader_bytes = shader_bytes
                    .checked_add(identity.text_bytes)
                    .ok_or("effect IR shader cost overflow")?;
                loops = loops
                    .checked_add(identity.loop_work)
                    .ok_or("effect IR loop cost overflow")?;
                samples = match (samples, identity.samples_per_fragment) {
                    (Some(a), Some(b)) => Some(
                        a.checked_add(b)
                            .ok_or("effect IR texture-read bound overflow")?,
                    ),
                    _ => None,
                };
            }
            let effect = value.effect.map(|i| &scene.effects[i]);
            result.nodes.push(ExecutionNode {
                id: binding.node,
                origin: OperationOrigin {
                    composition: semantic.composition.clone(),
                    layer: value.layer,
                    effect: value.instance,
                },
                // Dependency edges are supplied by the semantic lowerer. Only
                // checked native resource imports may be attached at this gate.
                inputs: vec![],
                output: ImageValueId(id as u32),
                output_type: semantic.value_type(node)?,
                semantics: OperationSemantics {
                    domain: if binding.original_passes.is_empty() {
                        ExecutionDomain::Source
                    } else if effect.is_some_and(|e| e.scene.is_some()) {
                        ExecutionDomain::External
                    } else if effect.is_some() {
                        ExecutionDomain::Image
                    } else {
                        ExecutionDomain::Materialization
                    },
                    time: TimeDependency::CurrentSample,
                    halo: None,
                    stateful: false,
                    side_effects: false,
                },
                cost: OperationCost {
                    shader_bytes,
                    static_loop_work: loops,
                    samples_per_fragment: samples,
                    passes: binding.original_passes.len() as u32,
                },
                code_identity: code.finalize().into(),
                uniform_cells: motion_effects::MAX_PARAMS as u16,
            });
            result.mapping[node] = Some(id);
            result.semantic_origins.push(node);
            if let Some(index) = value.effect {
                result.effects[index] = Some(id);
            }
            result.bindings.push(binding);
        }
        super::super::resource_imports::lower_package_images(
            &mut result.nodes,
            &mut result.bindings,
            frame,
            programs,
            scene,
        )?;
        super::super::frame_imports::lower_frame_images(
            &mut result.nodes,
            &mut result.bindings,
            frame,
            programs,
            scene,
        )?;
        Ok(result)
    }
}
