use super::*;
use motion_model::SampledDepthOfField;
use motion_render::scene_gpu::{ScenePass, ScenePassFrame, ScenePassInstance};

fn camera() -> ScenePassFrame {
    let mut camera = frame().camera;
    camera.projection = ProjectionKind::Perspective;
    camera.view_projection = Mat4::perspective_rh(60_f32.to_radians(), 1., 0.1, 10.)
        * Mat4::look_at_rh(camera.eye, camera.target, Vec3::Y);
    let mut frame = ScenePassFrame::new(camera);
    frame.background = [0., 0., 1., 1.];
    frame
}
fn instance() -> ScenePassInstance<'static> {
    let mut i = ScenePassInstance::new(0, REQUEST, 0, 1.);
    i.lights = &[];
    i.ambient = [0.; 3];
    i
}
fn aperture(focus: f32) -> SampledDepthOfField {
    SampledDepthOfField {
        focus_distance: focus,
        aperture_diameter: 4.,
        max_radius_pixels: 8.,
    }
}
fn source(renderer: &Renderer, alpha: &str) -> GpuScene {
    GpuScene::upload(
        renderer,
        REQUEST,
        fixture(
            vec![emissive([1., 0., 0.], 1., alpha)],
            &[(0., 0)],
            None,
            false,
        ),
        SceneGpuLimits::default(),
    )
    .unwrap()
}
fn pixel(image: &motion_render::CapturedImage, x: u32, y: u32) -> [f64; 4] {
    linear_at(image, x, y)
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn camera_aperture_keeps_focus_sharp_and_defocuses_real_geometry_in_four_precisions() {
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let renderer = gpu_renderer(precision);
        let target = renderer.render_target(SIZE, SIZE).unwrap();
        let mut source = source(&renderer, "OPAQUE");
        let mut pass = ScenePass::new(&renderer, 8).unwrap();
        for samples in [1, 4] {
            let mut frame = camera();
            frame.samples = samples;
            pass.draw(
                &renderer,
                REQUEST,
                &target,
                &mut [&mut source],
                &[instance()],
                &frame,
            )
            .unwrap();
            let sharp = pixels(&renderer, &target, precision);
            frame.depth_of_field = Some(aperture(3.));
            let stats = pass
                .draw(
                    &renderer,
                    REQUEST,
                    &target,
                    &mut [&mut source],
                    &[instance()],
                    &frame,
                )
                .unwrap();
            assert!(
                stats.depth_of_field_bytes > u64::from(SIZE * SIZE) * precision_bytes(precision)
            );
            let focused = pixels(&renderer, &target, precision);
            // Interior geometry stays exactly sharp at its axial focus. Sky is
            // defocused and may gather neighboring silhouette pixels.
            for y in 25..40 {
                assert_eq!(pixel(&focused, 32, y), pixel(&sharp, 32, y));
            }
            frame.depth_of_field = Some(aperture(6.));
            pass.draw(
                &renderer,
                REQUEST,
                &target,
                &mut [&mut source],
                &[instance()],
                &frame,
            )
            .unwrap();
            let blurred = pixels(&renderer, &target, precision);
            assert_ne!(
                blurred.pixels, focused.pixels,
                "{precision:?} samples={samples}"
            );
            let changed = (10..55)
                .flat_map(|y| (10..55).map(move |x| (x, y)))
                .filter(|&(x, y)| {
                    let p = pixel(&blurred, x, y);
                    let q = pixel(&sharp, x, y);
                    p[0] > 0.02 && p[0] < 0.98 && (p[0] - q[0]).abs() > 0.02
                })
                .count();
            assert!(
                changed > 30,
                "a real silhouette must soften: {precision:?} samples={samples} changed={changed}"
            );
            for (x, y) in [(32, 32), (0, 0), (20, 40), (45, 40)] {
                let p = pixel(&blurred, x, y);
                assert!(
                    (p[0] + p[2] - 1.).abs() < 0.015
                        && p[1].abs() < 0.002
                        && (p[3] - 1.).abs() < 0.002,
                    "premultiplied working color must remain finite: {p:?}"
                );
            }
        }
    }
}
fn precision_bytes(precision: ChannelPrecision) -> u64 {
    match precision {
        ChannelPrecision::Unorm8 => 4,
        ChannelPrecision::Unorm16 | ChannelPrecision::Float16 => 8,
        ChannelPrecision::Float32 => 16,
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn camera_aperture_rejects_invalid_or_transparent_depth_before_changing_front_and_recovers() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let mut source = source(&renderer, "OPAQUE");
    let mut pass = ScenePass::new(&renderer, 8).unwrap();
    let mut frame = camera();
    pass.draw(
        &renderer,
        REQUEST,
        &target,
        &mut [&mut source],
        &[instance()],
        &frame,
    )
    .unwrap();
    let front = pixels(&renderer, &target, ChannelPrecision::Unorm8);
    frame.depth_of_field = Some(aperture(0.));
    assert!(pass
        .draw(
            &renderer,
            REQUEST,
            &target,
            &mut [&mut source],
            &[instance()],
            &frame
        )
        .unwrap_err()
        .to_string()
        .contains("focus distance"));
    assert_eq!(
        pixels(&renderer, &target, ChannelPrecision::Unorm8).pixels,
        front.pixels
    );
    frame.depth_of_field = Some(aperture(6.));
    let mut blended = self::source(&renderer, "BLEND");
    assert!(pass
        .draw(
            &renderer,
            REQUEST,
            &target,
            &mut [&mut blended],
            &[instance()],
            &frame
        )
        .unwrap_err()
        .to_string()
        .contains("transparent-depth"));
    assert_eq!(
        pixels(&renderer, &target, ChannelPrecision::Unorm8).pixels,
        front.pixels
    );
    let budget = renderer.allocation_budget();
    drain(&renderer);
    let before = budget.info().unwrap().admitted_bytes;
    let limit = budget.info().unwrap().budget_bytes;
    budget.set_limit(before + 1).unwrap();
    let error = pass
        .draw(
            &renderer,
            REQUEST,
            &target,
            &mut [&mut source],
            &[instance()],
            &frame,
        )
        .unwrap_err();
    assert!(error.resource_rejection().is_some(), "{error}");
    budget.set_limit(limit).unwrap();
    assert_eq!(
        pixels(&renderer, &target, ChannelPrecision::Unorm8).pixels,
        front.pixels
    );
    pass.draw(
        &renderer,
        REQUEST,
        &target,
        &mut [&mut source],
        &[instance()],
        &frame,
    )
    .unwrap();
    assert_ne!(
        pixels(&renderer, &target, ChannelPrecision::Unorm8).pixels,
        front.pixels
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn camera_aperture_pending_frames_keep_distinct_focus_and_abandonment_releases_every_reservation() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    drain(&renderer);
    let baseline = renderer.allocation_budget().info().unwrap().admitted_bytes;
    let a = renderer.render_target(SIZE, SIZE).unwrap();
    let b = renderer.render_target(SIZE, SIZE).unwrap();
    let mut source = source(&renderer, "OPAQUE");
    let mut pass = ScenePass::new(&renderer, 8).unwrap();
    let mut frame = camera();
    frame.samples = 4;
    frame.depth_of_field = Some(aperture(3.));
    pass.draw(
        &renderer,
        REQUEST,
        &a,
        &mut [&mut source],
        &[instance()],
        &frame,
    )
    .unwrap();
    let sharp = pixels(&renderer, &a, ChannelPrecision::Unorm8).pixels;
    frame.depth_of_field = Some(aperture(6.));
    pass.draw(
        &renderer,
        REQUEST,
        &b,
        &mut [&mut source],
        &[instance()],
        &frame,
    )
    .unwrap();
    let blurred = pixels(&renderer, &b, ChannelPrecision::Unorm8).pixels;
    assert_ne!(sharp, blurred);
    let mut ea = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    frame.depth_of_field = Some(aperture(3.));
    pass.encode(
        &renderer,
        REQUEST,
        &mut ea,
        &a,
        &mut [&mut source],
        &[instance()],
        &frame,
    )
    .unwrap();
    let mut eb = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    frame.depth_of_field = Some(aperture(6.));
    pass.encode(
        &renderer,
        REQUEST,
        &mut eb,
        &b,
        &mut [&mut source],
        &[instance()],
        &frame,
    )
    .unwrap();
    renderer.submit_allocated(eb.finish(), |_| {}).unwrap();
    renderer.submit_allocated(ea.finish(), |_| {}).unwrap();
    assert_eq!(
        pixels(&renderer, &a, ChannelPrecision::Unorm8).pixels,
        sharp
    );
    assert_eq!(
        pixels(&renderer, &b, ChannelPrecision::Unorm8).pixels,
        blurred
    );
    drain(&renderer);
    let mut abandoned = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    pass.encode(
        &renderer,
        REQUEST,
        &mut abandoned,
        &a,
        &mut [&mut source],
        &[instance()],
        &frame,
    )
    .unwrap();
    let held = renderer.allocation_budget().info().unwrap().admitted_bytes;
    drop((pass, source, a, b));
    assert!(renderer.allocation_budget().info().unwrap().admitted_bytes > baseline);
    assert!(renderer.allocation_budget().info().unwrap().admitted_bytes <= held);
    drop(abandoned);
    drain(&renderer);
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        baseline
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn camera_aperture_preserves_native_hdr_premultiplied_clear_and_letterbox() {
    for precision in [ChannelPrecision::Float16, ChannelPrecision::Float32] {
        let renderer = gpu_renderer(precision);
        let target = renderer.render_target(SIZE, SIZE).unwrap();
        let scene = fixture(
            vec![
                json!({"pbrMetallicRoughness":{"baseColorFactor":[1,1,1,1],"metallicFactor":0,"roughnessFactor":1}}),
            ],
            &[(0., 0)],
            None,
            true,
        );
        let mut source =
            GpuScene::upload(&renderer, REQUEST, scene, SceneGpuLimits::default()).unwrap();
        let mut pass = ScenePass::new(&renderer, 8).unwrap();
        let mut frame = camera();
        frame.samples = 4;
        frame.viewport = Some([8., 8., 49., 49.]);
        frame.background = [0.25, 0.5, 0.75, 0.5];
        frame.depth_of_field = Some(aperture(6.));
        let lights = [SceneLight::Directional {
            direction: [0., 0., -1.],
            color: [1.; 3],
            intensity: std::f32::consts::PI * 4.,
        }];
        let mut instance = instance();
        instance.lights = &lights;
        pass.draw(
            &renderer,
            REQUEST,
            &target,
            &mut [&mut source],
            &[instance],
            &frame,
        )
        .unwrap();
        let result = pixels(&renderer, &target, precision);
        let center = center_linear(&result);
        assert!(
            center[..3].iter().all(|v| *v > 2.),
            "HDR must survive the aperture resolve: {precision:?} {center:?}"
        );
        for (x, y) in [(0, 0), (7, 32), (57, 32), (32, 64)] {
            let p = pixel(&result, x, y);
            assert!(
                p.into_iter()
                    .zip([0.125, 0.25, 0.375, 0.5])
                    .all(|(a, b)| (a - b).abs() < 0.001),
                "letterbox/clear must retain exact premultiplied values: {p:?}"
            );
        }
    }
}
