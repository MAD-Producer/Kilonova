//! Region membership is checked independently at every surface pixel centre.
//! The native CPU reference still qualifies the underlying display colours.
use super::*;

fn masked(full: &[u8], cells: &[View<'_>], inspections: &[ViewInspection<'_>]) -> Vec<u8> {
    let mut result = Vec::with_capacity(full.len());
    for y in 0..128 {
        for x in 0..128 {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let visible = cells.iter().zip(inspections).any(|(cell, inspection)| {
                let [vx, vy, vw, vh] = cell.placement.viewport;
                if px < vx || px >= vx + vw || py < vy || py >= vy + vh {
                    return false;
                }
                // Fixture compositions are square. The two-cell default fits
                // its image vertically, while an explicit image may pan/zoom.
                let image = cell.placement.image.unwrap_or_else(|| {
                    let side = vw.min(vh);
                    [vx + (vw - side) / 2., vy + (vh - side) / 2., side, side]
                });
                let u = (px - image[0]) / image[2];
                let v = (py - image[1]) / image[3];
                let [rx, ry, rw, rh] = inspection.region.unwrap();
                u >= rx && u < rx + rw && v >= ry && v < ry + rh
            });
            let at = (y * 128 + x) * 4;
            result.extend_from_slice(if visible {
                &full[at..at + 4]
            } else {
                &[0, 0, 0, 255]
            });
        }
    }
    result
}

fn qualify(events: &winit::event_loop::EventLoop<()>) {
    let reference = reference();
    let runtime = NativeColorRuntime::load(&super::super::runtime()).unwrap();
    let mut pixels_checked = 0;
    let mut native_checked = 0;
    for (case_index, case) in reference
        .cases
        .iter()
        .filter(|case| case.processor.phase == ColorProcessorPhase::InputToWorking)
        .enumerate()
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
            "ROI left",
            [32., 64.],
            [16., 32., 0.],
            case.input[3],
        ));
        let mut right = Layer::solid(2, "ROI right", [32., 64.], [48., 32., 0.], [1.; 4]);
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
        std::fs::remove_file(
            root.path()
                .join(&project.color_management.resources().unwrap()[0].path),
        )
        .unwrap();
        for count in [1, 2, 4] {
            for panned in [false, true] {
                let mut cells = views(count);
                if panned {
                    for cell in &mut cells {
                        let [x, y, w, h] = cell.placement.viewport;
                        cell.placement.image =
                            Some([x - w * 0.25, y - h * 0.125, w * 1.5, h * 1.25]);
                    }
                }
                let full = capture(&mut session, 0.5, &cells, false);
                if !panned {
                    native_checked += check(&full, count, expected);
                }
                let attempts = session.render_attempts;
                for flip in [false, true] {
                    let regions = [
                        [0.25, 0.25, 0.5, 0.5],
                        [0., 0., 0.25, 0.5],
                        [0.5, 0., 0.5, 0.25],
                        [0.5, 0.5, 0.5, 0.5],
                    ];
                    let inspection: Vec<_> = (0..count)
                        .map(|i| ViewInspection {
                            region: Some(regions[if flip { 3 - i } else { i }]),
                            ..Default::default()
                        })
                        .collect();
                    let expected_mask = masked(&full, &cells, &inspection);
                    assert_ne!(expected_mask, full, "disabled ROI must fail the oracle");
                    let surface =
                        capture_inspected(&mut session, 0.5, &cells, flip, 77, Some(&inspection));
                    assert_eq!(
                        surface, expected_mask,
                        "ROI case={case_index} cells={count} panned={panned} flip={flip}"
                    );
                    let screenshot =
                        display_capture::screenshot_inspected(&session, 77, &cells, &inspection)
                            .unwrap();
                    assert!(
                        screenshot
                            .iter()
                            .zip(&expected_mask)
                            .all(|(a, b)| a.abs_diff(*b) <= 1)
                    );
                    if case_index == 0 && !flip {
                        if let Some(evidence) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
                            let evidence =
                                std::path::PathBuf::from(evidence).join("managed-roi-pixels");
                            std::fs::create_dir_all(&evidence).unwrap();
                            for (name, pixels) in [
                                ("full", &full),
                                ("surface", &surface),
                                ("capture", &screenshot),
                                ("expected", &expected_mask),
                            ] {
                                image::RgbaImage::from_raw(128, 128, pixels.clone())
                                    .unwrap()
                                    .save(evidence.join(format!("{count}-pan-{panned}-{name}.png")))
                                    .unwrap();
                            }
                        }
                    }
                    assert_eq!(session.render_attempts, attempts, "ROI must not sample");
                    assert_eq!(session.frame, 0.5);
                    assert_eq!(
                        serde_json::to_vec(session.engine.project()).unwrap(),
                        before
                    );
                    pixels_checked += 128 * 128 * 2;
                }
                for invalid in [
                    [0., 0., 0., 1.],
                    [-0.1, 0., 0.5, 0.5],
                    [0.75, 0., 0.5, 0.5],
                    [f32::NAN, 0., 0.5, 0.5],
                ] {
                    let mut inspection = vec![ViewInspection::default(); count];
                    inspection[count - 1].region = Some(invalid);
                    let presented = session.presented;
                    let error = session
                        .render_views_with_inspection(77, 0.5, &cells, &inspection, |_, _, _| {})
                        .unwrap_err();
                    assert!(error.to_string().contains("phase=inspection"));
                    assert!(
                        display_capture::screenshot_inspected(&session, 77, &cells, &inspection)
                            .is_err()
                    );
                    assert_eq!(
                        session.presented, presented,
                        "invalid final cell must not publish"
                    );
                    assert_eq!(session.render_attempts, attempts);
                }
                assert_eq!(
                    capture(&mut session, 0.5, &cells, true),
                    full,
                    "region must not leak into a later draw"
                );
                assert_eq!(session.render_attempts, attempts);
            }
        }
    }
    assert_eq!(native_checked, 53_760);
    assert_eq!(pixels_checked, 2_359_296);
    println!(
        "managed ROI pixels={pixels_checked}; native CPU pixels={native_checked}; 6 inputs; 1/2/4 cells; pan and zoom; retained and capture; invalid final cells refused; no sampling; restored complete pixels"
    );
}

#[test]
#[ignore = "requires an actual Windows surface, GPU and pinned native OCIO runtime"]
fn managed_roi_clips_each_cell_without_resampling_or_changing_display_codes() {
    let events = winit::event_loop::EventLoop::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    qualify(&events);
}
