use super::*;
use motion_render::scene_gpu::{ScenePass, ScenePassFrame, ScenePassInstance};

fn instance(source: usize) -> ScenePassInstance<'static> {
    let mut instance = ScenePassInstance::new(source, REQUEST, 0, 1.);
    instance.lights = &[];
    instance.ambient = [0.; 3];
    instance
}
fn upload(renderer: &Renderer, materials: Vec<Value>, primitives: &[(f32, usize)]) -> GpuScene {
    GpuScene::upload(
        renderer,
        REQUEST,
        fixture(materials, primitives, None, false),
        SceneGpuLimits::default(),
    )
    .unwrap()
}
pub(super) fn depth(renderer: &Renderer, texture: &motion_render::AdmittedTexture) -> Vec<f32> {
    assert_eq!(texture.sample_count(), 1);
    let stride = (SIZE * 4).div_ceil(256) * 256;
    let buffer = renderer
        .allocation_context()
        .buffer(&wgpu::BufferDescriptor {
            label: Some("independent shared Depth32Float acceptance"),
            size: u64::from(stride * SIZE),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        })
        .unwrap();
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::DepthOnly,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(SIZE),
            },
        },
        wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
    );
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    rx.recv().unwrap().unwrap();
    let bytes = buffer.slice(..).get_mapped_range().unwrap();
    let values = bytes
        .chunks_exact(stride as usize)
        .flat_map(|row| {
            row[..(SIZE * 4) as usize]
                .chunks_exact(4)
                .map(|p| f32::from_le_bytes(p.try_into().unwrap()))
        })
        .collect();
    drop(bytes);
    buffer.unmap();
    values
}
fn expected_depth(camera: CameraPose, z: f32) -> f32 {
    let clip = camera.view_projection * glam::Vec4::new(0., 0., z, 1.);
    clip.z / clip.w
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shared_depth_interleaves_separate_sources_independently_of_submission_order() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let mut red = upload(
        &renderer,
        vec![emissive([1., 0., 0.], 1., "OPAQUE")],
        &[(0.6, 0)],
    );
    let mut green = upload(
        &renderer,
        vec![emissive([0., 1., 0.], 1., "OPAQUE")],
        &[(-0.6, 0)],
    );
    let frame = ScenePassFrame::new(frame().camera);
    let mut pass = ScenePass::new(&renderer, 16).unwrap();
    pass.draw(
        &renderer,
        REQUEST,
        &target,
        &mut [&mut red, &mut green],
        &[instance(0), instance(1)],
        &frame,
    )
    .unwrap();
    let first = pixels(&renderer, &target, ChannelPrecision::Unorm8);
    near8(center8(&first), [1., 0., 0., 1.]);
    let values = depth(&renderer, pass.depth().unwrap());
    assert!(
        (values[(SIZE / 2 * SIZE + SIZE / 2) as usize] - expected_depth(frame.camera, 0.6)).abs()
            < 1e-6
    );
    assert_eq!(
        values[0], 1.,
        "uncovered pixels must retain the actual depth clear"
    );
    pass.draw(
        &renderer,
        REQUEST,
        &target,
        &mut [&mut red, &mut green],
        &[instance(1), instance(0)],
        &frame,
    )
    .unwrap();
    assert_eq!(
        pixels(&renderer, &target, ChannelPrecision::Unorm8).pixels,
        first.pixels
    );
    assert_eq!(
        pass.texture_resource_ledger().total_known_bytes(),
        u64::from(SIZE * SIZE * 4)
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shared_blend_queue_is_global_and_masked_fragments_do_not_write_depth() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let mut reds = upload(
        &renderer,
        vec![emissive([1., 0., 0.], 0.5, "BLEND")],
        &[(-0.6, 0), (0.6, 0)],
    );
    let mut green = upload(
        &renderer,
        vec![emissive([0., 1., 0.], 0.5, "BLEND")],
        &[(0., 0)],
    );
    let mut solid = upload(
        &renderer,
        vec![
            emissive([0., 0., 1.], 1., "OPAQUE"),
            emissive([1., 1., 0.], 0., "MASK"),
        ],
        &[(-0.9, 0), (0.9, 1)],
    );
    let mut pass = ScenePass::new(&renderer, 16).unwrap();
    let frame = ScenePassFrame::new(frame().camera);
    let stats = pass
        .draw(
            &renderer,
            REQUEST,
            &target,
            &mut [&mut reds, &mut green, &mut solid],
            &[instance(0), instance(2), instance(1)],
            &frame,
        )
        .unwrap();
    assert_eq!((stats.draws, stats.blended_draws), (5, 3));
    // Analytical premultiplied over: blue, far red, green, near red.
    near8(
        center8(&pixels(&renderer, &target, ChannelPrecision::Unorm8)),
        [0.625, 0.25, 0.125, 1.],
    );
    let values = depth(&renderer, pass.depth().unwrap());
    assert!(
        (values[(SIZE / 2 * SIZE + SIZE / 2) as usize] - expected_depth(frame.camera, -0.9)).abs()
            < 1e-6,
        "neither blended fragments nor discarded mask fragments may replace opaque depth"
    );
    pass.draw(
        &renderer,
        REQUEST,
        &target,
        &mut [&mut reds, &mut green, &mut solid],
        &[instance(1), instance(0), instance(2)],
        &frame,
    )
    .unwrap();
    near8(
        center8(&pixels(&renderer, &target, ChannelPrecision::Unorm8)),
        [0.625, 0.25, 0.125, 1.],
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shared_instances_and_two_pending_encodes_retain_independent_uniforms_and_leases() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    drain(&renderer);
    let baseline = renderer.allocation_budget().info().unwrap().admitted_bytes;
    let a = renderer.render_target(SIZE, SIZE).unwrap();
    let b = renderer.render_target(SIZE, SIZE).unwrap();
    let mut source = upload(&renderer, vec![emissive([1.; 3], 1., "OPAQUE")], &[(0., 0)]);
    let mut pass = ScenePass::new(&renderer, 8).unwrap();
    let f = ScenePassFrame::new(frame().camera);
    let mut left = instance(0);
    left.world =
        Mat4::from_translation(Vec3::new(-0.5, 0., 0.)) * Mat4::from_scale(Vec3::splat(0.35));
    let mut right = instance(0);
    right.world =
        Mat4::from_translation(Vec3::new(0.5, 0., 0.)) * Mat4::from_scale(Vec3::splat(0.35));
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    let stats = pass
        .encode(
            &renderer,
            REQUEST,
            &mut encoder,
            &a,
            &mut [&mut source],
            &[left, right],
            &f,
        )
        .unwrap();
    assert_eq!(stats.draws, 2);
    let mut offscreen = instance(0);
    offscreen.world = Mat4::from_translation(Vec3::new(4., 0., 0.));
    pass.encode(
        &renderer,
        REQUEST,
        &mut encoder,
        &b,
        &mut [&mut source],
        &[offscreen],
        &f,
    )
    .unwrap();
    let held = renderer.allocation_budget().info().unwrap().admitted_bytes;
    drop((source, pass));
    assert_eq!(renderer.allocation_budget().info().unwrap().admitted_bytes, held,
        "unsubmitted allocated encoder must retain source, per-instance uniforms and shared attachments");
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    let image = pixels(&renderer, &a, ChannelPrecision::Unorm8);
    for x in [SIZE / 4, SIZE * 3 / 4] {
        assert_eq!(
            &image.pixels[((SIZE / 2 * SIZE + x) * 4) as usize..][..4],
            &[255; 4]
        );
    }
    near8(center8(&image), [0.; 4]);
    assert!(pixels(&renderer, &b, ChannelPrecision::Unorm8)
        .pixels
        .iter()
        .all(|v| *v == 0));
    drop((a, b));
    drain(&renderer);
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        baseline
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shared_attachment_budget_failure_rolls_back_and_recovers_without_losing_prior_depth() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let mut source = upload(&renderer, vec![emissive([1.; 3], 1., "OPAQUE")], &[(0., 0)]);
    let mut pass = ScenePass::new(&renderer, 8).unwrap();
    let mut f = ScenePassFrame::new(frame().camera);
    let stats = pass
        .draw(
            &renderer,
            REQUEST,
            &target,
            &mut [&mut source],
            &[instance(0)],
            &f,
        )
        .unwrap();
    let original = pixels(&renderer, &target, ChannelPrecision::Unorm8).pixels;
    drain(&renderer);
    let old_depth = pass.depth().unwrap().clone();
    let before = renderer.allocation_budget().info().unwrap().admitted_bytes;
    // Permit immutable uniforms and MSAA depth; refuse the color allocation by
    // precisely one byte. Partial attachment allocation must not replace cache.
    renderer
        .allocation_budget()
        .set_limit(before + stats.parameter_bytes + u64::from(SIZE * SIZE * 4 * 4 * 2) - 1)
        .unwrap();
    f.samples = 4;
    let error = pass
        .draw(
            &renderer,
            REQUEST,
            &target,
            &mut [&mut source],
            &[instance(0)],
            &f,
        )
        .unwrap_err();
    assert!(error.resource_rejection().is_some(), "{error}");
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        before
    );
    assert_eq!(&**pass.depth().unwrap(), &*old_depth);
    renderer.allocation_budget().set_limit(128 << 20).unwrap();
    assert_eq!(
        pixels(&renderer, &target, ChannelPrecision::Unorm8).pixels,
        original
    );
    pass.draw(
        &renderer,
        REQUEST,
        &target,
        &mut [&mut source],
        &[instance(0)],
        &f,
    )
    .unwrap();
    assert_eq!(pass.depth().unwrap().sample_count(), 4);
    let multi = pixels(&renderer, &target, ChannelPrecision::Unorm8);
    assert!(multi.pixels.chunks_exact(4).any(|p| p[3] > 0 && p[3] < 255));
    assert_eq!(center8(&multi), [255; 4]);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shared_owner_thread_source_limits_and_invalid_frames_reject_before_recording() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let mut source = upload(&renderer, vec![emissive([1.; 3], 1., "OPAQUE")], &[(0., 0)]);
    let mut pass = ScenePass::new(&renderer, 1).unwrap();
    let mut f = ScenePassFrame::new(frame().camera);
    pass.draw(
        &renderer,
        REQUEST,
        &target,
        &mut [&mut source],
        &[instance(0)],
        &f,
    )
    .unwrap();
    let original = pixels(&renderer, &target, ChannelPrecision::Unorm8).pixels;
    for draws in [vec![instance(1)], vec![instance(0), instance(0)]] {
        assert!(pass
            .draw(&renderer, REQUEST, &target, &mut [&mut source], &draws, &f)
            .is_err());
    }
    let mut bad = instance(0);
    bad.world = Mat4::from_scale(Vec3::ZERO);
    let error = pass
        .draw(&renderer, REQUEST, &target, &mut [&mut source], &[bad], &f)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("instance=0 request=#8703 source=87") && error.contains("singular"),
        "{error}"
    );
    for samples in [0, 2, 8] {
        f.samples = samples;
        assert!(pass
            .draw(
                &renderer,
                REQUEST,
                &target,
                &mut [&mut source],
                &[instance(0)],
                &f
            )
            .unwrap_err()
            .to_string()
            .contains("explicitly 1 or 4"));
    }
    assert_eq!(
        pixels(&renderer, &target, ChannelPrecision::Unorm8).pixels,
        original
    );
    let (renderer, target, mut source, mut pass) = std::thread::spawn(move || {
        let before = renderer.allocation_budget().info().unwrap().admitted_bytes;
        let error = pass
            .draw(
                &renderer,
                REQUEST,
                &target,
                &mut [&mut source],
                &[instance(0)],
                &ScenePassFrame::new(frame().camera),
            )
            .unwrap_err();
        assert!(error.to_string().contains("owning thread"));
        assert_eq!(
            renderer.allocation_budget().info().unwrap().admitted_bytes,
            before
        );
        (renderer, target, source, pass)
    })
    .join()
    .unwrap();
    let other = gpu_renderer(ChannelPrecision::Unorm8);
    assert!(pass
        .draw(
            &other,
            REQUEST,
            &target,
            &mut [&mut source],
            &[instance(0)],
            &ScenePassFrame::new(frame().camera)
        )
        .unwrap_err()
        .to_string()
        .contains("another renderer"));
    pass.draw(
        &renderer,
        REQUEST,
        &target,
        &mut [&mut source],
        &[instance(0)],
        &ScenePassFrame::new(frame().camera),
    )
    .unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shared_abandoned_encoder_retires_attachments_and_source_capacity() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    drain(&renderer);
    let baseline = renderer.allocation_budget().info().unwrap().admitted_bytes;
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let mut source = upload(&renderer, vec![emissive([1.; 3], 1., "OPAQUE")], &[(0., 0)]);
    // Retire actual queue uploads before the encoder-abandonment observation.
    drain(&renderer);
    let mut pass = ScenePass::new(&renderer, 8).unwrap();
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    pass.encode(
        &renderer,
        REQUEST,
        &mut encoder,
        &target,
        &mut [&mut source],
        &[instance(0)],
        &ScenePassFrame::new(frame().camera),
    )
    .unwrap();
    let held = renderer.allocation_budget().info().unwrap().admitted_bytes;
    drop((source, pass, target));
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        held
    );
    drop(encoder);
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        baseline
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shared_empty_pass_clears_actual_depth_and_four_precisions_keep_hdr_contracts() {
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let renderer = gpu_renderer(precision);
        let target = renderer.render_target(SIZE, SIZE).unwrap();
        let mut source = upload(
            &renderer,
            vec![emissive([0.3125, 0.0625, 0.5], 1., "OPAQUE")],
            &[(0., 0)],
        );
        let mut pass = ScenePass::new(&renderer, 8).unwrap();
        let mut f = ScenePassFrame::new(frame().camera);
        pass.draw(
            &renderer,
            REQUEST,
            &target,
            &mut [&mut source],
            &[instance(0)],
            &f,
        )
        .unwrap();
        let center = center_linear(&pixels(&renderer, &target, precision));
        let tolerance = match precision {
            ChannelPrecision::Unorm8 => 0.005,
            ChannelPrecision::Unorm16 => 1. / 65535.,
            ChannelPrecision::Float16 => 0.0005,
            ChannelPrecision::Float32 => 1e-6,
        };
        for (v, expected) in center.into_iter().zip([0.3125, 0.0625, 0.5, 1.]) {
            assert!(
                (v - expected).abs() < tolerance,
                "{precision:?}: {center:?}"
            );
        }
        f.background = [0.25, 0.5, 0.75, 0.5];
        let empty = pass
            .draw(&renderer, REQUEST, &target, &mut [], &[], &f)
            .unwrap();
        assert_eq!((empty.draws, empty.parameter_bytes), (0, 0));
        assert!(depth(&renderer, pass.depth().unwrap())
            .iter()
            .all(|z| *z == 1.));
        let center = center_linear(&pixels(&renderer, &target, precision));
        for (v, expected) in center.into_iter().zip([0.125, 0.25, 0.375, 0.5]) {
            assert!(
                (v - expected).abs() < tolerance,
                "{precision:?}: {center:?}"
            );
        }
        if matches!(
            precision,
            ChannelPrecision::Float16 | ChannelPrecision::Float32
        ) {
            let scene = fixture(
                vec![json!({"pbrMetallicRoughness":{
                "baseColorFactor":[1,1,1,1],"metallicFactor":0,"roughnessFactor":1}})],
                &[(0., 0)],
                None,
                true,
            );
            let mut hdr =
                GpuScene::upload(&renderer, REQUEST, scene, SceneGpuLimits::default()).unwrap();
            let lights = [SceneLight::Directional {
                direction: [0., 0., -1.],
                color: [1.; 3],
                intensity: std::f32::consts::PI * 4.,
            }];
            let mut item = instance(0);
            item.lights = &lights;
            pass.draw(&renderer, REQUEST, &target, &mut [&mut hdr], &[item], &f)
                .unwrap();
            let center = center_linear(&pixels(&renderer, &target, precision));
            for value in &center[..3] {
                assert!(
                    (*value - 3.88).abs() <= tolerance * 4.,
                    "{precision:?}: {center:?}"
                );
            }
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shared_perspective_depth_uses_the_actual_common_projection() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let mut front = upload(
        &renderer,
        vec![emissive([1., 0., 0.], 1., "OPAQUE")],
        &[(0.6, 0)],
    );
    let mut back = upload(
        &renderer,
        vec![emissive([0., 1., 0.], 1., "OPAQUE")],
        &[(0., 0)],
    );
    let mut camera = frame().camera;
    camera.projection = ProjectionKind::Perspective;
    camera.view_projection = Mat4::perspective_rh(1., 1., 0.1, 10.)
        * Mat4::look_at_rh(camera.eye, camera.target, Vec3::Y);
    let f = ScenePassFrame::new(camera);
    let mut pass = ScenePass::new(&renderer, 8).unwrap();
    let mut a = instance(0);
    a.pixels_per_metre = 2.;
    a.world = Mat4::from_translation(Vec3::new(0., 0., 0.2));
    pass.draw(
        &renderer,
        REQUEST,
        &target,
        &mut [&mut front, &mut back],
        &[instance(1), a],
        &f,
    )
    .unwrap();
    near8(
        center8(&pixels(&renderer, &target, ChannelPrecision::Unorm8)),
        [1., 0., 0., 1.],
    );
    let values = depth(&renderer, pass.depth().unwrap());
    assert!(
        (values[(SIZE / 2 * SIZE + SIZE / 2) as usize] - expected_depth(camera, 1.4)).abs() < 1e-6
    );
}

struct FailureLog(std::sync::Mutex<Vec<String>>);
impl log::Log for FailureLog {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = record.args().to_string();
        if line.starts_with("scene_shared_pass #987522 ") {
            let mut lines = self.0.lock().unwrap();
            assert!(lines.len() < 4, "persistent failures must be suppressed");
            assert_eq!(record.level(), log::Level::Error);
            lines.push(line);
        }
    }
    fn flush(&self) {}
}
static FAILURES: FailureLog = FailureLog(std::sync::Mutex::new(Vec::new()));

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shared_draw_allowance_counts_primitives_across_all_instances() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let mut source = upload(
        &renderer,
        vec![emissive([1.; 3], 1., "OPAQUE")],
        &[(-0.5, 0), (0.5, 0)],
    );
    let mut pass = ScenePass::new(&renderer, 3).unwrap();
    let f = ScenePassFrame::new(frame().camera);
    pass.draw(
        &renderer,
        REQUEST,
        &target,
        &mut [&mut source],
        &[instance(0)],
        &f,
    )
    .unwrap();
    let original = pixels(&renderer, &target, ChannelPrecision::Unorm8).pixels;
    // Two table entries fit the instance allowance, but four actual primitives
    // exceed its combined draw allowance. Count repeated sources separately.
    let error = pass
        .draw(
            &renderer,
            REQUEST,
            &target,
            &mut [&mut source],
            &[instance(0), instance(0)],
            &f,
        )
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("instance=1") && error.contains("draw count limit"),
        "{error}"
    );
    assert_eq!(
        pixels(&renderer, &target, ChannelPrecision::Unorm8).pixels,
        original
    );
    pass.draw(
        &renderer,
        REQUEST,
        &target,
        &mut [&mut source],
        &[instance(0)],
        &f,
    )
    .unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shared_failure_observation_is_bounded_retains_context_and_resets_after_recovery() {
    log::set_logger(&FAILURES).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let mut source = upload(&renderer, vec![emissive([1.; 3], 1., "OPAQUE")], &[(0., 0)]);
    let mut pass = ScenePass::new(&renderer, 8).unwrap();
    let request = SceneRequest {
        request: 987522,
        source: 5001,
    };
    let f = ScenePassFrame::new(frame().camera);
    for occurrence in 1..=65 {
        let mut bad = instance(0);
        bad.world = Mat4::from_scale(Vec3::ZERO);
        bad.request = request;
        assert!(pass
            .draw(&renderer, request, &target, &mut [&mut source], &[bad], &f)
            .is_err());
        assert_eq!(
            FAILURES.0.lock().unwrap().len(),
            if occurrence < 60 { 1 } else { 2 }
        );
    }
    pass.draw(
        &renderer,
        request,
        &target,
        &mut [&mut source],
        &[instance(0)],
        &f,
    )
    .unwrap();
    let mut bad = instance(0);
    bad.world = Mat4::from_scale(Vec3::ZERO);
    bad.request = request;
    assert!(pass
        .draw(&renderer, request, &target, &mut [&mut source], &[bad], &f)
        .is_err());
    let lines = FAILURES.0.lock().unwrap();
    assert_eq!(lines.len(), 3);
    assert!(lines.iter().all(
        |line| line.contains("instance=0 request=#987522 source=5001")
            && line.contains("singular")
            && line.contains("elapsed_ms=")
    ));
}
