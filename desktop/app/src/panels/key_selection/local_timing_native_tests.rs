//! Real GPU/worker evidence with injected main/floating chrome input.
//! Does not take the operating system foreground or inject physical input.
use super::*;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
fn record(path: &Path, value: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
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
#[test]
#[ignore = "manual native GPU/serial worker capture; leaves OS input untouched"]
fn fractional_key_drag_pixels_transactions_and_reopen() {
    const NAME:&str="panels::key_selection::tests::ui::local_timing::native::fractional_key_drag_pixels_transactions_and_reopen";
    const CHILD: &str = "MOTION_LOCAL_TIMING_GPU_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logging = crate::logging::init(root.join("logs")).unwrap();
        let project = root.join("project");
        if std::env::var_os("MOTION_LOCAL_TIMING_REOPEN").is_some() {
            let engine =
                crate::engine::Engine::start(project, motion_desktop_media::platform(), 8 << 30)
                    .unwrap();
            let actual = engine.state().unwrap();
            let expected: Value =
                serde_json::from_slice(&std::fs::read(root.join("saved.json")).unwrap()).unwrap();
            let a: Project = serde_json::from_value(actual["project"].clone()).unwrap();
            let b: Project = serde_json::from_value(expected["project"].clone()).unwrap();
            assert_eq!(a, b);
            assert_eq!(actual["history"]["undoEntries"], 0);
            record(&root.join("reopened.json"), &actual);
            record(
                &root.join("reopen-process.json"),
                &json!({"pid":std::process::id()}),
            );
            engine.request_shutdown().unwrap();
            return;
        }
        motion_core::storage::save(&project, &mapped(false).snapshot()).unwrap();
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
            for floating in [false, true] {
                for graph in [false, true] {
                    let tag = format!(
                        "{language}-{}-{}",
                        if floating { "float" } else { "main" },
                        if graph { "graph" } else { "timeline" }
                    );
                    let s = shell.engine.state().unwrap();
                    let mut panels = setup(locale, graph, false);
                    panels.styles = shell.panels.styles.clone();
                    let (paint, _) = draw(
                        &mut panels,
                        &s,
                        Input::default(),
                        floating,
                        &mut shell.atlas,
                    );
                    let start = at_time(&paint, &panels, 6.5);
                    let ppf = panels.timeline.zoom_track.unwrap().width() / 120.;
                    let next = [start[0] + 6. * ppf, start[1]];
                    let modifiers = Modifiers {
                        alt: true,
                        ..Default::default()
                    };
                    draw(
                        &mut panels,
                        &s,
                        pointer(start, true, false, modifiers),
                        floating,
                        &mut shell.atlas,
                    );
                    let (paint, actions) = draw(
                        &mut panels,
                        &s,
                        pointer(next, false, false, modifiers),
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
                        &s,
                        pointer(next, false, true, modifiers),
                        floating,
                        &mut shell.atlas,
                    );
                    let mut committed = None;
                    for action in actions {
                        match action {
                            Action::EditorAction { source, commands } => {
                                assert!(committed.is_none());
                                assert!(matches!(
                                    commands.as_slice(),
                                    [Command::LocalKeyframes {
                                        edit: LocalKeyEdit::Translate { delta_frame: 4, .. },
                                        ..
                                    }]
                                ));
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
                            other => panic!("unexpected native action {other:?}"),
                        }
                    }
                    let after = committed.expect("one guarded native edit");
                    assert_eq!(after["history"]["undoEntries"], 1);
                    let actual: Project = serde_json::from_value(after["project"].clone()).unwrap();
                    assert!(actual.layers[0]
                        .transform
                        .position
                        .keys
                        .iter()
                        .any(|k| k.frame == 7));
                    assert_eq!(
                        actual.layers[0].property_composition_frame(7.).unwrap(),
                        12.5
                    );
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
                    record(&root.join("saved.json"), &saved);
                    let undo = shell.engine.history(0).unwrap();
                    assert_eq!(undo["project"], baseline["project"]);
                    assert_eq!(undo["history"]["undoEntries"], 0);
                    record(&root.join(format!("{tag}-undo.json")), &undo);
                }
            }
        }
        shell.engine.request_shutdown().unwrap();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_LOCAL_TIMING_GPU_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().to_owned());
    std::fs::create_dir_all(&root).unwrap();
    for reopen in [false, true] {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
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
            command.env("MOTION_LOCAL_TIMING_REOPEN", "1");
        }
        let output = command.output().unwrap();
        let tag = if reopen { "reopen" } else { "capture" };
        std::fs::write(root.join(format!("{tag}-stdout.log")), &output.stdout).unwrap();
        std::fs::write(root.join(format!("{tag}-stderr.log")), &output.stderr).unwrap();
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
            .map(|e| e.unwrap())
            .filter(|e| e.path().extension().is_some_and(|e| e == "png"))
            .count(),
        16
    );
}
