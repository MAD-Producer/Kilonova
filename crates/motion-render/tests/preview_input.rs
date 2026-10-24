use motion_render::{preview_input::PreviewInputTexture, ChromaLayout, Renderer, Yuv420Frame};
fn renderer() -> Renderer {
    pollster::block_on(Renderer::headless()).unwrap()
}

fn profiled_renderer(budget: motion_render::AllocationBudget) -> Renderer {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    pollster::block_on(Renderer::new_profiled_with_allocation_budget(
        &instance,
        None,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        true,
        budget,
    ))
    .unwrap()
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn input_command_timers_keep_independent_owners_across_shared_budget_devices_and_restarts() {
    use motion_render::{
        gpu_commands::CommandStage, AllocationBudget, AllocationFrameOutcome, FrameOperation,
    };
    fn assert_send<T: Send>() {}
    assert_send::<PreviewInputTexture>();
    let budget = AllocationBudget::default();
    let mut first = profiled_renderer(budget.clone());
    let second = profiled_renderer(budget.clone());
    let mut a = PreviewInputTexture::new(
        first.device.clone(),
        first.queue.clone(),
        1 << 20,
        budget.clone(),
    )
    .unwrap();
    let mut b = PreviewInputTexture::new(
        second.device.clone(),
        second.queue.clone(),
        1 << 20,
        budget.clone(),
    )
    .unwrap();
    assert!(a.command_observer().is_none());
    assert!(b.command_observer().is_none());
    let Some(main) = first.start_command_profiling(2).unwrap() else {
        eprintln!("GPU_INPUT_OWNERS available=false; no actual owner acceptance");
        return;
    };
    let before = budget.info().unwrap().admitted_bytes;
    let a_old = a.start_command_profiling(2).unwrap().unwrap();
    let slot_bytes = wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT + 16;
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        before + 2 * slot_bytes
    );
    let b_observer = b.start_command_profiling(2).unwrap().unwrap();
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        before + 4 * slot_bytes
    );
    let a_epoch = a_old.snapshot().commands.epoch;
    assert_ne!(a_epoch, b_observer.snapshot().commands.epoch);
    assert_ne!(a_epoch, main.snapshot().commands.epoch);
    let frame = budget
        .begin_frame_at(711, FrameOperation::NodeCapture, 1.25)
        .unwrap()
        .unwrap();
    a.upload_rgba_with_context(
        4,
        4,
        &[255, 0, 0, 128].repeat(16),
        false,
        2,
        Some(motion_render::gpu_commands::InputCommandContext::image(
            501, 7, 1.25, 42,
        )),
    )
    .unwrap();
    let source = Yuv420Frame {
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
    b.upload_yuv_with_context(
        &source,
        4,
        Some(motion_render::gpu_commands::InputCommandContext::layer(
            502,
            7,
            1.25,
            23,
            "source-comp",
        )),
    )
    .unwrap();
    frame.finish(AllocationFrameOutcome::Rendered);
    assert_eq!(main.snapshot().commands.counts.started, 0);
    for observer in [&a_old, &b_observer] {
        let snapshot = observer.snapshot();
        assert_eq!(snapshot.commands.active.len(), 1);
        let command = &snapshot.commands.active[0];
        assert_eq!(command.stage, CommandStage::AuthoringInput);
        assert_eq!(command.contexts.len(), 1);
        assert_eq!(command.contexts[0].request, Some(711));
        assert_eq!(command.contexts[0].source_frame, Some(1.25));
    }
    let original_limit = budget.info().unwrap().budget_bytes;
    let admitted = budget.info().unwrap().admitted_bytes;
    budget.set_limit(admitted).unwrap();
    let primary = a.start_command_profiling(1).err().unwrap();
    assert!(primary.resource_rejection().is_some());
    assert_eq!(
        a.command_observer().unwrap().snapshot().commands.epoch,
        a_epoch
    );
    assert_eq!(budget.info().unwrap().admitted_bytes, admitted);
    budget.set_limit(original_limit).unwrap();
    assert!(a.start_command_profiling(0).is_err());
    assert_eq!(
        a.stop_command_profiling()
            .unwrap()
            .snapshot()
            .commands
            .epoch,
        a_epoch
    );
    let a_new = a.start_command_profiling(1).unwrap().unwrap();
    assert_ne!(a_new.snapshot().commands.epoch, a_epoch);
    // The old real mapping completes only after the new owner epoch exists.
    first
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    second
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    for observer in [&a_old, &b_observer] {
        let snapshot = observer.snapshot();
        assert_eq!(snapshot.commands.counts.completed, 1);
        assert_eq!(snapshot.readback.valid_samples, 1);
        assert!(snapshot.commands.recent[0]
            .timestamp_words
            .is_some_and(|w| w[1] > w[0]));
        assert_eq!(snapshot.commands.counts.omitted_slots, 0);
    }
    assert_eq!(a_new.snapshot().commands.counts.started, 0);
    let a_record = a_old.snapshot().commands.recent.pop().unwrap();
    let b_record = b_observer.snapshot().commands.recent.pop().unwrap();
    let a_input = a_record.input.unwrap();
    let b_input = b_record.input.unwrap();
    assert_eq!(
        (a_input.sequence, a_input.asset, a_input.revision),
        (501, Some(42), 7)
    );
    assert_eq!(
        (b_input.sequence, b_input.object, b_input.revision),
        (502, Some(23), 7)
    );
    assert_eq!(b_input.composition.as_deref(), Some("source-comp"));
    assert_eq!(a_record.contexts[0].request, Some(711));
    assert_eq!(b_record.contexts[0].request, Some(711));
    assert_eq!(a.dimensions(), (2, 2));
    let rgba = pixels(&first, &a);
    assert!(rgba
        .chunks_exact(4)
        .all(|pixel| pixel[0] > 250 && pixel[1] < 2 && pixel[2] < 2 && pixel[3] == 128));
    let yuv = pixels(&second, &b);
    let expected = source.to_rgba().unwrap();
    assert!(yuv.iter().zip(&expected).all(|(a, b)| a.abs_diff(*b) <= 1));
    // Direct premultiplied queue writes retain the existing no-encoder route.
    let scopes = budget.info().unwrap().encoder_scopes;
    a.upload_rgba(2, 2, &[0, 255, 0, 255].repeat(4), true, 2)
        .unwrap();
    assert_eq!(budget.info().unwrap().encoder_scopes, scopes);
    assert_eq!(a_new.snapshot().commands.counts.started, 0);
    assert_eq!(pixels(&first, &a), [0, 255, 0, 255].repeat(4));
    assert!(main
        .snapshot()
        .commands
        .recent
        .iter()
        .all(|command| command.stage != CommandStage::AuthoringInput));
    eprintln!(
        "GPU_INPUT_OWNERS {}",
        serde_json::json!({
            "available":true,"actualDevices":2,"distinctEpochs":[main.snapshot().commands.epoch,a_epoch,b_observer.snapshot().commands.epoch,a_new.snapshot().commands.epoch],
            "actualInputCompletions":2,"actualPixelsChecked":4+8+4,"originalBudgetErrorPreserved":true,
            "directUploadAddedEncoder":false,"scope":"actual RGBA/YUV conversion and pixels on two independent devices sharing admission; queue writes are not timed intervals"
        })
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn input_timer_observer_retains_metadata_without_retaining_native_query_allocations() {
    let renderer = profiled_renderer(Default::default());
    let budget = renderer.allocation_budget().clone();
    let before = budget.info().unwrap().admitted_bytes;
    let mut input = PreviewInputTexture::new(
        renderer.device.clone(),
        renderer.queue.clone(),
        1 << 20,
        budget.clone(),
    )
    .unwrap();
    let Some(observer) = input.start_command_profiling(1).unwrap() else {
        eprintln!("GPU_INPUT_RETIRE available=false; no actual retirement acceptance");
        return;
    };
    input
        .upload_rgba(4, 4, &[64, 128, 192, 255].repeat(16), false, 2)
        .unwrap();
    assert_eq!(observer.snapshot().commands.active.len(), 1);
    drop(input);
    assert!(budget.info().unwrap().admitted_bytes > before);
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    renderer.check_health().unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, before);
    let result = observer.snapshot();
    assert_eq!(result.commands.counts.completed, 1);
    assert_eq!(result.readback.valid_samples, 1);
    assert!(result.commands.active.is_empty());
    assert!(result.commands.recent[0]
        .timestamp_words
        .is_some_and(|w| w[1] > w[0]));
    eprintln!(
        "GPU_INPUT_RETIRE {}",
        serde_json::json!({
            "available":true,"actualInputCompletions":1,"admissionBefore":before,
            "admissionAfterActualPoll":budget.info().unwrap().admitted_bytes,
            "scope":"exact original queue completion and timestamp mapping after input owner destruction; logical admission, not physical residency"
        })
    );
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
