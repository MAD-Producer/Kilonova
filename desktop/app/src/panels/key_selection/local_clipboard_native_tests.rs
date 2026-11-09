//! Actual GPU chrome, clipboard worker transactions and a second-process reopen.
use super::*;
use std::{
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};
fn record(path: PathBuf, value: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn wait(shell: &mut crate::Shell) {
    let began = Instant::now();
    while shell.layer_clipboard.busy() {
        assert!(began.elapsed() < Duration::from_secs(30));
        shell.poll_layer_clipboard();
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn run(shell: &mut crate::Shell, actions: Vec<Action>, intent: Intent) {
    assert!(
        matches!(actions.as_slice(),[Action::KeyframeClipboard {intent:actual,..}] if *actual==intent),
        "{actions:?}"
    );
    for action in actions {
        shell.action(action, None).unwrap();
    }
    wait(shell);
}
fn audio(root: &Path, p: &mut Project) {
    if p.audio_assets.is_empty() {
        return;
    }
    let data = 192000u32 * 4;
    let mut wav = vec![];
    wav.extend(b"RIFF");
    wav.extend((36 + data).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(48000u32.to_le_bytes());
    wav.extend(192000u32.to_le_bytes());
    wav.extend(4u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend(data.to_le_bytes());
    wav.resize(44 + data as usize, 0);
    std::fs::create_dir_all(root.join("assets")).unwrap();
    std::fs::write(root.join("assets/tone.wav"), &wav).unwrap();
    p.audio_assets[0].bytes = wav.len() as u64;
}
#[test]
#[ignore = "manual native GPU clipboard capture with independent-process reopen"]
fn typed_fractional_clipboard_native_input_transactions_errors_and_reopen() {
    const NAME:&str="panels::key_selection::tests::ui::local_timing::clipboard::native::typed_fractional_clipboard_native_input_transactions_errors_and_reopen";
    const CHILD: &str = "MOTION_TIME_CLIPBOARD_GPU_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let reopening = std::env::var_os("MOTION_TIME_CLIPBOARD_REOPEN").is_some();
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
            for case in typed::CASES {
                let folder = root.join(format!("{case:?}"));
                let engine = crate::engine::Engine::start(
                    folder.join("project"),
                    motion_desktop_media::platform(),
                    8 << 30,
                )
                .unwrap();
                let actual = engine.state().unwrap();
                let expected: Value =
                    serde_json::from_slice(&std::fs::read(folder.join("saved.json")).unwrap())
                        .unwrap();
                let a: Project = serde_json::from_value(actual["project"].clone()).unwrap();
                let b: Project = serde_json::from_value(expected["project"].clone()).unwrap();
                assert_eq!(a, b);
                assert_eq!(actual["history"]["undoEntries"], 0);
                record(folder.join("reopened.json"), &actual);
                engine.request_shutdown().unwrap();
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
        let floating = Arc::new(
            events
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(1000, 800)),
                )
                .unwrap(),
        );
        for case in typed::CASES {
            let folder = root.join(format!("{case:?}"));
            std::fs::create_dir_all(&folder).unwrap();
            let (engine, catalogue, targets) = typed::typed(case, false);
            let mut p = engine.snapshot();
            audio(&folder.join("project"), &mut p);
            motion_core::storage::save(&folder.join("project"), &p).unwrap();
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
                .float_window(1, floating.clone(), 1000, 800)
                .unwrap();
            for (float, locale, tag) in [
                (false, Locale::Zh, "zh-main-forward"),
                (true, Locale::En, "en-float-reverse"),
            ] {
                if float {
                    shell.state = shell
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
                let before = shell.state.clone();
                let layer: Layer =
                    serde_json::from_value(before["project"]["layers"][0].clone()).unwrap();
                let mut panels = typed::ready(case, &catalogue, &before, locale, false);
                let (paint, _) = draw(
                    &mut panels,
                    &before,
                    Input::default(),
                    float,
                    &mut shell.atlas,
                );
                let time = layer.property_composition_frame(3.).unwrap();
                let point = typed::points(&paint, &panels, time)[0];
                draw(
                    &mut panels,
                    &before,
                    pointer(point, true, false, Modifiers::default()),
                    float,
                    &mut shell.atlas,
                );
                draw(
                    &mut panels,
                    &before,
                    pointer(point, false, true, Modifiers::default()),
                    float,
                    &mut shell.atlas,
                );
                let (paint, actions) =
                    draw(&mut panels, &before, copy_input(), float, &mut shell.atlas);
                run(&mut shell, actions, Intent::Copy);
                assert!(shell.layer_clipboard.keyframes_available());
                assert_eq!(shell.engine.state().unwrap()["project"], before["project"]);
                let mut ui = shell.ui_frame(paint, [1000., 800.], true);
                ui.window = u64::from(float);
                ui.views = None;
                ui.composition = None;
                ui.composition_image = None;
                ui.node_preview = None;
                crate::native_test_support::capture_ui(
                    &mut events,
                    &shell.engine,
                    ui,
                    folder.join(format!("{tag}-copied.png")),
                )
                .unwrap();
                let at = layer.property_composition_frame(7.).unwrap();
                assert!(at.fract() != 0.);
                shell.state = shell.engine.seek(at).unwrap();
                panels.keyframe_clipboard_available = true;
                let (_, actions) = draw(
                    &mut panels,
                    &shell.state,
                    Input {
                        keys: vec![(
                            InputKey::V,
                            Modifiers {
                                control: true,
                                ..Default::default()
                            },
                        )],
                        ..Default::default()
                    },
                    float,
                    &mut shell.atlas,
                );
                run(&mut shell, actions, Intent::Paste { in_place: false });
                let after = shell.state.clone();
                assert_eq!(
                    after["history"]["undoEntries"].as_u64().unwrap(),
                    before["history"]["undoEntries"].as_u64().unwrap() + 1
                );
                for target in &targets {
                    let (old, _) = authored_track(&before, target).unwrap();
                    let (new, _) = authored_track(&after, target).unwrap();
                    assert_eq!(
                        new["keys"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|k| k["frame"].as_i64().unwrap())
                            .collect::<Vec<_>>(),
                        [3, 7, 13]
                    );
                    let mut original = old["keys"][0].clone();
                    original["frame"] = json!(7);
                    assert_eq!(new["keys"][1], original);
                }
                panels.selected_keys.release();
                let (paint, _) = draw(
                    &mut panels,
                    &after,
                    Input::default(),
                    float,
                    &mut shell.atlas,
                );
                let mut ui = shell.ui_frame(paint, [1000., 800.], true);
                ui.window = u64::from(float);
                ui.views = None;
                ui.composition = None;
                ui.composition_image = None;
                ui.node_preview = None;
                crate::native_test_support::capture_ui(
                    &mut events,
                    &shell.engine,
                    ui,
                    folder.join(format!("{tag}-pasted.png")),
                )
                .unwrap();
                record(folder.join(format!("{tag}-after.json")), &after);
                if float {
                    motion_core::storage::save(
                        &folder.join("project"),
                        &serde_json::from_value::<Project>(after["project"].clone()).unwrap(),
                    )
                    .unwrap();
                    record(folder.join("saved.json"), &after);
                }
                shell.state = shell.engine.history(0).unwrap();
                assert_eq!(shell.state["project"], before["project"]);
                if float {
                    shell.state = shell.engine.history(0).unwrap();
                }
            }
            // Inject real rejected requests, preserving complete state/history.
            let before = shell.engine.state().unwrap();
            let cause = shell
                .engine
                .call(|reply| crate::engine::Command::KeyframeClipboardRead {
                    source: crate::layer_clipboard::KeyframeSource::of(&before),
                    selections: vec![],
                    local_selections: vec![motion_core::LocalKeyframeSelection {
                        target: targets[0].clone(),
                        frames: vec![4],
                    }],
                    reply,
                })
                .err()
                .expect("missing source must fail");
            assert!(cause.to_string().contains("source local key missing"), "{cause}");
            let read = shell
                .engine
                .call(|reply| crate::engine::Command::KeyframeClipboardRead {
                    source: crate::layer_clipboard::KeyframeSource::of(&before),
                    selections: vec![],
                    local_selections: vec![],
                    reply,
                })
                .unwrap();
            let clipboard = KeyframeClipboard::capture_mixed(
                &serde_json::from_value::<Project>(before["project"].clone()).unwrap(),
                &[],
                &[motion_core::LocalKeyframeSelection {
                    target: targets[0].clone(),
                    frames: vec![3],
                }],
            )
            .unwrap();
            let cause = shell
                .engine
                .call(|reply| crate::engine::Command::ClipboardCommit {
                    edit: crate::layer_clipboard::Edit::PasteKeyframes {
                        stamp: read.stamp,
                        clipboard: Arc::new(clipboard),
                        targets: vec![targets[0].clone()],
                        at_frame: Some(12.75),
                        cancel: Arc::new(AtomicBool::new(false)),
                    },
                    reply,
                })
                .unwrap_err();
            assert!(cause.to_string().contains("continuous key addressing"), "{cause}");
            let after = shell.engine.state().unwrap();
            for key in ["project", "revision", "history"] {
                assert_eq!(after[key], before[key]);
            }
            shell.engine.close_window(1).unwrap();
            shell.engine.request_shutdown().unwrap();
        }
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_TIME_CLIPBOARD_GPU_ARTIFACTS")
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
            command.env("MOTION_TIME_CLIPBOARD_REOPEN", "1");
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
    }
    let file = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    let stderr = std::fs::read_to_string(root.join("capture-stderr.log")).unwrap();
    for cause in ["source local key missing", "continuous key addressing"] {
        let line = file
            .lines()
            .find(|l| l.contains(" ERROR ") && l.contains(cause))
            .expect("default file Error");
        let request = line
            .split_once("error #")
            .unwrap()
            .1
            .split_whitespace()
            .next()
            .unwrap();
        assert!(file.contains(&format!("start #{request}")));
        assert!(stderr.contains(cause));
    }
    assert!(!std::fs::read_to_string(root.join("capture-stdout.log"))
        .unwrap()
        .contains("motion_studio::"));
    let a: Value =
        serde_json::from_slice(&std::fs::read(root.join("capture-process.json")).unwrap()).unwrap();
    let b: Value =
        serde_json::from_slice(&std::fs::read(root.join("reopen-process.json")).unwrap()).unwrap();
    assert_ne!(a["pid"], b["pid"]);
}
