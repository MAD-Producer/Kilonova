use super::*;
use crate::shortcuts::{bindings::Bindings, Command as Shortcut, Scope};
use crate::workspace::Workspace;
use motion_ui::input::{KeyChord, Modifiers};

fn changed() -> Bindings {
    let mut bindings = Bindings::default();
    bindings
        .bind(
            Scope::Application,
            Shortcut::Preferences,
            Some(KeyChord::new(Key::E, true, false, false)),
        )
        .unwrap();
    bindings
        .bind(
            Scope::Nodes,
            Shortcut::ToggleNodeGrid,
            Some(KeyChord::new(Key::B, false, false, false)),
        )
        .unwrap();
    bindings
        .bind(
            Scope::Animation,
            Shortcut::Reveal(motion_core::Property::Position),
            Some(KeyChord::new(Key::Q, false, false, false)),
        )
        .unwrap();
    bindings
}

#[test]
fn custom_preferences_routes_once_in_every_main_and_floating_context_and_defers_to_input_owners() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::Zh, Locale::En] {
        for panel in [
            dock::Panel::Timeline,
            dock::Panel::Properties,
            dock::Panel::Nodes,
            dock::Panel::NodeProperties,
            dock::Panel::NodePreview,
        ] {
            for floating in [false, true] {
                for (key, blocked) in [(Key::E, false), (Key::Comma, false), (Key::E, true)] {
                    let bindings = Arc::new(changed());
                    let mut panels = panels::Panels {
                        locale,
                        focused: Some(panel),
                        shortcut_bindings: bindings.clone(),
                        ..Default::default()
                    };
                    panels.node_editor.shortcut_bindings = bindings;
                    let input = Input {
                        keys: vec![(
                            key,
                            Modifiers {
                                control: true,
                                ..Default::default()
                            },
                        )],
                        ime_consumed: blocked,
                        ..Default::default()
                    };
                    let mut list = PaintList::default();
                    let actions = if floating {
                        panels::floating_chrome(
                            &mut list,
                            &mut atlas,
                            panel,
                            [900., 600.],
                            &mut panels,
                            &Value::Null,
                            &input,
                        )
                    } else {
                        panels::chrome(
                            &mut list,
                            &mut atlas,
                            [900., 600.],
                            &dock::Node::dock(panel.dock()),
                            &mut panels,
                            &Value::Null,
                            &input,
                        )
                    };
                    if key == Key::E && !blocked {
                        assert!(
                            matches!(actions.as_slice(), [panels::Action::Preferences]),
                            "{panel:?} floating={floating} {actions:?}"
                        );
                    } else {
                        assert!(
                            actions.is_empty(),
                            "{panel:?} floating={floating} {actions:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn actual_shortcut_presets_apply_reopen_cancel_and_failure_channels_preserve_valid_settings() {
    const CHILD: &str = "MOTION_SHORTCUT_PRESET_TEST_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let mut shell = Shell::new(root.join("project"), None, Some(Locale::En)).unwrap();
        shell.layout_path = root.join("settings.json");
        let before = shell.engine.state().unwrap();
        shell.open_preferences();
        let bindings = changed();
        let path = root.join("preset.json");
        std::fs::write(&path, bindings.export_bytes().unwrap()).unwrap();
        shell.shortcut_preset(&path, false).unwrap();
        assert_eq!(
            shell
                .dialog
                .as_ref()
                .unwrap()
                .preferences
                .as_ref()
                .unwrap()
                .draft
                .shortcuts,
            bindings
        );
        assert_eq!(
            shell.workspace.shortcut_bindings,
            Bindings::default(),
            "import changes only the draft"
        );
        let export = root.join("export.json");
        shell.shortcut_preset(&export, true).unwrap();
        assert_eq!(Bindings::import(&export).unwrap(), bindings);
        std::fs::write(&path, b"{").unwrap();
        assert!(shell
            .shortcut_preset(&path, false)
            .unwrap_err()
            .contains("invalid shortcut preset"));
        assert_eq!(
            shell
                .dialog
                .as_ref()
                .unwrap()
                .preferences
                .as_ref()
                .unwrap()
                .draft
                .shortcuts,
            bindings
        );
        let blocked = root.join("blocked.json");
        std::fs::create_dir(&blocked).unwrap();
        std::fs::write(blocked.join("keep"), b"original").unwrap();
        assert!(shell
            .shortcut_preset(&blocked, true)
            .unwrap_err()
            .contains("phase=publish"));
        assert_eq!(std::fs::read(blocked.join("keep")).unwrap(), b"original");
        assert!(!std::fs::read_dir(&root).unwrap().any(|p| p
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")));
        shell.dialog = None; // Cancel: neither runtime nor disk is changed.
        assert_eq!(shell.workspace.shortcut_bindings, Bindings::default());
        assert!(!shell.layout_path.exists());
        shell.open_preferences();
        shell.shortcut_preset(&export, false).unwrap();
        let values = shell
            .dialog
            .as_ref()
            .unwrap()
            .preferences
            .as_ref()
            .unwrap()
            .draft
            .clone();
        shell.apply_preferences(values, false).unwrap();
        assert_eq!(*shell.panels.shortcut_bindings, bindings);
        assert!(Arc::ptr_eq(
            &shell.panels.shortcut_bindings,
            &shell.panels.node_editor.shortcut_bindings
        ));
        assert!(Arc::ptr_eq(
            &shell.panels.shortcut_bindings,
            &shell.panels.layer_actions.shortcut_bindings
        ));
        assert_eq!(
            Workspace::load(&shell.layout_path).shortcut_bindings,
            bindings
        );
        shell.engine.save().unwrap();
        let after = shell.engine.state().unwrap();
        for field in ["project", "revision", "canUndo", "canRedo"] {
            assert_eq!(before[field], after[field], "{field}");
        }
        drop(shell);
        let mut reopened = Shell::new(root.join("project"), None, Some(Locale::En)).unwrap();
        reopened.workspace = Workspace::load(&root.join("settings.json"));
        reopened.install_shortcut_bindings();
        assert_eq!(*reopened.panels.node_editor.shortcut_bindings, bindings);
        assert_eq!(
            reopened
                .panels
                .shortcut_bindings
                .hint(Scope::Application, Shortcut::Preferences),
            "Ctrl+E"
        );
        crate::logging::flush();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output=std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact","shell::preferences::shortcut_tests::actual_shortcut_presets_apply_reopen_cancel_and_failure_channels_preserve_valid_settings"])
        .env(CHILD,root.path()).env("MOTION_UI_LAYOUT",root.path().join("settings.json"))
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
                .matches("preferences failed operation=import_shortcuts")
                .count(),
            1,
            "{channel}"
        );
        assert_eq!(
            channel
                .matches("preferences failed operation=export_shortcuts")
                .count(),
            1,
            "{channel}"
        );
        assert!(channel.contains("invalid shortcut preset") && channel.contains("phase=publish"));
    }
    assert_eq!(
        log.matches("preferences start operation=import_shortcuts")
            .count(),
        3
    );
    assert_eq!(
        log.matches("preferences start operation=export_shortcuts")
            .count(),
        2
    );
    assert!(
        !stderr.contains("preferences start")
            && !stdout.contains("preferences failed")
            && !stdout.contains("preferences start")
    );
}
