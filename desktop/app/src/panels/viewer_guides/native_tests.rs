//! Hidden native surfaces, real chrome routes, GPU pixels and the production PNG export command.
use super::*;
use crate::engine::{AtlasUpload, Command as WorkerCommand, Engine, UiFrame};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use winit::{
    dpi::PhysicalSize, event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows,
    window::Window,
};

fn draw(
    engine: &Engine,
    panels: &mut Panels,
    atlas: &mut TextAtlas,
    window: u64,
    size: [f32; 2],
    input: Input,
) -> (PaintList, Vec<Action>) {
    let mut paint = PaintList::default();
    let state = engine.state().unwrap();
    let actions = if window == 0 {
        crate::panels::chrome(
            &mut paint,
            atlas,
            size,
            &dock::Node::dock(Panel::Composition.dock()),
            panels,
            &state,
            &input,
        )
    } else {
        crate::panels::floating_chrome(
            &mut paint,
            atlas,
            Panel::Composition,
            size,
            panels,
            &state,
            &input,
        )
    };
    (paint, actions)
}
fn view(panels: &Panels, window: u64, size: [f32; 2]) -> Rect {
    let sheet = panels
        .styles
        .clone()
        .unwrap_or_else(motion_ui::style::default_stylesheet_arc);
    if window == 0 {
        crate::panels::composition_view_styled(
            &dock::Node::dock(Panel::Composition.dock()),
            size,
            &sheet,
        )
    } else {
        crate::panels::floating_view_styled(Panel::Composition, size, &sheet).unwrap()
    }
}
fn capture(
    events: &mut EventLoop<()>,
    engine: &Engine,
    panels: &mut Panels,
    atlas: &mut TextAtlas,
    window: u64,
    size: [f32; 2],
    ratio: f32,
    path: &Path,
) -> image::RgbaImage {
    atlas.set_pixel_ratio(ratio);
    let (paint, actions) = draw(engine, panels, atlas, window, size, Input::default());
    assert!(actions.is_empty());
    let viewport = view(panels, window, size);
    let mapping = panels
        .composition
        .navigation
        .mapping(viewport, [640., 480.])
        .unwrap();
    let rect = |r: Rect| {
        [
            r.min[0] * ratio,
            r.min[1] * ratio,
            r.width() * ratio,
            r.height() * ratio,
        ]
    };
    let composition = Some(rect(viewport));
    let composition_image = Some(rect(mapping.image));
    let presented = engine
        .present_async(UiFrame {
            paint: PaintList::default(),
            atlas: None,
            logical_size: size,
            composition,
            inspection: panels.viewer_inspection.frame(window != 0, panels.viewer_guides.channel(window != 0)),
            composition_image,
            window,
            node_preview: None,
        })
        .unwrap();
    assert!(
        crate::native_test_support::receive(events, &presented, Duration::from_secs(30))
            .unwrap()
            .unwrap()
    );
    let config = atlas.config();
    crate::native_test_support::capture_ui(
        events,
        engine,
        UiFrame {
            paint,
            atlas: Some(AtlasUpload {
                pixels: atlas.pixel_data().to_vec(),
                width: config.width,
                height: config.height,
            }),
            logical_size: size,
            composition,
            inspection: panels.viewer_inspection.frame(window != 0, panels.viewer_guides.channel(window != 0)),
            composition_image,
            window,
            node_preview: None,
        },
        path.into(),
    )
    .unwrap();
    let pixels = image::open(path).unwrap().to_rgba8();
    assert_eq!(
        pixels.dimensions(),
        ((size[0] * ratio) as u32, (size[1] * ratio) as u32)
    );
    pixels
}
fn dispatch(events: &mut EventLoop<()>, engine: &Engine, actions: Vec<Action>) {
    for action in actions {
        let (receive, hit) = match action {
            Action::EditorAction { source, commands } => (
                engine
                    .request(|reply| WorkerCommand::EditorAction {
                        source,
                        commands,
                        reply,
                    })
                    .unwrap(),
                false,
            ),
            Action::History(operation) => (engine.history_async(operation).unwrap(), false),
            Action::Edit(command) => (
                engine
                    .edit_async(serde_json::to_string(&command).unwrap())
                    .unwrap(),
                false,
            ),
            Action::EditBatch(commands) => (
                engine
                    .edit_async(serde_json::to_string(&commands).unwrap())
                    .unwrap(),
                false,
            ),
            Action::PreviewDrag {
                object,
                delta,
                size,
            } => (
                engine.drag_async(object as i64, delta, size).unwrap(),
                false,
            ),
            Action::SelectAt(point) => (
                engine
                    .request(|reply| WorkerCommand::HitCandidates {
                        x: point[0],
                        y: point[1],
                        reply,
                    })
                    .unwrap(),
                true,
            ),
            _ => panic!("unexpected action behind guide interaction"),
        };
        let value = crate::native_test_support::receive(events, &receive, Duration::from_secs(30))
            .unwrap()
            .unwrap();
        if hit {
            assert_eq!(value["candidates"][0]["id"], 7);
        }
    }
}
fn press(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        pressed: Some((MouseButton::Left, point)),
        held: Some(MouseButton::Left),
        ..Default::default()
    }
}
fn release(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        released: Some((MouseButton::Left, point)),
        ..Default::default()
    }
}

fn open_inspection(engine: &Engine, panels: &mut Panels, atlas: &mut TextAtlas, window: u64, size: [f32; 2]) -> Rect {
    let viewport = view(panels, window, size);
    let sheet = PaintList::default().styles();
    let button = [viewport.min[0] + sheet.metric("viewer.zoom.width", 64.)
        + sheet.metric("viewer.percent.width", 14.) + sheet.metric("viewer.controls.gap", 4.) * 2.
        + sheet.metric("viewer.fit.width", 40.) + sheet.metric("viewer.overlay.button.width", 64.) * 0.5,
        viewport.max[1] + sheet.metric("viewer.canvas.bottom", 6.) + sheet.metric("viewer.controls.height", 28.) * 0.5];
    assert!(draw(engine, panels, atlas, window, size, press(button)).1.is_empty());
    let point = panels.viewer_guides.menu.as_ref().unwrap().point;
    let padding = sheet.metric("padding.panel", 8.);
    let row = sheet.metric("row.height", 22.);
    let width = sheet.metric("viewer.overlay.menu.width", 232.).min(size[0]);
    let x = point[0].clamp(0., (size[0] - width).max(0.));
    let top = if window == 0 { CHROME_HEIGHT } else { sheet.metric("viewer.floating.top", 30.) };
    let height = (row * 10. + padding * 2.).min((size[1] - top).max(0.));
    let y = (point[1] - height).clamp(top.min(size[1]), (size[1] - height).max(top.min(size[1])));
    let actions = draw(engine, panels, atlas, window, size, press([x + padding + 10., y + padding + row * 9.5])).1;
    let [Action::OpenViewerInspection { floating, point }] = actions.as_slice() else { panic!("inspection entry: {actions:?}") };
    assert_eq!(*floating, window != 0);
    let point = *point;
    panels.viewer_inspection.open(*floating, point, &engine.state().unwrap());
    assert!(draw(engine, panels, atlas, window, size, Input::default()).1.is_empty());
    let height = (row * 6. + padding * 2.).min((size[1] - top).max(0.));
    let x = point[0].clamp(0., (size[0] - width).max(0.));
    let y = (point[1] - height).clamp(top.min(size[1]), (size[1] - height).max(top.min(size[1])));
    Rect::new(x + padding, y + padding, width - padding * 2., row)
}

#[test]
#[ignore = "manual native GPU viewer-channel captures; executed explicitly with retained artifacts"]
fn viewer_channel_native_main_float_locales_pixels_and_export() {
    const CHILD: &str = "MOTION_VIEWER_CHANNEL_NATIVE_CHILD";
    const NAME: &str = "panels::viewer_guides::native_tests::viewer_channel_native_main_float_locales_pixels_and_export";
    let Some(root) = std::env::var_os(CHILD) else {
        let temporary = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_VIEWER_CHANNEL_ARTIFACTS")
            .map(PathBuf::from).unwrap_or_else(|| temporary.path().into());
        std::fs::create_dir_all(&root).unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--ignored", "--nocapture", "--test-threads=1"])
            .env_remove("MOTION_LOG").env(CHILD, &root)
            .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped())
            .spawn().unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(240);
        loop {
            if child.try_wait().unwrap().is_some() { break; }
            if std::time::Instant::now() >= deadline {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
                std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
                panic!("native viewer channel capture timed out");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let output = child.wait_with_output().unwrap();
        std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
        std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
        assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.clone()).unwrap();
    let project_root = tempfile::tempdir().unwrap();
    let mut project = motion_core::Project::new(640, 480, 30, 24).unwrap();
    project.layers.push(motion_core::Layer::solid(7, "Channel fixture", [80., 60.], [320., 240., 0.], [0.8, 0.2, 0.1, 1.]));
    motion_core::storage::save(project_root.path(), &project).unwrap();
    let engine = Engine::start(project_root.path().into(), motion_desktop_media::platform(), 8 << 30).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let main = Arc::new(events.create_window(Window::default_attributes().with_visible(false).with_inner_size(PhysicalSize::new(900, 600))).unwrap());
    #[allow(deprecated)]
    let float = Arc::new(events.create_window(Window::default_attributes().with_visible(false).with_inner_size(PhysicalSize::new(720, 640))).unwrap());
    let attached = engine.attach_async(main.clone(), 900, 600).unwrap();
    crate::native_test_support::receive(&mut events, &attached, Duration::from_secs(30)).unwrap().unwrap();
    engine.float_window(1, float.clone(), 720, 640).unwrap();
    let export_before = root.join("export-before.png");
    let exported = engine.capture_async(export_before.clone(), Default::default()).unwrap();
    crate::native_test_support::receive(&mut events, &exported, Duration::from_secs(60)).unwrap().unwrap();
    let before_state = engine.state().unwrap();
    let before = before_state["project"].clone();
    let mut atlas = TextAtlas::from_family(motion_ui::style::default_stylesheet_arc().font_family(), 12., true).unwrap();
    for (locale_name, locale) in [("zh", Locale::Zh), ("en", Locale::En)] {
        let mut panels = Panels::default();
        panels.locale = locale;
        panels.focused = Some(Panel::Composition);
        for (window_name, window, size, ratio) in [
            ("main-min", 0, [900., 600.], 1.),
            ("main-default", 0, [1600., 1000.], 1.5),
            ("float", 1, [360., 320.], 2.),
        ] {
            let dimensions = [(size[0] * ratio) as u32, (size[1] * ratio) as u32];
            let _ = if window == 0 { main.request_inner_size(PhysicalSize::new(dimensions[0], dimensions[1])) }
                else { float.request_inner_size(PhysicalSize::new(dimensions[0], dimensions[1])) };
            let resized = engine.resize_async(window, dimensions[0], dimensions[1]).unwrap();
            crate::native_test_support::receive(&mut events, &resized, Duration::from_secs(30)).unwrap().unwrap();
            let viewport = view(&panels, window, size);
            let mapping = panels.composition.navigation.mapping(viewport, [640., 480.]).unwrap();
            let sample = mapping.screen([320., 240.]);
            let sheet = PaintList::default().styles();
            let button_x = viewport.min[0] + sheet.metric("viewer.zoom.width", 64.)
                + sheet.metric("viewer.percent.width", 14.) + sheet.metric("viewer.controls.gap", 4.) * 2.
                + sheet.metric("viewer.fit.width", 40.) + sheet.metric("viewer.overlay.button.width", 64.) * 0.5;
            let button_y = viewport.max[1] + sheet.metric("viewer.canvas.bottom", 6.) + sheet.metric("viewer.controls.height", 28.) * 0.5;
            let color = capture(&mut events, &engine, &mut panels, &mut atlas, window, size, ratio, &root.join(format!("{locale_name}-{window_name}-color.png")));
            let initial = *color.get_pixel((sample[0] * ratio) as u32, (sample[1] * ratio) as u32);
            let mut selected = Vec::new();
            for (index, name) in [(1, "red"), (2, "green"), (3, "blue"), (4, "alpha"), (0, "restore")] {
                assert!(draw(&engine, &mut panels, &mut atlas, window, size, press([button_x, button_y])).1.is_empty());
                let point = panels.viewer_guides.menu.as_ref().unwrap().point;
                let padding = sheet.metric("padding.panel", 8.);
                let row = sheet.metric("row.height", 22.);
                let width = sheet.metric("viewer.overlay.menu.width", 232.).min(size[0]);
                let x = point[0].clamp(0., (size[0] - width).max(0.));
                let top = if window == 0 { CHROME_HEIGHT } else { sheet.metric("viewer.floating.top", 30.) };
                let y = (point[1] - row * 10. - padding * 2.).clamp(top, (size[1] - row * 10. - padding * 2.).max(top));
                let channel_x = x + width - padding - sheet.metric("viewer.overlay.button.width", 64.) * 0.5;
                let anchor_y = y + padding + row * 9.;
                draw(&engine, &mut panels, &mut atlas, window, size, press([channel_x, anchor_y + row * 0.5]));
                assert!(panels.viewer_guides.channel_dropdown.open);
                draw(&engine, &mut panels, &mut atlas, window, size, press([channel_x, anchor_y - row * (4.5 - index as f32)]));
                assert!(!panels.viewer_guides.channel_dropdown.open);
                assert_eq!(panels.viewer_guides.channel(window == 0), motion_render::ViewChannel::Color);
                draw(&engine, &mut panels, &mut atlas, window, size, press([size[0] - 1., 0.]));
                assert!(panels.viewer_guides.menu.is_none());
                let pixels = capture(&mut events, &engine, &mut panels, &mut atlas, window, size, ratio, &root.join(format!("{locale_name}-{window_name}-{name}.png")));
                let pixel = *pixels.get_pixel((sample[0] * ratio) as u32, (sample[1] * ratio) as u32);
                if index == 0 { assert_eq!(pixel, initial); }
                else { assert_eq!(pixel[0], pixel[1]); assert_eq!(pixel[1], pixel[2]); assert_eq!(pixel[3], 255); selected.push(pixel[0]); }
            }
            assert!(selected[0] > selected[1] && selected[1] > selected[2] && selected[3] > selected[0], "{selected:?}");

            // Drive the actual chrome entry and the same asynchronous commands used by the shell.
            let row = sheet.metric("row.height", 22.);
            let menu = open_inspection(&engine, &mut panels, &mut atlas, window, size);
            let row_point = |index: f32| [menu.min[0] + menu.width() * 0.5, menu.min[1] + row * (index + 0.5)];
            let actions = draw(&engine, &mut panels, &mut atlas, window, size, press(row_point(0.))).1;
            assert!(matches!(actions.as_slice(), [Action::CaptureViewerSnapshot { floating }] if *floating == (window != 0)));
            let source = crate::panels::viewer_inspection::identity(&engine.state().unwrap()).unwrap();
            panels.viewer_inspection.set_busy(window != 0, true);
            let captured = engine.capture_viewer_snapshot_async(window).unwrap();
            crate::native_test_support::receive(&mut events, &captured, Duration::from_secs(30)).unwrap().unwrap();
            panels.viewer_inspection.capture_done(window != 0, &source, true);

            let menu = open_inspection(&engine, &mut panels, &mut atlas, window, size);
            let click = |index: f32| [menu.min[0] + menu.width() * 0.5, menu.min[1] + row * (index + 0.5)];
            let was_comparing = panels.viewer_inspection.frame(window != 0, motion_render::ViewChannel::Color).comparison_split.is_some();
            assert!(draw(&engine, &mut panels, &mut atlas, window, size, press(click(2.))).1.is_empty());
            assert_eq!(panels.viewer_inspection.frame(window != 0, motion_render::ViewChannel::Color).comparison_split.is_some(), !was_comparing);
            if was_comparing {
                assert!(draw(&engine, &mut panels, &mut atlas, window, size, press(click(2.))).1.is_empty());
            }
            assert_eq!(panels.viewer_inspection.frame(window != 0, motion_render::ViewChannel::Color).comparison_split, Some(0.5));
            let actions = draw(&engine, &mut panels, &mut atlas, window, size, press(click(1.))).1;
            assert!(matches!(actions.as_slice(), [Action::SaveViewerSnapshot { floating }] if *floating == (window != 0)));
            let saved = root.join(format!("{locale_name}-{window_name}-ui-snapshot.png"));
            let receive = engine.save_viewer_snapshot_async(window, saved.clone(), motion_render::ViewChannel::Color).unwrap();
            crate::native_test_support::receive(&mut events, &receive, Duration::from_secs(30)).unwrap().unwrap();
            assert!(image::open(saved).unwrap().to_rgba8().get_pixel(320, 240)[0] > 0);

            let menu = open_inspection(&engine, &mut panels, &mut atlas, window, size);
            let select = [menu.min[0] + menu.width() * 0.5, menu.min[1] + row * 3.5];
            assert!(draw(&engine, &mut panels, &mut atlas, window, size, press(select)).1.is_empty());
            let start = mapping.screen([160., 120.]);
            let end = mapping.screen([480., 360.]);
            assert!(draw(&engine, &mut panels, &mut atlas, window, size, press(start)).1.is_empty());
            assert!(draw(&engine, &mut panels, &mut atlas, window, size, release(end)).1.is_empty());
            let region = panels.viewer_inspection.frame(window != 0, motion_render::ViewChannel::Color).region.unwrap();
            for (actual, expected) in region.into_iter().zip([0.25, 0.25, 0.5, 0.5]) {
                assert!((actual - expected).abs() < 1e-6, "region={region:?}");
            }
            let inspected = capture(&mut events, &engine, &mut panels, &mut atlas, window, size, ratio, &root.join(format!("{locale_name}-{window_name}-ui-region.png")));
            assert_eq!(*inspected.get_pixel((sample[0] * ratio) as u32, (sample[1] * ratio) as u32), initial);
        }
    }
    assert_eq!(engine.state().unwrap()["project"], before);
    let after_state = engine.state().unwrap();
    for key in ["revision", "history", "canUndo", "canRedo"] { assert_eq!(after_state[key], before_state[key], "{key}"); }
    let export_after = root.join("export-after.png");
    let exported = engine.capture_async(export_after.clone(), Default::default()).unwrap();
    crate::native_test_support::receive(&mut events, &exported, Duration::from_secs(60)).unwrap().unwrap();
    assert_eq!(std::fs::read(export_before).unwrap(), std::fs::read(export_after).unwrap());
}

#[test]
#[ignore = "manual native GPU viewer snapshot captures; executed explicitly with retained artifacts"]
fn viewer_snapshot_native_preserves_pixels_history_windows_and_failure_logs() {
    const CHILD: &str = "MOTION_VIEWER_SNAPSHOT_NATIVE_CHILD";
    const NAME: &str = "panels::viewer_guides::native_tests::viewer_snapshot_native_preserves_pixels_history_windows_and_failure_logs";
    let Some(root) = std::env::var_os(CHILD) else {
        let temporary = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_VIEWER_SNAPSHOT_ARTIFACTS")
            .map(PathBuf::from).unwrap_or_else(|| temporary.path().into());
        std::fs::create_dir_all(&root).unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--ignored", "--nocapture", "--test-threads=1"])
            .env_remove("MOTION_LOG").env(CHILD, &root)
            .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(240);
        loop {
            if child.try_wait().unwrap().is_some() { break; }
            if std::time::Instant::now() >= deadline {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
                std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
                panic!("native viewer snapshot capture timed out");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let output = child.wait_with_output().unwrap();
        std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
        std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
        assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("viewer snapshot failed operation=save") && stderr.contains("phase=save"), "{stderr}");
        assert!(!String::from_utf8_lossy(&output.stdout).contains("viewer snapshot failed"));
        let log = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        assert!(log.contains("capture_viewer_snapshot") && log.contains("save_viewer_snapshot"));
        assert!(log.contains("viewer snapshot failed operation=save") && log.contains("phase=save"));
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.clone()).unwrap();
    let project_root = tempfile::tempdir().unwrap();
    let mut project = motion_core::Project::new(640, 480, 30, 24).unwrap();
    project.background = [0., 0., 0., 0.];
    project.layers.push(motion_core::Layer::solid(7, "Snapshot fixture", [640., 480.], [320., 240., 0.], [0.8, 0.2, 0.1, 0.25]));
    motion_core::storage::save(project_root.path(), &project).unwrap();
    let engine = Engine::start(project_root.path().into(), motion_desktop_media::platform(), 8 << 30).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let main = Arc::new(events.create_window(Window::default_attributes().with_visible(false).with_inner_size(PhysicalSize::new(900, 600))).unwrap());
    #[allow(deprecated)]
    let float = Arc::new(events.create_window(Window::default_attributes().with_visible(false).with_inner_size(PhysicalSize::new(720, 640))).unwrap());
    let attached = engine.attach_async(main, 900, 600).unwrap();
    crate::native_test_support::receive(&mut events, &attached, Duration::from_secs(30)).unwrap().unwrap();
    engine.float_window(1, float, 720, 640).unwrap();
    let missing_frame = engine.capture_viewer_snapshot_async(0).unwrap();
    assert!(crate::native_test_support::receive(&mut events, &missing_frame, Duration::from_secs(30)).unwrap().is_err());
    let before = engine.state().unwrap();
    let mut atlas = TextAtlas::from_family(motion_ui::style::default_stylesheet_arc().font_family(), 12., true).unwrap();
    let frame = |window, inspection, atlas: Option<AtlasUpload>| UiFrame {
        paint: PaintList::default(), atlas, logical_size: if window == 0 { [900., 600.] } else { [720., 640.] },
        composition: Some([50., 50., 640., 480.]), composition_image: Some([50., 50., 640., 480.]),
        inspection, window, node_preview: None,
    };
    let present = |events: &mut EventLoop<()>, window, inspection| {
        let reply = engine.present_async(frame(window, inspection, None)).unwrap();
        assert!(crate::native_test_support::receive(events, &reply, Duration::from_secs(30)).unwrap().unwrap());
    };
    for window in [0, 1] {
        present(&mut events, window, Default::default());
        let reply = engine.capture_viewer_snapshot_async(window).unwrap();
        let result = crate::native_test_support::receive(&mut events, &reply, Duration::from_secs(60)).unwrap().unwrap();
        assert_eq!(result["window"], window);
    }
    let save = |events: &mut EventLoop<()>, window, name: &str, channel| {
        let path = root.join(name);
        let reply = engine.save_viewer_snapshot_async(window, path.clone(), channel).unwrap();
        crate::native_test_support::receive(events, &reply, Duration::from_secs(60)).unwrap().unwrap();
        image::open(path).unwrap().to_rgba8()
    };
    let color = save(&mut events, 0, "snapshot-color.png", motion_render::ViewChannel::Color);
    let color = color.get_pixel(320, 240);
    assert!(color[0] > color[1] && color[1] > color[2] && (60..=68).contains(&color[3]), "{color:?}");
    let alpha = save(&mut events, 0, "snapshot-alpha.png", motion_render::ViewChannel::Alpha);
    let alpha = alpha.get_pixel(320, 240);
    assert!((60..=68).contains(&alpha[0]) && alpha[0] == alpha[1] && alpha[1] == alpha[2] && alpha[3] == 255, "{alpha:?}");
    let bad_path = root.join("absent").join("snapshot.png");
    let failed = engine.save_viewer_snapshot_async(0, bad_path, motion_render::ViewChannel::Color).unwrap();
    assert!(crate::native_test_support::receive(&mut events, &failed, Duration::from_secs(60)).unwrap().is_err());
    for key in ["project", "revision", "history", "canUndo", "canRedo"] { assert_eq!(engine.state().unwrap()[key], before[key], "{key}"); }
    engine.edit(serde_json::to_string(&Command::SetColor { object: 7, value: [0.1, 0.2, 0.8, 0.25] }).unwrap()).unwrap();
    for window in [0, 1] {
        let inspection = crate::engine::ViewerInspectionFrame { channel: motion_render::ViewChannel::Color,
            region: Some([0.25, 0.25, 0.5, 0.5]), comparison_split: Some(0.5) };
        present(&mut events, window, inspection);
        let config = atlas.config();
        let upload = AtlasUpload { pixels: atlas.pixel_data().to_vec(), width: config.width, height: config.height };
        let path = root.join(format!("comparison-{window}.png"));
        crate::native_test_support::capture_ui(&mut events, &engine, frame(window, inspection, Some(upload)), path.clone()).unwrap();
        let pixels = image::open(path).unwrap().to_rgba8();
        let outside = pixels.get_pixel(100, 290);
        let live = pixels.get_pixel(250, 290);
        let captured = pixels.get_pixel(490, 290);
        assert!(live[2] > live[0] && captured[0] > captured[2], "live={live:?} captured={captured:?}");
        assert!(outside[0] < live[0] && outside[2] < live[2], "outside={outside:?} live={live:?}");
        let cleared = crate::engine::ViewerInspectionFrame { region: Some([0., 0., 0.25, 0.25]), ..Default::default() };
        present(&mut events, window, cleared);
        let config = atlas.config();
        let upload = AtlasUpload { pixels: atlas.pixel_data().to_vec(), width: config.width, height: config.height };
        let path = root.join(format!("cleared-{window}.png"));
        crate::native_test_support::capture_ui(&mut events, &engine, frame(window, cleared, Some(upload)), path.clone()).unwrap();
        let pixels = image::open(path).unwrap().to_rgba8();
        assert_eq!(*pixels.get_pixel(490, 290), *pixels.get_pixel(100, 290));
    }
    let preserved = save(&mut events, 0, "snapshot-after-edit.png", motion_render::ViewChannel::Color);
    assert_eq!(preserved.get_pixel(320, 240), color);
    engine.close_window(1).unwrap();
    let closed = engine.save_viewer_snapshot_async(1, root.join("closed.png"), motion_render::ViewChannel::Color).unwrap();
    assert!(crate::native_test_support::receive(&mut events, &closed, Duration::from_secs(30)).unwrap().is_err());
}

fn line_pixels(pixels: &image::RgbaImage, mapping: Mapping, viewport: Rect, ratio: f32) -> Value {
    let position = mapping.screen([333.25, 251.5]);
    let mut measurements = vec![];
    for axis in 0..2 {
        for fraction in [0.25, 0.5, 0.75] {
            let other = viewport.min[1 - axis] + viewport.height() * fraction;
            let other = if axis == 0 {
                other
            } else {
                viewport.min[0] + viewport.width() * fraction
            };
            let expected = position[axis] * ratio;
            let row = (other * ratio).floor() as u32;
            let mut weight = 0.;
            let mut sum = 0.;
            for offset in -5..=5 {
                let index = expected.floor() as i32 + offset;
                if index < 0 {
                    continue;
                }
                let (x, y) = if axis == 0 {
                    (index as u32, row)
                } else {
                    (row, index as u32)
                };
                if x >= pixels.width() || y >= pixels.height() {
                    continue;
                }
                let [r, g, b, _] = pixels.get_pixel(x, y).0.map(i32::from);
                // Antialiasing over the red scene keeps red above green; the
                // blue guide still exceeds both. Keep probes over that scene.
                if b > g + 16 && b > r + 8 {
                    let w = (b - r) as f64;
                    weight += w;
                    sum += (index as f64 + 0.5) * w;
                }
            }
            assert!(
                weight > 0.,
                "missing guide pixels axis={axis} expected={expected} probe={other}"
            );
            let actual = sum / weight;
            assert!(
                (actual - f64::from(expected)).abs() < 1.5,
                "axis={axis} expected={expected} actual={actual}"
            );
            measurements.push(serde_json::json!({"axis":axis,"expected_physical":expected,"actual_physical":actual,"probe_logical":other}));
        }
    }
    let red = mapping
        .screen([285., 225.])
        .map(|v| (v * ratio).floor() as u32);
    let [r, g, b, _] = pixels.get_pixel(red[0], red[1]).0.map(i32::from);
    assert!(
        r > g + 40 && r > b + 40,
        "composition image must use the same zoom/pan mapping: {red:?} color={:?}",
        [r, g, b]
    );
    serde_json::json!(measurements)
}

#[test]
#[ignore = "manual native GPU viewer-guide captures; executed explicitly with retained artifacts"]
fn viewer_guides_native_main_float_dpi_transactions_pixels_and_export() {
    const CHILD: &str = "MOTION_GUIDES_NATIVE_CHILD";
    const NAME:&str="panels::viewer_guides::native_tests::viewer_guides_native_main_float_dpi_transactions_pixels_and_export";
    let Some(root) = std::env::var_os(CHILD) else {
        let temporary = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_GUIDES_NATIVE_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| temporary.path().into());
        std::fs::create_dir_all(&root).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                NAME,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env_remove("MOTION_LOG")
            .env(CHILD, &root)
            .output()
            .unwrap();
        std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
        std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.clone()).unwrap();
    let project_root = tempfile::tempdir().unwrap();
    let mut project = motion_core::Project::new(640, 480, 30, 24).unwrap();
    project.layers.push(motion_core::Layer::solid(
        7,
        "Native guide fixture",
        [80., 60.],
        [320., 240., 0.],
        [0.8, 0.2, 0.1, 1.],
    ));
    motion_core::storage::save(project_root.path(), &project).unwrap();
    let engine = Engine::start(
        project_root.path().into(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let main = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(900, 600)),
            )
            .unwrap(),
    );
    #[allow(deprecated)]
    let float = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(720, 640)),
            )
            .unwrap(),
    );
    let attached = engine.attach_async(main.clone(), 900, 600).unwrap();
    crate::native_test_support::receive(&mut events, &attached, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    engine.float_window(1, float.clone(), 720, 640).unwrap();
    let export_before = root.join("export-before.png");
    let exported = engine
        .capture_async(export_before.clone(), Default::default())
        .unwrap();
    crate::native_test_support::receive(&mut events, &exported, Duration::from_secs(60))
        .unwrap()
        .unwrap();
    let guides = vec![
        Guide {
            id: 1,
            axis: GuideAxis::X,
            position: 333.25,
        },
        Guide {
            id: 2,
            axis: GuideAxis::Y,
            position: 251.5,
        },
    ];
    engine
        .edit(
            serde_json::to_string(&Command::SetGuides {
                guides: guides.clone(),
            })
            .unwrap(),
        )
        .unwrap();
    let mut atlas = TextAtlas::from_family(
        motion_ui::style::default_stylesheet_arc().font_family(),
        12.,
        true,
    )
    .unwrap();
    let mut evidence = vec![];
    for (name, window, size, ratio, locale, zoom) in [
        ("en-main-fit", 0, [900., 600.], 1., Locale::En, None),
        (
            "zh-main-zoom",
            0,
            [1600., 1000.],
            1.5,
            Locale::Zh,
            Some(1.2),
        ),
        ("en-float-min", 1, [360., 320.], 2., Locale::En, None),
        ("zh-float-zoom", 1, [720., 520.], 1., Locale::Zh, Some(2.)),
    ] {
        let dimensions = [(size[0] * ratio) as u32, (size[1] * ratio) as u32];
        let _ = if window == 0 {
            main.request_inner_size(PhysicalSize::new(dimensions[0], dimensions[1]))
        } else {
            float.request_inner_size(PhysicalSize::new(dimensions[0], dimensions[1]))
        };
        let resized = engine
            .resize_async(window, dimensions[0], dimensions[1])
            .unwrap();
        crate::native_test_support::receive(&mut events, &resized, Duration::from_secs(30))
            .unwrap()
            .unwrap();
        let mut panels = Panels::default();
        panels.locale = locale;
        panels.focused = Some(Panel::Composition);
        panels.select_only(Some(7));
        atlas.set_pixel_ratio(ratio);
        assert!(draw(
            &engine,
            &mut panels,
            &mut atlas,
            window,
            size,
            Input::default()
        )
        .1
        .is_empty());
        let viewport = view(&panels, window, size);
        if let Some(zoom) = zoom {
            assert!(panels.composition.navigation.zoom_at(
                viewport,
                [640., 480.],
                [
                    (viewport.min[0] + viewport.max[0]) * 0.5,
                    (viewport.min[1] + viewport.max[1]) * 0.5
                ],
                zoom
            ));
            assert!(panels
                .composition
                .navigation
                .pan(viewport, [640., 480.], [13., -17.]));
        }
        panels.viewer_guides.visibility = Visibility {
            safe: true,
            grid: true,
            rulers: true,
            guides: true,
            handles: false,
            locked: false,
        };
        let mapping = panels
            .composition
            .navigation
            .mapping(viewport, [640., 480.])
            .unwrap();
        let pixels = capture(
            &mut events,
            &engine,
            &mut panels,
            &mut atlas,
            window,
            size,
            ratio,
            &root.join(format!("{name}-overlays.png")),
        );
        let measurements = line_pixels(&pixels, mapping, viewport, ratio);
        let before = engine.state().unwrap()["project"].clone();
        let sheet = PaintList::default().styles();
        let x = viewport.min[0]
            + sheet.metric("viewer.zoom.width", 64.)
            + sheet.metric("viewer.percent.width", 14.)
            + sheet.metric("viewer.controls.gap", 4.) * 2.
            + sheet.metric("viewer.fit.width", 40.)
            + sheet.metric("viewer.overlay.button.width", 64.) * 0.5;
        let y = viewport.max[1]
            + sheet.metric("viewer.canvas.bottom", 6.)
            + sheet.metric("viewer.controls.height", 28.) * 0.5;
        assert!(draw(
            &engine,
            &mut panels,
            &mut atlas,
            window,
            size,
            press([x, y])
        )
        .1
        .is_empty());
        assert!(panels.viewer_guides.menu.is_some());
        capture(
            &mut events,
            &engine,
            &mut panels,
            &mut atlas,
            window,
            size,
            ratio,
            &root.join(format!("{name}-menu.png")),
        );
        let menu_point = panels.viewer_guides.menu.as_ref().unwrap().point;
        let menu_width = sheet.metric("viewer.overlay.menu.width", 232.);
        let row = sheet.metric("row.height", 22.);
        let padding = sheet.metric("padding.panel", 8.);
        let card_x = menu_point[0].clamp(0., (size[0] - menu_width).max(0.));
        let height = row * 10. + padding * 2.;
        let card_y = (menu_point[1] - height).clamp(0., (size[1] - height).max(0.));
        let grid_row = [card_x + padding + row, card_y + padding + row * 1.5];
        assert!(draw(
            &engine,
            &mut panels,
            &mut atlas,
            window,
            size,
            press(grid_row)
        )
        .1
        .is_empty());
        assert!(!panels.viewer_guides.visibility.grid);
        assert_eq!(engine.state().unwrap()["project"], before);
        assert!(draw(
            &engine,
            &mut panels,
            &mut atlas,
            window,
            size,
            press(grid_row)
        )
        .1
        .is_empty());
        assert!(panels.viewer_guides.visibility.grid);
        assert!(draw(
            &engine,
            &mut panels,
            &mut atlas,
            window,
            size,
            press([viewport.min[0] + 60., viewport.min[1] + 60.])
        )
        .1
        .is_empty());
        assert!(panels.viewer_guides.menu.is_none());
        let start = [
            mapping.screen([333.25, 240.])[0],
            mapping.screen([320., 240.])[1],
        ];
        assert!(
            draw(&engine, &mut panels, &mut atlas, window, size, press(start))
                .1
                .is_empty()
        );
        assert!(panels.viewer_guides.dragging());
        let moved = [start[0] + 20., start[1]];
        assert!(draw(
            &engine,
            &mut panels,
            &mut atlas,
            window,
            size,
            Input {
                mouse: moved,
                held: Some(MouseButton::Left),
                modifiers: motion_ui::input::Modifiers {
                    alt: true,
                    ..Default::default()
                },
                ..Default::default()
            }
        )
        .1
        .is_empty());
        capture(
            &mut events,
            &engine,
            &mut panels,
            &mut atlas,
            window,
            size,
            ratio,
            &root.join(format!("{name}-preview.png")),
        );
        assert_eq!(engine.state().unwrap()["project"], before);
        assert!(draw(
            &engine,
            &mut panels,
            &mut atlas,
            window,
            size,
            Input {
                keys: vec![(Key::Escape, Default::default())],
                ..Default::default()
            }
        )
        .1
        .is_empty());
        assert!(!panels.viewer_guides.dragging());
        assert_eq!(engine.state().unwrap()["project"], before);
        let ruler = [
            viewport.min[0] + 10.,
            (viewport.min[1] + viewport.max[1]) * 0.5,
        ];
        assert!(
            draw(&engine, &mut panels, &mut atlas, window, size, press(ruler))
                .1
                .is_empty()
        );
        let release_point = mapping.screen([335.125, 240.]);
        let authored_position = mapping.local(release_point)[0];
        let mut released = release(release_point);
        released.modifiers.alt = true;
        let (_, actions) = draw(&engine, &mut panels, &mut atlas, window, size, released);
        assert_eq!(actions.len(), 1);
        dispatch(&mut events, &engine, actions);
        assert_eq!(
            engine.state().unwrap()["project"]["guides"][2]["position"],
            authored_position
        );
        assert_eq!(engine.history(0).unwrap()["project"], before);
        // Keep the original grip tolerances even when a nearby guide overlaps.
        panels.viewer_guides.visibility.handles = true;
        let scale_start = mapping.screen([360., 240.]);
        let target_grip = mapping.screen([333.25, 240.]);
        let scale_end = [target_grip[0] + 3., target_grip[1]];
        assert!(draw(
            &engine,
            &mut panels,
            &mut atlas,
            window,
            size,
            press(scale_start)
        )
        .1
        .is_empty());
        assert!(panels.transform_controls.gesture_active() && !panels.viewer_guides.dragging());
        let actions = draw(
            &engine,
            &mut panels,
            &mut atlas,
            window,
            size,
            Input {
                mouse: scale_end,
                held: Some(MouseButton::Left),
                ..Default::default()
            },
        )
        .1;
        dispatch(&mut events, &engine, actions);
        let actions = draw(
            &engine,
            &mut panels,
            &mut atlas,
            window,
            size,
            release(scale_end),
        )
        .1;
        dispatch(&mut events, &engine, actions);
        let scaled = engine.state().unwrap();
        let projection =
            crate::panels::path_controls::layer_projection(&scaled, 7, mapping.image).unwrap();
        let grip = projection.screen([40., 0.]).unwrap();
        assert!((grip[0] - mapping.screen([333.25, 240.])[0]).abs() < 0.1);
        capture(
            &mut events,
            &engine,
            &mut panels,
            &mut atlas,
            window,
            size,
            ratio,
            &root.join(format!("{name}-scale.png")),
        );
        assert_eq!(engine.history(0).unwrap()["project"], before);
        let move_start = mapping.screen([295., 230.]);
        let goal = mapping.screen([333.25, 251.5]);
        let pivot = mapping.screen([320., 240.]);
        let move_end = [
            move_start[0] + goal[0] - pivot[0] - 3.,
            move_start[1] + goal[1] - pivot[1] + 2.,
        ];
        let actions = draw(
            &engine,
            &mut panels,
            &mut atlas,
            window,
            size,
            press(move_start),
        )
        .1;
        dispatch(&mut events, &engine, actions);
        let actions = draw(
            &engine,
            &mut panels,
            &mut atlas,
            window,
            size,
            Input {
                mouse: move_end,
                held: Some(MouseButton::Left),
                ..Default::default()
            },
        )
        .1;
        dispatch(&mut events, &engine, actions);
        let actions = draw(
            &engine,
            &mut panels,
            &mut atlas,
            window,
            size,
            release(move_end),
        )
        .1;
        dispatch(&mut events, &engine, actions);
        let moved_state = engine.state().unwrap();
        let projection =
            crate::panels::path_controls::layer_projection(&moved_state, 7, mapping.image).unwrap();
        let anchor = projection.screen([0., 0.]).unwrap();
        assert!((anchor[0] - goal[0]).abs() < 0.1 && (anchor[1] - goal[1]).abs() < 0.1);
        assert_eq!(engine.history(0).unwrap()["project"], before);
        panels.viewer_guides.visibility.handles = false;
        assert!(
            draw(&engine, &mut panels, &mut atlas, window, size, press(start))
                .1
                .is_empty()
        );
        let other = 1 - window;
        assert!(draw(
            &engine,
            &mut panels,
            &mut atlas,
            other,
            [900., 600.],
            press([200., 200.])
        )
        .1
        .is_empty());
        assert!(!panels.viewer_guides.dragging());
        assert_eq!(engine.state().unwrap()["project"], before);
        assert!(
            draw(&engine, &mut panels, &mut atlas, window, size, press(start))
                .1
                .is_empty()
        );
        let mut replacement = guides.clone();
        replacement[0].position = 345.;
        let changed = engine
            .edit(
                serde_json::to_string(&Command::SetGuides {
                    guides: replacement,
                })
                .unwrap(),
            )
            .unwrap();
        assert!(draw(
            &engine,
            &mut panels,
            &mut atlas,
            window,
            size,
            release(moved)
        )
        .1
        .is_empty());
        assert!(!panels.viewer_guides.dragging());
        assert_eq!(engine.state().unwrap()["project"], changed["project"]);
        assert_eq!(engine.history(0).unwrap()["project"], before);
        let keys = Input {
            keys: vec![(
                Key::H,
                motion_ui::input::Modifiers {
                    control: true,
                    shift: true,
                    alt: false,
                },
            )],
            ..Default::default()
        };
        assert!(draw(&engine, &mut panels, &mut atlas, window, size, keys)
            .1
            .is_empty());
        assert!(panels.viewer_guides.visibility.handles);
        let handles = capture(
            &mut events,
            &engine,
            &mut panels,
            &mut atlas,
            window,
            size,
            ratio,
            &root.join(format!("{name}-handles.png")),
        );
        assert_ne!(pixels.as_raw(), handles.as_raw());
        assert_eq!(engine.state().unwrap()["project"], before);
        evidence.push(serde_json::json!({"case":name,"window":window,"logical_size":size,"pixel_ratio":ratio,"measurements":measurements,
            "guide_commit_undo":true,"authored_position_from_actual_input":authored_position,"cancel":true,"foreign_input_cancel":true,"stale_revision_cancel":true,
            "handle_toggle":true,"grid_checkbox":true,"scale_snap_undo":true,"move_snap_undo":true}));
    }
    let export_after = root.join("export-after.png");
    let exported = engine
        .capture_async(export_after.clone(), Default::default())
        .unwrap();
    crate::native_test_support::receive(&mut events, &exported, Duration::from_secs(60))
        .unwrap()
        .unwrap();
    assert_eq!(
        image::open(export_before).unwrap().to_rgba8(),
        image::open(export_after).unwrap().to_rgba8()
    );
    std::fs::write(root.join("measurements.json"),serde_json::to_vec_pretty(&serde_json::json!({"cases":evidence,"production_png_export_rgba_equal":true,"physical_os_input":false})).unwrap()).unwrap();
}
