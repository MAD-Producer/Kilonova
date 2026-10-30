//! Real model-library menus and native pixels through the production actions.
use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

const NAME: &str = "panels::project::tests::native::models::model_library_native_menu_and_placement_reuse_owned_sources_in_both_languages";
const CHILD: &str = "MOTION_MODEL_LIBRARY_UI_CHILD";

#[test]
#[ignore = "manual native model-library screenshots and ownership transactions"]
fn model_library_native_menu_and_placement_reuse_owned_sources_in_both_languages() {
    if let Some(out) = std::env::var_os(CHILD) {
        capture_model_case(out.into());
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_MODEL_LIBRARY_UI_ARTIFACTS").map_or_else(
        || temporary.path().join("native-model-library"),
        PathBuf::from,
    );
    std::fs::create_dir_all(&root).unwrap();
    let output = root.join(format!(
        "capture-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&output).unwrap();
    let mut cases = Vec::new();
    for case in ["zh-1600", "en-1600", "zh-900", "en-900"] {
        let out = output.join(case);
        std::fs::create_dir(&out).unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                NAME,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, &out)
            .env("MOTION_MODEL_LIBRARY_UI_CASE", case)
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        std::fs::write(out.join("child.stdout.txt"), &result.stdout).unwrap();
        std::fs::write(out.join("child.stderr.txt"), &result.stderr).unwrap();
        assert!(
            result.status.success(),
            "{case}: {}\n{}\n{}",
            result.status,
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        let receipt: Value =
            serde_json::from_slice(&std::fs::read(out.join("accepted.json")).unwrap()).unwrap();
        cases.push(receipt);
    }
    std::fs::write(root.join("accepted.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "cases":cases,"nativeGpu":true,"physicalOsInputInjected":false,
        "scope":"Controlled production project-drop/menu actions, default1600 workspace and existing Project/EffectControls tab docking at900, 1x fixture projection, both locales"
    })).unwrap()).unwrap();
}

fn capture_model_case(out: PathBuf) {
    let case = std::env::var("MOTION_MODEL_LIBRARY_UI_CASE").unwrap();
    let locale = if case.starts_with("zh-") {
        Locale::Zh
    } else {
        Locale::En
    };
    let size = if case.ends_with("900") {
        [900., 600.]
    } else {
        [1600., 1000.]
    };
    let _logger = crate::logging::init(out.join("logs")).unwrap();
    let root = out.join("project");
    motion_core::storage::save(&root, &Project::new(960, 540, 30, 180).unwrap()).unwrap();
    let original = out.join("originals");
    let source = crate::engine::model_import_tests::triangle(&original, "obj");
    let mut shell = Shell::new(root.clone(), None, Some(locale)).unwrap();
    shell.home = None;
    shell.workspace.root = dock::editor_layout();
    shell.workspace.locale = locale;
    shell.panels.locale = locale;
    let before = shell.state.clone();
    let refusal = shell
        .engine
        .request(|reply| engine::Command::ProjectAsset {
            source: crate::layer_clipboard::KeyframeSource::of(&before),
            operation: crate::workflow::library_media::Request::Model {
                file: out.join("missing-library.obj"),
                folder: None,
            },
            reply,
        })
        .unwrap()
        .recv_timeout(Duration::from_secs(30))
        .unwrap()
        .unwrap_err();
    assert!(
        refusal.contains("model_register") && refusal.contains("missing-library.obj"),
        "{refusal}"
    );
    assert_eq!(shell.engine.state().unwrap()["project"], before["project"]);
    // Enter the same captured action as a project-panel file drop.
    shell
        .action(
            Action::ProjectImport {
                source: crate::layer_clipboard::KeyframeSource::of(&shell.state),
                folder: None,
                path: Some(source),
            },
            None,
        )
        .unwrap();
    let (_, reply) = shell.editing.take().unwrap();
    shell.accept_edit_state(
        reply
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap(),
    );
    let registered = shell.state.clone();
    assert!(registered["project"]["layers"]
        .as_array()
        .unwrap()
        .is_empty());
    let asset = registered["assetRegistration"]["asset"].as_u64().unwrap();
    std::fs::remove_dir_all(original).unwrap();

    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(size[0] as u32, size[1] as u32)),
            )
            .unwrap(),
    );
    let pending = shell
        .engine
        .attach_async(window.clone(), size[0] as u32, size[1] as u32)
        .unwrap();
    shell.state = native_test_support::receive(&mut events, &pending, Duration::from_secs(60))
        .unwrap()
        .unwrap();
    shell.window = Some(window);
    shell.attached = true;
    if case.ends_with("900") {
        // The existing default Project dock has no item viewport at this size.
        // Retain that baseline, then use supported tab docking to expose rows;
        // do not change product layout, row geometry or the assertions below.
        present(&mut shell, &mut events, size);
        let (paint, actions) = frame(&mut shell, None, size, Input::default());
        assert!(actions.is_empty());
        screenshot(
            &mut shell,
            &mut events,
            paint,
            size,
            out.join("registered-model-default-layout.png"),
        );
        assert!(shell.workspace.move_panel(
            Panel::EffectControls.dock(),
            Panel::Project.dock(),
            None
        ));
        assert!(shell
            .workspace
            .root
            .activate(Panel::Project.dock(), Panel::Project.dock()));
    }
    present(&mut shell, &mut events, size);
    let (paint, actions) = frame(&mut shell, None, size, Input::default());
    assert!(actions.is_empty());
    screenshot(
        &mut shell,
        &mut events,
        paint,
        size,
        out.join("registered-model.png"),
    );

    let row = rect(
        &shell.panels,
        &RowKey::Item(ProjectItem::Model { id: asset }),
    );
    let (paint, actions) = frame(&mut shell, None, size, click(row, MouseButton::Right));
    assert!(actions.is_empty());
    assert!(shell.panels.project.context_menu.is_some());
    screenshot(
        &mut shell,
        &mut events,
        paint,
        size,
        out.join("model-context-menu.png"),
    );
    let choice = shell.panels.project.browser.buttons[0];
    let (_, actions) = frame(&mut shell, None, size, click(choice, MouseButton::Left));
    assert!(
        matches!(actions.as_slice(), [Action::ProjectPlaceModel { asset: id, at_frame: 0, .. }] if *id == asset),
        "{actions:?}"
    );
    run(&mut shell, &mut events, actions);
    assert_eq!(
        shell.state["project"]["model_assets"],
        registered["project"]["model_assets"]
    );
    let object = shell.state["assetPlacement"]["object"].as_u64().unwrap();
    assert_eq!(shell.panels.selected, Some(object));
    shell.panels.timeline.expanded.insert(object);
    present(&mut shell, &mut events, size);
    let (paint, actions) = frame(&mut shell, None, size, Input::default());
    assert!(actions.is_empty());
    let path = out.join("placed-model.png");
    screenshot(&mut shell, &mut events, paint, size, path.clone());
    let pixels = image::open(&path).unwrap().into_rgba8();
    let area = crate::panels::composition_view(shell.workspace.display_root(), size);
    let composition = image::imageops::crop_imm(
        &pixels,
        area.min[0] as u32,
        area.min[1] as u32,
        area.width() as u32,
        area.height() as u32,
    )
    .to_image();
    let red = composition
        .pixels()
        .filter(|p| p[0] > p[1].saturating_add(20) && p[0] > p[2].saturating_add(20))
        .count();
    assert!(red > 200, "{case}: only {red} native red pixels");
    shell.engine.save().unwrap();
    let placed = shell.state["project"].clone();
    shell.state = shell.engine.history(0).unwrap();
    assert_eq!(shell.state["project"], registered["project"]);
    shell.state = shell.engine.history(1).unwrap();
    assert_eq!(shell.state["project"], placed);
    // Start on the real model row and release through the production pointer route.
    let row = rect(
        &shell.panels,
        &RowKey::Item(ProjectItem::Model { id: asset }),
    );
    let (_, actions) = frame(&mut shell, None, size, click(row, MouseButton::Left));
    run(&mut shell, &mut events, actions);
    assert!(shell.panels.project_drag.pending());
    frame(&mut shell, None, size, Input::default());
    let (track, offset, ppf, frames) = shell.panels.project_drag.track_geometry().unwrap();
    let target = [
        track.min[0] + track.width() * 0.35,
        track.min[1] + track.height() * 0.5,
    ];
    let expected = (offset + (target[0] - track.min[0]) / ppf)
        .round()
        .clamp(0., frames.saturating_sub(1) as f32) as u32;
    let window_id = shell.window.as_ref().unwrap().id();
    assert!(shell.route_project_drag_event(
        window_id,
        &winit::event::WindowEvent::CursorMoved {
            device_id: winit::event::DeviceId::dummy(),
            position: winit::dpi::PhysicalPosition::new(target[0] as f64, target[1] as f64),
        }
    ));
    let (paint, actions) = frame(&mut shell, None, size, Input::default());
    assert!(actions.is_empty());
    let drag_path = out.join("model-drag-preview.png");
    screenshot(&mut shell, &mut events, paint, size, drag_path.clone());
    assert!(shell.route_project_drag_event(
        window_id,
        &winit::event::WindowEvent::MouseInput {
            device_id: winit::event::DeviceId::dummy(),
            state: winit::event::ElementState::Released,
            button: winit::event::MouseButton::Left,
        }
    ));
    let (_, actions) = frame(&mut shell, None, size, Input::default());
    assert!(
        matches!(actions.as_slice(), [Action::ProjectPlaceModel { asset: id, at_frame, .. }]
        if *id == asset && *at_frame == expected),
        "{actions:?}"
    );
    run(&mut shell, &mut events, actions);
    assert!(!shell.panels.project_drag.pending());
    assert_eq!(
        shell.state["project"]["model_assets"],
        registered["project"]["model_assets"]
    );
    assert_eq!(
        shell.state["project"]["layers"].as_array().unwrap().len(),
        2
    );
    let dragged = shell.state["project"].clone();
    assert_eq!(shell.engine.history(0).unwrap()["project"], placed);
    assert_eq!(shell.engine.history(1).unwrap()["project"], dragged);
    shell.engine.save().unwrap();
    assert_eq!(
        std::fs::read_dir(root.join("assets/models"))
            .unwrap()
            .count(),
        1
    );
    shell.engine.request_shutdown().unwrap();
    std::fs::write(out.join("accepted.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "case":case,"size":size,"fixturePixelRatio":1,"redCompositionPixels":red,
        "object":object,"asset":asset,"originalSourcesRemoved":true,"sharedAsset":true,
        "separatePlacementUndoRedo":true,"dragPlacementFrame":expected,
        "projectEffectsTabbedAtMinimum":case.ends_with("900"),
        "screenshots":[out.join("registered-model.png"),out.join("model-context-menu.png"),path,drag_path],
        "physicalOsInputInjected":false
    })).unwrap()).unwrap();
}

fn present(shell: &mut Shell, events: &mut EventLoop<()>, size: [f32; 2]) {
    let (paint, actions) = frame(shell, None, size, Input::default());
    assert!(actions.is_empty());
    let ui = shell.ui_frame(paint, size, true);
    let pending = shell.engine.present_async(ui).unwrap();
    assert!(
        native_test_support::receive(events, &pending, Duration::from_secs(60))
            .unwrap()
            .unwrap()
    );
}

fn screenshot(
    shell: &mut Shell,
    events: &mut EventLoop<()>,
    paint: PaintList,
    size: [f32; 2],
    path: PathBuf,
) {
    let ui = shell.ui_frame(paint, size, true);
    native_test_support::capture_ui(events, &shell.engine, ui, path).unwrap();
}
