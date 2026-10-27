//! Guide transactions through the actual serial worker and host source guard.
use super::tests::{draw, press, release};
use super::*;
use crate::{
    engine::{Command as WorkerCommand, Engine},
    viewer_navigation::Navigation,
};
use std::time::Duration;

fn fixture() -> (tempfile::TempDir, Engine) {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("project");
    motion_core::storage::save(&root, &motion_core::Project::new(640, 480, 30, 24).unwrap())
        .unwrap();
    let engine = Engine::start(root, motion_desktop_media::platform(), 8 << 30).unwrap();
    (folder, engine)
}
fn dispatch(engine: &Engine, action: Action) -> Result<Value, String> {
    let Action::EditorAction { source, commands } = action else {
        panic!("guide edit bypassed the source-checked worker")
    };
    engine.call(|reply| WorkerCommand::EditorAction {
        source,
        commands,
        reply,
    })
}
fn set(state: &Value, position: f64) -> Action {
    edit(
        state,
        KeyframeSource::of(state),
        vec![Guide {
            id: 1,
            axis: GuideAxis::X,
            position,
        }],
    )
}

#[test]
fn real_ui_guide_release_commits_once_undoes_redoes_saves_and_reopens_in_both_locales_and_windows()
{
    let (_folder, mut engine) = fixture();
    let mut atlas = TextAtlas::from_system(18.).unwrap();
    let nav = Navigation::default();
    let map = nav
        .mapping(Rect::new(30., 70., 800., 600.), [640., 480.])
        .unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for window in [None, Some(Panel::Composition)] {
            let before = engine
                .edit(serde_json::to_string(&Command::SetGuides { guides: vec![] }).unwrap())
                .unwrap();
            let mut controls = Controls::default();
            controls.visibility.rulers = true;
            let (owned, actions) = draw(
                &mut controls,
                &before,
                &press([38., 370.]),
                window,
                nav,
                locale,
                &mut atlas,
            );
            assert!(owned && actions.is_empty());
            let (_, mut actions) = draw(
                &mut controls,
                &before,
                &release(map.screen([333.25, 240.])),
                window,
                nav,
                locale,
                &mut atlas,
            );
            assert_eq!(actions.len(), 1);
            let after = dispatch(&engine, actions.remove(0)).unwrap();
            assert_eq!(
                after["project"]["guides"],
                serde_json::json!([{"id":1,"axis":"x","position":333.25}])
            );
            assert_eq!(after["project"]["layers"], before["project"]["layers"]);
            assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
            assert_eq!(engine.history(1).unwrap()["project"], after["project"]);
            engine.save().unwrap();
            let live = engine.state().unwrap();
            let root = std::path::PathBuf::from(live["root"].as_str().unwrap());
            let old_action = set(&live, 100.);
            drop(engine);
            engine = Engine::start(root, motion_desktop_media::platform(), 8 << 30).unwrap();
            let reopened = engine.state().unwrap();
            assert_eq!(reopened["project"], after["project"]);
            let cancelled = dispatch(&engine, old_action).unwrap();
            assert_eq!(cancelled["editorAction"]["cancelled"], true);
            assert_eq!(cancelled["project"], reopened["project"]);
            assert_eq!(cancelled["revision"], reopened["revision"]);
        }
    }
}

#[test]
fn queued_guide_action_cannot_overwrite_a_new_revision_and_individual_source_fields_are_checked() {
    let (_folder, engine) = fixture();
    let before = engine.state().unwrap();
    let (entered, entered_rx) = std::sync::mpsc::channel();
    let (release, release_rx) = std::sync::mpsc::channel();
    let waiting = engine
        .request(|reply| WorkerCommand::TestWait {
            entered,
            release: release_rx,
            reply,
        })
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let changed = engine
        .edit_async(
            serde_json::to_string(&Command::SetWorkArea {
                in_frame: 2,
                out_frame: 20,
            })
            .unwrap(),
        )
        .unwrap();
    let Action::EditorAction { source, commands } = set(&before, 250.) else {
        unreachable!()
    };
    let stale = engine
        .request(|reply| WorkerCommand::EditorAction {
            source,
            commands,
            reply,
        })
        .unwrap();
    assert!(matches!(
        changed.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    assert!(matches!(
        stale.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    release.send(()).unwrap();
    waiting
        .recv_timeout(Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let changed = changed
        .recv_timeout(Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let stale = stale
        .recv_timeout(Duration::from_secs(30))
        .unwrap()
        .unwrap();
    assert_eq!(stale["editorAction"]["cancelled"], true);
    assert_eq!(stale["project"], changed["project"]);
    assert_eq!(stale["revision"], changed["revision"]);
    assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
    let current = engine.state().unwrap();
    for field in ["root", "composition", "document_generation", "revision"] {
        let mut other = current.clone();
        other[field] = if matches!(field, "root" | "composition") {
            "different-source".into()
        } else {
            (current[field].as_u64().unwrap() + 1).into()
        };
        let action = edit(
            &current,
            KeyframeSource::of(&other),
            vec![Guide {
                id: 1,
                axis: GuideAxis::Y,
                position: 12.,
            }],
        );
        let cancelled = dispatch(&engine, action).unwrap();
        assert_eq!(cancelled["editorAction"]["cancelled"], true, "{field}");
        assert_eq!(cancelled["project"], current["project"]);
        assert_eq!(cancelled["revision"], current["revision"]);
    }
    let after = dispatch(&engine, set(&current, 123.5)).unwrap();
    assert_eq!(after["project"]["guides"][0]["position"], 123.5);
    assert_eq!(engine.history(0).unwrap()["project"], current["project"]);
}

#[test]
fn real_worker_guide_failure_preserves_default_file_error_stderr_and_stdout_and_normal_cancellation(
) {
    const CHILD: &str = "MOTION_GUIDE_WORKER_FAULT_CHILD";
    const NAME:&str="panels::viewer_guides::worker_tests::real_worker_guide_failure_preserves_default_file_error_stderr_and_stdout_and_normal_cancellation";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let (_folder, engine) = fixture();
        let before = engine.state().unwrap();
        let action = edit(
            &before,
            KeyframeSource::of(&before),
            vec![Guide {
                id: 0,
                axis: GuideAxis::X,
                position: 20.,
            }],
        );
        let error = dispatch(&engine, action).unwrap_err();
        assert!(error.contains("guide IDs must be unique and nonzero"));
        let after = engine.state().unwrap();
        assert_eq!(after["project"], before["project"]);
        assert_eq!(after["revision"], before["revision"]);
        crate::logging::flush();
        let path = root.join("motion-studio.log");
        let log = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<_> = log.lines().collect();
        let start = lines
            .iter()
            .position(|line| {
                line.contains("INFO")
                    && line.contains("start #")
                    && line.ends_with(" editor_action")
            })
            .unwrap();
        let request = lines[start]
            .split("start #")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        let failed = lines
            .iter()
            .position(|line| {
                line.contains("ERROR")
                    && line.contains(&format!("error #{request} editor_action"))
                    && line.contains("guide IDs must be unique and nonzero")
            })
            .unwrap();
        assert!(start < failed, "start must precede the matching failure");
        let errors = log
            .lines()
            .filter(|line| line.contains("ERROR") && line.contains(" editor_action "))
            .count();
        let mut other = before.clone();
        other["document_generation"] = (before["document_generation"].as_u64().unwrap() + 1).into();
        assert_eq!(
            dispatch(&engine, set(&other, 50.)).unwrap()["editorAction"]["cancelled"],
            true
        );
        crate::logging::flush();
        let after_log = std::fs::read_to_string(path).unwrap();
        assert_eq!(
            after_log
                .lines()
                .filter(|line| line.contains("ERROR") && line.contains(" editor_action "))
                .count(),
            errors
        );
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_GUIDE_WORKER_FAULT_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env_remove("MOTION_LOG")
        .env(CHILD, &root)
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
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("guide IDs must be unique and nonzero")
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("motion_studio::engine:"));
}
