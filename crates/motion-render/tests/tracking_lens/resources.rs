use super::*;
use motion_render::{AllocationBudget, FIXED_RENDERER_ALLOCATION_BYTES};

fn encoder(renderer: &Renderer) -> motion_render::AllocationEncoder {
    renderer
        .allocation_command_encoder(&Default::default())
        .unwrap()
}
fn pipeline(renderer: &Renderer, target: &ImageCaptureTarget) -> LensOutputPipeline {
    LensOutputPipeline::new(renderer, image_formats::native_format(target.storage())).unwrap()
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires actual GPU")]
fn rejects_aliasing_wrong_footprints_precision_meaning_and_device_ownership() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let error = LensOutputPipeline::new(&renderer, wgpu::TextureFormat::R32Float)
        .err()
        .unwrap();
    assert!(
        error.to_string().contains("supported RGBA color storage"),
        "{error}"
    );
    let identity = lens(Rotation::None, 0.);
    let raster = identity.raster(1.).unwrap();
    let source = renderer
        .image_capture_target(W, H, contract(ChannelPrecision::Unorm8))
        .unwrap();
    let output = renderer
        .image_capture_target(W, H, contract(ChannelPrecision::Unorm8))
        .unwrap();
    let p = pipeline(&renderer, &output);
    let mut e = encoder(&renderer);
    let error = p
        .encode_images(&renderer, &mut e, identity, raster, &source, &source)
        .unwrap_err();
    assert!(
        error.to_string().contains("cannot sample its destination"),
        "{error}"
    );
    let small = renderer
        .image_capture_target(W - 1, H, contract(ChannelPrecision::Unorm8))
        .unwrap();
    let error = p
        .encode_images(&renderer, &mut e, identity, raster, &small, &output)
        .unwrap_err();
    assert!(
        error.to_string().contains("do not match certified raster"),
        "{error}"
    );
    let error = p
        .encode_images(
            &renderer,
            &mut e,
            lens(Rotation::None, -0.025),
            raster,
            &source,
            &output,
        )
        .unwrap_err();
    assert!(error.to_string().contains("does not cover"), "{error}");
    let high = renderer
        .image_capture_target(W, H, contract(ChannelPrecision::Float16))
        .unwrap();
    let error = p
        .encode_images(&renderer, &mut e, identity, raster, &high, &output)
        .unwrap_err();
    assert!(error.to_string().contains("storage mismatch"), "{error}");
    let error = pipeline(&renderer, &high)
        .encode_images(&renderer, &mut e, identity, raster, &source, &output)
        .unwrap_err();
    assert!(error.to_string().contains("storage mismatch"), "{error}");
    let straight = renderer
        .image_capture_target(
            W,
            H,
            ImageContract::new(
                ImageMeaning::Color {
                    space: WorkingSpace::Linear,
                    alpha: AlphaMode::Straight,
                },
                ChannelPrecision::Unorm8,
            ),
        )
        .unwrap();
    let error = p
        .encode_images(&renderer, &mut e, identity, raster, &straight, &output)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("must be linear premultiplied color"),
        "{error}"
    );
    drop(e);
    let foreign = pollster::block_on(Renderer::headless()).unwrap();
    let foreign_source = foreign
        .image_capture_target(W, H, contract(ChannelPrecision::Unorm8))
        .unwrap();
    let mut e = encoder(&renderer);
    let error = p
        .encode_images(
            &renderer,
            &mut e,
            identity,
            raster,
            &foreign_source,
            &output,
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("image target belongs to another renderer"),
        "{error}"
    );
    let mut foreign_encoder = encoder(&foreign);
    let error = p
        .encode_images(
            &renderer,
            &mut foreign_encoder,
            identity,
            raster,
            &source,
            &output,
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("encoder belongs to another renderer"),
        "{error}"
    );
    let foreign_output = foreign
        .image_capture_target(W, H, contract(ChannelPrecision::Unorm8))
        .unwrap();
    let error = p
        .encode_images(
            &foreign,
            &mut e,
            identity,
            raster,
            &foreign_source,
            &foreign_output,
        )
        .unwrap_err();
    assert!(error.to_string().contains("another renderer"), "{error}");
    renderer.check_health().unwrap();
    foreign.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires actual GPU")]
fn queued_cameras_do_not_overwrite_uniforms_and_budget_lifetime_matches_submission() {
    let budget = AllocationBudget::new(64 << 20).unwrap();
    let renderer =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    let lenses = [lens(Rotation::None, -0.025), lens(Rotation::None, 0.025)];
    let raster = lenses[0].raster(1.).unwrap();
    let size = raster.size();
    lenses[1].validate_raster(raster).unwrap();
    let source = renderer
        .image_capture_target(size[0], size[1], contract(ChannelPrecision::Unorm8))
        .unwrap();
    let targets: Vec<_> = (0..2)
        .map(|_| {
            renderer
                .image_capture_target(W, H, contract(ChannelPrecision::Unorm8))
                .unwrap()
        })
        .collect();
    let p = pipeline(&renderer, &targets[0]);
    fill(&renderer, &source, &pattern(raster.rect(), size));
    let mut baseline = Vec::new();
    for (lens, target) in lenses.into_iter().zip(&targets) {
        let mut e = encoder(&renderer);
        p.encode_images(&renderer, &mut e, lens, raster, &source, target)
            .unwrap();
        renderer.submit_allocated(e.finish(), |_| {}).unwrap();
        baseline.push(renderer.read_image_target(target).unwrap().pixels);
    }
    assert!(
        baseline[0]
            .iter()
            .zip(&baseline[1])
            .filter(|(a, b)| a.abs_diff(**b) > 3)
            .count()
            > 80
    );
    let used = budget.info().unwrap().admitted_bytes;
    budget.set_limit(used + 127).unwrap();
    let mut e = encoder(&renderer);
    let error = p
        .encode_images(&renderer, &mut e, lenses[0], raster, &source, &targets[0])
        .unwrap_err();
    assert!(error.resource_rejection().is_some(), "{error}");
    assert!(
        error
            .to_string()
            .contains("immutable calibrated lens parameters"),
        "{error}"
    );
    assert_eq!(budget.info().unwrap().admitted_bytes, used);
    drop(e);
    budget.set_limit(used + 256).unwrap();
    let mut e = encoder(&renderer);
    for i in 0..2 {
        p.encode_images(&renderer, &mut e, lenses[i], raster, &source, &targets[i])
            .unwrap();
        assert_eq!(
            budget.info().unwrap().admitted_bytes,
            used + 128 * (i as u64 + 1)
        );
    }
    renderer.submit_allocated(e.finish(), |_| {}).unwrap();
    for i in 0..2 {
        assert_eq!(
            renderer.read_image_target(&targets[i]).unwrap().pixels,
            baseline[i],
            "queued camera {i}"
        );
    }
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        used,
        "completed immutable uniforms retire"
    );
    // A finished but unsubmitted frame must still own its parameters and input
    // allocations after user-facing target owners have been dropped.
    let mut e = encoder(&renderer);
    p.encode_images(&renderer, &mut e, lenses[0], raster, &source, &targets[0])
        .unwrap();
    let pending = e.finish();
    drop(source);
    drop(targets);
    assert_eq!(budget.info().unwrap().admitted_bytes, used + 128);
    renderer.submit_allocated(pending, |_| {}).unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    renderer.check_health().unwrap();
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        FIXED_RENDERER_ALLOCATION_BYTES
    );
}
