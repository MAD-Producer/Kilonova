//! A different attachment captures the completed native window display codes.
use super::*;
use motion_effects::{
    WorkingSpace,
    image_contract::{ImageContract, ImageMeaning},
};

fn screenshot(session: &Session, owner: u64, cells: &[View<'_>]) -> motion_host::Result<Vec<u8>> {
    screenshot_inspected(session, owner, cells, &[ViewInspection::default(); 4][..cells.len()])
}
pub(super) fn screenshot_inspected(
    session: &Session, owner: u64, cells: &[View<'_>], inspections: &[ViewInspection<'_>],
) -> motion_host::Result<Vec<u8>> {
    let g = session.graphics.as_ref().unwrap();
    let target = g.renderer.image_capture_target(
        128,
        128,
        ImageContract::new(
            ImageMeaning::Color {
                space: WorkingSpace::Linear,
                alpha: AlphaMode::Premultiplied,
            },
            ChannelPrecision::Unorm8,
        ),
    )?;
    let mut encoder = g.renderer.allocation_command_encoder(&Default::default())?;
    let capture = session.encode_managed_view_capture(
        owner,
        cells,
        inspections,
        &mut encoder,
        &target,
        [128, 128],
    )?;
    g.renderer.submit_allocated(encoder.finish(), |ledger| {
        ledger.merge(target.texture_resource_ledger());
        capture.collect_texture_resources(ledger);
    })?;
    Ok(g.renderer.read_image_target(&target)?.pixels)
}

fn refuse_targets(session: &Session, cells: &[View<'_>]) {
    use motion_effects::image_contract::DataChannels;
    let g = session.graphics.as_ref().unwrap();
    let contract = |alpha, precision| {
        ImageContract::new(
            ImageMeaning::Color {
                space: WorkingSpace::Linear,
                alpha,
            },
            precision,
        )
    };
    let foreign = pollster::block_on(Renderer::headless_with_working_precision(
        ChannelPrecision::Float32,
        Default::default(),
    ))
    .unwrap();
    let target = foreign
        .image_capture_target(
            128,
            128,
            contract(AlphaMode::Premultiplied, ChannelPrecision::Unorm8),
        )
        .unwrap();
    let error = |target: &motion_render::ImageCaptureTarget, extent| {
        let mut encoder = g
            .renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        session
            .encode_managed_view_capture(
                77,
                cells,
                &[ViewInspection::default()],
                &mut encoder,
                target,
                extent,
            )
            .err()
            .expect("unqualified target accepted")
            .to_string()
    };
    assert!(error(&target, [128, 128]).contains("image target belongs to another renderer"));
    for invalid in [
        contract(AlphaMode::Straight, ChannelPrecision::Unorm8),
        contract(AlphaMode::Premultiplied, ChannelPrecision::Float32),
        ImageContract::new(
            ImageMeaning::Data {
                channels: DataChannels::Vector4,
            },
            ChannelPrecision::Unorm8,
        ),
    ] {
        let target = g.renderer.image_capture_target(128, 128, invalid).unwrap();
        assert!(error(&target, [128, 128]).contains("qualified sRGB display capture contract"));
    }
    let target = g
        .renderer
        .image_capture_target(
            128,
            128,
            contract(AlphaMode::Premultiplied, ChannelPrecision::Unorm8),
        )
        .unwrap();
    assert!(error(&target, [129, 128]).contains("capture extent differs from the request"));
}

pub(super) fn qualify(events: &winit::event_loop::EventLoop<()>) {
    let reference = reference();
    let runtime = NativeColorRuntime::load(&super::super::runtime()).unwrap();
    let mut checked = 0;
    for case in reference
        .cases
        .iter()
        .filter(|case| case.processor.phase == ColorProcessorPhase::InputToWorking)
    {
        let root = tempfile::tempdir().unwrap();
        let mut project = document(
            &reference,
            case.config,
            case.processor.input_alpha,
            root.path(),
        );
        settings(&mut project).display.transfer = ColorTransfer::Srgb;
        project.layers.push(Layer::solid(
            1,
            "capture left",
            [32., 64.],
            [16., 32., 0.],
            case.input[3],
        ));
        let mut right = Layer::solid(2, "capture right", [32., 64.], [48., 32., 0.], [1.; 4]);
        let mut vector = VectorContent::shape(ShapeKind::Rectangle);
        vector.fill = Some(Track::constant(case.input[4]));
        right.content = Content::Vector { vector };
        project.layers.push(right);
        let expected = expected(&reference, case, &runtime);
        let before = serde_json::to_vec(&project).unwrap();
        let mut session = Session::new(
            project.clone(),
            root.path().to_owned(),
            Arc::new(session_installation::NativePlatform(super::super::runtime())),
        )
        .unwrap();
        attach(&mut session, events);
        assert!(
            screenshot(&session, 77, &views(1))
                .unwrap_err()
                .to_string()
                .contains("completed group is no longer current")
        );
        std::fs::remove_file(
            root.path()
                .join(&project.color_management.resources().unwrap()[0].path),
        )
        .unwrap();
        for count in [1, 2, 4] {
            let cells = views(count);
            let frame = count as f64 / 8.;
            let surface = capture(&mut session, frame, &cells, false);
            if checked == 0 {
                refuse_targets(&session, &cells);
            }
            let attempts = session.render_attempts;
            let presented = session.presented;
            let last = session.last_presented_frame;
            let pixels = screenshot(&session, 77, &cells).unwrap();
            checked += check(&pixels, count, expected);
            assert!(
                surface
                    .iter()
                    .zip(&pixels)
                    .all(|(a, b)| a.abs_diff(*b) <= 1)
            );
            assert_eq!(screenshot(&session, 77, &cells).unwrap(), pixels);
            assert!(
                screenshot(&session, 78, &cells)
                    .unwrap_err()
                    .to_string()
                    .contains("completed group is no longer current")
            );
            session.frame = frame + 0.0625;
            assert!(
                screenshot(&session, 77, &cells)
                    .unwrap_err()
                    .to_string()
                    .contains("completed group is no longer current")
            );
            session.frame = frame;
            assert_eq!(screenshot(&session, 77, &cells).unwrap(), pixels);
            assert_eq!(
                session.render_attempts, attempts,
                "capture must not resample"
            );
            assert_eq!(
                session.presented, presented,
                "capture must not publish a viewer frame"
            );
            assert_eq!(session.last_presented_frame, last);
            assert_eq!(
                serde_json::to_vec(session.engine.project()).unwrap(),
                before
            );
        }
    }
    assert_eq!(checked, 53_760);
    println!(
        "managed display capture CPU pixels={checked}; 6 inputs; 1/2/4 views; native surface comparison; configuration deleted; wrong owner and stale clock refused; no sampling or publication"
    );
}
