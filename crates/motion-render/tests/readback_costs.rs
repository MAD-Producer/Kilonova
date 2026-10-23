use motion_core::{Layer, Project};
use motion_render::{
    GpuTimer, ImageReadbackInfo, Presenter, Renderer, Scene, TimestampReadbackInfo,
};

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn real_image_readbacks_separate_payload_padding_failure_and_new_renderer_lifetimes() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(3, 2).unwrap();
    let mut project = Project::new(3, 2, 30, 2).unwrap();
    project.background = [1., 0., 1., 1.];
    let scene = Scene::new(&project).unwrap();
    renderer.draw(&scene, &target.view, 3, 2).unwrap();
    assert_eq!(renderer.image_readback_info(), ImageReadbackInfo::default());
    for _ in 0..2 {
        let pixels = renderer.read_target(&target).unwrap();
        assert_eq!(pixels, [255, 0, 255, 255].repeat(6));
    }
    assert_eq!(
        renderer.image_readback_info(),
        ImageReadbackInfo {
            requested_calls: 2,
            submitted_calls: 2,
            submitted_payload_bytes: 48,
            // Six RGBA pixels: 24 payload bytes, two mapped 256-byte rows, and
            // a copy ending after byte268. Padding is not returned pixel data.
            submitted_buffer_span_bytes: 536,
            mapped_buffer_bytes: 1024,
            returned_payload_bytes: 48,
            completed_calls: 2,
            failed_calls: 0,
            in_flight_calls: 0,
            counters_saturated: false,
        }
    );
    renderer.device.destroy();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let primary = renderer.check_health().unwrap_err().to_string();
    let failure = renderer.read_target(&target).unwrap_err();
    assert_eq!(failure.to_string(), primary);
    let observed = renderer.image_readback_info();
    assert_eq!(observed.requested_calls, 3);
    assert_eq!(observed.failed_calls, 1);
    assert_eq!(observed.submitted_calls, 2);
    assert_eq!(observed.completed_calls, 2);
    assert_eq!(observed.returned_payload_bytes, 48);
    assert_eq!(observed.in_flight_calls, 0);
    drop(target);
    drop(renderer);
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    assert_eq!(renderer.image_readback_info(), ImageReadbackInfo::default());
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_timestamp_readbacks_count_discarded_encoding_and_invalid_mapped_words() {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let mut renderer = pollster::block_on(Renderer::new_profiled(
        &instance,
        None,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        true,
    ))
    .unwrap();
    let mut timer = GpuTimer::new(&renderer)
        .unwrap()
        .expect("validation GPU requires timestamps");
    assert_eq!(timer.readback_info(), TimestampReadbackInfo::default());
    let slot = timer.begin(1).unwrap();
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    timer.resolve(slot, &mut encoder);
    drop(encoder);
    timer.cancel_unsubmitted(slot);
    let encoded = timer.readback_info();
    assert_eq!(encoded.encoded_resolve_calls, 1);
    assert_eq!(encoded.encoded_copy_bytes, 32);
    assert_eq!(encoded.map_requests, 0);
    assert_eq!(encoded.mapped_buffer_bytes, 0);
    let slot = timer.begin(2).unwrap();
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    // Resolving unwritten timestamp queries yields actual zero GPU words.
    // Mapping them consumed bytes, but must not invent a valid GPU sample.
    timer.resolve(slot, &mut encoder);
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    timer.map(slot);
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert!(timer.collect().into_iter().all(|timing| timing.is_none()));
    assert_eq!(
        timer.readback_info(),
        TimestampReadbackInfo {
            encoded_resolve_calls: 2,
            encoded_resolve_bytes: 64,
            encoded_copy_bytes: 64,
            map_requests: 1,
            map_successes: 1,
            map_failures: 0,
            mapped_buffer_bytes: 32,
            valid_samples: 0,
            invalid_samples: 1,
            counters_saturated: false,
        }
    );
    let before = timer.readback_info();
    assert!(timer.collect().into_iter().all(|timing| timing.is_none()));
    assert_eq!(timer.readback_info(), before);
    let mut project = Project::new(128, 256, 30, 10).unwrap();
    project.layers.push(Layer::solid(
        1,
        "timestamp fixture",
        [128., 256.],
        [64., 128., 0.],
        [1., 0., 1., 1.],
    ));
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 2., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let source = renderer.render_target(128, 256).unwrap();
    let output = renderer.capture_target(256, 256).unwrap();
    let presenter = Presenter::new(&renderer, &source.view, renderer.target_format);
    let slot = timer.begin(3).unwrap();
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(
            &scene,
            &source.view,
            128,
            256,
            &mut encoder,
            Some(timer.writes(slot, 0)),
        )
        .unwrap();
    presenter.encode(
        &mut encoder,
        &output.view,
        Some(timer.writes(slot, 1)),
        None,
        [0., 0., 0., 1.],
    );
    timer.resolve(slot, &mut encoder);
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    timer.map(slot);
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let sample = timer.collect().into_iter().flatten().next().unwrap();
    assert_eq!(sample.sequence, 3);
    assert_eq!(
        timer.readback_info(),
        TimestampReadbackInfo {
            encoded_resolve_calls: 3,
            encoded_resolve_bytes: 96,
            encoded_copy_bytes: 96,
            map_requests: 2,
            map_successes: 2,
            map_failures: 0,
            mapped_buffer_bytes: 64,
            valid_samples: 1,
            invalid_samples: 1,
            counters_saturated: false,
        }
    );
    assert_eq!(
        GpuTimer::new(&renderer).unwrap().unwrap().readback_info(),
        TimestampReadbackInfo::default()
    );
    assert_eq!(renderer.image_readback_info(), ImageReadbackInfo::default());
    renderer.check_health().unwrap();
}
