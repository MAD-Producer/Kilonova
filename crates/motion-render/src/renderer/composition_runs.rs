//! Consecutive direct inputs share an attachment without crossing any producer.
//! Effects, restored snapshots, transfers and adjustments stay on the serial path.
use super::*;
use crate::{effect_plan::EffectFramePlan, PlaneBatch};

pub(super) struct CompositionRun<'a> {
    pub scene: &'a Scene,
    pub frame: &'a EffectFramePlan,
    pub batches: &'a [PlaneBatch],
    pub view: &'a wgpu::TextureView,
    pub viewport: [f32; 4],
    pub timestamps: Option<wgpu::RenderPassTimestampWrites<'a>>,
    pub compositing: bool,
}

impl Renderer {
    pub(super) fn composition_run_end(
        &self,
        scene: &Scene,
        frame: &EffectFramePlan,
        start: usize,
    ) -> usize {
        #[cfg(test)]
        if self.legacy_composition_passes {
            return start;
        }
        (start..frame.batches.len())
            .find(|&index| {
                let layer = frame.batches[index].layer;
                let draw = &frame.draws[layer];
                let composite = frame.compositing[layer];
                !composite.visible
                    || scene.layers[layer].adjustment
                    || composite.blend != 0
                    || composite.space != 0
                    || draw.pass_start != draw.pass_end
                    || draw.words[27] >= 0.
                    || self
                        .images
                        .contains_key(&TextureKey::EffectInput(draw.layer))
            })
            .unwrap_or(frame.batches.len())
    }

    pub(super) fn encode_composition_run(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        run: CompositionRun<'_>,
    ) -> Result<usize> {
        let gpu = if run.compositing {
            Some(self.compositing_gpu.as_ref().ok_or_else(|| {
                RenderError::Invalid("composition run: compositing resources are missing".into())
            })?)
        } else {
            None
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            multiview_mask: None,
            label: Some("effect composition run"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                depth_slice: None,
                view: run.view,
                resolve_target: None,
                // The existing composition clear is retained. Only the
                // unnecessary intermediate store/load boundaries disappear.
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: run.timestamps,
            occlusion_query_set: None,
        });
        let [x, y, width, height] = run.viewport;
        pass.set_viewport(x, y, width, height, 0., 1.);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        for batch in run.batches {
            let index = batch.layer;
            let layer = &run.scene.layers[index];
            let draw = &run.frame.draws[index];
            let masked = !layer.masks.is_empty();
            pass.set_pipeline(if let Some(gpu) = gpu {
                if draw.words[23] > 0.5 {
                    &gpu.additive
                } else {
                    &gpu.normal
                }
            } else if masked {
                &self.masked_pipeline
            } else if draw.words[23] > 0.5 {
                &self.additive_pipeline
            } else {
                &self.pipeline
            });
            pass.set_bind_group(
                0,
                &self.uniform_group,
                &[(index * self.uniform_stride) as u32],
            );
            let key = texture_key(run.scene, layer);
            let image = self.images.get(&key).ok_or_else(|| {
                RenderError::Invalid(format!(
                    "composition run layer={} frame={} source={key:?}: draw image is not uploaded",
                    layer.id, run.scene.frame
                ))
            })?;
            pass.set_bind_group(1, &image.bind_group, &[]);
            let mask = if masked {
                Some(
                    self.images
                        .get(&TextureKey::Mask(layer.id))
                        .ok_or_else(|| {
                            RenderError::Invalid(format!(
                                "composition run layer={} frame={}: mask image is not prepared",
                                layer.id, run.scene.frame
                            ))
                        })?,
                )
            } else {
                None
            };
            if let Some(gpu) = gpu {
                let composite = run.frame.compositing[index];
                pass.set_bind_group(
                    2,
                    mask.map_or(&gpu.zero.composite, |mask| &mask.bind_group),
                    &[],
                );
                let matte = if composite.matte >= 0 {
                    let source = run.frame.draws[composite.matte as usize].layer;
                    &gpu.mattes.get(&(source, composite.mode >= 3)).ok_or_else(|| RenderError::Invalid(format!(
                        "composition run layer={} frame={} matte_source={source}: matte target is not prepared",
                        layer.id, run.scene.frame)))?.composite
                } else {
                    &gpu.zero.composite
                };
                pass.set_bind_group(3, matte, &[]);
            } else if let Some(mask) = mask {
                pass.set_bind_group(2, &mask.bind_group, &[]);
            }
            pass.draw(batch.vertices.clone(), 0..1);
        }
        Ok(run.batches.len())
    }
}
