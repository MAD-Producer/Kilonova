//! Current-frame private numeric resources. No retained history or package reload.
use super::*;
use std::sync::Arc;

impl GpuState {
    pub fn resource_bytes(&self) -> u64 {
        self.resource_bytes
            + self
                .instance_resources
                .iter()
                .flatten()
                .map(|resource| resource.values.texture_bytes())
                .sum::<u64>()
    }

    pub(super) fn pending_instance_bytes(&self, frame: &EffectFramePlan) -> u64 {
        frame
            .passes
            .iter()
            .enumerate()
            .filter_map(|(index, pass)| {
                let values = frame
                    .instance_data
                    .get(pass.instance_data.checked_sub(1)? as usize)?;
                let reused = self
                    .instance_resources
                    .get(index)
                    .and_then(Option::as_ref)
                    .is_some_and(|old| {
                        old.program == pass.program && Arc::ptr_eq(&old.values, values)
                    });
                (!reused).then(|| values.texture_bytes())
            })
            .sum()
    }

    pub(super) fn prepare_instance_data(
        &mut self,
        index: usize,
        builder: &PlanBuilder,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        composite: &wgpu::BindGroupLayout,
        asset_bytes: u64,
        source_budget: u64,
    ) -> Result<(), RenderError> {
        let pass = &builder.frame.passes[index];
        if pass.instance_data == 0 {
            return Ok(());
        }
        if pass.resource_input != 0 || pass.sprite {
            return Err(
                "instance shader data cannot override image-input or sprite resources".into(),
            );
        }
        let values = builder
            .frame
            .instance_data
            .get((pass.instance_data - 1) as usize)
            .ok_or("instance shader data index is invalid")?;
        let program = builder
            .programs
            .get(pass.program as usize)
            .ok_or("instance shader data program is invalid")?;
        use crate::effect_plan::{FrameImageImport, FrameImageSource};
        let import = builder
            .effect_execution()
            .and_then(|execution| {
                execution.frame_import(FrameImageSource::InstanceData(pass.instance_data))
            })
            .ok_or("phase=frame_import: instance shader data has no checked execution value")?;
        let expected = FrameImageImport::instance(
            pass.instance_data,
            values,
            program
                .imports
                .first()
                .ok_or("instance import resource zero is absent")?,
        )?;
        if import != &expected {
            return Err(
                "phase=frame_import: instance shader data differs from its checked execution value"
                    .into(),
            );
        }
        if let Some(old) = self.instance_resources[index]
            .as_ref()
            .filter(|old| old.program == pass.program && Arc::ptr_eq(&old.values, values))
        {
            import.validate_native(&old.texture.texture)?;
            return Ok(());
        }
        let package = program
            .package
            .as_ref()
            .ok_or("instance shader data package is missing")?;
        let declaration = package
            .manifest
            .effects
            .iter()
            .find(|effect| {
                effect.passes.iter().enumerate().any(|(index, _)| {
                    program.key == format!("{}:{}:{index}", package.hash, effect.id)
                        && package
                            .shaders
                            .get(&(effect.id.clone(), index))
                            .is_some_and(|shader| shader.wgsl == program.shader.wgsl)
                })
            })
            .and_then(|effect| effect.shader_data.as_ref())
            .ok_or("instance shader data declaration is missing")?;
        values.validate_layout(declaration)?;
        if declaration
            .overflow
            .as_ref()
            .is_none_or(|texture| program.resources.first() != Some(&texture.path))
        {
            return Err("instance shader data requires declared resource zero".into());
        }
        let [width, height] = import.value_type().rect.size;
        if height == 0 {
            return Err("instance shader data texture is empty".into());
        }
        if width > device.limits().max_texture_dimension_2d
            || height > device.limits().max_texture_dimension_2d
        {
            return Err(RenderError::Resource {
                rejection: crate::ResourceRejection::TextureDimensions {
                    width,
                    height,
                    max_dimension: device.limits().max_texture_dimension_2d,
                },
                message: "instance shader data exceeds device dimensions".into(),
            });
        }
        // Admission includes both old and candidate allocations until replacement.
        let requested = asset_bytes
            .saturating_add(self.resource_bytes())
            .saturating_add(values.texture_bytes());
        if requested > source_budget {
            return Err(RenderError::budget(
                crate::ResourceBudgetScope::SourceTextures,
                requested,
                source_budget,
                "instance shader data exceeds source texture budget",
            ));
        }
        let pixels = values.texture_pixels()?;
        let texture = admitted_texture(
            device,
            composite,
            &self.sampler,
            width,
            height,
            crate::image_formats::native_format(import.storage()),
            &self.allocation_budget,
            crate::TextureCategory::PluginResources,
        )
        .map_err(|error| {
            let message = format!(
                "instance shader data program={} pass={index} phase=allocate: {error}",
                pass.program
            );
            error.with_diagnostic(message)
        })?;
        let mut entries = Vec::with_capacity(8);
        for slot in 0..4 {
            let view = if slot == 0 {
                &texture.view
            } else if let Some(path) = program.resources.get(slot) {
                &self
                    .resource_textures
                    .get(&(package.hash.clone(), path.clone()))
                    .ok_or("instance shader data companion resource is missing")?
                    .view
            } else {
                &self.white.view
            };
            entries.push(wgpu::BindGroupEntry {
                binding: slot as u32 * 2,
                resource: wgpu::BindingResource::TextureView(view),
            });
            entries.push(wgpu::BindGroupEntry {
                binding: slot as u32 * 2 + 1,
                resource: wgpu::BindingResource::Sampler(&self.sampler),
            });
        }
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("instance shader data"),
            layout: &self.resource_layout,
            entries: &entries,
        });
        let validation = scope.pop();
        let polled = device.poll(wgpu::PollType::wait_indefinitely());
        if let Some(error) = pollster::block_on(validation) {
            let mut message = format!(
                "instance shader data program={} pass={index} phase=bind: {error}",
                pass.program
            );
            if let Err(error) = polled {
                message.push_str(&format!("; GPU wait also failed: {error}"));
            }
            return Err(message.into());
        }
        polled.map_err(|error| {
            RenderError::Invalid(format!(
                "instance shader data program={} pass={index} phase=wait: {error}",
                pass.program
            ))
        })?;
        self.allocation_uploads
            .track(texture.texture.allocation())?;
        upload(queue, &texture.texture, width, height, &pixels);
        self.parameter_resource_upload_bytes = self
            .parameter_resource_upload_bytes
            .saturating_add(values.texture_bytes());
        self.instance_resources[index] = Some(InstanceResource {
            program: pass.program,
            values: values.clone(),
            group,
            texture,
        });
        Ok(())
    }
}
