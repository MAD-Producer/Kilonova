use super::*;
use motion_ui::input::MouseButton;

fn library_frame(shell: &mut Shell, floating: bool, input: &Input) -> Vec<panels::Action> {
    let mut paint = PaintList::default();
    if floating {
        panels::floating_chrome(
            &mut paint,
            &mut shell.atlas,
            Panel::Effects,
            [400., 400.],
            &mut shell.panels,
            &shell.state,
            input,
        )
    } else {
        panels::chrome(
            &mut paint,
            &mut shell.atlas,
            [900., 600.],
            &dock::Node::dock(Panel::Effects.dock()),
            &mut shell.panels,
            &shell.state,
            input,
        )
    }
}

fn main_menu_frame(shell: &mut Shell, input: &Input) -> Vec<panels::Action> {
    panels::chrome(
        &mut PaintList::default(),
        &mut shell.atlas,
        [900., 600.],
        &dock::Node::dock(Panel::Info.dock()),
        &mut shell.panels,
        &shell.state,
        input,
    )
}

fn recent_action(shell: &mut Shell) -> panels::Action {
    shell.panels.menu = Some(4);
    assert!(main_menu_frame(shell, &Input::default()).is_empty());
    let x = 8.
        + [T::File, T::Edit, T::Composition, T::Layer]
            .iter()
            .map(|key| shell.atlas.measure(shell.workspace.locale.t(*key)) + 22.)
            .sum::<f32>();
    let point = [
        x + 50.,
        motion_ui::theme::metrics::TITLE_BAR + motion_ui::theme::metrics::MENU_BAR + 4. + 64. + 15.,
    ];
    let actions = main_menu_frame(
        shell,
        &Input {
            mouse: point,
            pressed: Some((MouseButton::Left, point)),
            ..Default::default()
        },
    );
    assert_eq!(actions.len(), 1, "{actions:?}");
    actions.into_iter().next().unwrap()
}

#[test]
fn actual_library_inputs_in_both_windows_feed_the_recent_menu_and_atomic_save_undo() {
    for locale in [Locale::En, Locale::Zh] {
        for floating in [false, true] {
            let (_folder, mut shell, _) = fixture(locale);
            shell.panels.select_only(Some(1));
            shell.panels.add_selection(3);
            shell.panels.selected = Some(1);
            shell.panels.browser.catalogue = shell.panels.effect_controls.catalogue.clone();
            shell.panels.browser.preview_categories(locale);
            let entry =
                panels::effect_controls::entries(&shell.panels.effect_controls.catalogue, locale)
                    .into_iter()
                    .find(|entry| entry.id == "glow")
                    .unwrap();
            assert!(library_frame(&mut shell, floating, &Input::default()).is_empty());
            let row = shell
                .panels
                .browser
                .library_row(&entry.key)
                .expect("visible library row");
            let point = [
                (row.min[0] + row.max[0]) * 0.5,
                (row.min[1] + row.max[1]) * 0.5,
            ];
            assert!(library_frame(
                &mut shell,
                floating,
                &Input {
                    mouse: point,
                    pressed: Some((MouseButton::Left, point)),
                    ..Default::default()
                }
            )
            .is_empty());
            // Supply the existing bridge's release, without pointer movement,
            // before Enter. This is a click, never an active effect drag.
            shell
                .panels
                .effect_drag
                .pointer(floating.then_some(Panel::Effects), point, true, 3.);
            assert!(library_frame(
                &mut shell,
                floating,
                &Input {
                    mouse: point,
                    released: Some((MouseButton::Left, point)),
                    ..Default::default()
                }
            )
            .is_empty());
            let actions = library_frame(
                &mut shell,
                floating,
                &Input {
                    keys: vec![(Key::Enter, Default::default())],
                    ..Default::default()
                },
            );
            assert!(
                matches!(actions.as_slice(), [panels::Action::Plugin(_)]),
                "{actions:?}"
            );
            for action in actions {
                shell.action(action, None).unwrap();
            }
            assert_eq!(shell.panels.browser.recent_keys()[0], entry.key);
            let after_library = shell.state["project"].clone();
            assert_eq!(
                after_library["layers"][0]["effects"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
            assert_eq!(
                after_library["layers"][2]["effects"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
            let action = recent_action(&mut shell);
            assert!(
                matches!(&action,panels::Action::Plugin(request) if request["objects"]==json!([1,3]) && request["effect"]==entry.id)
            );
            shell.action(action, None).unwrap();
            let applied = shell.state["project"].clone();
            assert_eq!(applied["layers"][0]["effects"].as_array().unwrap().len(), 2);
            assert_eq!(applied["layers"][1]["effects"].as_array().unwrap().len(), 0);
            assert_eq!(applied["layers"][2]["effects"].as_array().unwrap().len(), 2);
            assert!(shell.panels.message.as_deref().unwrap().contains("One"));
            shell.engine.save().unwrap();
            let loaded = motion_core::storage::load(std::path::Path::new(
                shell.state["root"].as_str().unwrap(),
            ))
            .unwrap();
            assert_eq!(serde_json::to_value(loaded).unwrap(), applied);
            shell.state = shell.engine.history(0).unwrap();
            assert_eq!(
                shell.state["project"], after_library,
                "one undo removes the entire recent-menu batch"
            );
            shell.state = shell.engine.history(1).unwrap();
            assert_eq!(shell.state["project"], applied);
        }
    }
}

#[test]
fn an_actual_worker_rejects_stale_menu_targets_without_partial_additions_or_extra_history() {
    const CHILD: &str = "MOTION_RECENT_MENU_FAILURE_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let folder = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "shell::effect_application::tests::recent_menu::an_actual_worker_rejects_stale_menu_targets_without_partial_additions_or_extra_history", "--nocapture"])
            .env_remove("MOTION_LOG")
            .env(CHILD, folder.path())
            .output().unwrap();
        assert!(
            output.status.success(),
            "child test=recent_menu_stale_targets status={}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let cause = std::fs::read_to_string(folder.path().join("cause.txt")).unwrap();
        let file = std::fs::read_to_string(folder.path().join("logs/motion-studio.log")).unwrap();
        assert!(!cause.is_empty());
        assert!(file.contains("logging initialized") && file.contains(&cause));
        assert!(String::from_utf8_lossy(&output.stderr).contains("ERROR"));
        assert!(String::from_utf8_lossy(&output.stderr).contains(&cause));
        assert!(!String::from_utf8_lossy(&output.stdout).contains(&cause));
        let failure = file
            .lines()
            .find(|line| {
                line.contains("ERROR")
                    && line.contains("error #")
                    && line.contains(" plugin ")
                    && line.contains(&cause)
            })
            .unwrap();
        let request = failure
            .split("error #")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        assert!(
            file.find(&format!("start #{request} plugin")).unwrap() < file.find(failure).unwrap()
        );
        if let Some(destination) = std::env::var_os("MOTION_RECENT_MENU_EVIDENCE") {
            let destination = std::path::PathBuf::from(destination);
            std::fs::create_dir_all(&destination).unwrap();
            for (name, bytes) in [
                ("stdout.txt", output.stdout.as_slice()),
                ("stderr.txt", output.stderr.as_slice()),
                ("motion-studio.log", file.as_bytes()),
            ] {
                std::fs::write(destination.join(name), bytes).unwrap();
            }
        }
        return;
    };
    let root = std::path::PathBuf::from(root);
    let _logger = crate::logging::init(root.join("logs")).unwrap();
    let (_folder, mut shell, request) = fixture(Locale::En);
    shell.panels.select_only(Some(1));
    shell.panels.add_selection(3);
    shell.panels.selected = Some(1);
    shell.panels.browser.record_recent(format!(
        "{}/{}",
        request["plugin"].as_str().unwrap(),
        request["effect"].as_str().unwrap()
    ));
    let original = shell.state["project"].clone();
    let action = recent_action(&mut shell);
    shell.state = shell
        .engine
        .edit(serde_json::to_string(&Command::Delete { object: 3 }).unwrap())
        .unwrap();
    let after_delete = shell.state["project"].clone();
    let error = shell.action(action, None).unwrap_err();
    assert!(!error.is_empty());
    std::fs::write(root.join("cause.txt"), &error).unwrap();
    assert!(shell
        .panels
        .message
        .as_deref()
        .unwrap()
        .starts_with("Could not add effect"));
    assert_eq!(
        shell.engine.state().unwrap()["project"],
        after_delete,
        "the surviving target must not receive a partial effect"
    );
    shell.state = shell.engine.history(0).unwrap();
    assert_eq!(
        shell.state["project"], original,
        "failed add must not insert an undo item"
    );
    crate::logging::flush();
}
