use super::*;
use motion_ui::dock::Panel;

fn press(mouse: [f32; 2]) -> Input {
    Input {
        mouse,
        keys: vec![(Key::Backquote, Modifiers::default())],
        text: "`".into(),
        ..Default::default()
    }
}

#[test]
fn idle_panels_do_not_block_layout_shortcuts_just_because_they_have_search_fields() {
    let mut panels = panels::Panels::default();
    for panel in Panel::available() {
        panels.focused = Some(panel);
        assert!(!panels::layout_shortcuts_blocked(&panels), "{panel:?}");
    }
}

#[test]
fn backquote_remains_text_in_an_active_search_field() {
    let folder = tempfile::tempdir().unwrap();
    let mut shell = Shell::new(folder.path().join("project"), None, Some(Locale::En)).unwrap();
    // Shell::new loads user preferences; this input fixture needs a visible Project tab.
    shell.workspace = workspace::Workspace::default();
    assert!(
        panels::frames(&shell.workspace.root, [1600., 1000.])
            .iter()
            .any(|frame| frame.panel == Panel::Project),
        "search fixture must expose the Project panel"
    );
    shell.panels.focused = Some(Panel::Project);
    shell.panels.project.search = true;
    shell.panels.project.search_field.editor.begin("");
    shell.input = press([f32::NAN, f32::NAN]);
    let (_, actions) = shell.draw_commands([1600., 1000.]);
    assert!(actions.is_empty());
    assert_eq!(shell.workspace.maximized_panel(), None);
    assert_eq!(shell.panels.project.query, "`");
    shell.panels.project.search_field.editor.cancel();
    shell.panels.project.search = false;
    shell.input = press([f32::NAN, f32::NAN]);
    assert!(shell.panel_maximize_shortcut([1600., 1000.]));
    assert_eq!(
        shell.workspace.maximized_panel(),
        Some(Panel::Project.dock())
    );
}

#[test]
fn pointer_then_visible_focus_select_target_and_restore_without_project_edits() {
    for locale in [Locale::En, Locale::Zh] {
        let folder = tempfile::tempdir().unwrap();
        let mut shell = Shell::new(folder.path().join("project"), None, Some(locale)).unwrap();
        let size = [1600., 1000.];
        let before = shell.engine.state().unwrap();
        let layout = serde_json::to_value(&shell.workspace).unwrap();
        let area = panels::frames(&shell.workspace.root, size)
            .into_iter()
            .find(|f| f.panel == Panel::Composition)
            .unwrap()
            .content;
        shell.panels.focused = Some(Panel::Timeline);
        shell.input = press([area.min[0] + 20., area.min[1] + 20.]);
        let (paint, actions) = shell.draw_commands(size);
        assert!(actions.is_empty());
        let ui = shell.ui_frame(paint, size, false);
        assert_eq!(
            shell.workspace.maximized_panel(),
            Some(Panel::Composition.dock())
        );
        assert_eq!(shell.panels.focused, Some(Panel::Composition));
        assert!(ui.composition.unwrap()[2] > area.width());
        assert!(shell.input.text.is_empty());
        // Restore works even with the pointer outside every panel.
        shell.input = press([f32::NAN, f32::NAN]);
        assert!(shell.panel_maximize_shortcut(size));
        assert_eq!(shell.workspace.maximized_panel(), None);
        assert_eq!(serde_json::to_value(&shell.workspace).unwrap(), layout);
        shell.panels.focused = Some(Panel::Timeline);
        shell.input = press([f32::NAN, f32::NAN]);
        assert!(shell.panel_maximize_shortcut(size));
        assert_eq!(
            shell.workspace.maximized_panel(),
            Some(Panel::Timeline.dock())
        );
        shell.workspace.restore_panel();
        shell.panels.focused = Some(Panel::Audio); // Not visible in the main window.
        shell.input = press([f32::NAN, f32::NAN]);
        assert!(!shell.panel_maximize_shortcut(size));
        let after = shell.engine.state().unwrap();
        for key in ["project", "revision", "history"] {
            assert_eq!(after[key], before[key], "{key}");
        }
        assert!(shell.actions.is_empty());
    }
}

#[test]
fn editors_popups_ime_repeat_modifiers_and_gestures_keep_keyboard_priority() {
    let folder = tempfile::tempdir().unwrap();
    let mut shell = Shell::new(folder.path().join("project"), None, Some(Locale::En)).unwrap();
    let size = [1600., 1000.];
    shell.panels.focused = Some(Panel::Timeline);
    for case in 0..19 {
        shell.input = press([f32::NAN, f32::NAN]);
        match case {
            0 => shell.input.composing = true,
            1 => shell.input.ime_consumed = true,
            2 => shell.input.key_repeats = vec![true],
            3 => shell.input.keys[0].1.control = true,
            4 => shell.input.keys[0].1.shift = true,
            5 => shell.input.keys[0].1.alt = true,
            6 => shell.input.held = Some(MouseButton::Middle),
            7 => shell.input.focus_lost = true,
            8 => shell.panels.project.search = true,
            9 => shell.panels.timeline.timeline_search = true,
            10 => shell.panels.project.context_menu = Some([20., 20.]),
            11 => shell.panels.timeline.timeline_context_menu = Some([20., 20.]),
            12 => shell.panels.timeline.scrubbing = true,
            13 => shell.workspace_manager = Some(workspace::Manager::new(&shell.workspace)),
            14 => shell.panels.menu = Some(0),
            15 => shell.panels.panel_menu = Some((Panel::Timeline.dock(), [20., 20.])),
            16 => shell.panels.parent_controls.open = true,
            17 => shell.panels.timeline.reorder = Some((1, [20., 20.], true)),
            18 => {
                shell.panels.effect_search = Some(panels::EffectSearch {
                    position: [20., 20.],
                    query: String::new(),
                    selected_index: 0,
                    owner: None,
                    continuous: false,
                    ..Default::default()
                })
            }
            _ => unreachable!(),
        }
        assert!(!shell.panel_maximize_shortcut(size), "case {case}");
        assert_eq!(shell.workspace.maximized_panel(), None);
        assert_eq!(shell.input.text, "`");
        shell.panels.project.search = false;
        shell.panels.timeline.timeline_search = false;
        shell.panels.project.context_menu = None;
        shell.panels.timeline.timeline_context_menu = None;
        shell.panels.timeline.scrubbing = false;
        shell.workspace_manager = None;
        shell.panels.menu = None;
        shell.panels.panel_menu = None;
        shell.panels.parent_controls.open = false;
        shell.panels.timeline.reorder = None;
        shell.panels.effect_search = None;
    }
}

#[test]
fn focus_loss_keeps_projection_and_panel_visibility_actions_restore_it() {
    let folder = tempfile::tempdir().unwrap();
    let mut shell = Shell::new(folder.path().join("project"), None, Some(Locale::En)).unwrap();
    let size = [900., 600.];
    shell.panels.focused = Some(Panel::Timeline);
    shell.input = press([f32::NAN, f32::NAN]);
    assert!(shell.panel_maximize_shortcut(size));
    shell.lose_focus();
    assert_eq!(
        shell.workspace.maximized_panel(),
        Some(Panel::Timeline.dock())
    );
    shell
        .action(panels::Action::ShowPanel(Panel::Composition.dock()), None)
        .unwrap();
    assert_eq!(shell.workspace.maximized_panel(), None);
    shell.input = press([f32::NAN, f32::NAN]);
    assert!(shell.panel_maximize_shortcut(size));
    shell
        .action(
            panels::Action::ClosePanel(Panel::EffectControls.dock()),
            None,
        )
        .unwrap();
    assert_eq!(shell.workspace.maximized_panel(), None);
}
