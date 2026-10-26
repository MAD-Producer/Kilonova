//! Actual native GPU pixels and serial engine transactions. Input is provided
//! to main/floating chrome, not to the operating system's shared foreground.
use super::*;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

fn capture(
    shell: &mut crate::Shell,
    events: &mut winit::event_loop::EventLoop<()>,
    mut paint: PaintList,
    path: PathBuf,
) {
    paint.stylesheet = shell.panels.styles.clone();
    let mut ui = shell.ui_frame(paint, [1000., 800.], true);
    ui.composition = None;
    ui.composition_image = None;
    ui.node_preview = None;
    crate::native_test_support::capture_ui(events, &shell.engine, ui, path.clone()).unwrap();
    let pixels = image::open(path).unwrap();
    assert_eq!((pixels.width(), pixels.height()), (1000, 800));
}
fn record(path: &Path, value: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

#[test]
#[ignore = "manual native GPU capture; keeps OS foreground and physical input untouched"]
fn native_key_drag_pixels_transactions_and_independent_reopen() {
    const NAME: &str = "panels::key_selection::tests::ui::dragging::native::native_key_drag_pixels_transactions_and_independent_reopen";
    const CHILD: &str = "MOTION_KEY_DRAG_GPU_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logging = crate::logging::init(root.join("logs")).unwrap();
        let project = root.join("project");
        if std::env::var_os("MOTION_KEY_DRAG_REOPEN").is_some() {
            let engine =
                crate::engine::Engine::start(project, motion_desktop_media::platform(), 8 << 30)
                    .unwrap();
            let state = engine.state().unwrap();
            let expected: Value =
                serde_json::from_slice(&std::fs::read(root.join("saved-state.json")).unwrap())
                    .unwrap();
            // JSON's f64 parser may change the last digit of a widened f32.
            // Compare every authored field exactly in its actual model type;
            // this uses neither a numeric tolerance nor a partial projection.
            let actual: Project = serde_json::from_value(state["project"].clone()).unwrap();
            let expected: Project = serde_json::from_value(expected["project"].clone()).unwrap();
            assert_eq!(actual, expected);
            assert_eq!(state["history"]["undoEntries"], 0);
            assert_eq!(state["history"]["redoEntries"], 0);
            record(&root.join("reopened-state.json"), &state);
            record(
                &root.join("reopen-process.json"),
                &json!({"pid":std::process::id()}),
            );
            engine.request_shutdown().unwrap();
            return;
        }
        motion_core::storage::save(&project, &fixture().snapshot()).unwrap();
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
        record(&root.join("baseline.json"), &baseline);
        record(
            &root.join("capture-process.json"),
            &json!({"pid":std::process::id()}),
        );
        for (locale, language) in [(Locale::Zh, "zh"), (Locale::En, "en")] {
            shell.workspace.locale = locale;
            for floating in [false, true] {
                for graph in [false, true] {
                    let tag = format!(
                        "{language}-{}-{}",
                        if floating { "float" } else { "main" },
                        if graph { "graph" } else { "timeline" }
                    );
                    let state = shell.engine.state().unwrap();
                    let mut panels = Panels {
                        locale,
                        styles: shell.panels.styles.clone(),
                        focused: Some(Panel::Timeline),
                        ..Default::default()
                    };
                    panels.select_only(Some(7));
                    let (paint, _) = draw(
                        &mut panels,
                        &state,
                        Input::default(),
                        floating,
                        &mut shell.atlas,
                    );
                    let a = point(&paint, &panels, 10);
                    let b = point(&paint, &panels, if graph { 20 } else { 43 });
                    click(
                        &mut panels,
                        &state,
                        a,
                        Modifiers::default(),
                        floating,
                        &mut shell.atlas,
                    );
                    click(
                        &mut panels,
                        &state,
                        b,
                        Modifiers {
                            shift: true,
                            ..Default::default()
                        },
                        floating,
                        &mut shell.atlas,
                    );
                    assert_eq!(selected(&panels), if graph { 2 } else { 4 });
                    panels.timeline.graph_mode = graph;
                    let (paint, actions) = draw(
                        &mut panels,
                        &state,
                        Input::default(),
                        floating,
                        &mut shell.atlas,
                    );
                    selection_only(&actions);
                    let start = point(&paint, &panels, 10);
                    let ppf = panels.timeline.zoom_track.unwrap().width() / 120.;
                    let end = [start[0] + 5. * ppf, start[1] - if graph { 45. } else { 0. }];
                    draw(
                        &mut panels,
                        &state,
                        pointer(start, true, false, Modifiers::default()),
                        floating,
                        &mut shell.atlas,
                    );
                    let (paint, actions) = draw(
                        &mut panels,
                        &state,
                        pointer(end, false, false, Modifiers::default()),
                        floating,
                        &mut shell.atlas,
                    );
                    selection_only(&actions);
                    assert_eq!(
                        shell.engine.state().unwrap()["project"],
                        baseline["project"]
                    );
                    assert_eq!(shell.engine.state().unwrap()["history"]["undoEntries"], 0);
                    capture(
                        &mut shell,
                        &mut events,
                        paint,
                        root.join(format!("{tag}-preview.png")),
                    );
                    let (_, actions) = draw(
                        &mut panels,
                        &state,
                        pointer(end, false, true, Modifiers::default()),
                        floating,
                        &mut shell.atlas,
                    );
                    let mut committed = None;
                    for action in actions {
                        match action {
                            Action::EditorAction { source, commands } => {
                                assert!(committed.is_none());
                                committed = Some(
                                    shell
                                        .engine
                                        .call(|reply| crate::engine::Command::EditorAction {
                                            source,
                                            commands,
                                            reply,
                                        })
                                        .unwrap(),
                                );
                            }
                            Action::Seek(_) => {}
                            other => panic!("unexpected native drag action {other:?}"),
                        }
                    }
                    let after = committed.expect("one atomic native edit");
                    assert_ne!(after["project"], baseline["project"]);
                    assert_eq!(after["history"]["undoEntries"], 1);
                    record(&root.join(format!("{tag}-after.json")), &after);
                    let (paint, actions) = draw(
                        &mut panels,
                        &after,
                        Input::default(),
                        floating,
                        &mut shell.atlas,
                    );
                    selection_only(&actions);
                    capture(
                        &mut shell,
                        &mut events,
                        paint,
                        root.join(format!("{tag}-after.png")),
                    );
                    let saved = shell.engine.save().unwrap();
                    assert_eq!(saved["project"], after["project"]);
                    record(&root.join("saved-state.json"), &saved);
                    let undone = shell.engine.history(0).unwrap();
                    assert_eq!(undone["project"], baseline["project"]);
                    assert_eq!(undone["history"]["undoEntries"], 0);
                    record(&root.join(format!("{tag}-undo.json")), &undone);
                }
            }
        }
        shell.engine.request_shutdown().unwrap();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_KEY_DRAG_GPU_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().to_owned());
    std::fs::create_dir_all(&root).unwrap();
    for reopen in [false, true] {
        let mut process = std::process::Command::new(std::env::current_exe().unwrap());
        process
            .args([
                "--exact",
                NAME,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, &root)
            .env_remove("MOTION_LOG");
        if reopen {
            process.env("MOTION_KEY_DRAG_REOPEN", "1");
        }
        let output = process.output().unwrap();
        let prefix = if reopen { "reopen" } else { "capture" };
        std::fs::write(root.join(format!("{prefix}-stdout.log")), &output.stdout).unwrap();
        std::fs::write(root.join(format!("{prefix}-stderr.log")), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let read = |name: &str| -> Value {
        serde_json::from_slice(&std::fs::read(root.join(name)).unwrap()).unwrap()
    };
    assert_ne!(
        read("capture-process.json")["pid"],
        read("reopen-process.json")["pid"]
    );
    assert_eq!(
        std::fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|e| e == "png"))
            .count(),
        16
    );
}
