use super::*;
use motion_render::scene_gpu::{ScenePass, ScenePassFrame, ScenePassInstance};

fn source(camera: Value) -> Arc<motion_scene::Scene> {
    fixture_with(
        vec![emissive([1.; 3], 1., "OPAQUE")],
        &[(0., 0)],
        None,
        false,
        |doc, _| {
            doc["cameras"] = json!([camera]);
            let nodes = doc["nodes"].as_array_mut().unwrap();
            nodes.push(json!({"camera":0,"translation":[0,0,3]}));
            let id = nodes.len() - 1;
            doc["scenes"][0]["nodes"]
                .as_array_mut()
                .unwrap()
                .push(json!(id));
        },
    )
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn imported_camera_and_model_instance_move_together_without_changing_pixels_or_depth() {
    let prefix = Mat4::from_cols(glam::Vec4::Y, -glam::Vec4::X, glam::Vec4::Z,
        glam::Vec4::new(8., -4., 16., 1.));
    for precision in [ChannelPrecision::Unorm8, ChannelPrecision::Unorm16,
        ChannelPrecision::Float16, ChannelPrecision::Float32] {
        let renderer = gpu_renderer(precision);
        let target = renderer.render_target(SIZE, SIZE).unwrap();
        for camera in [json!({"type":"perspective","perspective":{"yfov":1.,"znear":0.1,"zfar":10.}}),
            json!({"type":"orthographic","orthographic":{"xmag":1.,"ymag":0.5,"znear":0.1,"zfar":10.}})] {
            let scene = source(camera);
            let base = scene.camera_view(0, 1, 1., None, [SIZE, SIZE]).unwrap();
            let placed = scene.camera_view_in_world(0, 1, 1., None, [SIZE, SIZE], prefix).unwrap();
            let mut gpu = GpuScene::upload(&renderer, REQUEST, scene, SceneGpuLimits::default()).unwrap();
            let mut pass = ScenePass::new(&renderer, 8).unwrap();
            for samples in [1, 4] {
                let mut item = ScenePassInstance::new(0, REQUEST, 0, 1.);
                item.lights = &[];
                item.ambient = [0.; 3];
                let mut items = [item];
                let mut frame = ScenePassFrame::from_imported_camera(&base);
                frame.samples = samples;
                pass.draw(&renderer, REQUEST, &target, &mut [&mut gpu], &items, &frame).unwrap();
                let original = pixels(&renderer, &target, precision).pixels;
                assert!(original.iter().any(|&b| b != 0));
                let original_depth = camera_depth_samples(&renderer, pass.depth().unwrap());
                items[0].world = prefix;
                let mut frame = ScenePassFrame::from_imported_camera(&placed);
                frame.samples = samples;
                pass.draw(&renderer, REQUEST, &target, &mut [&mut gpu], &items, &frame).unwrap();
                assert_eq!(pixels(&renderer, &target, precision).pixels, original,
                    "camera/model covariance precision={precision:?} samples={samples}");
                let depth = camera_depth_samples(&renderer, pass.depth().unwrap());
                assert!(depth.iter().zip(original_depth).all(|(a, b)| (a - b).abs() < 1e-6));
            }
        }
    }
}

fn camera_depth_samples(renderer: &Renderer, texture: &motion_render::AdmittedTexture) -> Vec<f32> {
    if texture.sample_count() == 1 { return super::shared::depth(renderer, texture); }
    assert_eq!(texture.sample_count(), 4);
    assert!(texture.usage().contains(wgpu::TextureUsages::TEXTURE_BINDING));
    let size = u64::from(SIZE * SIZE * 4 * 4);
    let context = renderer.allocation_context();
    let output = context.buffer(&wgpu::BufferDescriptor {
        label: Some("all four native camera depth samples"), size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    }).unwrap();
    let readback = context.buffer(&wgpu::BufferDescriptor {
        label: Some("camera depth sample readback"), size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    }).unwrap();
    let shader = renderer.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("independent per-sample depth readback"),
        source: wgpu::ShaderSource::Wgsl(r#"
@group(0) @binding(0) var input_depth: texture_depth_multisampled_2d;
@group(0) @binding(1) var<storage, read_write> output_depth: array<f32>;
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(input_depth);
    if id.x >= size.x || id.y >= size.y { return; }
    for (var s = 0u; s < 4u; s += 1u) {
        output_depth[(id.y * size.x + id.x) * 4u + s] = textureLoad(input_depth, vec2<i32>(id.xy), i32(s));
    }
}
"#.into()),
    });
    let pipeline = renderer.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("camera depth samples"), layout: None, module: &shader,
        entry_point: Some("main"), compilation_options: Default::default(), cache: None,
    });
    let view = texture.create_view(&Default::default());
    let group = renderer.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("camera native depth samples"), layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
            wgpu::BindGroupEntry { binding: 1, resource: output.as_entire_binding() },
        ],
    });
    let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(SIZE.div_ceil(8), SIZE.div_ceil(8), 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, size);
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    readback.slice(..).map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let mapped = readback.slice(..).get_mapped_range().unwrap();
    let samples = mapped.chunks_exact(4).map(|v| f32::from_le_bytes(v.try_into().unwrap())).collect();
    drop(mapped);
    readback.unmap();
    samples
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn imported_orthographic_camera_letterboxes_single_and_shared_native_draws() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let a = renderer.render_target(SIZE, SIZE).unwrap();
    let b = renderer.render_target(SIZE, SIZE).unwrap();
    let scene = source(json!({"type":"orthographic","orthographic":{
        "xmag":1.,"ymag":0.5,"znear":0.1,"zfar":10.}}));
    let view = scene.camera_view(0, 1, 1., None, [SIZE, SIZE]).unwrap();
    let mut gpu = GpuScene::upload(&renderer, REQUEST, scene, SceneGpuLimits::default()).unwrap();
    let mut frame = SceneFrame::from_imported_camera(&view, 0, 1.);
    frame.lights = &[];
    frame.ambient = [0.; 3];
    gpu.draw(&renderer, REQUEST, &a, &frame).unwrap();
    let first = pixels(&renderer, &a, ChannelPrecision::Unorm8);
    assert_eq!(center8(&first), [255; 4]);
    for y in [0, 10, SIZE - 11, SIZE - 1] {
        assert!(
            first.pixels[(y * SIZE * 4) as usize..((y + 1) * SIZE * 4) as usize]
                .iter()
                .all(|v| *v == 0),
            "authored aspect must leave actual transparent letterbox rows"
        );
    }
    let mut pass = ScenePass::new(&renderer, 8).unwrap();
    let mut item = ScenePassInstance::new(0, REQUEST, 0, 1.);
    item.lights = &[];
    item.ambient = [0.; 3];
    let mut shared = ScenePassFrame::from_imported_camera(&view);
    pass.draw(&renderer, REQUEST, &b, &mut [&mut gpu], &[item], &shared)
        .unwrap();
    assert_eq!(
        pixels(&renderer, &b, ChannelPrecision::Unorm8).pixels,
        first.pixels
    );
    let values = super::shared::depth(&renderer, pass.depth().unwrap());
    assert!(
        (values[(SIZE / 2 * SIZE + SIZE / 2) as usize] - (3. - 0.1) / (10. - 0.1)).abs() < 1e-6
    );
    assert_eq!(
        values[(5 * SIZE + SIZE / 2) as usize],
        1.,
        "letterbox must not write depth"
    );
    frame.viewport = Some([0., 0., SIZE as f32 + 1., SIZE as f32]);
    assert!(gpu
        .draw(&renderer, REQUEST, &a, &frame)
        .unwrap_err()
        .to_string()
        .contains("viewport"));
    assert_eq!(
        pixels(&renderer, &a, ChannelPrecision::Unorm8).pixels,
        first.pixels
    );
    shared.viewport = Some([f32::NAN, 0., 1., 1.]);
    assert!(pass
        .draw(&renderer, REQUEST, &b, &mut [], &[], &shared)
        .unwrap_err()
        .to_string()
        .contains("viewport"));
    assert_eq!(
        pixels(&renderer, &b, ChannelPrecision::Unorm8).pixels,
        first.pixels
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn imported_finite_and_infinite_perspective_cameras_drive_real_hardware_depth() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let mut pass = ScenePass::new(&renderer, 8).unwrap();
    for far in [Some(10.), None] {
        let mut camera = json!({"type":"perspective","perspective":{"yfov":1.,"znear":0.1}});
        if let Some(far) = far {
            camera["perspective"]["zfar"] = json!(far);
        }
        let scene = source(camera);
        let view = scene.camera_view(0, 1, 1., None, [SIZE, SIZE]).unwrap();
        let mut gpu =
            GpuScene::upload(&renderer, REQUEST, scene, SceneGpuLimits::default()).unwrap();
        let mut item = ScenePassInstance::new(0, REQUEST, 0, 1.);
        item.lights = &[];
        item.ambient = [0.; 3];
        pass.draw(
            &renderer,
            REQUEST,
            &target,
            &mut [&mut gpu],
            &[item],
            &ScenePassFrame::from_imported_camera(&view),
        )
        .unwrap();
        assert_eq!(
            center8(&pixels(&renderer, &target, ChannelPrecision::Unorm8)),
            [255; 4]
        );
        let values = super::shared::depth(&renderer, pass.depth().unwrap());
        let expected = far.map_or(1. - 0.1 / 3., |far| far / (far - 0.1) * (1. - 0.1 / 3.));
        assert!((values[(SIZE / 2 * SIZE + SIZE / 2) as usize] - expected).abs() < 1e-6);
    }
}
