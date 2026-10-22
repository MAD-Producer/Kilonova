//! Independent N1 preview on the desktop's shared GPU. No project/Registry mutation.
use crate::node_editor::EditorSnapshot;
use motion_effects::{AlphaMode, WorkingSpace};
use motion_render::effect_plan::EffectUniform;
use motion_render::{
    AdmittedBuffer, AdmittedTexture, AdmittedTextureView, AllocationContext, RenderError,
};
use motion_ui::{
    paint,
    ui::{self, Align},
    Input, PaintList, Rect, TextAtlas,
};
use std::time::Instant;

pub const LOGICAL_SIZE: u32 = 64;
pub const RASTER_SIZE: u32 = 512;
pub const FPS: f64 = 30.;
#[cfg(test)]
#[path = "node_preview/admission_tests.rs"]
mod admission_tests;
#[derive(Clone)]
pub struct GpuContext {
    pub allocations: AllocationContext,
    pub format: wgpu::TextureFormat,
}
#[derive(Clone, Copy)]
pub struct Viewport {
    pub rect: [f32; 4],
    pub seconds: f64,
}
pub struct Controls {
    pub active: Option<(u64, u64)>,
    pub playing: bool,
    pub looping: bool,
    start: Instant,
    origin: f64,
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            active: None,
            playing: true,
            looping: true,
            start: Instant::now(),
            origin: 0.,
        }
    }
}
impl Controls {
    pub fn seconds(&self) -> f64 {
        let time = self.origin
            + if self.playing {
                self.start.elapsed().as_secs_f64()
            } else {
                0.
            };
        if self.looping {
            time.rem_euclid(4.)
        } else {
            time
        }
    }
    pub fn toggle(&mut self) {
        self.origin = self.seconds();
        self.start = Instant::now();
        self.playing = !self.playing;
    }
    pub fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        input: &Input,
        locale: crate::i18n::Locale,
        stamp: (u64, u64),
        target: Option<&motion_graph::PortRef>,
    ) -> Rect {
        let styles = list.styles();
        let colors = styles.colors();

        let zh = locale == crate::i18n::Locale::Zh;
        paint::rect(list, area, colors.PANEL);
        let title = if let Some(target) = target {
            format!(
                "{} · {}.{}",
                if zh { "临时预览" } else { "Preview" },
                target.node,
                target.port
            )
        } else {
            if zh {
                "实时效果预览"
            } else {
                "LIVE EFFECT PREVIEW"
            }
            .into()
        };
        ui::text(
            list,
            atlas,
            Rect::new(area.min[0] + 10., area.min[1] + 3., area.width() - 20., 25.),
            &title,
            colors.TEXT,
            Align::Left,
        );
        let play = Rect::new(area.min[0] + 10., area.min[1] + 32., 66., 27.);
        if ui::button(
            list,
            atlas,
            play,
            if self.playing {
                if zh {
                    "暂停"
                } else {
                    "Pause"
                }
            } else {
                if zh {
                    "播放"
                } else {
                    "Play"
                }
            },
            input,
            true,
        )
        .clicked
        {
            self.toggle();
        }
        if ui::button(
            list,
            atlas,
            Rect::new(play.max[0] + 5., play.min[1], 58., 27.),
            if zh { "归零" } else { "Reset" },
            input,
            true,
        )
        .clicked
        {
            self.origin = 0.;
            self.start = Instant::now();
        }
        ui::checkbox(
            list,
            atlas,
            Rect::new(play.max[0] + 68., play.min[1], 90., 27.),
            if zh { "4秒循环" } else { "Loop 4s" },
            &mut self.looping,
            input,
        );
        ui::text(
            list,
            atlas,
            Rect::new(
                area.min[0] + 10.,
                area.min[1] + 63.,
                area.width() - 20.,
                24.,
            ),
            if zh {
                "示例色彩图 · 64×64 · 30 fps"
            } else {
                "Sample colors · 64×64 · 30 fps"
            },
            colors.TEXT_MUTED,
            Align::Left,
        );
        let available = Rect::new(
            area.min[0] + 10.,
            area.min[1] + 91.,
            (area.width() - 20.).max(1.),
            (area.height() - 135.).max(1.),
        );
        let side = available.width().min(available.height());
        let image = Rect::new(
            available.min[0] + (available.width() - side) * 0.5,
            available.min[1],
            side,
            side,
        );
        paint::rect(list, image, colors.SUNKEN);
        let status = match self.active {
            None => {
                if zh {
                    "等待有效图编译"
                } else {
                    "Waiting for a valid graph"
                }
            }
            Some(active) if active != stamp => {
                if zh {
                    "上次有效图 · 请查看诊断"
                } else {
                    "Last valid graph · see diagnostics"
                }
            }
            _ => {
                if zh {
                    "预览已更新"
                } else {
                    "Preview updated"
                }
            }
        };
        if self.active.is_none() {
            ui::text(
                list,
                atlas,
                image.shrink(8.),
                status,
                colors.TEXT_MUTED,
                Align::Center,
            );
        }
        ui::text(
            list,
            atlas,
            Rect::new(
                area.min[0] + 10.,
                area.max[1] - 38.,
                area.width() - 20.,
                30.,
            ),
            status,
            colors.TEXT_MUTED,
            Align::Left,
        );
        image
    }
}

pub fn regions(area: Rect) -> (Rect, Rect) {
    if area.width() >= 850. {
        let width = (area.width() * 0.28).clamp(240., 360.);
        (
            Rect::new(
                area.min[0],
                area.min[1],
                area.width() - width,
                area.height(),
            ),
            Rect::new(area.max[0] - width, area.min[1], width, area.height()),
        )
    } else {
        let height = (area.height() * 0.40)
            .clamp(180., 320.)
            .min((area.height() - 180.).max(0.));
        (
            Rect::new(
                area.min[0],
                area.min[1],
                area.width(),
                area.height() - height,
            ),
            Rect::new(area.min[0], area.max[1] - height, area.width(), height),
        )
    }
}
pub fn image_rect(area: Rect) -> Rect {
    let (_, area) = regions(area);
    panel_image_rect(area)
}
pub fn panel_image_rect(area: Rect) -> Rect {
    let available = Rect::new(
        area.min[0] + 10.,
        area.min[1] + 91.,
        (area.width() - 20.).max(1.),
        (area.height() - 135.).max(1.),
    );
    let side = available.width().min(available.height());
    Rect::new(
        available.min[0] + (available.width() - side) * 0.5,
        available.min[1],
        side,
        side,
    )
}

pub struct NodePreview {
    pub stamp: (u64, u64),
    pipeline: wgpu::RenderPipeline,
    groups: [wgpu::BindGroup; 3],
    uniform: EffectUniform,
    view: AdmittedTextureView,
    display: Display,
    display_mode: [u32; 4],
    pub frames: u64,
    pub renders: u64,
    cached_seconds: Option<f64>,
    // Raw bind groups/views above must retire before their admitted owners.
    buffer: AdmittedBuffer,
    _source: AdmittedTexture,
    _lut: AdmittedTexture,
    _texture: AdmittedTexture,
    allocations: AllocationContext,
}
struct Display {
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    format: wgpu::TextureFormat,
    buffer: AdmittedBuffer,
}
fn texture(
    allocations: &AllocationContext,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> Result<AdmittedTexture, RenderError> {
    allocations.texture(&wgpu::TextureDescriptor {
        label: Some("independent node preview"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}
fn upload(
    allocations: &AllocationContext,
    texture: &AdmittedTexture,
    width: u32,
    height: u32,
    pixels: &[u8],
) -> Result<(), RenderError> {
    allocations.write_texture(
        texture,
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    )
}
fn input_layout(device: &wgpu::Device, count: u32) -> wgpu::BindGroupLayout {
    let entries: Vec<_> = (0..count)
        .flat_map(|i| {
            [
                wgpu::BindGroupLayoutEntry {
                    binding: i * 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: i * 2 + 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ]
        })
        .collect();
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("SDK node inputs"),
        entries: &entries,
    })
}
impl NodePreview {
    pub fn build(
        context: &GpuContext,
        snapshot: &EditorSnapshot,
        compiled: &motion_graph::CompiledGraph,
    ) -> Result<Self, RenderError> {
        let allocations = &context.allocations;
        let device = allocations.device();
        let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let result = (|| {
            let uniform_layout =
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("SDK node uniform"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(
                                motion_effects::shader::UNIFORM_BYTES as u64,
                            ),
                        },
                        count: None,
                    }],
                });
            let input = input_layout(device, 3);
            let resources = input_layout(device, 4);
            let linear = snapshot.document.working_space == WorkingSpace::Linear;
            let premult = snapshot.document.alpha_mode == AlphaMode::Premultiplied;
            let source = texture(
                allocations,
                LOGICAL_SIZE,
                LOGICAL_SIZE,
                if linear {
                    wgpu::TextureFormat::Rgba8UnormSrgb
                } else {
                    wgpu::TextureFormat::Rgba8Unorm
                },
            )?;
            let pixels: Vec<u8> = (0..LOGICAL_SIZE * LOGICAL_SIZE)
                .flat_map(|i| {
                    let x = i % LOGICAL_SIZE;
                    let y = i / LOGICAL_SIZE;
                    [
                        (x * 255 / (LOGICAL_SIZE - 1)) as u8,
                        (y * 255 / (LOGICAL_SIZE - 1)) as u8,
                        if (x / 8 + y / 8) % 2 == 0 { 160 } else { 64 },
                        255,
                    ]
                })
                .collect();
            upload(allocations, &source, LOGICAL_SIZE, LOGICAL_SIZE, &pixels)?;
            let lut = texture(allocations, 256, 1, wgpu::TextureFormat::Rgba8Unorm)?;
            let lut_pixels: Vec<_> = (0u8..=255).flat_map(|i| [i; 4]).collect();
            upload(allocations, &lut, 256, 1, &lut_pixels)?;
            let source_view = source.view()?;
            let lut_view = lut.view()?;
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
            let bind = |layout: &wgpu::BindGroupLayout, views: &[&wgpu::TextureView]| {
                let entries: Vec<_> = views
                    .iter()
                    .enumerate()
                    .flat_map(|(i, view)| {
                        [
                            wgpu::BindGroupEntry {
                                binding: i as u32 * 2,
                                resource: wgpu::BindingResource::TextureView(view),
                            },
                            wgpu::BindGroupEntry {
                                binding: i as u32 * 2 + 1,
                                resource: wgpu::BindingResource::Sampler(&sampler),
                            },
                        ]
                    })
                    .collect();
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("SDK node textures"),
                    layout,
                    entries: &entries,
                })
            };
            let input_group = bind(&input, &[&source_view, &source_view, &lut_view]);
            let resource_group = bind(&resources, &[&*source_view; 4]);
            let size = LOGICAL_SIZE as f32;
            let region = [0., 0., size, size];
            let mut uniform = EffectUniform {
                size: [size, size, 1. / size, 1. / size],
                region,
                input_region: region,
                source_region: region,
                clock: [0., 0., FPS as f32, 0.],
                mode: [
                    0.,
                    if linear { 0. } else { 1. },
                    if premult { 1. } else { 0. },
                    1.,
                ],
                output_mode: [
                    if linear { 0. } else { 1. },
                    if premult { 1. } else { 0. },
                    1.,
                    RASTER_SIZE as f32 / size,
                ],
                params: [[0.; 4]; 32],
            };
            for (index, p) in snapshot.document.public_parameters.iter().enumerate() {
                uniform.params[index] = p.default;
            }
            let buffer = allocations.buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("SDK node clock and parameters"),
                contents: bytemuck::bytes_of(&uniform),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            })?;
            let uniform_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("SDK node uniform"),
                layout: &uniform_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            });
            let texture = texture(
                allocations,
                RASTER_SIZE,
                RASTER_SIZE,
                wgpu::TextureFormat::Rgba16Float,
            )?;
            let view = texture.view()?;
            let display_mode = [
                u32::from(!linear),
                u32::from(premult),
                u32::from(context.format.is_srgb()),
                0,
            ];
            let display = Display::new(allocations, &view, context.format, display_mode, None)?;
            // Admit output and display capacity before compiling user WGSL, so
            // a later budget refusal cannot replace an earlier shader fault.
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("N1 compiled graph"),
                source: wgpu::ShaderSource::Wgsl(compiled.shader.wgsl.clone().into()),
            });
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("SDK node pipeline"),
                bind_group_layouts: &[Some(&uniform_layout), Some(&input), Some(&resources)],
                immediate_size: 0,
            });
            let pipeline = pipeline(
                device,
                &layout,
                &shader,
                "sdk_vertex",
                "sdk_fragment",
                wgpu::TextureFormat::Rgba16Float,
            );
            Ok(Self {
                stamp: snapshot.preview_stamp(),
                pipeline,
                groups: [uniform_group, input_group, resource_group],
                uniform,
                buffer,
                _source: source,
                _lut: lut,
                _texture: texture,
                view,
                display,
                display_mode,
                frames: 0,
                renders: 0,
                cached_seconds: None,
                allocations: allocations.clone(),
            })
        })();
        let error = pollster::block_on(error_scope.pop());
        match (result, error) {
            (Err(primary), Some(error)) => Err(RenderError::Context {
                message: format!("{primary}; GPU validation also failed: {error}"),
                cause: Box::new(primary),
            }),
            (Err(primary), None) => Err(primary),
            (Ok(_), Some(error)) => Err(RenderError::Invalid(error.to_string())),
            (Ok(value), None) => Ok(value),
        }
    }
    pub fn encode(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        viewport: Viewport,
        format: wgpu::TextureFormat,
    ) -> Result<(), RenderError> {
        if self.display.format != format {
            self.display_mode[2] = u32::from(format.is_srgb());
            self.display = Display::new(
                &self.allocations,
                &self.view,
                format,
                self.display_mode,
                Some(self.display.buffer.clone()),
            )?;
        }
        if self.cached_seconds != Some(viewport.seconds) {
            self.uniform.clock[0] = viewport.seconds as f32;
            self.uniform.clock[1] = (viewport.seconds * FPS) as f32;
            self.allocations
                .write_buffer(&self.buffer, 0, bytemuck::bytes_of(&self.uniform))?;
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    multiview_mask: None,
                    label: Some("N1 live graph"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        depth_slice: None,
                        view: &self.view,
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
                pass.set_pipeline(&self.pipeline);
                for (i, group) in self.groups.iter().enumerate() {
                    pass.set_bind_group(i as u32, group, &[]);
                }
                pass.draw(0..3, 0..1);
            }
            self.cached_seconds = Some(viewport.seconds);
            self.renders += 1;
        }
        let [x, y, w, h] = viewport.rect;
        if w > 0. && h > 0. {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                multiview_mask: None,
                label: Some("GPU node preview presentation"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    depth_slice: None,
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_viewport(x, y, w, h, 0., 1.);
            pass.set_scissor_rect(x as u32, y as u32, w.ceil() as u32, h.ceil() as u32);
            pass.set_pipeline(&self.display.pipeline);
            pass.set_bind_group(0, &self.display.group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.frames += 1;
        Ok(())
    }
}
fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    vertex: &str,
    fragment: &str,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("independent node shader"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vertex),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
impl Display {
    fn new(
        allocations: &AllocationContext,
        source: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        mode: [u32; 4],
        existing: Option<AdmittedBuffer>,
    ) -> Result<Self, RenderError> {
        let device = allocations.device();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("node preview presentation"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let buffer = if let Some(buffer) = existing {
            allocations.write_buffer(&buffer, 0, bytemuck::cast_slice(&mode))?;
            buffer
        } else {
            allocations.buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("node preview color contract"),
                contents: bytemuck::cast_slice(&mode),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            })?
        };
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: buffer.as_entire_binding(),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("node preview display shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("node_preview.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        Ok(Self {
            pipeline: pipeline(
                device,
                &pipeline_layout,
                &shader,
                "vertex_main",
                "fragment_main",
                format,
            ),
            group,
            format,
            buffer,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn node_preview_respects_occupied_session_admission() {
        let budget = motion_render::AllocationBudget::default();
        let renderer = pollster::block_on(
            motion_render::Renderer::headless_with_allocation_budget(budget.clone()),
        )
        .unwrap();
        let context = GpuContext {
            allocations: renderer.allocation_context(),
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
        };
        let editor = crate::node_editor::NodeEditor::default();
        let snapshot = editor.snapshot();
        let compiled = snapshot.compile().unwrap();
        let occupied = budget.info().unwrap().admitted_bytes;
        budget.set_limit(occupied + 1).unwrap();
        let result = NodePreview::build(&context, &snapshot, &compiled);
        assert!(result.is_err(),
            "independent N1 preview bypassed the live shared allowance: occupied={occupied} budget={}", occupied + 1);
    }

    fn pixels(
        renderer: &motion_render::Renderer,
        preview: &mut NodePreview,
        seconds: f64,
    ) -> Vec<u8> {
        let target = renderer.capture_target(64, 64).unwrap();
        let mut encoder = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        preview
            .encode(
                &mut encoder,
                &target.view,
                Viewport {
                    rect: [0., 0., 64., 64.],
                    seconds,
                },
                wgpu::TextureFormat::Rgba8UnormSrgb,
            )
            .unwrap();
        renderer
            .submit_allocated(encoder.finish(), |ledger| {
                ledger.merge(target.texture_resource_ledger())
            })
            .unwrap();
        renderer.read_target(&target).unwrap()
    }
    #[test]
    fn gpu_preview_obeys_live_time_parameters_alpha_and_pause_cache() {
        let renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
        let context = GpuContext {
            allocations: renderer.allocation_context(),
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
        };
        let mut editor = crate::node_editor::NodeEditor::default();
        let snapshot = editor.snapshot();
        let compiled = snapshot.compile().unwrap();
        let mut preview = NodePreview::build(&context, &snapshot, &compiled).unwrap();
        let first = pixels(&renderer, &mut preview, 0.);
        assert_eq!(preview.renders, 1);
        assert_eq!(first, pixels(&renderer, &mut preview, 0.));
        assert_eq!(preview.renders, 1, "paused preview reuses shader output");
        let later = pixels(&renderer, &mut preview, 0.65);
        assert_ne!(first, later, "Time changes GPU pixels");
        assert_eq!(preview.renders, 2);
        editor.set_parameter_default("color", 0, 0.9).unwrap();
        let snapshot = editor.snapshot();
        let compiled = snapshot.compile().unwrap();
        let mut changed = NodePreview::build(&context, &snapshot, &compiled).unwrap();
        let color = pixels(&renderer, &mut changed, 0.);
        assert_ne!(first, color, "public parameter changes pixels");
        let mut premult = snapshot.clone();
        premult.document.alpha_mode = AlphaMode::Premultiplied;
        let compiled = premult.compile().unwrap();
        let mut premult_preview = NodePreview::build(&context, &premult, &compiled).unwrap();
        let premult_pixels = pixels(&renderer, &mut premult_preview, 0.);
        assert!(
            color
                .iter()
                .zip(premult_pixels.iter())
                .all(|(a, b)| a.abs_diff(*b) <= 1),
            "straight/premultiplied display must agree"
        );
        editor.disconnect_input(&motion_graph::PortRef::new("output", "color"));
        assert!(editor.snapshot().compile().is_err());
        assert_eq!(
            color,
            pixels(&renderer, &mut changed, 0.),
            "compile failure leaves actual valid GPU texture"
        );
    }
    #[test]
    fn temporary_output_changes_gpu_pixels_and_restores_canonical_save_and_export() {
        let renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
        let context = GpuContext {
            allocations: renderer.allocation_context(),
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
        };
        let mut editor = crate::node_editor::NodeEditor::default();
        let id = editor
            .add_node(
                motion_graph::NodeOp::Vec3 {
                    value: [0.2, 0.4, 0.6],
                },
                [0., 0.],
            )
            .unwrap();
        let original = editor.snapshot();
        let canonical = original.compile().unwrap();
        let mut full = NodePreview::build(&context, &original, &canonical).unwrap();
        let first = pixels(&renderer, &mut full, 0.);
        let dirty = editor.dirty();
        let target = motion_graph::PortRef::new(id, "value");
        editor.toggle_preview(target.clone()).unwrap();
        let inspected = editor.snapshot();
        assert_eq!(inspected.revision, original.revision);
        assert_eq!(inspected.semantic_revision, original.semantic_revision);
        assert_eq!(editor.dirty(), dirty);
        assert_eq!(inspected.document, original.document);
        assert_eq!(inspected.compile().unwrap().source, canonical.source);
        let compiled = inspected.compile_preview().unwrap();
        let mut preview = NodePreview::build(&context, &inspected, &compiled).unwrap();
        assert_eq!(preview.stamp, inspected.preview_stamp());
        assert_ne!(first, pixels(&renderer, &mut preview, 0.));
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("saved.msgraph");
        inspected.save(&path).unwrap();
        assert_eq!(
            motion_graph::GraphDocument::load(&path).unwrap(),
            original.document
        );
        let export = inspected
            .export_msfx(&folder.path().join("canonical.msfx"))
            .unwrap();
        assert_eq!(
            export.compile.reachable_nodes,
            canonical.report.reachable_nodes
        );
        editor.toggle_preview(target).unwrap();
        let restored = editor.snapshot();
        let compiled = restored.compile_preview().unwrap();
        assert_eq!(compiled.source, canonical.source);
        let mut full = NodePreview::build(&context, &restored, &compiled).unwrap();
        assert_eq!(first, pixels(&renderer, &mut full, 0.));
        assert!(!editor.accept_compile(&inspected, Ok(compiled.report)));
    }
    #[test]
    fn node_viewports_stay_inside_editor_panel() {
        for (width, height) in [(375., 600.), (768., 680.), (1280., 800.), (1440., 900.)] {
            let area = Rect::new(20., 90., width, height);
            let (editor, preview) = regions(area);
            assert!(area.contains_rect(editor));
            assert!(area.contains_rect(preview));
            assert!(area.contains_rect(image_rect(area)));
        }
    }
}
