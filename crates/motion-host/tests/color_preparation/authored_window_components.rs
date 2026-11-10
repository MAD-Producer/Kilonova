//! Real window and capture consumers of numeric working channels, before matte.
use super::*;
use motion_render::ViewChannel;

fn oracle(
    cells: &[View<'_>],
    inspections: &[ViewInspection<'_>],
    working: [f32; 4],
    color: [u8; 4],
) -> Vec<u8> {
    let mut pixels = Vec::with_capacity(128 * 128 * 4);
    for y in 0..128 {
        for x in 0..128 {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let mut value = [0, 0, 0, 255];
            for (cell, inspection) in cells.iter().zip(inspections) {
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
                let [rx, ry, rw, rh] = inspection.region.unwrap_or([0., 0., 1., 1.]);
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
                value = match inspection.channel {
                    ViewChannel::Color => color,
                    channel => {
                        let code =
                            (working[channel as usize - 1].clamp(0., 1.) * 255.).round() as u8;
                        [code, code, code, 255]
                    }
                };
            }
            pixels.extend(value);
        }
    }
    pixels
}

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
        project.layers.push(Layer::solid(
            1,
            "numeric channels",
            [64., 64.],
            [32., 32., 0.],
            case.input[4],
        ));
        let color = expected(&reference, case, &runtime)[1];
        let working = premult(case.expected[4], case.processor.output_alpha);
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
                let attempts = session.render_attempts;
                for phase in 0..5 {
                    let channels = [
                        ViewChannel::Red,
                        ViewChannel::Green,
                        ViewChannel::Blue,
                        ViewChannel::Alpha,
                    ];
                    let inspections: Vec<_> = (0..count)
                        .map(|i| ViewInspection {
                            channel: if phase == 4 {
                                ViewChannel::Color
                            } else {
                                channels[(i + phase) % 4]
                            },
                            region: (phase % 2 == 1).then_some([0.25, 0.25, 0.5, 0.5]),
                            ..Default::default()
                        })
                        .collect();
                    let expected = oracle(&cells, &inspections, working, color);
                    let surface =
                        capture_inspected(&mut session, 0.5, &cells, true, 77, Some(&inspections));
                    let screenshot =
                        display_capture::screenshot_inspected(&session, 77, &cells, &inspections)
                            .unwrap();
                    for (name, actual) in [("surface", &surface), ("capture", &screenshot)] {
                        assert!(
                            actual
                                .iter()
                                .zip(&expected)
                                .all(|(a, b)| a.abs_diff(*b) <= 1),
                            "{name} case={case_index} cells={count} pan={panned} phase={phase}"
                        );
                    }
                    if phase == 3 {
                        let mut wrong = inspections.clone();
                        wrong[0].channel = ViewChannel::Color;
                        assert_ne!(
                            oracle(&cells, &wrong, working, color),
                            expected,
                            "display alpha must fail numeric oracle"
                        );
                    }
                    if case_index == 0 && count == 4 && !panned {
                        if let Some(path) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
                            let dir =
                                std::path::PathBuf::from(path).join("managed-components-window");
                            std::fs::create_dir_all(&dir).unwrap();
                            image::RgbaImage::from_raw(128, 128, screenshot)
                                .unwrap()
                                .save(dir.join(format!("phase-{phase}.png")))
                                .unwrap();
                        }
                    }
                    assert_eq!(
                        session.render_attempts, attempts,
                        "inspection must not sample"
                    );
                    assert_eq!(session.frame, 0.5);
                    assert_eq!(
                        serde_json::to_vec(session.engine.project()).unwrap(),
                        before
                    );
                    assert_eq!(session.engine.history_diagnostics(), history);
                    checked += 128 * 128 * 2;
                }
            }
        }
    }
    assert_eq!(checked, 5_898_240);
    println!(
        "managed component window/capture pixels={checked}; six inputs; independent 1/2/4 cells; ROI/pan; color restored; no sampling or model/history mutation"
    );
}

#[test]
#[ignore = "requires an actual Windows surface, GPU and pinned native OCIO runtime"]
fn managed_components_preserve_raw_values_across_window_capture_roi_and_color_restore() {
    const CHILD: &str = "MOTION_OCIO_COMPONENT_WINDOW_CHILD";
    const TEST: &str = "authored::window::components::managed_components_preserve_raw_values_across_window_capture_roi_and_color_restore";
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
            panic!("component window exceeded 120s: kill={killed:?} wait={joined:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = std::fs::read_to_string(root.path().join("stdout")).unwrap();
    let stderr = std::fs::read_to_string(root.path().join("stderr")).unwrap();
    if let Some(path) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
        let dir = std::path::PathBuf::from(path).join("managed-components-window");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("stdout"), &stdout).unwrap();
        std::fs::write(dir.join("stderr"), &stderr).unwrap();
    }
    assert!(status.success(), "{status}\n{stdout}\n{stderr}");
    assert!(stdout.contains("window/capture pixels=5898240"), "{stdout}");
}
