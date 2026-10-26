//! Frame-local semantic ownership is established before raster preparation.
//! Physical proposals can admit an operation or alias it to its input; they
//! cannot create producers, change sampled inputs, or invent effect origins.
use super::*;
use motion_effects::execution_plan::{CoordinateDomain, ImageValueType, PixelRect};
use motion_effects::image_contract::{ImageContract, ImageMeaning};
use std::ops::Range;

pub(in crate::effect_plan) struct SemanticFrame {
    pub composition: String,
    pub layers: Vec<SemanticLayer>,
    pub nodes: Vec<SemanticNode>,
    precision: ChannelPrecision,
}

pub(in crate::effect_plan) struct SemanticLayer {
    pub source: usize,
    pub source_size: [f32; 2],
    pub matte: Option<usize>,
    pub mask: Option<usize>,
    pub effects: Vec<usize>,
    pub materialize: Option<usize>,
    pub output: usize,
}

pub(in crate::effect_plan) struct SemanticNode {
    pub draw: usize,
    pub layer: u64,
    pub instance: Option<u64>,
    pub effect: Option<usize>,
    pub input: Option<usize>,
    pub image: PreparedImage,
    contract: ImageContract,
    coordinates: CoordinateDomain,
    recipe: RegionRecipe,
    pub admission: Option<Admission>,
}

enum RegionRecipe {
    Source([f32; 4]),
    Image(Option<Arc<PreparedEffect>>),
    Generator([f32; 4]),
    Identity,
}

pub(in crate::effect_plan) struct Admission {
    pub passes: Range<usize>,
    pub region: [f32; 4],
    pub size: [u32; 2],
    pub alias: bool,
}

impl PreparationPlan {
    pub(in crate::effect_plan) fn semantic_frame(
        &self,
        scene: &Scene,
        resolved: &[Option<Resolved>],
        precision: ChannelPrecision,
        scales: &[f32],
        targets: &[u64],
    ) -> Result<SemanticFrame, RenderError> {
        let composition = [0., 0., scene.width as f32, scene.height as f32];
        let required: BTreeSet<_> = scene
            .effects
            .iter()
            .filter(|e| e.enabled && scene.layers.iter().any(|l| l.id == e.layer))
            .filter_map(|e| match e.image_input {
                Some(EffectImageInput::Layer {
                    layer,
                    stage: EffectImageStage::Effects,
                }) => Some(layer),
                _ => None,
            })
            .chain(
                scene
                    .layers
                    .iter()
                    .filter_map(|l| l.track_matte.map(|m| m.source)),
            )
            .chain(targets.iter().copied())
            .collect();
        let mut graph = SemanticFrame {
            composition: scene.composition_id.clone(),
            layers: Vec::new(),
            nodes: Vec::new(),
            precision,
        };
        for (draw, layer) in scene.layers.iter().enumerate() {
            let region = if layer.adjustment {
                composition
            } else {
                layer.source_rect
            };
            graph.nodes.push(SemanticNode {
                draw,
                layer: layer.id,
                instance: None,
                effect: None,
                input: None,
                image: PreparedImage::None,
                contract: ImageContract::new(
                    ImageMeaning::Color {
                        space: WorkingSpace::Linear,
                        alpha: AlphaMode::Premultiplied,
                    },
                    precision,
                ),
                coordinates: if layer.adjustment || layer.screen_overlay {
                    CoordinateDomain::Composition
                } else {
                    CoordinateDomain::Layer { layer: layer.id }
                },
                recipe: RegionRecipe::Source(region),
                admission: Some(Admission {
                    passes: 0..0,
                    region,
                    size: texels(region, scales[draw]),
                    alias: false,
                }),
            });
            graph.layers.push(SemanticLayer {
                source: draw,
                matte: layer
                    .track_matte
                    .map(|matte| {
                        scene
                            .layers
                            .iter()
                            .position(|l| l.id == matte.source)
                            .ok_or_else(|| {
                                RenderError::Invalid("semantic matte source is absent".into())
                            })
                    })
                    .transpose()?,
                source_size: if layer.adjustment {
                    [composition[2], composition[3]]
                } else {
                    layer.source_size
                },
                mask: None,
                effects: Vec::new(),
                materialize: None,
                output: draw,
            });
        }
        for (draw, layer) in scene.layers.iter().enumerate() {
            let authored = &self.effects_by_layer[draw];
            let image_effect = authored.iter().any(|&i| {
                resolved[i]
                    .as_ref()
                    .is_some_and(|r| r.definition.renderer == motion_effects::RendererKind::Image)
            });
            let generator_first = authored.first().is_some_and(|&i| {
                resolved[i]
                    .as_ref()
                    .is_some_and(|r| r.definition.renderer != motion_effects::RendererKind::Image)
            });
            if !layer.masks.is_empty() && image_effect && !generator_first {
                let id = graph.push(draw, None, RegionRecipe::Identity, PreparedImage::None)?;
                graph.layers[draw].mask = Some(id);
            }
            for &index in authored {
                let prepared = self.inputs[index].as_ref().and_then(|p| p.as_ref().ok());
                let image = prepared
                    .filter(|p| p.consumes_image)
                    .map_or(PreparedImage::None, |p| p.image);
                let recipe = if resolved[index]
                    .as_ref()
                    .is_some_and(|r| r.definition.renderer != motion_effects::RendererKind::Image)
                {
                    RegionRecipe::Generator(composition)
                } else {
                    RegionRecipe::Image(prepared.cloned())
                };
                let id = graph.push(draw, Some(index), recipe, image)?;
                graph.nodes[id].instance = Some(scene.effects[index].instance);
                graph.layers[draw].effects.push(id);
            }
            if required.contains(&layer.id) {
                let id = graph.push(draw, None, RegionRecipe::Identity, PreparedImage::None)?;
                graph.layers[draw].materialize = Some(id);
            }
        }
        Ok(graph)
    }
}

fn texels(region: [f32; 4], scale: f32) -> [u32; 2] {
    [
        (region[2] * scale).ceil().max(1.) as u32,
        (region[3] * scale).ceil().max(1.) as u32,
    ]
}

impl SemanticFrame {
    fn push(
        &mut self,
        draw: usize,
        effect: Option<usize>,
        recipe: RegionRecipe,
        image: PreparedImage,
    ) -> Result<usize, RenderError> {
        if self.nodes.len() >= MAX_EXECUTION_NODES {
            return Err("phase=effect_ir_preparation: operation budget exceeded".into());
        }
        let source = &self.nodes[self.layers[draw].source];
        let id = self.nodes.len();
        self.nodes.push(SemanticNode {
            draw,
            layer: source.layer,
            instance: None,
            effect,
            input: Some(self.layers[draw].output),
            image,
            contract: source.contract,
            coordinates: source.coordinates,
            recipe,
            admission: None,
        });
        self.layers[draw].output = id;
        Ok(id)
    }

    pub fn resolved(&self, mut node: usize) -> Result<usize, RenderError> {
        // Every alias points to an earlier value in the same authored chain.
        for _ in 0..self.nodes.len() {
            let value = self.nodes.get(node).ok_or("semantic value is absent")?;
            let admission = value
                .admission
                .as_ref()
                .ok_or("semantic value is not admitted")?;
            if !admission.alias {
                return Ok(node);
            }
            let input = value.input.ok_or("semantic alias has no input")?;
            if input >= node {
                return Err("semantic alias is not an upstream value".into());
            }
            node = input;
        }
        Err("semantic alias exceeds operation bounds".into())
    }

    pub fn region(&self, node: usize) -> Result<[f32; 4], RenderError> {
        Ok(self.nodes[self.resolved(node)?]
            .admission
            .as_ref()
            .ok_or("semantic admission is absent")?
            .region)
    }

    pub fn output_region(&self, node: usize) -> Result<[f32; 4], RenderError> {
        let value = &self.nodes[node];
        match &value.recipe {
            RegionRecipe::Source(region) | RegionRecipe::Generator(region) => Ok(*region),
            RegionRecipe::Identity => self.region(value.input.ok_or("semantic input is absent")?),
            RegionRecipe::Image(prepared) => prepared
                .as_ref()
                .ok_or("prepared semantic rule is absent")?
                .output_region(self.region(value.input.ok_or("semantic input is absent")?)?),
        }
    }

    pub fn admit(
        &mut self,
        node: usize,
        passes: Range<usize>,
        region: [f32; 4],
        size: [u32; 2],
    ) -> Result<(), RenderError> {
        if self.nodes[node].admission.is_some() {
            return Err("semantic operation was admitted twice".into());
        }
        let alias = passes.is_empty();
        if alias {
            let input = self.nodes[node]
                .input
                .ok_or("semantic alias has no input")?;
            let admitted = self.nodes[self.resolved(input)?]
                .admission
                .as_ref()
                .ok_or("semantic input is absent")?;
            self.nodes[node].admission = Some(Admission {
                passes,
                region: admitted.region,
                size: admitted.size,
                alias,
            });
        } else {
            let expected = self.output_region(node)?;
            if expected != region {
                return Err("native proposal changed the semantic output region".into());
            }
            self.nodes[node].admission = Some(Admission {
                passes,
                region,
                size,
                alias,
            });
        }
        Ok(())
    }

    pub fn refine_source_grid(&mut self, draw: usize, scale: f32) -> Result<(), RenderError> {
        let value = &mut self.nodes[self.layers[draw].source];
        let admission = value
            .admission
            .as_mut()
            .ok_or("semantic source is absent")?;
        admission.size = texels(admission.region, scale);
        Ok(())
    }

    pub fn value_type(&self, node: usize) -> Result<ImageValueType, RenderError> {
        let value = &self.nodes[self.resolved(node)?];
        let admission = value
            .admission
            .as_ref()
            .ok_or("semantic admission is absent")?;
        Ok(ImageValueType {
            image: value.contract,
            rect: PixelRect {
                origin: [0; 2],
                size: admission.size,
            },
            coordinates: value.coordinates,
            depth: None,
        })
    }

    pub fn validate(
        &self,
        frame: &EffectFramePlan,
        clocks: &[Option<usize>],
    ) -> Result<(), RenderError> {
        if frame.precision != self.precision || frame.draws.len() != self.layers.len() {
            return Err("semantic frame contract does not match the native proposal".into());
        }
        let mut covered = 0;
        for (draw, layer) in self.layers.iter().enumerate() {
            if frame.draws[draw].layer != self.nodes[layer.source].layer {
                return Err("native draw changed the semantic layer origin".into());
            }
            if frame.compositing.get(draw).map(|c| c.matte)
                != Some(layer.matte.map_or(-1, |index| index as i32))
            {
                return Err("native matte changed the semantic layer dependency".into());
            }
            if frame.draws[draw].pass_start != covered {
                return Err("semantic draw range is not contiguous".into());
            }
            for &id in layer
                .mask
                .iter()
                .chain(layer.effects.iter())
                .chain(layer.materialize.iter())
            {
                let node = &self.nodes[id];
                let admission = node
                    .admission
                    .as_ref()
                    .ok_or("semantic operation is still pending")?;
                if admission.passes.start != covered
                    || admission.passes.end > frame.draws[draw].pass_end
                {
                    return Err("semantic operation does not own the native range".into());
                }
                if clocks
                    .get(admission.passes.clone())
                    .is_none_or(|c| c.iter().any(|&c| c != node.effect))
                {
                    return Err("native clocks disagree with semantic effect ownership".into());
                }
                if let RegionRecipe::Image(Some(prepared)) = &node.recipe {
                    let (params, token) = prepared.native_uniforms();
                    for pass in &frame.passes[admission.passes.clone()] {
                        if pass.program > 2
                            && (pass.resource_input != token
                                || pass.uniform.params[30..32] != params[30..32])
                        {
                            return Err(
                                "native image selector disagrees with the sampled semantic input"
                                    .into(),
                            );
                        }
                    }
                }
                if let Some(last) = frame
                    .passes
                    .get(admission.passes.clone())
                    .and_then(|p| p.last())
                {
                    if last.output != 0
                        || (!matches!(last.program, 0 | 2)
                            && (last.uniform.output_mode[0] != 0.
                                || last.uniform.output_mode[1] != 0.))
                    {
                        return Err("effect boundary must be canonical linear premultiplied".into());
                    }
                    if last.uniform.region != admission.region
                        || [last.width, last.height] != admission.size
                    {
                        return Err("native output disagrees with admitted semantic value".into());
                    }
                }
                covered = admission.passes.end;
            }
            if frame.draws[draw].pass_end != covered {
                return Err("native pass has no semantic owner".into());
            }
        }
        if covered != frame.passes.len() {
            return Err("native proposal has unowned trailing passes".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
