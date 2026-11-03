//! Projected matte extraction shares the effects-stage snapshots used by image inputs.
use super::*;
#[cfg(test)]
mod tests;

// Producer plans are bounded by MAX_LAYERS. Retain only the current traversal
// and demand; no history or frame keys accumulate in this scratch storage.
#[derive(Default)]
pub(super) struct MattePlanning {
    pub needed: Vec<(u64, bool)>,
    order: Vec<(usize, bool)>,
    active: std::collections::HashSet<usize>,
    done: std::collections::HashSet<(usize, bool)>,
}
impl MattePlanning {
    pub fn collect_targets(&mut self, frame: &crate::effect_plan::EffectFramePlan) {
        self.needed.clear();
        self.needed.extend(
            frame
                .compositing
                .iter()
                .filter(|c| c.matte >= 0)
                .map(|c| (frame.draws[c.matte as usize].layer, c.mode >= 3)),
        );
    }

    pub fn prepare_coverage(
        &mut self,
        frame: &crate::effect_plan::EffectFramePlan,
    ) -> Result<&[(usize, bool)]> {
        fn visit(
            frame: &crate::effect_plan::EffectFramePlan,
            index: usize,
            luma: bool,
            active: &mut std::collections::HashSet<usize>,
            done: &mut std::collections::HashSet<(usize, bool)>,
            out: &mut Vec<(usize, bool)>,
        ) -> Result<()> {
            if done.contains(&(index, luma)) {
                return Ok(());
            }
            if !active.insert(index) {
                return Err(RenderError::Invalid("track matte dependency cycle".into()));
            }
            let c = frame.compositing[index];
            if c.matte >= 0 {
                visit(frame, c.matte as usize, c.mode >= 3, active, done, out)?;
            }
            active.remove(&index);
            done.insert((index, luma));
            out.push((index, luma));
            Ok(())
        }
        self.active.clear();
        self.done.clear();
        self.order.clear();
        for c in &frame.compositing {
            if c.matte >= 0 {
                visit(
                    frame,
                    c.matte as usize,
                    c.mode >= 3,
                    &mut self.active,
                    &mut self.done,
                    &mut self.order,
                )?;
            }
        }
        Ok(&self.order)
    }
}

impl Renderer {
    pub(super) fn ensure_compositing_gpu(&mut self, frame: f64, phase: &str) -> Result<()> {
        if self.compositing_gpu.is_none() {
            let gpu = crate::compositing_gpu::CompositingGpu::new(
                &self.device,
                &self.queue,
                &self.uniform_layout,
                &self.image_layout,
                &self.sampler,
                self.target_format,
                &self.allocation_budget,
                self.allocation_uploads.clone(),
            )
            .map_err(|error| RenderError::Context {
                message: format!("compositing frame={frame} phase={phase}: {error}"),
                cause: Box::new(error),
            })?;
            self.compositing_gpu = Some(gpu);
            self.compositing_pipeline_set_builds += 1;
        }
        Ok(())
    }

    pub(super) fn encode_track_mattes(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        size: [u32; 2],
    ) -> Result<usize> {
        let Some(gpu) = &self.compositing_gpu else {
            return Ok(0);
        };
        let frame = &self.effect_gpu.builder.frame;
        let mut draws = 0;
        for &(index, luma) in &self.matte_planning.order {
            let d = &frame.draws[index];
            let c = frame.compositing[index];
            let target = &gpu.mattes[&(d.layer, luma)];
            let parent = if c.matte >= 0 {
                &gpu.mattes[&(frame.draws[c.matte as usize].layer, c.mode >= 3)].composite
            } else {
                &gpu.zero.composite
            };
            let flags = u32::from(c.mode != 0) * 2 + u32::from(c.mode == 2 || c.mode == 4) * 4;
            let u = DrawUniform {
                mvp: std::array::from_fn(|col| std::array::from_fn(|row| d.words[col * 4 + row])),
                color: [1.; 4],
                extent_opacity: [0., 0., d.words[22], flags as f32],
                uv_scale: [1., 1., size[0] as f32, size[1] as f32],
                screen_uv: crate::plane_coordinates::screen_map(
                    frame.uv_maps[index],
                    [0., 0., size[0] as f32, size[1] as f32],
                    d.layer,
                )?,
            };
            self.allocation_uploads
                .track(Some(gpu.extract_buffer.allocation()))?;
            self.queue.write_buffer(
                &gpu.extract_buffer,
                (index * gpu.extract_stride) as u64,
                bytemuck::bytes_of(&u),
            );
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                multiview_mask: None,
                label: Some("projected track matte"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    depth_slice: None,
                    view: &target.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(if luma { &gpu.luma } else { &gpu.alpha });
            pass.set_bind_group(
                0,
                &gpu.extract_group,
                &[(index * gpu.extract_stride) as u32],
            );
            pass.set_bind_group(
                1,
                &self.images[&TextureKey::EffectInput(d.layer)].bind_group,
                &[],
            );
            pass.set_bind_group(2, &gpu.zero.composite, &[]);
            pass.set_bind_group(3, parent, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            for batch in frame.batches.iter().filter(|b| b.layer == index) {
                pass.draw(batch.vertices.clone(), 0..1);
                draws += 1;
            }
        }
        Ok(draws)
    }
}
