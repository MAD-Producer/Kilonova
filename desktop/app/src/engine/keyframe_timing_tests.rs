//! Actual serial refusal, recovery and default output channels for timing edits.
use super::*;
use motion_core::timing::{TimingInterpolation, TimingSettings, TimingSide, VelocityUnits};
use motion_core::{Ease, Layer, Project, Property};

#[test]
fn timing_refusals_reach_default_file_and_stderr_without_partial_history_then_recover() {
    const NAME: &str = "engine::keyframe_timing_tests::timing_refusals_reach_default_file_and_stderr_without_partial_history_then_recover";
    const CHILD: &str = "MOTION_KEY_TIMING_ERROR_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logging = crate::logging::init(root.join("logs")).unwrap();
        let mut project = Project::new(640, 360, 30, 120).unwrap();
        let mut layer = Layer::solid(7, "Timing", [64.; 2], [0.; 3], [1.; 4]);
        for frame in [0, 30, 60] {
            layer
                .transform
                .position
                .upsert(frame, [frame as f32, 0., 0.], Ease::Linear)
                .unwrap();
            layer
                .transform
                .opacity
                .upsert(frame, 0.5, Ease::Linear)
                .unwrap();
        }
        project.layers.push(layer);
        let directory = root.join("project");
        motion_core::storage::save(&directory, &project).unwrap();
        let engine = Engine::start(directory, motion_desktop_media::platform(), 8 << 30).unwrap();
        let selected = |object, property, frame| motion_core::KeyframeSelection {
            target: motion_core::KeyframeTarget::Property {
                object,
                property,
                axis: None,
            },
            frames: vec![frame],
        };
        let config = |speed, influence| TimingSettings {
            interpolation: TimingInterpolation::Bezier,
            units: VelocityUnits::PropertyPerSecond,
            incoming: None,
            outgoing: Some(TimingSide { speed, influence }),
            match_speed: false,
        };
        let apply = |state: &Value, selections, settings| {
            engine.call(|reply| Command::EditorAction {
                source: crate::layer_clipboard::KeyframeSource::of(state),
                commands: vec![motion_core::Command::KeyframeTiming {
                    selections,
                    settings,
                }],
                reply,
            })
        };
        let before = engine.state().unwrap();
        for (target, settings, reason) in [
            (
                selected(7, Property::Position, 0),
                config(Some(f64::NAN), None),
                "speed must be finite",
            ),
            (
                selected(7, Property::Position, 0),
                config(None, Some(101.)),
                "influence must be greater than zero",
            ),
            (
                selected(7, Property::Opacity, 0),
                config(Some(1.), None),
                "stationary keyframe endpoint cannot have nonzero speed",
            ),
            (
                selected(999, Property::Position, 0),
                config(None, Some(30.)),
                "object 999 does not exist",
            ),
            (
                selected(7, Property::Position, 1),
                config(None, Some(30.)),
                "selected keyframe does not exist",
            ),
        ] {
            let error = apply(&before, vec![target], settings).unwrap_err();
            assert!(
                error.contains(reason) && error.contains("operation=timing"),
                "{error}"
            );
            let after = engine.state().unwrap();
            for field in ["project", "revision", "history"] {
                assert_eq!(after[field], before[field]);
            }
        }
        let changed = apply(
            &before,
            vec![selected(7, Property::Position, 30)],
            config(Some(10.), Some(40.)),
        )
        .unwrap();
        assert_ne!(changed["project"], before["project"]);
        assert_eq!(changed["history"]["undoEntries"], 1);
        let stale = apply(
            &before,
            vec![selected(7, Property::Position, 30)],
            config(Some(5.), Some(20.)),
        )
        .unwrap();
        assert_eq!(stale["editorAction"]["cancelled"], true);
        assert_eq!(stale["editorAction"]["operation"], "keyframe_timing");
        assert_eq!(stale["project"], changed["project"]);
        assert_eq!(stale["history"], changed["history"]);
        assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
        engine.request_shutdown().unwrap();
        let open_directory = root.join("open-project");
        let original: Project = serde_json::from_value(before["project"].clone()).unwrap();
        motion_core::storage::save(&open_directory, &original).unwrap();
        let mut shell =
            crate::Shell::new(open_directory, None, Some(crate::i18n::Locale::En)).unwrap();
        let before_open = shell.state.clone();
        let mut stale_open = before_open.clone();
        stale_open["revision"] = 9999.into();
        shell
            .open_keyframe_timing(
                crate::layer_clipboard::KeyframeSource::of(&stale_open),
                vec![selected(7, Property::Position, 30)],
            )
            .unwrap();
        assert!(shell.dialog.is_none());
        assert!(shell
            .panels
            .message
            .as_ref()
            .unwrap()
            .contains("not applied"));
        for field in ["project", "revision", "history"] {
            assert_eq!(shell.state[field], before_open[field]);
        }
        let cause = shell
            .open_keyframe_timing(
                crate::layer_clipboard::KeyframeSource::of(&before_open),
                vec![selected(999, Property::Position, 0)],
            )
            .unwrap_err();
        assert!(cause.contains("object 999 does not exist"));
        assert!(shell.dialog.is_none());
        for field in ["project", "revision", "history"] {
            assert_eq!(shell.state[field], before_open[field]);
        }
        let invalid_precision = "z".repeat(8192);
        shell.state["project"]["working_precision"] = invalid_precision.clone().into();
        let cause = shell
            .open_keyframe_timing(
                crate::layer_clipboard::KeyframeSource::of(&before_open),
                vec![selected(7, Property::Position, 30)],
            )
            .unwrap_err();
        assert!(
            cause.contains(&invalid_precision),
            "the returned primary cause must stay complete"
        );
        assert!(shell.dialog.is_none());
        shell.state = before_open.clone();
        shell
            .open_keyframe_timing(
                crate::layer_clipboard::KeyframeSource::of(&before_open),
                vec![selected(7, Property::Position, 30)],
            )
            .unwrap();
        assert!(shell.dialog.is_some());
        shell.engine.request_shutdown().unwrap();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_KEY_TIMING_ERROR_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
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
    let file = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for reason in [
        "speed must be finite",
        "influence must be greater than zero",
        "stationary keyframe endpoint cannot have nonzero speed",
        "object 999 does not exist",
        "selected keyframe does not exist",
    ] {
        assert!(
            file.lines().any(|l| l.contains(" ERROR ")
                && l.contains("editor_action")
                && l.contains(reason)),
            "{file}"
        );
        assert!(stderr.contains(reason), "{stderr}");
        assert!(!stdout.contains(reason), "{stdout}");
    }
    assert_eq!(
        file.lines()
            .filter(|l| l.contains(" ERROR ") && l.contains("editor_action"))
            .count(),
        5
    );
    assert!(file
        .lines()
        .any(|l| l.contains(" INFO ") && l.contains("start #") && l.ends_with(" editor_action")));
    assert!(!stderr.contains("start #") && !stdout.contains("start #"));
    assert!(!stderr.contains("context_changed") && !stdout.contains("context_changed"));
    assert!(file.lines().any(|l| l.contains(" INFO ")
        && l.contains("keyframe timing start operation=open")
        && l.contains("revision=Some(")
        && l.contains("generation=Some(")
        && l.contains("object=Some(999)")));
    assert!(file.lines().any(|l| l.contains(" ERROR ")
        && l.contains("keyframe timing failed operation=open")
        && l.contains("object=Some(999)")
        && l.contains("object 999 does not exist")));
    assert!(stderr.contains("keyframe timing failed operation=open"));
    assert!(!stderr.contains("keyframe timing start") && !stdout.contains("keyframe timing"));
    assert_eq!(
        file.lines()
            .filter(|l| l.contains("keyframe timing start operation=open"))
            .count(),
        3,
        "stale opens must not start a profile operation"
    );
    assert_eq!(file.lines().filter(|l| l.contains(" ERROR ") && l.contains("keyframe timing failed operation=open")).count(), 2,
        "stale opens must not be logged as faults");
    for output in [&file, stderr.as_ref()] {
        let line = output
            .lines()
            .find(|l| l.contains("keyframe timing failed operation=open") && l.contains("zzzzzzzz"))
            .unwrap();
        assert_eq!(
            line.split_once("cause=").unwrap().1.chars().count(),
            2048,
            "bound the original cause independently of the logger's thread-name prefix"
        );
        assert!(!line.contains(&"z".repeat(8192)));
    }
}
