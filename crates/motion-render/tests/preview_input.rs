use motion_render::{preview_input::PreviewInputTexture, ChromaLayout, Renderer, Yuv420Frame};
fn renderer() -> Renderer {
    pollster::block_on(Renderer::headless()).unwrap()
}
fn pixels(renderer: &Renderer, input: &PreviewInputTexture) -> Vec<u8> {
    let (width, height) = input.dimensions();
    let target = renderer.capture_target(width, height).unwrap();
    let device = &renderer.device;
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
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
    let sampler = device.create_sampler(&Default::default());
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(input.view().unwrap()),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    let shader=device.create_shader_module(wgpu::ShaderModuleDescriptor{label:None,source:wgpu::ShaderSource::Wgsl(r#"
struct V{@builtin(position) p:vec4<f32>,@location(0) uv:vec2<f32>}
@group(0) @binding(0) var t:texture_2d<f32>;
@group(0) @binding(1) var s:sampler;
@vertex fn vs(@builtin(vertex_index) i:u32)->V{let p=vec2(f32((i<<1u)&2u)*2.0-1.0,f32(i&2u)*2.0-1.0);var v:V;v.p=vec4(p,0.0,1.0);v.uv=vec2(p.x*0.5+0.5,0.5-p.y*0.5);return v;}
@fragment fn fs(v:V)->@location(0) vec4<f32>{return textureSample(t,s,v.uv);}
"#.into())});
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            multiview_mask: None,
            label: None,
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
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
    renderer.queue.submit(Some(encoder.finish()));
    renderer.read_target(&target).unwrap()
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn rgba_inputs_resize_preserve_aspect_and_share_linear_premultiplied_contract() {
    let renderer = renderer();
    let mut texture =
        PreviewInputTexture::new(renderer.device.clone(), renderer.queue.clone(), 1 << 20, renderer.allocation_budget().clone()).unwrap();
    texture
        .upload_rgba(2, 6, &[188, 0, 0, 128].repeat(12), true, 3)
        .unwrap();
    assert_eq!(texture.dimensions(), (1, 3));
    let premult = pixels(&renderer, &texture);
    texture
        .upload_rgba(2, 6, &[255, 0, 0, 128].repeat(12), false, 3)
        .unwrap();
    let straight = pixels(&renderer, &texture);
    assert!(straight
        .iter()
        .zip(&premult)
        .all(|(a, b)| a.abs_diff(*b) <= 1));
    // read_target converts the premultiplied GPU result to straight PNG bytes.
    assert!(straight[0] > 250);
    assert_eq!(straight[3], 128);
    texture
        .upload_rgba(2, 1, &[255, 0, 0, 255, 0, 0, 255, 0], false, 1)
        .unwrap();
    let edge = pixels(&renderer, &texture);
    assert!(
        edge[0] > 250 && edge[2] < 2,
        "transparent blue must not bleed into opaque red when resizing"
    );
    assert!(edge[3].abs_diff(128) <= 1);
    texture.transparent().unwrap();
    assert_eq!(texture.dimensions(), (1, 1));
    assert_eq!(pixels(&renderer, &texture), vec![0; 4]);
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn yuv_rotation_and_bounded_gpu_resize_use_correct_source_coordinates() {
    let renderer = renderer();
    let mut texture =
        PreviewInputTexture::new(renderer.device.clone(), renderer.queue.clone(), 1 << 20, renderer.allocation_budget().clone()).unwrap();
    let frame = Yuv420Frame {
        width: 4,
        height: 2,
        rotation: 90,
        standard: 1,
        range: 2,
        phase: [0, 0],
        y: vec![16, 40, 80, 120, 160, 200, 220, 235],
        uv: vec![128; 4],
        chroma_layout: ChromaLayout::Uv,
    };
    let reference = frame.to_rgba().unwrap();
    texture.upload_yuv(&frame, 4).unwrap();
    let full = pixels(&renderer, &texture);
    assert_eq!(texture.dimensions(), (2, 4));
    assert!(full
        .iter()
        .zip(&reference)
        .all(|(a, b)| a.abs_diff(*b) <= 1));
    texture.upload_yuv(&frame, 2).unwrap();
    assert_eq!(texture.dimensions(), (1, 2));
    let resized = pixels(&renderer, &texture);
    for (index, source_row) in [1usize, 3].into_iter().enumerate() {
        let expected = &reference[(source_row * 2 + 1) * 4..(source_row * 2 + 2) * 4];
        assert!(resized[index * 4..index * 4 + 4]
            .iter()
            .zip(expected)
            .all(|(a, b)| a.abs_diff(*b) <= 1));
    }
    let count = texture.memory_bytes();
    texture.upload_yuv(&frame, 2).unwrap();
    assert_eq!(texture.memory_bytes(), count);
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn invalid_upload_or_budget_rejection_preserves_last_valid_input() {
    let renderer = renderer();
    let mut texture =
        PreviewInputTexture::new(renderer.device.clone(), renderer.queue.clone(), 320, renderer.allocation_budget().clone()).unwrap();
    texture
        .upload_rgba(8, 8, &[188, 0, 0, 128].repeat(64), true, 8)
        .unwrap();
    let valid = pixels(&renderer, &texture);
    assert!(texture
        .upload_rgba(8, 8, &[255, 0, 0, 128].repeat(64), false, 8)
        .is_err());
    assert!(texture.upload_rgba(8, 8, &[0; 4], true, 8).is_err());
    assert_eq!(pixels(&renderer, &texture), valid);
    assert_eq!(texture.memory_bytes(), 256);
}

const SYSTEM_BYTES: u64 = motion_render::FIXED_RENDERER_ALLOCATION_BYTES;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn authoring_constructor_and_renderer_targets_share_the_same_combined_allowance() {
    let budget = motion_render::AllocationBudget::new(SYSTEM_BYTES + 15).unwrap();
    let renderer = pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    let error = PreviewInputTexture::new(renderer.device.clone(), renderer.queue.clone(), 1024, budget.clone()).err().unwrap();
    assert_eq!(error.resource_rejection(), Some(&motion_render::ResourceRejection::Budget {
        scope: motion_render::ResourceBudgetScope::RenderAllocations, required_bytes: SYSTEM_BYTES + 16, budget_bytes: SYSTEM_BYTES + 15,
    }));
    assert!(error.to_string().contains("buffer size=16 phase=allocate"));
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES);
    assert_eq!(budget.info().unwrap().live_allocations, 6);
    budget.set_limit(SYSTEM_BYTES + 16).unwrap();
    let mut input = PreviewInputTexture::new(renderer.device.clone(), renderer.queue.clone(), 1024, budget.clone()).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES + 16);
    let error = input.upload_rgba(1, 1, &[255; 4], true, 1).unwrap_err();
    assert_eq!(error.resource_rejection(), Some(&motion_render::ResourceRejection::Budget {
        scope: motion_render::ResourceBudgetScope::RenderAllocations, required_bytes: SYSTEM_BYTES + 20, budget_bytes: SYSTEM_BYTES + 16,
    }));
    assert!(input.view().is_none());
    assert_eq!(input.dimensions(), (0, 0));
    assert!(renderer.render_target(1, 1).is_err());
    drop(input);
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES);
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn cloned_authoring_views_keep_admission_after_teardown_and_completed_pending_writes() {
    let budget = motion_render::AllocationBudget::new(SYSTEM_BYTES + 32).unwrap();
    for _ in 0..3 {
        let renderer = pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
        let mut input = PreviewInputTexture::new(renderer.device.clone(), renderer.queue.clone(), 1024, budget.clone()).unwrap();
        input.upload_rgba(2, 2, &[255; 16], true, 2).unwrap();
        let view = input.view().unwrap().clone();
        assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES + 32);
        let device = renderer.device.clone();
        drop(input);
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES + 16);
        drop(view);
        assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES);
        renderer.check_health().unwrap();
        drop(renderer);
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        assert_eq!(budget.info().unwrap().admitted_bytes, 0);
        assert_eq!(budget.info().unwrap().live_allocations, 0);
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn combined_authoring_refusals_preserve_real_pixels_then_recover_with_admitted_yuv_planes() {
    let budget = motion_render::AllocationBudget::new(SYSTEM_BYTES + 4092).unwrap();
    let renderer = pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    let mut input = PreviewInputTexture::new(renderer.device.clone(), renderer.queue.clone(), 4096, budget.clone()).unwrap();
    input.upload_rgba(2, 2, &[0, 255, 0, 255].repeat(4), true, 2).unwrap();
    let valid = pixels(&renderer, &input);
    assert_eq!(valid, [0, 255, 0, 255].repeat(4));
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES + 32);
    budget.set_limit(SYSTEM_BYTES + 95).unwrap();
    let error = input.upload_rgba(4, 4, &[255; 64], false, 2).unwrap_err();
    assert_eq!(error.resource_rejection(), Some(&motion_render::ResourceRejection::Budget {
        scope: motion_render::ResourceBudgetScope::RenderAllocations, required_bytes: SYSTEM_BYTES + 96, budget_bytes: SYSTEM_BYTES + 95,
    }));
    assert!(error.to_string().contains("phase=rgba_upload"));
    assert_eq!(input.dimensions(), (2, 2));
    assert_eq!(input.memory_bytes(), 16);
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES + 32);
    budget.set_limit(SYSTEM_BYTES + 4092).unwrap();
    assert_eq!(pixels(&renderer, &input), valid);
    let frame = Yuv420Frame {
        width: 2, height: 2, rotation: 0, standard: 1, range: 2,
        phase: [0, 0], chroma_layout: ChromaLayout::Uv, y: vec![128; 4], uv: vec![128; 2],
    };
    budget.set_limit(SYSTEM_BYTES + 85).unwrap();
    let error = input.upload_yuv(&frame, 2).unwrap_err();
    assert_eq!(error.resource_rejection(), Some(&motion_render::ResourceRejection::Budget {
        scope: motion_render::ResourceBudgetScope::RenderAllocations, required_bytes: SYSTEM_BYTES + 86, budget_bytes: SYSTEM_BYTES + 85,
    }));
    for context in ["phase=yuv_upload", "buffer size=48 phase=allocate"] {
        assert!(error.to_string().contains(context), "{error}");
    }
    assert_eq!(input.dimensions(), (2, 2));
    assert_eq!(input.memory_bytes(), 16);
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES + 32);
    budget.set_limit(SYSTEM_BYTES + 4092).unwrap();
    assert_eq!(pixels(&renderer, &input), valid);
    input.upload_yuv(&frame, 2).unwrap();
    let actual = pixels(&renderer, &input);
    assert!(actual.iter().zip(frame.to_rgba().unwrap()).all(|(a,b)| a.abs_diff(b) <= 1));
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES + 86);
    assert_eq!(input.memory_bytes(), 22, "texture subtotal excludes the two uniform buffers");
    let error = input.upload_rgba(2, 2, &[0; 4], false, 2).unwrap_err();
    assert!(error.resource_rejection().is_none(), "malformed pixels are not a budget refusal");
    assert_eq!(pixels(&renderer, &input), actual);
    drop(input);
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES);
    renderer.check_health().unwrap();
}
