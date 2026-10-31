//! Real GPU pixels and worker commits, with no physical pointer or focus theft.
use super::*;
use std::{path::PathBuf, sync::Arc, time::Duration};

#[test]
#[ignore = "manual native GPU evidence for stable spatial-key timeline gestures"]
fn native_spatial_timeline_pixels_and_atomic_worker_moves() {
    const NAME: &str = "panels::key_selection::tests::ui::spatial::native::native_spatial_timeline_pixels_and_atomic_worker_moves";
    const CHILD: &str = "MOTION_SPATIAL_TIMELINE_GPU_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logging = crate::logging::init(root.join("logs")).unwrap();
        let project = root.join("project");
        motion_core::storage::save(&project, &spatial_fixture(false).snapshot()).unwrap();
        let mut shell = crate::Shell::new(project, None, Some(Locale::Zh)).unwrap();
        shell.home = None;
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let mut events = winit::event_loop::EventLoop::builder()
            .with_any_thread(true)
            .build()
            .unwrap();
        #[allow(deprecated)]
        let window = Arc::new(
            events
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(1000, 800)),
                )
                .unwrap(),
        );
        let reply = shell
            .engine
            .attach_async(window.clone(), 1000, 800)
            .unwrap();
        crate::native_test_support::receive(&mut events, &reply, Duration::from_secs(30))
            .unwrap()
            .unwrap();
        shell.window = Some(window);
        shell.attached = true;
        let baseline = shell.engine.state().unwrap();
        let mut report = vec![];
        for (locale, language) in [(Locale::Zh, "zh"), (Locale::En, "en")] {
            for floating in [false, true] {
                for expanded in [false, true] {
                    let state = shell.engine.state().unwrap();
                    let mut panels = Panels {
                        locale,
                        focused: Some(Panel::Timeline),
                        styles: shell.panels.styles.clone(),
                        ..Default::default()
                    };
                    panels.select_only(Some(7));
                    if expanded {
                        panels.timeline.expanded.insert(7);
                    }
                    let (paint, actions) = draw(
                        &mut panels,
                        &state,
                        Input::default(),
                        floating,
                        &mut shell.atlas,
                    );
                    selection_only(&actions);
                    let start = at_time(&paint, &panels, 9.5);
                    let ppf = panels.timeline.zoom_track.unwrap().width() / 120.;
                    let modifiers = Modifiers {
                        alt: true,
                        ..Default::default()
                    };
                    let (_, actions) = draw(
                        &mut panels,
                        &state,
                        pointer(start, true, false, modifiers),
                        floating,
                        &mut shell.atlas,
                    );
                    selection_only(&actions);
                    let end = [start[0] + 5. * ppf, start[1]];
                    let (mut paint, actions) = draw(
                        &mut panels,
                        &state,
                        pointer(end, false, false, modifiers),
                        floating,
                        &mut shell.atlas,
                    );
                    selection_only(&actions);
                    assert_eq!(
                        shell.engine.state().unwrap()["project"],
                        baseline["project"]
                    );
                    assert_eq!(shell.engine.state().unwrap()["history"]["undoEntries"], 0);
                    let tag = format!(
                        "{language}-{}-{}",
                        if floating { "float" } else { "main" },
                        if expanded { "property" } else { "summary" }
                    );
                    paint.stylesheet = shell.panels.styles.clone();
                    let mut ui = shell.ui_frame(paint, [1000., 800.], true);
                    ui.composition = None;
                    ui.composition_image = None;
                    ui.node_preview = None;
                    let image_path = root.join(format!("{tag}.png"));
                    crate::native_test_support::capture_ui(
                        &mut events,
                        &shell.engine,
                        ui,
                        image_path.clone(),
                    )
                    .unwrap();
                    let image = image::open(image_path).unwrap();
                    assert_eq!((image.width(), image.height()), (1000, 800));
                    let (_, actions) = draw(
                        &mut panels,
                        &state,
                        pointer(end, false, true, modifiers),
                        floating,
                        &mut shell.atlas,
                    );
                    let commands = atomic(actions);
                    assert!(
                        matches!(commands.as_slice(), [Command::SpatialKeyframes { edit: SpatialKeyEdit::Move { keys, delta }, .. }] if keys == &[2] && *delta == 5.)
                    );
                    let after = shell
                        .engine
                        .call(|reply| crate::engine::Command::EditorAction {
                            source: crate::layer_clipboard::KeyframeSource::of(&state),
                            commands,
                            reply,
                        })
                        .unwrap();
                    assert_eq!(after["history"]["undoEntries"], 1);
                    let track = &after["project"]["layers"][0]["transform"]["position"]
                        ["spatial_animation"]["keys"];
                    assert_eq!(track[1]["id"], 2);
                    assert_eq!(track[1]["fixed"], 7.5);
                    assert_eq!(identities(&panels), [2]);
                    let undone = shell.engine.history(0).unwrap();
                    assert_eq!(undone["project"], baseline["project"]);
                    assert_eq!(undone["history"]["undoEntries"], 0);
                    std::fs::write(
                        root.join(format!("{tag}-after.json")),
                        serde_json::to_vec_pretty(&after).unwrap(),
                    )
                    .unwrap();
                    report.push(json!({"tag":tag,"ids":identities(&panels),"delta":5,"fractionalOrigin":9.5,"fixedLocalTime":7.5,"undoEntries":1,"undoRestoredFullProject":true}));
                }
            }
        }
        std::fs::write(
            root.join("report.json"),
            serde_json::to_vec_pretty(&json!({"pid":std::process::id(),"cases":report})).unwrap(),
        )
        .unwrap();
        shell.engine.request_shutdown().unwrap();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_SPATIAL_TIMELINE_GPU_ARTIFACTS")
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
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
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
    assert_eq!(
        std::fs::read_dir(&root)
            .unwrap()
            .map(Result::unwrap)
            .filter(|e| e.path().extension().is_some_and(|e| e == "png"))
            .count(),
        8
    );
}
