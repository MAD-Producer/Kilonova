//! One native compilation operation, usable by the renderer owner or an owned
//! preparation task. Every error scope stays on the calling thread.
use crate::{effect_plan::EffectProgram, RenderError};
use std::{error::Error, fmt};

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub struct NativeEffectPipelineError {
    program: String,
    native: wgpu::Error,
    related: Vec<wgpu::Error>,
    poll: Option<wgpu::PollError>,
}
impl NativeEffectPipelineError {
    pub fn program(&self) -> &str {
        &self.program
    }
    pub fn native(&self) -> &wgpu::Error {
        &self.native
    }
    pub fn related(&self) -> &[wgpu::Error] {
        &self.related
    }
    pub fn poll_error(&self) -> Option<&wgpu::PollError> {
        self.poll.as_ref()
    }
}
impl fmt::Display for NativeEffectPipelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "effect program {}: {}", self.program, self.native)?;
        for error in &self.related {
            write!(f, "; GPU creation also failed: {error}")?;
        }
        if let Some(error) = &self.poll {
            write!(f, "; GPU poll also failed: {error}")?;
        }
        Ok(())
    }
}
impl Error for NativeEffectPipelineError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.native)
    }
}

#[derive(Clone)]
pub(crate) struct NativePipelineCompiler {
    device: wgpu::Device,
    uniform_layout: wgpu::BindGroupLayout,
    input_layout: wgpu::BindGroupLayout,
    resource_layout: wgpu::BindGroupLayout,
    driver_pipeline_cache: Option<wgpu::PipelineCache>,
}
impl NativePipelineCompiler {
    pub(crate) fn new(
        device: &wgpu::Device,
        uniform: &wgpu::BindGroupLayout,
        input: &wgpu::BindGroupLayout,
        resource: &wgpu::BindGroupLayout,
        cache: Option<&wgpu::PipelineCache>,
    ) -> Self {
        Self {
            device: device.clone(),
            uniform_layout: uniform.clone(),
            input_layout: input.clone(),
            resource_layout: resource.clone(),
            driver_pipeline_cache: cache.cloned(),
        }
    }
    pub(crate) fn compile(
        &self,
        program: &EffectProgram,
        target: wgpu::TextureFormat,
        suffix: Option<&'static str>,
    ) -> Result<wgpu::RenderPipeline, RenderError> {
        let mut source = motion_effects::shader::native_output_wgsl(
            &program.shader,
            target == wgpu::TextureFormat::Rgba16Float,
        )?
        .into_owned();
        if let Some(suffix) = suffix {
            source.push_str(suffix);
        }
        let memory = self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let internal = self.device.push_error_scope(wgpu::ErrorFilter::Internal);
        let validation = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("effect WGSL"),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
        let layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[
                    Some(&self.uniform_layout),
                    Some(&self.input_layout),
                    Some(&self.resource_layout),
                ],
                immediate_size: 0,
            });
        let sprite = program.shader.sprite;
        let additive = program.shader.additive;
        const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
            wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4];
        let vertex_layout = Some(wgpu::VertexBufferLayout {
            array_stride: 48,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &ATTRIBUTES,
        });
        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("effect fullscreen pass"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("sdk_vertex"),
                    compilation_options: Default::default(),
                    buffers: if sprite {
                        std::slice::from_ref(&vertex_layout)
                    } else {
                        &[]
                    },
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("sdk_fragment"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target,
                        blend: if sprite {
                            Some(if additive {
                                wgpu::BlendState {
                                    color: wgpu::BlendComponent {
                                        src_factor: wgpu::BlendFactor::One,
                                        dst_factor: wgpu::BlendFactor::One,
                                        operation: wgpu::BlendOperation::Add,
                                    },
                                    alpha: wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING.alpha,
                                }
                            } else {
                                wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING
                            })
                        } else {
                            None
                        },
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: self.driver_pipeline_cache.as_ref(),
            });
        let validation = validation.pop();
        let internal = internal.pop();
        let memory = memory.pop();
        // Service callbacks without waiting for unrelated in-flight frames.
        let polled = self.device.poll(wgpu::PollType::Poll);
        let errors = pollster::block_on(validation)
            .into_iter()
            .chain(pollster::block_on(internal))
            .chain(pollster::block_on(memory));
        let mut errors = errors;
        if let Some(native) = errors.next() {
            return Err(RenderError::EffectPipeline(NativeEffectPipelineError {
                program: program.key.clone(),
                native,
                related: errors.collect(),
                poll: polled.err(),
            }));
        }
        polled?;
        Ok(pipeline)
    }
}
