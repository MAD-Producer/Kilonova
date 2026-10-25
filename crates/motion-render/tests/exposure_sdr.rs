#[path = "support/sdr_fixture.rs"]
mod fixture;
use motion_effects::image_contract::ChannelPrecision;
use motion_render::{OutputTransform, Renderer, Scene};

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn four_mode_exposure_and_submitted_native_outputs_convert_with_retained_storage() {
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let (project, registry) = fixture::project(precision, false);
        let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
            precision,
            Default::default(),
        ))
        .unwrap();
        renderer.set_effect_registry(registry);
        let mut scene = Scene::new(&project).unwrap();
        scene
            .sample(&project, 2., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let target = renderer
            .sdr_capture_target(18, 18, OutputTransform::SDR_CLIP)
            .unwrap();
        assert_eq!(target.working_format(), renderer.target_format);
        let mut encoder = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        renderer
            .encode(&scene, target.working_view(), 18, 18, &mut encoder, None)
            .unwrap();
        renderer
            .submit_allocated(encoder.finish(), |ledger| {
                ledger.merge(target.texture_resource_ledger())
            })
            .unwrap();
        fixture::assert_png(&renderer.read_sdr_target(&target).unwrap().pixels);
        let mut exposure = renderer.exposure_target(18, 18, &[0.25; 4]).unwrap();
        assert!(
            renderer
                .capture_exposure_sdr(&exposure, &target)
                .unwrap_err()
                .to_string()
                .contains("incomplete")
        );
        for _ in 0..4 {
            renderer
                .accumulate_exposure(&mut exposure, Some(&scene), false)
                .unwrap();
        }
        let before = renderer.allocation_budget().info().unwrap();
        let ids: Vec<_> = target
            .texture_resource_ledger()
            .resources()
            .map(|r| r.id.clone())
            .collect();
        for _ in 0..4 {
            fixture::assert_png(
                &renderer
                    .capture_exposure_sdr(&exposure, &target)
                    .unwrap()
                    .pixels,
            );
        }
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        assert_eq!(
            renderer.allocation_budget().info().unwrap().admitted_bytes,
            before.admitted_bytes
        );
        assert_eq!(
            renderer
                .allocation_budget()
                .info()
                .unwrap()
                .live_allocations,
            before.live_allocations
        );
        assert_eq!(
            target
                .texture_resource_ledger()
                .resources()
                .map(|r| r.id.clone())
                .collect::<Vec<_>>(),
            ids
        );
        renderer.check_health().unwrap();
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn video_output_composites_linear_black_before_encoding_instead_of_ignoring_straight_alpha() {
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let (project, registry) = fixture::project(precision, false);
        let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
            precision,
            Default::default(),
        ))
        .unwrap();
        renderer.set_effect_registry(registry);
        let mut scene = Scene::new(&project).unwrap();
        scene
            .sample(&project, 2., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let target = renderer
            .sdr_video_target(18, 18, OutputTransform::SDR_CLIP)
            .unwrap();
        let (image, _) = renderer.capture_sdr(&scene, &target).unwrap();
        for pixel in image.pixels.chunks_exact(4) {
            // Linear black matte of clipped straight color is (0.25,0,0.125).
            for (actual, expected) in pixel[..3].iter().zip([137u8, 0, 99]) {
                assert!(
                    actual.abs_diff(expected) <= 2,
                    "{precision:?} pixel={pixel:?}"
                );
            }
            assert_eq!(pixel[3], 255, "video output must not be composited twice");
        }
        let mut exposure = renderer.exposure_target(18, 18, &[0.5; 2]).unwrap();
        for _ in 0..2 {
            renderer
                .accumulate_exposure(&mut exposure, Some(&scene), false)
                .unwrap();
        }
        let temporal = renderer.capture_exposure_sdr(&exposure, &target).unwrap();
        assert!(
            temporal
                .pixels
                .iter()
                .zip(&image.pixels)
                .all(|(a, b)| a.abs_diff(*b) <= 1)
        );
        let mut empty = motion_core::Project::new(18, 18, 30, 6).unwrap();
        empty.working_precision = precision;
        empty.background = [0.; 4];
        let empty = Scene::new(&empty).unwrap();
        let black = renderer.capture_sdr(&empty, &target).unwrap().0;
        assert!(
            black
                .pixels
                .chunks_exact(4)
                .all(|pixel| pixel == [0, 0, 0, 255])
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn submitted_native_readback_refuses_foreign_and_obsolete_mode_before_submission() {
    let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
        ChannelPrecision::Float16,
        Default::default(),
    ))
    .unwrap();
    let foreign = pollster::block_on(Renderer::headless_with_working_precision(
        ChannelPrecision::Float16,
        renderer.allocation_budget().clone(),
    ))
    .unwrap();
    let target = renderer
        .sdr_capture_target(18, 18, OutputTransform::SDR_CLIP)
        .unwrap();
    assert!(
        foreign
            .read_sdr_target(&target)
            .unwrap_err()
            .to_string()
            .contains("another renderer")
    );
    let change = renderer
        .prepare_working_precision(ChannelPrecision::Float32)
        .unwrap()
        .unwrap();
    renderer.commit_working_precision(change).unwrap();
    assert!(
        renderer
            .read_sdr_target(&target)
            .unwrap_err()
            .to_string()
            .contains("working target format")
    );
    renderer.check_health().unwrap();
    foreign.check_health().unwrap();
}
