//! Actual GPU captures and guarded worker edits; no operating system input injection.
use super::*;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
fn record(path: impl AsRef<Path>, value: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn capture(
    shell: &mut crate::Shell,
    events: &mut winit::event_loop::EventLoop<()>,
    paint: PaintList,
    floating: bool,
    path: PathBuf,
) {
    let mut ui = shell.ui_frame(paint, [1000., 800.], true);
    ui.window = u64::from(floating);
    ui.composition = None;
    ui.composition_image = None;
    ui.node_preview = None;
    crate::native_test_support::capture_ui(events, &shell.engine, ui, path.clone()).unwrap();
    let image = image::open(path).unwrap();
    assert_eq!((image.width(), image.height()), (1000, 800));
}
#[test]
#[ignore = "manual native GPU capture and independent-process reopen"]
fn mapped_comparison_native_pixels_worker_undo_save_and_reopen() {
    const NAME:&str="panels::key_selection::tests::ui::local_timing::graphs::native::mapped_comparison_native_pixels_worker_undo_save_and_reopen";
    const CHILD: &str = "MOTION_TIME_GRAPHS_CHILD";
    let cases = [typed::Case::GroupVector, typed::Case::Effects];
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let reopening = std::env::var_os("MOTION_TIME_GRAPHS_REOPEN").is_some();
        let _log = crate::logging::init(root.join(if reopening { "reopen-logs" } else { "logs" }))
            .unwrap();
        record(
            root.join(if reopening {
                "reopen-process.json"
            } else {
                "capture-process.json"
            }),
            &json!({"pid":std::process::id()}),
        );
        if reopening {
            for case in cases {
                let folder = root.join(format!("{case:?}"));
                let e = crate::engine::Engine::start(
                    folder.join("project"),
                    motion_desktop_media::platform(),
                    8 << 30,
                )
                .unwrap();
                let actual = e.state().unwrap();
                let expected: Value =
                    serde_json::from_slice(&std::fs::read(folder.join("saved.json")).unwrap())
                        .unwrap();
                assert_eq!(actual["project"], expected["project"]);
                assert_eq!(actual["history"]["undoEntries"], 0);
                record(folder.join("reopened.json"), &actual);
                e.request_shutdown().unwrap();
            }
            return;
        }
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let mut events = winit::event_loop::EventLoop::builder()
            .with_any_thread(true)
            .build()
            .unwrap();
        #[allow(deprecated)]
        let main = Arc::new(
            events
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(1000, 800)),
                )
                .unwrap(),
        );
        #[allow(deprecated)]
        let float = Arc::new(
            events
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(1000, 800)),
                )
                .unwrap(),
        );
        for case in cases {
            let folder = root.join(format!("{case:?}"));
            std::fs::create_dir_all(&folder).unwrap();
            let (e, catalogue, targets) = varied(case, false);
            motion_core::storage::save(&folder.join("project"), e.project()).unwrap();
            let mut shell =
                crate::Shell::new(folder.join("project"), None, Some(Locale::Zh)).unwrap();
            shell.home = None;
            let attached = shell.engine.attach_async(main.clone(), 1000, 800).unwrap();
            shell.state = crate::native_test_support::receive(
                &mut events,
                &attached,
                Duration::from_secs(30),
            )
            .unwrap()
            .unwrap();
            shell.window = Some(main.clone());
            shell
                .engine
                .float_window(1, float.clone(), 1000, 800)
                .unwrap();
            for (floating, locale, tag) in [
                (false, Locale::Zh, "zh-main-forward"),
                (true, Locale::En, "en-float-reverse"),
            ] {
                if floating {
                    shell
                        .engine
                        .edit(
                            serde_json::to_string(&Command::LayerTime {
                                object: 7,
                                action: motion_core::layer_time::LayerTimeAction::Reverse {},
                            })
                            .unwrap(),
                        )
                        .unwrap();
                }
                let before = shell.engine.state().unwrap();
                let layer: Layer =
                    serde_json::from_value(before["project"]["layers"][0].clone()).unwrap();
                let mut panels = typed::ready(case, &catalogue, &before, locale, false);
                panels.timeline.set_graph_mode(true);
                collect_targets(&mut panels, &before, &targets[..1]);
                let (paint, actions) = draw(
                    &mut panels,
                    &before,
                    Input::default(),
                    floating,
                    &mut shell.atlas,
                );
                selection_only(&actions);
                let start = typed::points(
                    &paint,
                    &panels,
                    layer.property_composition_frame(3.).unwrap(),
                )[0];
                let modifiers = Modifiers {
                    alt: true,
                    ..Default::default()
                };
                let ppf = panels.timeline.zoom_track.unwrap().width() / 120.;
                let next = [start[0] + 6. * ppf, start[1]];
                draw(
                    &mut panels,
                    &before,
                    pointer(start, true, false, modifiers),
                    floating,
                    &mut shell.atlas,
                );
                assert_eq!(
                    panels.selected_keys.local_selections(),
                    vec![(targets[0].clone(), vec![3])]
                );
                let (paint, actions) = draw(
                    &mut panels,
                    &before,
                    pointer(next, false, false, modifiers),
                    floating,
                    &mut shell.atlas,
                );
                selection_only(&actions);
                assert_eq!(shell.engine.state().unwrap()["project"], before["project"]);
                capture(
                    &mut shell,
                    &mut events,
                    paint,
                    floating,
                    folder.join(format!("{tag}-preview.png")),
                );
                let (_, actions) = draw(
                    &mut panels,
                    &before,
                    pointer(next, false, true, modifiers),
                    floating,
                    &mut shell.atlas,
                );
                let mut after = None;
                for action in actions {
                    match action {
                        Action::EditorAction { source, commands } => {
                            assert!(after.is_none());
                            assert!(
                                matches!(commands.as_slice(),[Command::LocalKeyframes {edit:LocalKeyEdit::Translate {frames,delta_frame,..},..}] if frames==&[3] && *delta_frame==if floating {-4}else{4})
                            );
                            after = Some(
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
                        other => panic!("unexpected action {other:?}"),
                    }
                }
                let after = after.expect("one guarded worker transaction");
                record(folder.join(format!("{tag}-before.json")), &before);
                record(folder.join(format!("{tag}-after.json")), &after);
                for (speed, suffix) in [(false, "values"), (true, "speed")] {
                    panels.timeline.speed_graph = speed;
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
                        floating,
                        folder.join(format!("{tag}-{suffix}.png")),
                    );
                }
                let mut final_state = after.clone();
                if matches!(case, typed::Case::Effects) {
                    let target = targets[0].clone();
                    let KeyframeTarget::Effect { effect, param, .. } = &target else {
                        panic!()
                    };
                    let local = if floating { -1 } else { 7 };
                    panels.timeline.comparison.enabled = false;
                    panels.timeline.speed_graph = false;
                    panels
                        .effect_controls
                        .choose_parameter(&after, 7, *effect, param);
                    panels
                        .selected_keys
                        .apply_addressed(
                            &after,
                            [LocalKey {
                                target: target.clone(),
                                frame: local,
                            }
                            .into()],
                            Mode::Replace,
                        )
                        .unwrap();
                    draw(
                        &mut panels,
                        &after,
                        Input::default(),
                        floating,
                        &mut shell.atlas,
                    );
                    let hit = panels.effect_timeline.rects[&(*effect, param.clone(), -100)];
                    let start = [
                        (hit.min[0] + hit.max[0]) * 0.5,
                        (hit.min[1] + hit.max[1]) * 0.5,
                    ];
                    let (_, actions) = draw(
                        &mut panels,
                        &after,
                        pointer(start, true, false, Modifiers::default()),
                        floating,
                        &mut shell.atlas,
                    );
                    selection_only(&actions);
                    let end = [start[0] + if floating { -5. } else { 5. }, start[1] - 12.];
                    let (_, actions) = draw(
                        &mut panels,
                        &after,
                        pointer(end, false, true, Modifiers::default()),
                        floating,
                        &mut shell.atlas,
                    );
                    assert!(
                        matches!(actions.as_slice(),[Action::History(2),Action::Edit(Command::LocalKeyframes {edit:LocalKeyEdit::Curve {frames,..},..}),Action::History(3)] if frames==&[local]),
                        "{actions:?}"
                    );
                    for action in actions {
                        match action {
                            Action::History(op) => {
                                shell.engine.history(op).unwrap();
                            }
                            Action::Edit(command) => {
                                shell
                                    .engine
                                    .edit(serde_json::to_string(&command).unwrap())
                                    .unwrap();
                            }
                            _ => unreachable!(),
                        }
                    }
                    final_state = shell.engine.state().unwrap();
                    assert_ne!(final_state["project"], after["project"]);
                    assert_eq!(
                        final_state["history"]["undoEntries"].as_u64().unwrap(),
                        after["history"]["undoEntries"].as_u64().unwrap() + 1
                    );
                    record(folder.join(format!("{tag}-handle.json")), &final_state);
                    let (paint, actions) = draw(
                        &mut panels,
                        &final_state,
                        Input::default(),
                        floating,
                        &mut shell.atlas,
                    );
                    selection_only(&actions);
                    capture(
                        &mut shell,
                        &mut events,
                        paint,
                        floating,
                        folder.join(format!("{tag}-handle.png")),
                    );
                }
                let saved = shell.engine.save().unwrap();
                assert_eq!(saved["project"], final_state["project"]);
                record(folder.join("saved.json"), &saved);
                if matches!(case, typed::Case::Effects) {
                    assert_eq!(
                        shell.engine.history(0).unwrap()["project"],
                        after["project"],
                        "one handle gesture = one undo"
                    );
                }
                let undo = shell.engine.history(0).unwrap();
                assert_eq!(undo["project"], before["project"]);
                assert_eq!(
                    undo["history"]["undoEntries"],
                    before["history"]["undoEntries"]
                );
                record(folder.join(format!("{tag}-undo.json")), &undo);
            }
            shell.engine.close_window(1).unwrap();
            shell.engine.request_shutdown().unwrap();
        }
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_TIME_GRAPHS_GPU_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().into());
    std::fs::create_dir_all(&root).unwrap();
    for phase in ["capture", "reopen"] {
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
        if phase == "reopen" {
            command.env("MOTION_TIME_GRAPHS_REOPEN", "1");
        }
        let output = command.output().unwrap();
        std::fs::write(root.join(format!("{phase}-stdout.log")), &output.stdout).unwrap();
        std::fs::write(root.join(format!("{phase}-stderr.log")), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "{phase}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!String::from_utf8_lossy(&output.stdout).contains("motion_studio::"));
    }
    let a: Value =
        serde_json::from_slice(&std::fs::read(root.join("capture-process.json")).unwrap()).unwrap();
    let b: Value =
        serde_json::from_slice(&std::fs::read(root.join("reopen-process.json")).unwrap()).unwrap();
    assert_ne!(a["pid"], b["pid"]);
}
