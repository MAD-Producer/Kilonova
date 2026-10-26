use super::{
    charge,
    images::{ImageRequest, Images},
    invalid, unsupported, Result, SceneGpuLimits,
};
use crate::{AdmittedBuffer, AllocationContext};
use bytemuck::{Pod, Zeroable};
use motion_scene::{
    obj::{MapKind, MaterialColorSpace},
    AlphaMode, Material, MaterialModel,
};

pub(super) const MAPS: usize = 7;
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MapUniform {
    transform: [f32; 4],
    options: [u32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    base: [f32; 4],
    emissive_metallic: [f32; 4],
    parameters: [f32; 4],
    specular_exponent: [f32; 4],
    ambient: [f32; 4],
    flags: [u32; 4],
    maps: [MapUniform; MAPS],
}
pub(super) struct GpuMaterial {
    pub group: wgpu::BindGroup,
    pub alpha: AlphaMode,
    pub double_sided: bool,
    _uniform: AdmittedBuffer,
}

pub(super) fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let mut entries = vec![wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<Uniform>() as u64),
        },
        count: None,
    }];
    for i in 0..MAPS as u32 {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 1 + i * 2,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 2 + i * 2,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        });
    }
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("scene material maps"),
        entries: &entries,
    })
}

pub(super) fn prepare(
    context: &AllocationContext,
    source: &motion_scene::Scene,
    images: &mut Images,
    layout: &wgpu::BindGroupLayout,
    limits: SceneGpuLimits,
    bytes: &mut u64,
) -> Result<Vec<GpuMaterial>> {
    let mut out = Vec::new();
    for (id, m) in source
        .materials()
        .iter()
        .chain(std::iter::once(&Material::default()))
        .enumerate()
    {
        let mut uniform = Uniform {
            base: m.base_color,
            emissive_metallic: [m.emissive[0], m.emissive[1], m.emissive[2], m.metallic],
            parameters: [
                m.roughness,
                m.normal_scale,
                m.occlusion_strength,
                m.alpha_cutoff,
            ],
            specular_exponent: [0.; 4],
            ambient: [1.; 4],
            flags: [
                0,
                match m.alpha {
                    AlphaMode::Opaque => 0,
                    AlphaMode::Mask => 1,
                    AlphaMode::Blend => 2,
                },
                u32::from(m.normal_texture.is_some()),
                0,
            ],
            maps: [MapUniform {
                transform: [0., 0., 1., 1.],
                options: [0; 4],
            }; MAPS],
        };
        let mut slots: [Option<usize>; MAPS] = [None; MAPS];
        let mut colors = [true, false, false, false, true, false, false];
        for (i, slot) in [
            &m.base_color_texture,
            &m.metallic_roughness_texture,
            &m.normal_texture,
            &m.occlusion_texture,
            &m.emissive_texture,
        ]
        .into_iter()
        .enumerate()
        {
            if let Some(slot) = slot {
                slots[i] = Some(slot.texture);
                uniform.maps[i].options[0] = slot.texcoord as u32;
            }
        }
        if let MaterialModel::Wavefront(classic) = &m.model {
            let illumination = classic.illumination.unwrap_or(2);
            if illumination > 2 {
                return Err(unsupported(
                    id,
                    "classic illumination models above 2 require reflection/refraction support",
                ));
            }
            if classic.dissolve_halo {
                return Err(unsupported(id, "view-dependent MTL halo dissolve"));
            }
            if classic.maps.contains_key(&MapKind::Displacement) {
                return Err(unsupported(
                    id,
                    "MTL displacement needs tessellation or explicit baking",
                ));
            }
            if classic.maps.contains_key(&MapKind::Normal)
                && classic.maps.contains_key(&MapKind::Bump)
            {
                return Err(unsupported(id, "simultaneous MTL bump and normal maps"));
            }
            let linear = |v: [f32; 3]| {
                if classic.color_space == MaterialColorSpace::Srgb {
                    v.map(crate::renderer::srgb_to_linear)
                } else {
                    v
                }
            };
            let specular = linear(classic.specular.unwrap_or([0.; 3]));
            let ambient = linear(classic.ambient.unwrap_or([0.; 3]));
            uniform.specular_exponent = [
                specular[0],
                specular[1],
                specular[2],
                classic.specular_exponent.unwrap_or(0.),
            ];
            uniform.ambient = [ambient[0], ambient[1], ambient[2], 0.];
            uniform.flags[0] = 1;
            uniform.flags[3] = u32::from(illumination);
            colors[1] = true;
            colors[3] = true;
            let normal = if classic.maps.contains_key(&MapKind::Normal) {
                MapKind::Normal
            } else {
                MapKind::Bump
            };
            for (i, kind) in [
                MapKind::Diffuse,
                MapKind::Specular,
                normal,
                MapKind::Ambient,
                MapKind::Emissive,
                MapKind::Opacity,
                MapKind::Exponent,
            ]
            .into_iter()
            .enumerate()
            {
                if let Some(map) = classic.maps.get(&kind) {
                    if map.offset[2] != 0. || map.scale[2] != 1. {
                        return Err(unsupported(id, "3D material map coordinates"));
                    }
                    let flip = source.obj_metadata().is_some_and(|m| m.options.flip_v);
                    uniform.maps[i].transform = [
                        map.offset[0],
                        if flip {
                            1. - map.scale[1] - map.offset[1]
                        } else {
                            map.offset[1]
                        },
                        map.scale[0],
                        map.scale[1],
                    ];
                    uniform.maps[i].options[1] = match map.channel {
                        None => 0,
                        Some('r') => 1,
                        Some('g') => 2,
                        Some('b') => 3,
                        Some('m') => 4,
                        Some('l') => 5,
                        _ => return Err(unsupported(id, "depth channel in a 2D material map")),
                    };
                    if i == 2 {
                        if map.channel.is_some() && kind == MapKind::Normal {
                            return Err(unsupported(id, "normal map requires RGB channels"));
                        }
                        uniform.flags[2] = if kind == MapKind::Bump { 2 } else { 1 };
                        uniform.parameters[1] = map.bump_multiplier
                            * if kind == MapKind::Bump {
                                source
                                    .obj_metadata()
                                    .map_or(1., |m| m.options.metres_per_unit)
                            } else {
                                1.
                            };
                    }
                    slots[i] = Some(map.texture);
                }
            }
        }
        let mut texture_ids = [0usize; MAPS];
        let mut samplers = Vec::new();
        for i in 0..MAPS {
            let texture = slots[i].map(|id| &source.textures()[id]);
            let fallback = if i == 2 {
                [128, 128, 255, 255]
            } else {
                [255; 4]
            };
            texture_ids[i] = images.get(
                context,
                source,
                ImageRequest {
                    image: texture.map(|t| t.image),
                    color: colors[i],
                    fallback,
                },
                limits,
                bytes,
            )?;
            let descriptor = sampler(
                texture
                    .and_then(|t| t.sampler)
                    .map(|s| &source.samplers()[s]),
            );
            samplers.push(context.device().create_sampler(&descriptor));
        }
        if uniform
            .base
            .iter()
            .chain(&uniform.emissive_metallic)
            .chain(&uniform.parameters)
            .chain(&uniform.specular_exponent)
            .chain(&uniform.ambient)
            .any(|v| !v.is_finite())
        {
            return Err(invalid(
                "material",
                id,
                "material conversion produced non-finite factors",
            ));
        }
        charge(bytes, std::mem::size_of::<Uniform>() as u64, limits)?;
        let buffer = context.buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("scene material constants"),
            contents: bytemuck::bytes_of(&uniform),
            usage: wgpu::BufferUsages::UNIFORM,
        })?;
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }];
        for i in 0..MAPS {
            entries.push(wgpu::BindGroupEntry {
                binding: 1 + i as u32 * 2,
                resource: wgpu::BindingResource::TextureView(images.view(texture_ids[i])),
            });
            entries.push(wgpu::BindGroupEntry {
                binding: 2 + i as u32 * 2,
                resource: wgpu::BindingResource::Sampler(&samplers[i]),
            });
        }
        let group = context
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("scene material"),
                layout,
                entries: &entries,
            });
        out.push(GpuMaterial {
            group,
            alpha: m.alpha,
            double_sided: m.double_sided,
            _uniform: buffer,
        });
    }
    Ok(out)
}

fn sampler(source: Option<&motion_scene::Sampler>) -> wgpu::SamplerDescriptor<'static> {
    let address = |mode| match mode {
        33071 => wgpu::AddressMode::ClampToEdge,
        33648 => wgpu::AddressMode::MirrorRepeat,
        _ => wgpu::AddressMode::Repeat,
    };
    let (min, mip, mips) = match source.and_then(|s| s.min_filter) {
        Some(9728) => (
            wgpu::FilterMode::Nearest,
            wgpu::MipmapFilterMode::Nearest,
            false,
        ),
        Some(9729) => (
            wgpu::FilterMode::Linear,
            wgpu::MipmapFilterMode::Nearest,
            false,
        ),
        Some(9984) => (
            wgpu::FilterMode::Nearest,
            wgpu::MipmapFilterMode::Nearest,
            true,
        ),
        Some(9985) => (
            wgpu::FilterMode::Linear,
            wgpu::MipmapFilterMode::Nearest,
            true,
        ),
        Some(9986) => (
            wgpu::FilterMode::Nearest,
            wgpu::MipmapFilterMode::Linear,
            true,
        ),
        _ => (
            wgpu::FilterMode::Linear,
            wgpu::MipmapFilterMode::Linear,
            true,
        ),
    };
    wgpu::SamplerDescriptor {
        label: Some("scene authored sampler"),
        address_mode_u: address(source.map_or(10497, |s| s.wrap_s)),
        address_mode_v: address(source.map_or(10497, |s| s.wrap_t)),
        mag_filter: if source.and_then(|s| s.mag_filter) == Some(9728) {
            wgpu::FilterMode::Nearest
        } else {
            wgpu::FilterMode::Linear
        },
        min_filter: min,
        mipmap_filter: mip,
        lod_max_clamp: if mips { 32. } else { 0. },
        ..Default::default()
    }
}
