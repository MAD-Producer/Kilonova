use super::*;
use crate::workspace::Workspace;

#[test]
fn preview_options_persist_restart_and_save_failure_rolls_back() {
    use crate::preview_playback::{Options,Start,Skip};
    let root = tempfile::tempdir().unwrap();
    let mut shell = Shell::new(root.path().join("project"),None,Some(Locale::En)).unwrap();
    shell.layout_path = root.path().join("layout.json");
    let before = shell.state["project"].clone();
    let options = Options { scope:motion_core::FrameScope::WorkArea,start:Start::RangeStart,skip:Skip::Two,looping:false };
    shell.action(panels::Action::PreviewPlayback(options),None).unwrap();
    shell.action(panels::Action::MovieExportScope(motion_core::FrameScope::WorkArea),None).unwrap();
    let saved = Workspace::load(&shell.layout_path);
    assert_eq!(saved.preview_playback,options); assert_eq!(saved.movie_export_scope,motion_core::FrameScope::WorkArea);
    let persisted = std::fs::read(&shell.layout_path).unwrap();
    let blocked = root.path().join("blocked");std::fs::create_dir(&blocked).unwrap();shell.layout_path = blocked;
    assert!(shell.action(panels::Action::PreviewPlayback(Default::default()),None).is_err());
    assert!(shell.action(panels::Action::MovieExportScope(motion_core::FrameScope::Composition),None).is_err());
    assert_eq!(shell.workspace.preview_playback,options); assert_eq!(shell.panels.preview_playback,options);
    assert_eq!(shell.workspace.movie_export_scope,motion_core::FrameScope::WorkArea);
    assert_eq!(shell.panels.movie_export_scope,motion_core::FrameScope::WorkArea);
    assert_eq!(std::fs::read(root.path().join("layout.json")).unwrap(),persisted);
    assert_eq!(shell.state["project"],before);
}

#[test]
fn selected_preview_options_drive_the_real_clock_and_stop_at_the_work_area_end() {
    let root = tempfile::tempdir().unwrap();
    let mut shell = Shell::new(root.path().join("project"),None,Some(Locale::En)).unwrap();
    shell.layout_path = root.path().join("layout.json");
    shell.state = shell.engine.edit(r#"{"op":"set_work_area","in_frame":20,"out_frame":50}"#.into()).unwrap();
    let options = crate::preview_playback::Options {
        scope:motion_core::FrameScope::WorkArea, start:crate::preview_playback::Start::RangeStart,
        skip:crate::preview_playback::Skip::Two, looping:false,
    };
    shell.frame = 93.;
    shell.action(panels::Action::PreviewPlayback(options),None).unwrap();
    shell.action(panels::Action::TogglePlay,None).unwrap();
    assert_eq!(shell.frame,20.); assert!(shell.panels.playing);
    let plan = shell.playback_plan.unwrap();assert_eq!(plan.range,motion_core::WorkArea {in_frame:20,out_frame:50});assert_eq!(plan.step,3);
    shell.playback = Some((Instant::now() - Duration::from_secs(2),20.));
    shell.advance_playback().unwrap();
    assert_eq!(shell.frame,49.);assert!(!shell.panels.playing && shell.playback.is_none() && shell.playback_plan.is_none());
    let (_,reply) = shell.editing.take().unwrap();assert_eq!(reply.recv_timeout(Duration::from_secs(10)).unwrap().unwrap()["frame"],49.);
    shell.action(panels::Action::TogglePlay,None).unwrap();assert!(shell.playback.is_some());
    shell.action(panels::Action::PreviewPlayback(Default::default()),None).unwrap();
    assert!(shell.playback.is_none() && shell.audio.is_none());
}

#[test]
fn preferences_shortcut_routes_once_from_main_and_floating_focus_and_respects_ime() {
    use motion_ui::{dock::Panel, input::Modifiers};
    for locale in [Locale::En, Locale::Zh] {
        for panel in [
            Panel::Timeline,
            Panel::Properties,
            Panel::Nodes,
            Panel::NodeProperties,
        ] {
            for owner in [None, Some(panel)] {
                for composing in [false, true] {
                    let mut panels = panels::Panels {
                        locale,
                        focused: Some(panel),
                        ..Default::default()
                    };
                    let input = Input {
                        keys: vec![(
                            Key::Comma,
                            Modifiers {
                                control: true,
                                ..Default::default()
                            },
                        )],
                        composing,
                        ..Default::default()
                    };
                    let mut actions = vec![];
                    let mut paint = PaintList::default();
                    let mut atlas = TextAtlas::from_system(12.).unwrap();
                    actions.extend(if owner.is_some() {
                        panels::floating_chrome(
                            &mut paint,
                            &mut atlas,
                            panel,
                            [900., 600.],
                            &mut panels,
                            &Value::Null,
                            &input,
                        )
                    } else {
                        panels::chrome(
                            &mut paint,
                            &mut atlas,
                            [900., 600.],
                            &dock::Node::dock(panel.dock()),
                            &mut panels,
                            &Value::Null,
                            &input,
                        )
                    });
                    if composing {
                        assert!(actions.is_empty());
                    } else {
                        assert!(
                            matches!(actions.as_slice(), [panels::Action::Preferences]),
                            "{actions:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn actual_preferences_apply_restart_and_failures_preserve_project_and_error_channels() {
    const CHILD: &str = "MOTION_PREFERENCES_TEST_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let mut shell = Shell::new(root.join("project"), None, Some(Locale::En)).unwrap();
        shell.layout_path = root.join("settings.json");
        let before = shell.engine.state().unwrap();
        shell.open_preferences();
        shell
            .dialog
            .as_mut()
            .unwrap()
            .preferences
            .as_mut()
            .unwrap()
            .draft
            .preview = PreviewQuality::Economy;
        shell.dialog = None; // Same discard path as Cancel/Escape.
        assert_eq!(shell.workspace.preview_quality, PreviewQuality::Auto);
        assert!(!shell.layout_path.exists());
        let mut values = shell.workspace.preferences();
        values.locale = Locale::Zh;
        values.preview = PreviewQuality::Balanced;
        values.node_view.grid = false;
        values.node_view.minimap = true;
        values.columns.matte = false;
        shell.apply_preferences(values.clone(), true).unwrap();
        assert_eq!(shell.workspace.preferences(), values);
        assert_eq!(shell.panels.locale, Locale::Zh);
        assert_eq!(shell.panels.node_editor.view_preferences, values.node_view);
        assert_eq!(shell.panels.timeline.column_visibility, values.columns);
        assert_eq!(Workspace::load(&shell.layout_path).preferences(), values);
        let persisted = std::fs::read(&shell.layout_path).unwrap();
        let old_preview = shell.engine.state().unwrap()["preview"].clone();
        let mut invalid = values.clone();
        invalid.style_path = Some(root.join("bad-style.json"));
        std::fs::write(
            invalid.style_path.as_ref().unwrap(),
            br#"{"styles":{"button":{"radius":-1}}}"#,
        )
        .unwrap();
        let error = shell.apply_preferences(invalid, true).unwrap_err();
        assert!(error.contains("phase=style"), "{error}");
        assert_eq!(shell.workspace.preferences(), values);
        assert_eq!(std::fs::read(&shell.layout_path).unwrap(), persisted);
        let valid_path = shell.layout_path.clone();
        let blocked = root.join("blocked.json");
        std::fs::create_dir(&blocked).unwrap();
        std::fs::write(blocked.join("keep"), b"existing").unwrap();
        shell.layout_path = blocked.clone();
        let mut rejected = values.clone();
        rejected.preview = PreviewQuality::Economy;
        let error = shell.apply_preferences(rejected, false).unwrap_err();
        assert!(error.contains("phase=publish"), "{error}");
        assert_eq!(shell.workspace.preferences(), values);
        assert_eq!(shell.engine.state().unwrap()["preview"], old_preview);
        assert_eq!(std::fs::read(blocked.join("keep")).unwrap(), b"existing");
        assert!(!std::fs::read_dir(&root).unwrap().any(|p| p
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")));
        shell.layout_path = valid_path;
        // A repaired, explicit user request succeeds without a retry or hidden reset.
        shell.apply_preferences(values.clone(), true).unwrap();
        let after = shell.engine.state().unwrap();
        for key in ["project", "revision", "canUndo", "canRedo", "history"] {
            assert_eq!(before[key], after[key], "changed {key}");
        }
        shell.engine.save().unwrap();
        drop(shell);
        let mut reopened = Shell::new(root.join("project"), None, None).unwrap();
        // Explicit fixture loading also works with #351's isolated unit-test Shell.
        // Production startup itself is checked with the native executable.
        reopened.workspace = Workspace::load(&root.join("settings.json"));
        reopened.restore_preview_preference().unwrap();
        assert_eq!(reopened.workspace.preferences(), values);
        assert_eq!(reopened.engine.state().unwrap()["preview"], old_preview);
        assert_eq!(
            reopened.engine.state().unwrap()["project"],
            before["project"]
        );
        crate::logging::flush();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "shell::preferences::tests::actual_preferences_apply_restart_and_failures_preserve_project_and_error_channels"])
        .env(CHILD, root.path()).env("MOTION_UI_LAYOUT", root.path().join("settings.json"))
        .env_remove("MOTION_LOG").env_remove("MOTION_LOCALE").output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let log = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    for channel in [&log, &stderr] {
        assert_eq!(
            channel
                .lines()
                .filter(|l| l.contains("preferences failed operation=apply"))
                .count(),
            2,
            "{channel}"
        );
        assert!(
            channel.contains("phase=style") && channel.contains("phase=publish"),
            "{channel}"
        );
    }
    assert_eq!(log.matches("preferences start operation=apply").count(), 4);
    assert!(!stdout.contains("preferences failed") && !stdout.contains("preferences start"));
    assert!(!stderr.contains("preferences start"));
}
