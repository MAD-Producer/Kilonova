use super::*;
use std::fs;
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::DeviceId,
    event_loop::EventLoop,
    platform::windows::EventLoopBuilderExtWindows,
};

fn capture(
    events: &mut EventLoop<()>,
    shell: &mut Shell,
    owner: Option<Panel>,
    size: [f32; 2],
    scale: f32,
    path: PathBuf,
) {
    shell.atlas.set_pixel_ratio(scale);
    let (paint, actions) = draw(shell, owner, size, &Input::default());
    assert!(actions.is_empty());
    let mut ui = shell.ui_frame(paint, size, true);
    ui.window = u64::from(owner.is_some());
    if owner.is_some() {
        ui.composition = None;
        ui.composition_image = None;
        ui.node_preview = None;
    }
    let front = shell
        .engine
        .present_async(crate::engine::UiFrame {
            paint: PaintList::default(),
            atlas: None,
            logical_size: size,
            composition: ui.composition,
            composition_image: ui.composition_image,
            window: ui.window,
            node_preview: None,
        })
        .unwrap();
    assert!(
        crate::native_test_support::receive(events, &front, Duration::from_secs(30))
            .unwrap()
            .unwrap()
    );
    crate::native_test_support::capture_ui(events, &shell.engine, ui, path.clone()).unwrap();
    let image = image::open(path).unwrap().to_rgba8();
    assert_eq!(
        image.dimensions(),
        (
            (size[0] * scale).round() as u32,
            (size[1] * scale).round() as u32
        )
    );
    assert!(image.pixels().any(|p| p.0[2] > 100));
}
fn move_to(shell: &mut Shell, source: WindowId, owner: Option<Panel>, point: [f32; 2]) {
    let (source_window, _) = if shell.window.as_ref().unwrap().id() == source {
        (shell.window.as_ref().unwrap().clone(), shell.scale)
    } else {
        let f = &shell.floating[&source];
        (f.window.clone(), f.scale)
    };
    let (target_window, scale) = if let Some(panel) = owner {
        let f = shell.floating.values().find(|f| f.panel == panel).unwrap();
        (f.window.clone(), f.scale)
    } else {
        (shell.window.as_ref().unwrap().clone(), shell.scale)
    };
    let source_origin = source_window.inner_position().unwrap();
    let target_origin = target_window.inner_position().unwrap();
    let position = PhysicalPosition::new(
        target_origin.x as f64 + point[0] as f64 * scale as f64 - source_origin.x as f64,
        target_origin.y as f64 + point[1] as f64 * scale as f64 - source_origin.y as f64,
    );
    assert!(shell.route_effect_drag_event(
        source,
        &WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position
        }
    ));
    let (actual_owner, actual_point) = shell.panels.effect_drag.pointer_owner().unwrap();
    assert_eq!(actual_owner, owner);
    assert!((actual_point[0] - point[0]).abs() < 0.01);
    assert!((actual_point[1] - point[1]).abs() < 0.01);
}
fn release(shell: &mut Shell, source: WindowId) {
    assert!(shell.route_effect_drag_event(
        source,
        &WindowEvent::MouseInput {
            device_id: DeviceId::dummy(),
            state: ElementState::Released,
            button: WinitMouseButton::Left
        }
    ));
}
#[test]
fn effect_drag_native_windows_capture_targets_and_preserve_release_coordinates() {
    const CHILD: &str = "MOTION_EFFECT_DRAG_NATIVE_CHILD";
    const NAME:&str="shell::effect_drag::integration_tests::native::effect_drag_native_windows_capture_targets_and_preserve_release_coordinates";
    let Some(root) = std::env::var_os(CHILD) else {
        let folder = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_EFFECT_DRAG_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| folder.path().into());
        fs::create_dir_all(&root).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env(CHILD, &root)
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        fs::write(root.join("native.stdout.log"), &output.stdout).unwrap();
        fs::write(root.join("native.stderr.log"), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "status={}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let logs = fs::read_to_string(root.join("motion-studio.log")).unwrap();
        for marker in [
            "controlled client position failure",
            "effect drag cancelled phase=window_coordinates",
            "missing-effect-drag-hash",
        ] {
            assert!(logs.contains(marker), "file missing {marker}");
            assert!(stderr.contains(marker), "stderr missing {marker}");
            assert!(!stdout.contains(marker), "stdout polluted with {marker}");
        }
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.clone()).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    let mut shell = fixture(&root.join("project"), Locale::Zh);
    #[allow(deprecated)]
    let main = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_decorations(false)
                    .with_position(PhysicalPosition::new(0, 0))
                    .with_inner_size(PhysicalSize::new(1600, 1000)),
            )
            .unwrap(),
    );
    #[allow(deprecated)]
    let floating = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_decorations(false)
                    .with_position(PhysicalPosition::new(1800, 0))
                    .with_inner_size(PhysicalSize::new(360, 500)),
            )
            .unwrap(),
    );
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let RawWindowHandle::Win32(handle) = main.window_handle().unwrap().as_raw() else {
            panic!("expected Win32");
        };
        assert!(matches_native_hit(&main, Some(handle.hwnd.get())).unwrap());
        assert!(!matches_native_hit(&main, Some(0)).unwrap());
        assert_eq!(
            native_window_at([-1_000_000., -1_000_000.]),
            0,
            "outside desktop is a normal miss"
        );
    }
    let attach = shell.engine.attach_async(main.clone(), 1600, 1000).unwrap();
    shell.state =
        crate::native_test_support::receive(&mut events, &attach, Duration::from_secs(30))
            .unwrap()
            .unwrap();
    shell.state = shell
        .engine
        .float_window(1, floating.clone(), 360, 500)
        .unwrap();
    shell.window = Some(main.clone());
    shell.scale = 1.;
    shell.attached = true;
    shell.floating.insert(
        floating.id(),
        FloatWindow {
            window: floating.clone(),
            key: 1,
            panel: Panel::Effects,
            input: Input::default(),
            paint_input: Input::default(),
            pointer_position: Default::default(),
            scale: 1.,
        },
    );
    let before = shell.state["project"].clone();
    begin(&mut shell, Some(Panel::Effects), [360., 500.]);
    assert!(draw(&mut shell, None, [1600., 1000.], &Input::default())
        .1
        .is_empty());
    let row = center(shell.panels.effect_drag.target_rect(Some(1)).unwrap());
    move_to(&mut shell, floating.id(), None, row);
    assert!(shell.route_effect_drag_event(
        floating.id(),
        &WindowEvent::CursorLeft {
            device_id: DeviceId::dummy()
        }
    ));
    assert!(
        shell.panels.effect_drag.pending(),
        "internal crossing must not cancel"
    );
    capture(
        &mut events,
        &mut shell,
        None,
        [1600., 1000.],
        1.,
        root.join("batch-target-zh.png"),
    );
    capture(
        &mut events,
        &mut shell,
        Some(Panel::Effects),
        [360., 500.],
        1.,
        root.join("library-source-zh.png"),
    );
    draw(&mut shell, None, [1600., 1000.], &Input::default());
    let locked = center(shell.panels.effect_drag.target_rect(Some(3)).unwrap());
    move_to(&mut shell, floating.id(), None, locked);
    capture(
        &mut events,
        &mut shell,
        None,
        [1600., 1000.],
        1.,
        root.join("locked-target-zh.png"),
    );
    assert_eq!(
        shell.engine.state().unwrap()["project"],
        before,
        "hover must never add instances"
    );
    move_to(&mut shell, floating.id(), None, row);
    release(&mut shell, floating.id());
    let actions = draw(&mut shell, None, [1600., 1000.], &Input::default()).1;
    assert_eq!(actions.len(), 1);
    for action in actions {
        shell.action(action, None).unwrap();
    }
    assert_eq!(
        shell.input.mouse, row,
        "next main click keeps its actual destination coordinates"
    );
    assert!(shell.input.held.is_none() && shell.floating[&floating.id()].input.held.is_none());
    let after = shell.state["project"].clone();
    assert_eq!(after["layers"][0]["effects"].as_array().unwrap().len(), 1);
    assert_eq!(after["layers"][1]["effects"].as_array().unwrap().len(), 1);
    capture(
        &mut events,
        &mut shell,
        None,
        [1600., 1000.],
        1.,
        root.join("applied-zh.png"),
    );
    shell.state = shell.engine.history(0).unwrap();
    assert_eq!(shell.state["project"], before);
    // The minimum main window uses a real resized surface and the same target geometry.
    let _ = main.request_inner_size(PhysicalSize::new(900, 600));
    let resize = shell.engine.resize_async(0, 900, 600).unwrap();
    shell.state =
        crate::native_test_support::receive(&mut events, &resize, Duration::from_secs(30))
            .unwrap()
            .unwrap();
    begin(&mut shell, Some(Panel::Effects), [360., 500.]);
    draw(&mut shell, None, [900., 600.], &Input::default());
    let minimum_target = center(shell.panels.effect_drag.target_rect(Some(1)).unwrap());
    move_to(&mut shell, floating.id(), None, minimum_target);
    capture(
        &mut events,
        &mut shell,
        None,
        [900., 600.],
        1.,
        root.join("minimum-target-zh.png"),
    );
    assert!(draw(
        &mut shell,
        None,
        [900., 600.],
        &Input {
            keys: vec![(Key::Escape, Default::default())],
            ..Default::default()
        }
    )
    .1
    .is_empty());
    assert!(!shell.panels.effect_drag.pending());
    assert_eq!(shell.engine.state().unwrap()["project"], before);
    let _ = main.request_inner_size(PhysicalSize::new(1600, 1000));
    let resize = shell.engine.resize_async(0, 1600, 1000).unwrap();
    shell.state =
        crate::native_test_support::receive(&mut events, &resize, Duration::from_secs(30))
            .unwrap()
            .unwrap();
    // Reverse direction, actual native clients and 100/150/200 percent target DPI.
    shell.workspace.root = motion_ui::dock::Node::dock(Panel::Effects.dock());
    shell.floating.get_mut(&floating.id()).unwrap().panel = Panel::Timeline;
    for (index, scale) in [1., 1.5, 2.].into_iter().enumerate() {
        shell.panels.locale = Locale::En;
        shell.workspace.locale = Locale::En;
        shell.panels.browser.preview_categories(Locale::En);
        shell.floating.get_mut(&floating.id()).unwrap().scale = scale;
        let size = [360. / scale, 500. / scale];
        shell.atlas.set_pixel_ratio(1.);
        begin(&mut shell, None, [1600., 1000.]);
        draw(&mut shell, Some(Panel::Timeline), size, &Input::default());
        let point = center(shell.panels.effect_drag.target_rect(Some(1)).unwrap());
        move_to(&mut shell, main.id(), Some(Panel::Timeline), point);
        capture(
            &mut events,
            &mut shell,
            Some(Panel::Timeline),
            size,
            scale,
            root.join(format!("floating-target-dpi-{index}.png")),
        );
        release(&mut shell, main.id());
        let actions = draw(&mut shell, Some(Panel::Timeline), size, &Input::default()).1;
        assert!(matches!(actions.as_slice(), [panels::Action::Plugin(_)]));
        for action in actions {
            shell.action(action, None).unwrap();
        }
        assert_eq!(shell.floating[&floating.id()].input.mouse, point);
        shell.state = shell.engine.history(0).unwrap();
        assert_eq!(shell.state["project"], before);
    }
    // Geometry failure has one Error, a visible explanation and no pending/delayed edit.
    begin(&mut shell, None, [1600., 1000.]);
    shell.cancel_effect_drag_coordinates(main.id(), "controlled client position failure");
    assert!(!shell.panels.effect_drag.pending());
    assert!(shell
        .panels
        .message
        .as_deref()
        .unwrap()
        .contains("controlled client position failure"));
    assert_eq!(shell.engine.state().unwrap()["project"], before);
    // A package disappearing after UI validation still goes through the existing worker error channel.
    let entry =
        panels::effect_controls::entries(&shell.panels.effect_controls.catalogue, Locale::En)
            .remove(0);
    let mut request = entry.command(1);
    request["hash"] = json!("missing-effect-drag-hash");
    let error = shell
        .action(panels::Action::Plugin(request), None)
        .unwrap_err();
    assert!(!error.is_empty());
    assert_eq!(shell.engine.state().unwrap()["project"], before);
    assert!(shell.panels.message.as_deref().unwrap().contains(&error));
    shell.panels.browser.preview_categories(Locale::En);
    shell.floating.clear();
    shell.engine.close_window(1).unwrap();
    shell.window = None;
    drop(shell);
    drop(floating);
    drop(main);
    crate::logging::flush();
}
