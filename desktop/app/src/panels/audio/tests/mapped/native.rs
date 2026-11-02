use super::*;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows};

fn capture(events: &mut EventLoop<()>, h: &mut Harness, path: &Path) {
    h.state = h.engine.state().unwrap();
    let mut paint = PaintList::default();
    let actions = if h.floating {
        floating_chrome(
            &mut paint,
            &mut h.atlas,
            Panel::Timeline,
            [1000., 700.],
            &mut h.panels,
            &h.state,
            &Input::default(),
        )
    } else {
        chrome(
            &mut paint,
            &mut h.atlas,
            [1000., 700.],
            &Node::dock(Panel::Timeline.dock()),
            &mut h.panels,
            &h.state,
            &Input::default(),
        )
    };
    assert!(actions.is_empty());
    crate::native_test_support::capture_ui(
        events,
        &h.engine,
        crate::engine::UiFrame {
            views: None,
            inspection: Default::default(),
            paint,
            atlas: Some(crate::engine::AtlasUpload {
                pixels: h.atlas.pixel_data().to_vec(),
                width: h.atlas.config().width,
                height: h.atlas.config().height,
            }),
            logical_size: [1000., 700.],
            composition: None,
            composition_image: None,
            window: u64::from(h.floating),
            node_preview: None,
        },
        path.to_owned(),
    )
    .unwrap();
    assert_eq!(
        image::open(path).unwrap().to_rgba8().dimensions(),
        (1000, 700)
    );
}
fn record(path: impl AsRef<Path>, value: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

#[test]
#[ignore = "manual native mapped audio envelope input, GPU captures, error channel and separate-process reopen"]
fn native_audio_envelopes_keep_exact_time_and_cancel_in_both_owners_and_languages() {
    const NAME:&str="panels::audio::tests::mapped::native::native_audio_envelopes_keep_exact_time_and_cancel_in_both_owners_and_languages";
    const CHILD: &str = "MOTION_AUDIO_AUTHORING_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let temporary = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_AUDIO_AUTHORING_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| temporary.path().into());
        std::fs::create_dir_all(&root).unwrap();
        for reopening in [false, true] {
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
            if reopening {
                command.env("MOTION_AUDIO_AUTHORING_REOPEN", "1");
            } else {
                command.env_remove("MOTION_AUDIO_AUTHORING_REOPEN");
            }
            let result = command.output().unwrap();
            std::fs::write(root.join(format!("{reopening}-stdout.log")), &result.stdout).unwrap();
            std::fs::write(root.join(format!("{reopening}-stderr.log")), &result.stderr).unwrap();
            assert!(
                result.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
        }
        let log = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
        let stderr = std::fs::read_to_string(root.join("false-stderr.log")).unwrap();
        let stdout = std::fs::read_to_string(root.join("false-stdout.log")).unwrap();
        assert!(log
            .lines()
            .any(|l| l.contains("INFO") && l.contains("start #") && l.ends_with(" edit")));
        for text in [
            "continuous key addressing",
            "phase=track_at object=7 channel=audio/Left",
        ] {
            assert!(log.contains(text), "{text}: {log}");
            assert!(stderr.contains(text));
            assert!(!stdout.contains(text));
        }
        let first: Value =
            serde_json::from_slice(&std::fs::read(root.join("capture-process.json")).unwrap())
                .unwrap();
        let second: Value =
            serde_json::from_slice(&std::fs::read(root.join("reopen-process.json")).unwrap())
                .unwrap();
        assert_ne!(first["pid"], second["pid"]);
        return;
    };
    let root = PathBuf::from(root);
    let reopening = std::env::var_os("MOTION_AUDIO_AUTHORING_REOPEN").is_some();
    let _log =
        crate::logging::init(root.join(if reopening { "reopen-logs" } else { "logs" })).unwrap();
    record(
        root.join(if reopening {
            "reopen-process.json"
        } else {
            "capture-process.json"
        }),
        &json!({"pid":std::process::id()}),
    );
    if reopening {
        let folders: Vec<_> = std::fs::read_dir(&root)
            .unwrap()
            .map(Result::unwrap)
            .map(|e| e.path())
            .filter(|p| p.join("saved.json").is_file())
            .collect();
        assert_eq!(folders.len(), 8);
        for folder in folders {
            let engine = Engine::start(
                folder.join("project"),
                motion_desktop_media::platform(),
                8 << 30,
            )
            .unwrap();
            let saved: Value =
                serde_json::from_slice(&std::fs::read(folder.join("saved.json")).unwrap()).unwrap();
            let actual = engine.state().unwrap();
            assert_eq!(actual["project"], saved["project"]);
            assert_eq!(actual["history"]["undoEntries"], 0);
            record(folder.join("reopened.json"), &actual);
            engine.request_shutdown().unwrap();
        }
        return;
    }
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let windows: [Arc<winit::window::Window>; 2] = std::array::from_fn(|_| {
        Arc::new(
            events
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(1000, 700)),
                )
                .unwrap(),
        )
    });
    for reverse in [false, true] {
        for locale in [Locale::Zh, Locale::En] {
            for floating in [false, true] {
                let folder = root.join(format!("{locale:?}-{floating}-{reverse}"));
                std::fs::create_dir_all(&folder).unwrap();
                let mut h = harness(floating, locale, reverse);
                crate::native_test_support::receive(
                    &mut events,
                    &h.engine
                        .attach_async(windows[0].clone(), 1000, 700)
                        .unwrap(),
                    Duration::from_secs(60),
                )
                .unwrap()
                .unwrap();
                h.engine
                    .float_window(1, windows[1].clone(), 1000, 700)
                    .unwrap();
                let before = h.engine.state().unwrap()["project"].clone();
                capture(&mut events, &mut h, &folder.join("before.png"));
                h.type_number(Control::Left, "-17.25");
                assert_eq!(h.state["project"], expected(&before, "left", -17.25));
                let after = h.state["project"].clone();
                capture(&mut events, &mut h, &folder.join("committed.png"));
                assert_eq!(h.engine.history(0).unwrap()["project"], before);
                assert_eq!(h.engine.history(1).unwrap()["project"], after);
                let r = h.field(Control::Right);
                let point = [r.min[0] + 5., r.min[1] + 10.];
                let actions = h.draw(pointer(point, true, false));
                h.run(actions);
                let actions = h.draw(pointer([point[0] + 20., point[1]], false, false));
                h.run(actions);
                assert_ne!(h.state["project"], after);
                let actions = h.draw(Input {
                    focus_lost: true,
                    ..Default::default()
                });
                h.run(actions);
                assert_eq!(h.state["project"], after);
                capture(&mut events, &mut h, &folder.join("cancelled.png"));
                let error = h
                    .engine
                    .edit(
                        serde_json::to_string(&Command::TrackAt {
                            target: target(AudioChannel::Left),
                            frame: 8.75,
                            edit: TrackEdit::Set {
                                value: TrackValue::Scalar(-10.),
                            },
                        })
                        .unwrap(),
                    )
                    .unwrap_err();
                assert!(error.contains("continuous key addressing"));
                assert_eq!(h.engine.state().unwrap()["project"], after);
                let saved = h.engine.save().unwrap();
                record(folder.join("saved.json"), &saved);
                let source = h._folder.path().join("mapped-project");
                let dest = folder.join("project");
                std::fs::create_dir_all(dest.join("assets")).unwrap();
                std::fs::copy(source.join("project.json"), dest.join("project.json")).unwrap();
                std::fs::copy(source.join("assets/tone.wav"), dest.join("assets/tone.wav"))
                    .unwrap();
                h.engine.close_window(1).unwrap();
                h.engine.request_shutdown().unwrap();
            }
        }
    }
    crate::logging::flush();
}
