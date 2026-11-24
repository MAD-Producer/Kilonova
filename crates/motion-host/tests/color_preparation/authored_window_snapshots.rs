//! Frozen managed cell comparison, including retained windows and captures.
use super::*;
use motion_render::ViewChannel;

fn qualify() {
    let events = winit::event_loop::EventLoop::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    let reference = reference();
    let runtime = NativeColorRuntime::load(&super::super::runtime()).unwrap();
    let mut checked = 0;
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
        let mut layer = Layer::solid(
            1,
            "frozen comparison",
            [64., 64.],
            [32., 32., 0.],
            case.input[3],
        );
        let mut vector = VectorContent::shape(ShapeKind::Rectangle);
        let mut color = Track::constant(case.input[3]);
        color
            .upsert(0, case.input[3], motion_core::Ease::Hold)
            .unwrap();
        color
            .upsert(1, case.input[4], motion_core::Ease::Hold)
            .unwrap();
        vector.fill = Some(color);
        layer.content = Content::Vector { vector };
        project.layers.push(layer);
        let colors = expected(&reference, case, &runtime);
        let working = [3, 4].map(|i| premult(case.expected[i], case.processor.output_alpha));
        let before = serde_json::to_vec(&project).unwrap();
        let mut session = Session::new(
            project.clone(),
            root.path().to_owned(),
            Arc::new(session_installation::NativePlatform(super::super::runtime())),
        )
        .unwrap();
        attach(&mut session, &events);
        std::fs::remove_file(
            root.path()
                .join(&project.color_management.resources().unwrap()[0].path),
        )
        .unwrap();
        let history = session.engine.history_diagnostics();
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
                capture(&mut session, 0.5, &cells, false);
                let reads = session
                    .graphics
                    .as_ref()
                    .unwrap()
                    .renderer
                    .image_readback_info();
                let snapshot = session.capture_managed_view_cell(77, count - 1).unwrap();
                assert_eq!(
                    session
                        .graphics
                        .as_ref()
                        .unwrap()
                        .renderer
                        .image_readback_info(),
                    reads
                );
                assert!(session.capture_managed_view_cell(78, 0).is_err());
                assert!(session.capture_managed_view_cell(77, count).is_err());
                capture(&mut session, 1.5, &cells, false);
                let attempts = session.render_attempts;
                for phase in 0..5 {
                    let inspections: Vec<_> = (0..count)
                        .map(|i| ViewInspection {
                            channel: if phase == 4 {
                                ViewChannel::Color
                            } else {
                                [
                                    ViewChannel::Red,
                                    ViewChannel::Green,
                                    ViewChannel::Blue,
                                    ViewChannel::Alpha,
                                ][(i + phase) % 4]
                            },
                            region: (phase % 2 == 1).then_some([0.25, 0.25, 0.5, 0.5]),
                            managed_comparison: Some((
                                &snapshot,
                                [0., 0.5, 1., 0.25][(phase + i) % 4],
                            )),
                            ..Default::default()
                        })
                        .collect();
                    let mut expected = Vec::with_capacity(128 * 128 * 4);
                    for y in 0..128 {
                        for x in 0..128 {
                            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                            let mut pixel = [0, 0, 0, 255];
                            for (cell, inspection) in cells.iter().zip(&inspections) {
                                let [vx, vy, vw, vh] = cell.placement.viewport;
                                if px < vx || px >= vx + vw || py < vy || py >= vy + vh {
                                    continue;
                                }
                                let side = vw.min(vh);
                                let [ix, iy, iw, ih] = cell.placement.image.unwrap_or([
                                    vx + (vw - side) / 2.,
                                    vy + (vh - side) / 2.,
                                    side,
                                    side,
                                ]);
                                let (u, v) = ((px - ix) / iw, (py - iy) / ih);
                                let [rx, ry, rw, rh] =
                                    inspection.region.unwrap_or([0., 0., 1., 1.]);
                                if u < 0.
                                    || u >= 1.
                                    || v < 0.
                                    || v >= 1.
                                    || u < rx
                                    || u >= rx + rw
                                    || v < ry
                                    || v >= ry + rh
                                {
                                    continue;
                                }
                                let which = if u >= inspection.managed_comparison.unwrap().1 {
                                    0
                                } else {
                                    1
                                };
                                pixel = match inspection.channel {
                                    ViewChannel::Color => colors[which],
                                    channel => {
                                        let c = (working[which][channel as usize - 1].clamp(0., 1.)
                                            * 255.)
                                            .round()
                                            as u8;
                                        [c, c, c, 255]
                                    }
                                };
                            }
                            expected.extend(pixel);
                        }
                    }
                    let surface =
                        capture_inspected(&mut session, 1.5, &cells, true, 77, Some(&inspections));
                    let screenshot =
                        display_capture::screenshot_inspected(&session, 77, &cells, &inspections)
                            .unwrap();
                    for (name, pixels) in [("surface", &surface), ("capture", &screenshot)] {
                        assert!(pixels.iter().zip(&expected).all(|(a,b)|a.abs_diff(*b)<=1),
                        "snapshot {name} case={case_index} count={count} pan={panned} phase={phase}");
                    }
                    if case_index == 0 && count == 4 && phase == 4 {
                        if let Some(path) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
                            let dir =
                                std::path::PathBuf::from(path).join("managed-snapshot-window");
                            std::fs::create_dir_all(&dir).unwrap();
                            image::RgbaImage::from_raw(128, 128, screenshot)
                                .unwrap()
                                .save(dir.join(format!("comparison-pan-{panned}.png")))
                                .unwrap();
                        }
                    }
                    assert_eq!(session.render_attempts, attempts);
                    assert_eq!(session.frame, 1.5);
                    assert_eq!(
                        serde_json::to_vec(session.engine.project()).unwrap(),
                        before
                    );
                    assert_eq!(session.engine.history_diagnostics(), history);
                    checked += 128 * 128 * 2;
                }
                let mut invalid = vec![ViewInspection::default(); count];
                invalid[count - 1].managed_comparison = Some((&snapshot, f32::NAN));
                assert!(session
                    .render_views_with_inspection(77, 2., &cells, &invalid, |_, _, _| panic!(
                        "invalid snapshot published"
                    ))
                    .is_err());
                assert_eq!(session.frame, 1.5);
                assert_eq!(session.render_attempts, attempts);
                session.frame = 2.;
                assert!(session
                    .capture_managed_view_cell(77, 0)
                    .err()
                    .expect("stale snapshot accepted")
                    .to_string()
                    .contains("no longer current"));
                session.frame = 1.5;
            }
        }
    }
    assert_eq!(checked, 5_898_240);
    println!("managed frozen window/capture pixels={checked}; independent channels/ROI/pan; snapshot survives front reuse; invalid final cell and stale owners refused");
}

#[test]
#[ignore = "requires an actual Windows surface, GPU and pinned native OCIO runtime"]
fn managed_snapshots_compare_frozen_cells_without_resampling_or_color_reinterpretation() {
    const TEST:&str="authored::window::snapshots::managed_snapshots_compare_frozen_cells_without_resampling_or_color_reinterpretation";
    const CHILD: &str = "MOTION_OCIO_SNAPSHOT_WINDOW_CHILD";
    if std::env::var_os(CHILD).is_some() {
        qualify();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            TEST,
            "--include-ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, "1")
        .stdout(std::fs::File::create(root.path().join("stdout")).unwrap())
        .stderr(std::fs::File::create(root.path().join("stderr")).unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let killed = child.kill();
            let joined = child.wait();
            panic!("managed snapshot exceeded 120s: kill={killed:?} wait={joined:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = std::fs::read_to_string(root.path().join("stdout")).unwrap();
    let stderr = std::fs::read_to_string(root.path().join("stderr")).unwrap();
    if let Some(path) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
        let dir = std::path::PathBuf::from(path).join("managed-snapshot-window");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("stdout"), &stdout).unwrap();
        std::fs::write(dir.join("stderr"), &stderr).unwrap();
    }
    assert!(status.success(), "{status}\n{stdout}\n{stderr}");
    assert!(stdout.contains("window/capture pixels=5898240"), "{stdout}");
}
