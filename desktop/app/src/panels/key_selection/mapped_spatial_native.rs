//! Actual GPU captures and serial-worker commits; OS focus and input stay untouched.
use super::*;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows};
const NAME: &str = "panels::key_selection::tests::ui::spatial::mapped::native::mapped_spatial_native_views_commit_and_reopen";
fn record(path: impl AsRef<Path>, value: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn capture(
    shell: &mut crate::Shell,
    events: &mut EventLoop<()>,
    mut paint: PaintList,
    floating: bool,
    path: PathBuf,
) {
    paint.stylesheet = shell.panels.styles.clone();
    let mut ui = shell.ui_frame(paint, [1000., 800.], true);
    ui.composition = None;
    ui.composition_image = None;
    ui.node_preview = None;
    ui.window = u64::from(floating);
    crate::native_test_support::capture_ui(events, &shell.engine, ui, path.clone()).unwrap();
    assert_eq!(
        image::open(path).unwrap().to_rgba8().dimensions(),
        (1000, 800)
    );
}

#[test]
#[ignore = "manual mapped spatial GPU captures, worker commits and independent process reopen"]
fn mapped_spatial_native_views_commit_and_reopen() {
    const CHILD: &str = "MOTION_MAPPED_SPATIAL_NATIVE_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let temp = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_MAPPED_SPATIAL_NATIVE_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| temp.path().into());
        std::fs::create_dir_all(&root).unwrap();
        for reopen in [false, true] {
            let out = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    NAME,
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(CHILD, &root)
                .env(
                    "MOTION_MAPPED_SPATIAL_REOPEN",
                    if reopen { "1" } else { "0" },
                )
                .env_remove("MOTION_LOG")
                .output()
                .unwrap();
            std::fs::write(root.join(format!("{reopen}-stdout.log")), &out.stdout).unwrap();
            std::fs::write(root.join(format!("{reopen}-stderr.log")), &out.stderr).unwrap();
            assert!(
                out.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        let read = |name| -> Value {
            serde_json::from_slice(&std::fs::read(root.join(name)).unwrap()).unwrap()
        };
        assert_ne!(
            read("capture-process.json")["pid"],
            read("reopen-process.json")["pid"]
        );
        let count = ["forward", "reverse"]
            .iter()
            .map(|d| {
                std::fs::read_dir(root.join(d))
                    .unwrap()
                    .map(Result::unwrap)
                    .filter(|e| e.path().extension().is_some_and(|x| x == "png"))
                    .count()
            })
            .sum::<usize>();
        assert_eq!(count, 80);
        return;
    };
    let root = PathBuf::from(root);
    let reopen = std::env::var("MOTION_MAPPED_SPATIAL_REOPEN").unwrap() == "1";
    let _log = crate::logging::init(root.join(if reopen {
        "reopen-logs"
    } else {
        "capture-logs"
    }))
    .unwrap();
    record(
        root.join(if reopen {
            "reopen-process.json"
        } else {
            "capture-process.json"
        }),
        &json!({"pid":std::process::id()}),
    );
    if reopen {
        for tag in ["forward", "reverse"] {
            let folder = root.join(tag);
            let expected: Value =
                serde_json::from_slice(&std::fs::read(folder.join("saved.json")).unwrap()).unwrap();
            let engine = crate::engine::Engine::start(
                folder.join("project"),
                motion_desktop_media::platform(),
                8 << 30,
            )
            .unwrap();
            let state = engine.state().unwrap();
            assert_eq!(state["project"], expected["project"]);
            assert_eq!(state["history"]["undoEntries"], 0);
            record(folder.join("reopened.json"), &state);
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
                        .with_inner_size(winit::dpi::PhysicalSize::new(1000, 800)),
                )
                .unwrap(),
        )
    });
    for reverse in [false, true] {
        let folder = root.join(if reverse { "reverse" } else { "forward" });
        std::fs::create_dir_all(&folder).unwrap();
        let before = mapped(reverse).snapshot();
        motion_core::storage::save(&folder.join("project"), &before).unwrap();
        let mut shell = crate::Shell::new(folder.join("project"), None, Some(Locale::Zh)).unwrap();
        shell.home = None;
        crate::native_test_support::receive(
            &mut events,
            &shell
                .engine
                .attach_async(windows[0].clone(), 1000, 800)
                .unwrap(),
            Duration::from_secs(60),
        )
        .unwrap()
        .unwrap();
        shell
            .engine
            .float_window(1, windows[1].clone(), 1000, 800)
            .unwrap();
        shell.window = Some(windows[0].clone());
        shell.attached = true;
        let baseline = shell.engine.state().unwrap();
        let expected = serde_json::to_value(shifted(&before, reverse, 3.)).unwrap();
        let mut cases = vec![];
        for (locale, language) in [(Locale::Zh, "zh"), (Locale::En, "en")] {
            for floating in [false, true] {
                for mode in MODES {
                    let state = shell.engine.state().unwrap();
                    assert_eq!(state["project"], baseline["project"]);
                    let mut p = setup(locale, mode, &state);
                    p.styles = shell.panels.styles.clone();
                    let (paint, out) =
                        draw(&mut p, &state, Input::default(), floating, &mut shell.atlas);
                    selection_only(&out);
                    let at = at_time(&paint, &p, selected_time(&state));
                    let ppf = p.timeline.zoom_track.unwrap().width() / 120. * p.timeline.zoom;
                    let to = [at[0] + 3. * ppf, at[1]];
                    let (_, out) = draw(
                        &mut p,
                        &state,
                        pointer(at, true, false, delta_input()),
                        floating,
                        &mut shell.atlas,
                    );
                    selection_only(&out);
                    assert!(p.key_gestures.dragging());
                    let (paint, out) = draw(
                        &mut p,
                        &state,
                        pointer(to, false, false, delta_input()),
                        floating,
                        &mut shell.atlas,
                    );
                    selection_only(&out);
                    assert_eq!(
                        shell.engine.state().unwrap()["project"],
                        baseline["project"]
                    );
                    let tag = format!(
                        "{language}-{}-{mode}",
                        if floating { "float" } else { "main" }
                    );
                    capture(
                        &mut shell,
                        &mut events,
                        paint,
                        floating,
                        folder.join(format!("{tag}-preview.png")),
                    );
                    let (_, out) = draw(
                        &mut p,
                        &state,
                        pointer(to, false, true, delta_input()),
                        floating,
                        &mut shell.atlas,
                    );
                    let commands = atomic(out);
                    assert!(matches!(commands.as_slice(),[Command::SpatialKeyframes {
                    edit: SpatialKeyEdit::Move {keys,delta},..}] if keys==&[2] && *delta==3.));
                    let after = shell
                        .engine
                        .call(|reply| crate::engine::Command::EditorAction {
                            source: crate::layer_clipboard::KeyframeSource::of(&state),
                            commands,
                            reply,
                        })
                        .unwrap();
                    assert_eq!(after["project"], expected);
                    assert_eq!(after["history"]["undoEntries"], 1);
                    assert_eq!(identities(&p), [2]);
                    let (paint, out) =
                        draw(&mut p, &after, Input::default(), floating, &mut shell.atlas);
                    selection_only(&out);
                    capture(
                        &mut shell,
                        &mut events,
                        paint,
                        floating,
                        folder.join(format!("{tag}-after.png")),
                    );
                    record(folder.join(format!("{tag}-after.json")), &after);
                    assert_eq!(
                        shell.engine.history(0).unwrap()["project"],
                        baseline["project"]
                    );
                    assert_eq!(shell.engine.history(1).unwrap()["project"], expected);
                    let saved = shell.engine.save().unwrap();
                    assert_eq!(saved["project"], expected);
                    record(folder.join("saved.json"), &saved);
                    assert_eq!(
                        shell.engine.history(0).unwrap()["project"],
                        baseline["project"]
                    );
                    cases.push(
                        json!({"tag":tag,"ids":[2],"compositionDelta":3.,"undo":true,"redo":true}),
                    );
                }
            }
        }
        assert_eq!(cases.len(), 20);
        // Leave the explicit saved document as the final worker state too.
        assert_eq!(shell.engine.history(1).unwrap()["project"], expected);
        record(folder.join("saved.json"), &shell.engine.save().unwrap());
        record(folder.join("cases.json"), &json!(cases));
        shell.engine.request_shutdown().unwrap();
        drop(shell);
    }
}
