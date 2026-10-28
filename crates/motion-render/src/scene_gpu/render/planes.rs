//! Independently authored plane geometry in the common mesh depth/alpha pass.
//! Per-encode clip vertices are immutable and join the existing admission owner.
use super::{features, invalid, unsupported, Result, ScenePassFrame, SceneRequest};
use crate::{AdmittedBuffer, AdmittedTextureView, Renderer};
use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;

pub(crate) struct ScenePlane {
    pub object: u64,
    pub world: glam::Mat4,
    pub size: [f32; 2],
    /// Existing straight sRGB tint and combined layer/content opacity.
    pub color: [f32; 4],
    /// Alpha sources test opaque depth without writing it.
    pub blend: bool,
    /// Static source retaining its admitted upload revision.
    pub texture: Option<AdmittedTextureView>,
    pub surface: Option<std::sync::Arc<motion_model::depth::DepthSurface>>,
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    clip: [f32; 4],
    color: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TexturedVertex {
    clip: [f32; 4],
    color: [f32; 4],
    uv: [f32; 2],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ProjectedVertex {
    clip: [f32; 4],
    color: [f32; 4],
    uvq: [f32; 3],
}
const SOLID_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4];
const IMAGE_ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x2];
const PROJECTED_ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x3];
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    format: wgpu::TextureFormat,
    samples: u32,
    textured: bool,
    projected: bool,
    blend: bool,
}
#[derive(Clone)]
struct Pipeline {
    render: wgpu::RenderPipeline,
    image: Option<(wgpu::BindGroupLayout, wgpu::Sampler)>,
}
#[derive(Default)]
pub(super) struct Pipelines(HashMap<Key, Pipeline>);
pub(super) struct Draw {
    pub object: u64,
    pub pipeline: wgpu::RenderPipeline,
    pub group: Option<wgpu::BindGroup>,
    pub vertices: std::ops::Range<u64>,
    pub blend: bool,
    pub distance: f32,
    pub vertex_count: u32,
}
pub(super) struct Prepared {
    pub vertices: AdmittedBuffer,
    pub draws: Vec<Draw>,
    pub count: usize,
    pub triangles: u64,
}

pub(super) fn prepare(
    renderer: &Renderer,
    request: SceneRequest,
    format: wgpu::TextureFormat,
    frame: &ScenePassFrame,
    planes: &[ScenePlane],
    cache: &mut Pipelines,
) -> Result<Option<Prepared>> {
    if planes.is_empty() {
        return Ok(None);
    }
    if planes.len() > motion_model::MAX_LAYERS {
        return Err(invalid(
            "shared_planes",
            planes.len(),
            "plane count exceeds composition limit",
        ));
    }
    let mut vertices = Vec::new();
    let staging_bytes = planes.iter().try_fold(0usize, |bytes, plane| {
        let count = plane.surface.as_ref().map_or(6, |s| s.vertices().len());
        let stride = if plane.texture.is_some() && plane.surface.is_some() { std::mem::size_of::<ProjectedVertex>() }
            else if plane.texture.is_some() { std::mem::size_of::<TexturedVertex>() } else { std::mem::size_of::<Vertex>() };
        bytes.checked_add(count.checked_mul(stride)?).filter(|n| *n <= motion_model::depth::MAX_DEPTH_RESOURCE_BYTES)
    }).ok_or_else(|| invalid("shared_planes", 0, "plane vertex staging byte budget exceeded"))?;
    vertices
        .try_reserve_exact(staging_bytes)
        .map_err(|_| invalid("shared_planes", 0, "plane vertex staging allocation failed"))?;
    let mut draws = Vec::with_capacity(planes.len());
    let projection = frame.camera.view_projection.as_dmat4();
    for plane in planes {
        if !plane.world.is_finite()
            || plane.world.row(3) != glam::Vec4::W
            || plane.size.iter().any(|n| !n.is_finite() || *n < 0.)
            || plane
                .color
                .iter()
                .any(|n| !n.is_finite() || !(0. ..=1.).contains(n))
            || (!plane.blend && plane.color[3] != 1.)
        {
            return Err(crate::RenderError::Invalid(format!(
                "scene plane object={} phase=shared_plane: requires finite affine plane geometry and valid premultiplied color policy", plane.object
            )).into());
        }
        if plane.texture.as_ref().is_some_and(|view| {
            let texture = view.texture();
            texture.format() != wgpu::TextureFormat::Rgba8UnormSrgb
                || texture.dimension() != wgpu::TextureDimension::D2
                || texture.depth_or_array_layers() != 1
                || texture.sample_count() != 1
                || !texture
                    .usage()
                    .contains(wgpu::TextureUsages::TEXTURE_BINDING)
        }) {
            return Err(crate::RenderError::Invalid(format!(
                "scene plane object={} phase=shared_plane_image: expected admitted single-sample RGBA8 sRGB source", plane.object
            )).into());
        }
        let first = vertices.len() as u64;
        let mut color = plane.color;
        for c in &mut color[..3] {
            *c = crate::renderer::srgb_to_linear(*c) * plane.color[3];
        }
        // Use the same camera-forward metric as mesh primitives. Intersecting
        // translucent geometry still needs a separate fragment-order policy.
        let center = plane.world.transform_point3(glam::Vec3::ZERO);
        let forward = (frame.camera.target - frame.camera.eye).normalize();
        let distance = (center - frame.camera.eye).dot(forward);
        if !center.is_finite() || !distance.is_finite() {
            return Err(crate::RenderError::Invalid(format!(
                "scene plane object={} phase=shared_plane_sort: plane camera distance overflow",
                plane.object
            ))
            .into());
        }
        // Match the authored XY geometry and camera, without assigning a single
        // depth to its raster. Hardware clipping/interpolation supplies every Z.
        let world = plane.world.as_dmat4();
        let mut append = |position: glam::DVec3, uv: [f32; 2], q: f32| -> Result<()> {
            let clip = projection * world * position.extend(1.);
            if !clip.is_finite() || !clip.as_vec4().is_finite() {
                return Err(crate::RenderError::Invalid(format!(
                    "scene plane object={} phase=shared_plane_clip: plane camera projection overflow", plane.object
                )).into());
            }
            let clip = clip.as_vec4().to_array();
            if plane.texture.is_some() && plane.surface.is_some() {
                vertices.extend_from_slice(bytemuck::bytes_of(&ProjectedVertex { clip, color, uvq:[uv[0]*q,uv[1]*q,q] }));
            } else if plane.texture.is_some() {
                vertices.extend_from_slice(bytemuck::bytes_of(&TexturedVertex { clip, color, uv }));
            } else {
                vertices.extend_from_slice(bytemuck::bytes_of(&Vertex { clip, color }));
            }
            Ok(())
        };
        let vertex_count = if let Some(surface) = &plane.surface {
            for vertex in surface.vertices() {
                append(glam::Vec3::from_array(vertex.position).as_dvec3(), vertex.uv, vertex.texture_weight)?;
            }
            u32::try_from(surface.vertices().len()).map_err(|_| invalid("shared_planes", 0, "surface vertex count overflow"))?
        } else {
        for [x, y] in [
            [-0.5, -0.5],
            [0.5, -0.5],
            [0.5, 0.5],
            [-0.5, -0.5],
            [0.5, 0.5],
            [-0.5, 0.5],
        ] {
            append(glam::DVec3::new(x * f64::from(plane.size[0]), y * f64::from(plane.size[1]), 0.),
                [(x + 0.5) as f32, (0.5 - y) as f32], 1.)?;
        }
        6
        };
        let key = Key {
            format,
            samples: frame.samples,
            textured: plane.texture.is_some(),
            projected: plane.texture.is_some() && plane.surface.is_some(),
            blend: plane.blend,
        };
        let pipeline = if let Some(pipeline) = cache.0.get(&key) {
            pipeline.clone()
        } else {
            // Four formats x two sample counts x three sources x two alpha modes.
            // Asset IDs and animation never create additional pipeline keys.
            if cache.0.len() >= 48 {
                return Err(invalid(
                    "shared_plane_pipeline",
                    0,
                    "bounded pipeline cache is full",
                ));
            }
            log::info!(
                "start #{} scene_plane_pipeline source={} object={} format={format:?} samples={} textured={} blend={}",
                request.request, request.source, plane.object, frame.samples, key.textured, key.blend
            );
            let pipeline = pipeline(renderer, key)?;
            cache.0.insert(key, pipeline.clone());
            pipeline
        };
        let group = match (&plane.texture, &pipeline.image) {
            (Some(view), Some((layout, sampler))) => Some(renderer.device.create_bind_group(
                &wgpu::BindGroupDescriptor {
                    label: Some("immutable shared plane image revision"),
                    layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(sampler),
                        },
                    ],
                },
            )),
            (None, None) => None,
            _ => {
                return Err(invalid(
                    "shared_plane_pipeline",
                    0,
                    "source and pipeline binding disagree",
                ))
            }
        };
        draws.push(Draw {
            object: plane.object,
            pipeline: pipeline.render,
            group,
            vertices: first..vertices.len() as u64,
            blend: key.blend,
            distance,
            vertex_count,
        });
    }
    let vertices =
        renderer
            .allocation_context()
            .buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("immutable shared plane vertices"),
                contents: &vertices,
                usage: wgpu::BufferUsages::VERTEX,
            })?;
    let triangles = draws.iter().map(|draw| u64::from(draw.vertex_count) / 3).sum();
    Ok(Some(Prepared {
        vertices,
        draws,
        count: planes.len(),
        triangles,
    }))
}

fn pipeline(renderer: &Renderer, key: Key) -> Result<Pipeline> {
    if !matches!(
        key.format,
        wgpu::TextureFormat::Rgba8UnormSrgb
            | wgpu::TextureFormat::Rgba16Unorm
            | wgpu::TextureFormat::Rgba16Float
            | wgpu::TextureFormat::Rgba32Float
    ) {
        return Err(unsupported(
            0,
            "shared planes require a declared working color format",
        ));
    }
    let support = features(renderer, key.format);
    if key.blend
        && !support
            .flags
            .contains(wgpu::TextureFormatFeatureFlags::BLENDABLE)
    {
        return Err(unsupported(
            0,
            "shared alpha planes require blendable working storage",
        ));
    }
    if !support
        .allowed_usages
        .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
        || !support.flags.sample_count_supported(key.samples)
        || (key.samples == 4
            && !support
                .flags
                .contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE))
    {
        return Err(unsupported(
            0,
            "shared plane target format/MSAA combination",
        ));
    }
    let oom = renderer
        .device
        .push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let validation = renderer
        .device
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let shader = renderer
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shared plane geometry"),
            source: wgpu::ShaderSource::Wgsl(
                if key.projected {
                    include_str!("projected_planes.wgsl")
                } else if key.textured {
                    include_str!("textured_planes.wgsl")
                } else {
                    include_str!("planes.wgsl")
                }
                .into(),
            ),
        });
    let image = key.textured.then(|| {
        let layout = renderer
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("shared plane static source"),
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
                ],
            });
        let sampler = renderer.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shared plane linear clamp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        (layout, sampler)
    });
    let groups: Vec<_> = image.iter().map(|(layout, _)| Some(layout)).collect();
    let layout = renderer
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shared opaque planes"),
            bind_group_layouts: &groups,
            immediate_size: 0,
        });
    let pipeline = renderer
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("planes in shared mesh depth and alpha order"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: if key.projected {
                        std::mem::size_of::<ProjectedVertex>()
                    } else if key.textured {
                        std::mem::size_of::<TexturedVertex>()
                    } else {
                        std::mem::size_of::<Vertex>()
                    } as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: if key.projected {
                        &PROJECTED_ATTRIBUTES
                    } else if key.textured {
                        &IMAGE_ATTRIBUTES
                    } else {
                        &SOLID_ATTRIBUTES
                    },
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: key.format,
                    blend: key
                        .blend
                        .then_some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(!key.blend),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: key.samples,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        });
    let validation_cause = pollster::block_on(validation.pop());
    let oom_cause = pollster::block_on(oom.pop());
    if let Some(cause) = validation_cause.or(oom_cause) {
        return Err(cause.into());
    }
    Ok(Pipeline {
        render: pipeline,
        image,
    })
}
