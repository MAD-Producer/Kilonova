use super::*;
use crate::{mask_plan::MaskCache, Renderer, Scene};
use motion_core::{
    masks::LayerMask,
    vector::{PathNode, VectorPath},
    Layer, Project, Track,
};

mod admission_tests;
mod texture_admission;

fn admitted_gpu(renderer: &Renderer, layout: &wgpu::BindGroupLayout) -> MaskGpu {
    MaskGpu::new(&renderer.device, layout, renderer.allocation_budget().clone(),
        std::sync::Arc::new(crate::allocation_uploads::AllocationUploads::new(renderer.queue.clone()))).unwrap()
}

fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("mask reuse image layout"),
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
    })
}

fn project() -> Project {
    let mut p = Project::new(64, 64, 30, 180).unwrap();
    let mut layer = Layer::solid(1, "masked", [64.; 2], [32., 32., 0.], [1.; 4]);
    let mut mask = LayerMask::new(
        1,
        VectorPath {
            id: 1,
            closed: true,
            nodes: [[8., 8.], [56., 8.], [56., 56.], [8., 56.]]
                .into_iter()
                .enumerate()
                .map(|(i, p)| PathNode {
                    id: i as u64 + 1,
                    geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
                })
                .collect(),
        },
    );
    mask.feather.value = [4.; 2];
    layer.masks.push(mask);
    p.layers.push(layer);
    p
}

fn read(renderer: &Renderer, texture: &wgpu::Texture) -> Vec<u8> {
    let size = texture.size();
    let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mask reuse readback"),
        size: 256 * u64::from(size.height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(size.height),
            },
        },
        size,
    );
    renderer.submit_tracked(encoder.finish(), |_| {});
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            send.send(result).unwrap();
        });
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    receive.recv().unwrap().unwrap();
    let pixels = buffer
        .slice(..)
        .get_mapped_range()
        .unwrap()
        .chunks_exact(256)
        .flat_map(|row| row[..size.width as usize].iter().copied())
        .collect();
    buffer.unmap();
    renderer.check_health().unwrap();
    pixels
}

#[test]
fn animated_feathered_masks_reuse_actual_bindings_with_stable_memory_and_pixels() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let layout = layout(&renderer.device);
    let mut gpu = admitted_gpu(&renderer, &layout);
    let mut p = project();
    let mut scene = Scene::new(&p);
    let mut cache = MaskCache::default();
    let mut images = HashMap::new();
    let mut bytes = 0;
    let mut bindings = HashSet::new();
    for frame in 0..60 {
        let (opacity, expected) = if frame % 2 == 0 {
            (35., 89)
        } else {
            (65., 166)
        };
        p.layers[0].masks[0].opacity.value = opacity;
        scene
            .sample(&p, frame as f64, None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let masks = cache.build(&scene, 1., false).unwrap();
        gpu.input_binding_observations.clear();
        let mut encoder = renderer.device.create_command_encoder(&Default::default());
        assert!(gpu
            .prepare(
                &renderer.device,
                &renderer.queue,
                &mut encoder,
                &layout,
                &scene,
                &masks,
                &mut images,
                &mut bytes,
                0,
                128 << 20,
            )
            .unwrap());
        renderer.submit_tracked(encoder.finish(), |_| {});
        assert_eq!(gpu.input_binding_observations.len(), 3);
        bindings.extend(gpu.input_binding_observations.iter().cloned());
        let pixels = read(&renderer, &images[&TextureKey::Mask(1)]._texture);
        assert!(pixels[32 * 64 + 32].abs_diff(expected) <= 2);
        assert_eq!(pixels[32 * 64 + 2], 0);
        assert_eq!(bytes, 64 * 64);
        assert_eq!(gpu.scratch_bytes(), 64 * 64 * 2);
        assert_eq!(gpu.uniforms.len(), 3);
    }
    assert_eq!(
        gpu.uploads, 60,
        "every frame must actually re-encode the mask"
    );
    assert_eq!(
        bindings.len(),
        3,
        "animated masks rebuilt input bindings across frames"
    );
}

fn filled(renderer: &Renderer, value: u8) -> wgpu::Texture {
    let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mask binding identity fixture"),
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    renderer.queue.write_texture(
        texture.as_image_copy(),
        &vec![value; 64 * 64],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(64),
            rows_per_image: Some(64),
        },
        texture.size(),
    );
    texture
}

#[test]
fn input_and_source_view_identity_control_rebinding_but_target_and_pipeline_do_not() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let layout = layout(&renderer.device);
    let mut gpu = admitted_gpu(&renderer, &layout);
    let textures = [204, 51, 102, 153].map(|v| filled(&renderer, v));
    let views = textures
        .each_ref()
        .map(|t| t.create_view(&Default::default()));
    let outputs = [filled(&renderer, 0), filled(&renderer, 0)];
    let targets = outputs
        .each_ref()
        .map(|t| t.create_view(&Default::default()));
    let mut bindings = Vec::new();
    for (input, source, target, pipeline, expected) in [
        (0, 1, 0, 1, 41),
        (0, 1, 0, 1, 41),
        (0, 2, 0, 1, 82),
        (3, 2, 0, 1, 61),
        (3, 2, 0, 0, 153),
        (3, 2, 1, 1, 61),
    ] {
        let mut u = mask_plan::gaussian_uniform(64, 64, 0, 4.);
        if pipeline == 1 {
            u = mask_plan::uniform(64, 64);
            u.params[0] = [3., 0., 1., -1.]; // Multiply input and source coverage.
        }
        gpu.input_binding_observations.clear();
        let mut encoder = renderer.device.create_command_encoder(&Default::default());
        gpu.filter(
            &renderer.device,
            &renderer.queue,
            &mut encoder,
            &mut 0,
            pipeline,
            u,
            &views[input].clone(),
            &views[source].clone(),
            &targets[target],
        ).unwrap();
        renderer.submit_tracked(encoder.finish(), |_| {});
        let pixels = read(&renderer, &outputs[target]);
        assert!(
            pixels[32 * 64 + 32].abs_diff(expected) <= 2,
            "input={input} source={source} target={target} pipeline={pipeline}: actual={} expected={expected}",
            pixels[32 * 64 + 32]
        );
        bindings.push(gpu.input_binding_observations[0].clone());
        assert_eq!(gpu.uniforms.len(), 1);
    }
    assert_eq!(
        bindings[0], bindings[1],
        "cloned handles must reuse the binding"
    );
    assert_ne!(bindings[1], bindings[2], "a new source view must rebind");
    assert_ne!(bindings[2], bindings[3], "a new input view must rebind");
    assert_eq!(
        bindings[3], bindings[4],
        "pipeline is not an input resource"
    );
    assert_eq!(bindings[4], bindings[5], "target is not an input resource");
}

#[test]
fn resize_slot_shrink_and_empty_masks_release_cached_views_and_recover() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let layout = layout(&renderer.device);
    let mut gpu = admitted_gpu(&renderer, &layout);
    let mut p = project();
    let mask = p.layers[0].masks[0].clone();
    let mut scene = Scene::new(&p);
    let mut cache = MaskCache::default();
    let mut images = HashMap::new();
    let mut bytes = 0;
    let mut previous = Vec::new();
    for (width, feather, active, cached) in [
        (64, 4., true, 3),
        (96, 4., true, 3),
        (96, 0., true, 1),
        (96, 0., false, 0),
        (96, 0., true, 1),
    ] {
        p.width = width;
        p.layers[0].size[0] = width as f32;
        p.layers[0].masks = if active { vec![mask.clone()] } else { vec![] };
        if active {
            p.layers[0].masks[0].feather.value = [feather; 2];
        }
        scene
            .sample(&p, 0., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let masks = cache.build(&scene, 1., false).unwrap();
        let mut encoder = renderer.device.create_command_encoder(&Default::default());
        assert!(gpu
            .prepare(
                &renderer.device,
                &renderer.queue,
                &mut encoder,
                &layout,
                &scene,
                &masks,
                &mut images,
                &mut bytes,
                0,
                128 << 20,
            )
            .unwrap());
        renderer.submit_tracked(encoder.finish(), |_| {});
        let observed = std::mem::take(&mut gpu.input_binding_observations);
        assert_eq!(observed.len(), cached);
        assert_eq!(
            gpu.uniforms.len(),
            3,
            "retain the existing uniform high-water pool"
        );
        assert_eq!(
            gpu.uniforms.iter().filter(|s| s.inputs.is_some()).count(),
            cached
        );
        for binding in gpu.uniforms.iter().filter_map(|s| s.inputs.as_ref()) {
            assert!(gpu.scratch.iter().any(|s| s.view == binding.input));
            assert!(gpu.scratch.iter().any(|s| s.view == binding.source));
        }
        if active {
            assert!(
                observed.iter().all(|b| !previous.contains(b)),
                "retired scratch views were reused"
            );
            previous = observed;
            let pixels = read(&renderer, &images[&TextureKey::Mask(1)]._texture);
            assert_eq!(pixels[32 * width as usize + 32], 255);
            assert_eq!(bytes, u64::from(width) * 64);
            assert_eq!(
                gpu.scratch_bytes(),
                u64::from(width) * 64 * if feather > 0. { 2 } else { 1 }
            );
        } else {
            assert!(images.is_empty());
            assert_eq!(bytes, 0);
            assert_eq!(gpu.scratch_bytes(), 0);
        }
        let mut ledger = crate::TextureResourceLedger::default();
        gpu.collect_textures(&mut ledger);
        assert_eq!(ledger.total_known_bytes(), gpu.scratch_bytes());
    }
    gpu.input_binding_observations.clear();
    gpu.clear();
    gpu.clear();
    assert!(gpu.uniforms.iter().all(|s| s.inputs.is_none()));
    assert_eq!(gpu.scratch_bytes(), 0);
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    renderer.check_health().unwrap();
}
