use crate::*;

#[cfg(target_os = "windows")]
mod native;

#[test]
fn audio_preview_shortcut_routes_once_in_both_owners_and_languages_and_respects_ime() {
    for locale in [Locale::En, Locale::Zh] {
        for panel in [
            dock::Panel::Timeline,
            dock::Panel::Audio,
            dock::Panel::Curves,
            dock::Panel::Nodes,
        ] {
            for floating in [false, true] {
                for composing in [false, true] {
                    let mut panels = panels::Panels {
                        locale,
                        focused: Some(panel),
                        ..Default::default()
                    };
                    let input = Input {
                        keys: vec![(Key::NumpadDecimal, Default::default())],
                        composing,
                        ..Default::default()
                    };
                    let mut paint = PaintList::default();
                    let mut atlas = TextAtlas::from_system(12.).unwrap();
                    let actions = if floating {
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
                    };
                    if composing {
                        assert!(actions.is_empty(), "{actions:?}");
                    } else {
                        assert!(
                            matches!(actions.as_slice(), [panels::Action::ToggleAudioPreview]),
                            "{panel:?} floating={floating}: {actions:?}"
                        );
                    }
                }
            }
        }
    }
    let bindings = crate::shortcuts::bindings::Bindings::default();
    for (key, modifiers, repeat) in [
        (Key::Period, Default::default(), false),
        (Key::NumpadDecimal, Default::default(), true),
        (
            Key::NumpadDecimal,
            motion_ui::input::Modifiers {
                control: true,
                ..Default::default()
            },
            false,
        ),
    ] {
        assert_ne!(
            bindings.resolve(crate::shortcuts::Scope::Application, key, modifiers, repeat),
            Some(crate::shortcuts::Command::AudioPreview)
        );
    }
}

#[test]
fn empty_audio_preview_does_not_start_silent_playback_or_change_history() {
    for locale in [Locale::En, Locale::Zh] {
        let root = tempfile::tempdir().unwrap();
        let mut shell = Shell::new(root.path().join("project"), None, Some(locale)).unwrap();
        let before = shell.engine.state().unwrap();
        shell.frame = 23.;
        shell
            .action(panels::Action::ToggleAudioPreview, None)
            .unwrap();
        assert!(shell.playback.is_none() && shell.audio.is_none() && !shell.audio_only_preview);
        assert_eq!(shell.frame, 23.);
        assert!(shell.panels.message.is_some());
        let after = shell.engine.state().unwrap();
        for key in ["project", "revision", "canUndo", "canRedo", "frame"] {
            assert_eq!(before[key], after[key], "{key}");
        }
        // A plan failure is not the same as an empty composition.
        shell.state["audio_error"] = serde_json::json!("controlled invalid audio plan");
        assert_eq!(
            shell
                .action(panels::Action::ToggleAudioPreview, None)
                .unwrap_err(),
            "controlled invalid audio plan"
        );
        assert!(shell.playback.is_none());
    }
}
