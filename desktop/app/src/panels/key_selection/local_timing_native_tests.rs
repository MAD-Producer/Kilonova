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

#[test]
#[ignore = "manual native typed-lane GPU/worker capture and independent reopen"]
fn typed_lanes_pixels_worker_undo_and_reopen() {
    const NAME:&str="panels::key_selection::tests::ui::local_timing::native::typed_lanes_pixels_worker_undo_and_reopen";
    const CHILD: &str = "MOTION_TYPED_TIMING_GPU_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logging = crate::logging::init(root.join("logs")).unwrap();
        let project = root.join("project");
        if std::env::var_os("MOTION_TYPED_TIMING_REOPEN").is_some() {
            let engine =
                crate::engine::Engine::start(project, motion_desktop_media::platform(), 8 << 30)
                    .unwrap();
            let actual = engine.state().unwrap();
            let saved: Value =
                serde_json::from_slice(&std::fs::read(root.join("saved.json")).unwrap()).unwrap();
            let actual_project: Project =
                serde_json::from_value(actual["project"].clone()).unwrap();
            let saved_project: Project = serde_json::from_value(saved["project"].clone()).unwrap();
            assert_eq!(actual_project, saved_project);
            assert_eq!(actual["history"]["undoEntries"], 0);
            record(&root.join("reopened.json"), &actual);
            record(
                &root.join("reopen-process.json"),
                &json!({"pid":std::process::id()}),
            );
            engine.request_shutdown().unwrap();
            return;
        }
        let index: usize = std::env::var("MOTION_TYPED_TIMING_CASE")
            .unwrap()
            .parse()
            .unwrap();
        let case = typed::CASES[index];
        let (model, catalogue, _) = typed::typed(case, false);
        let mut fixture = model.snapshot();
        if matches!(case, typed::Case::Audio) {
            std::fs::create_dir_all(project.join("assets")).unwrap();
            let count = 192000u32;
            let size = count * 4;
            let mut wav = Vec::new();
            wav.extend(b"RIFF");
            wav.extend((36 + size).to_le_bytes());
            wav.extend(b"WAVEfmt ");
            wav.extend(16u32.to_le_bytes());
            wav.extend(1u16.to_le_bytes());
            wav.extend(2u16.to_le_bytes());
            wav.extend(48000u32.to_le_bytes());
            wav.extend(192000u32.to_le_bytes());
            wav.extend(4u16.to_le_bytes());
            wav.extend(16u16.to_le_bytes());
            wav.extend(b"data");
            wav.extend(size.to_le_bytes());
            wav.resize(44 + size as usize, 0);
            fixture.audio_assets[0].bytes = wav.len() as u64;
            std::fs::write(project.join("assets/tone.wav"), wav).unwrap();
        }
        motion_core::storage::save(&project, &fixture).unwrap();
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
        for (locale, language, floating) in [(Locale::Zh, "zh", false), (Locale::En, "en", true)] {
            let s = shell.engine.state().unwrap();
            let mut panels = typed::ready(case, &catalogue, &s, locale, true);
            panels.styles = shell.panels.styles.clone();
            let (paint, actions) = draw(
                &mut panels,
                &s,
                Input::default(),
                floating,
                &mut shell.atlas,
            );
            selection_only(&actions);
            let start = typed::points(&paint, &panels, 6.5)[0];
            let ppf = panels.timeline.zoom_track.unwrap().width() / 120.;
            let next = [start[0] + 6. * ppf, start[1]];
            let modifiers = Modifiers {
                alt: true,
                ..Default::default()
            };
            let (_, actions) = draw(
                &mut panels,
                &s,
                pointer(start, true, false, modifiers),
                floating,
                &mut shell.atlas,
            );
            selection_only(&actions);
            let selected = panels.selected_keys.local_selections();
            assert!(!selected.is_empty());
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
            capture(
                &mut shell,
                &mut events,
                paint,
                root.join(format!("{language}-preview.png")),
            );
            let (_, actions) = draw(
                &mut panels,
                &s,
                pointer(next, false, true, modifiers),
                floating,
                &mut shell.atlas,
            );
            let mut source = None;
            let mut commands = None;
            for action in actions {
                match action {
                    Action::EditorAction {
                        source: stamp,
                        commands: edits,
                    } => {
                        assert!(source.replace(stamp).is_none());
                        assert!(commands.replace(edits).is_none());
                    }
                    Action::Seek(_) => {}
                    other => panic!("unexpected typed native action {other:?}"),
                }
            }
            let commands = commands.expect("one typed commit");
            assert_eq!(commands.len(), selected.len());
            assert!(commands.iter().all(|c| matches!(
                c,
                Command::LocalKeyframes {
                    edit: LocalKeyEdit::Translate { delta_frame: 4, .. },
                    ..
                }
            )));
            let expected_commands = commands.clone();
            let after = shell
                .engine
                .call(|reply| crate::engine::Command::EditorAction {
                    source: source.unwrap(),
                    commands,
                    reply,
                })
                .unwrap();
            let mut expected =
                Engine::new(serde_json::from_value(s["project"].clone()).unwrap()).unwrap();
            expected.apply_batch(expected_commands).unwrap();
            let actual: Project = serde_json::from_value(after["project"].clone()).unwrap();
            assert_eq!(&actual, expected.project());
            assert_eq!(after["history"]["undoEntries"], 1);
            record(&root.join(format!("{language}-after.json")), &after);
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
                root.join(format!("{language}-after.png")),
            );
            record(&root.join("saved.json"), &shell.engine.save().unwrap());
            let undo = shell.engine.history(0).unwrap();
            assert_eq!(undo["project"], baseline["project"]);
            assert_eq!(undo["history"]["undoEntries"], 0);
            record(&root.join(format!("{language}-undo.json")), &undo);
        }
        shell.engine.request_shutdown().unwrap();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_TYPED_TIMING_GPU_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().into());
    for index in 0..typed::CASES.len() {
        let folder = root.join(format!("case-{index}"));
        std::fs::create_dir_all(&folder).unwrap();
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
                .env(CHILD, &folder)
                .env("MOTION_TYPED_TIMING_CASE", index.to_string())
                .env_remove("MOTION_LOG");
            if reopen {
                command.env("MOTION_TYPED_TIMING_REOPEN", "1");
            }
            let output = command.output().unwrap();
            let phase = if reopen { "reopen" } else { "capture" };
            std::fs::write(folder.join(format!("{phase}-stdout.log")), &output.stdout).unwrap();
            std::fs::write(folder.join(format!("{phase}-stderr.log")), &output.stderr).unwrap();
            assert!(
                output.status.success(),
                "case={index} {phase}\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let read = |name| -> Value {
            serde_json::from_slice(&std::fs::read(folder.join(name)).unwrap()).unwrap()
        };
        assert_ne!(
            read("capture-process.json")["pid"],
            read("reopen-process.json")["pid"]
        );
        assert_eq!(
            std::fs::read_dir(&folder)
                .unwrap()
                .filter(|e| e
                    .as_ref()
                    .unwrap()
                    .path()
                    .extension()
                    .is_some_and(|x| x == "png"))
                .count(),
            4
        );
    }
}
