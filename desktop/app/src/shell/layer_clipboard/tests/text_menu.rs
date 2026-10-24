use super::*;
use motion_ui::{dock::Panel, theme::metrics, Input, Key, MouseButton};

fn draw(shell: &mut Shell, input: Input) -> Vec<Action> {
    shell.input = input;
    shell.draw_commands([1280., 800.]).1
}
fn key(shell: &mut Shell, key: Key) -> Vec<Action> {
    draw(
        shell,
        Input {
            keys: vec![(key, Default::default())],
            ..Default::default()
        },
    )
}
fn click(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        pressed: Some((MouseButton::Left, point)),
        ..Default::default()
    }
}
fn edit_x(shell: &mut Shell) -> f32 {
    8. + shell
        .atlas
        .measure(shell.panels.locale.t(crate::i18n::Text::File))
        + 22.
}
pub(super) fn open(shell: &mut Shell) -> Vec<Action> {
    let x = edit_x(shell) + 8.;
    draw(
        shell,
        click([x, metrics::TITLE_BAR + metrics::MENU_BAR * 0.5]),
    )
}
pub(super) fn choose(shell: &mut Shell, index: usize) -> Vec<Action> {
    let x = edit_x(shell) + 12.;
    draw(
        shell,
        click([
            x,
            metrics::TITLE_BAR + metrics::MENU_BAR + 4. + index as f32 * 32. + 15.,
        ]),
    )
}

#[test]
fn text_menu_clipboard_rename_retains_the_draft_and_commits_one_saved_undoable_edit() {
    for locale in [Locale::En, Locale::Zh] {
        let directory = tempfile::tempdir().unwrap();
        project(directory.path(), false);
        let mut shell = shell(directory.path(), locale);
        shell.panels.focused = Some(Panel::Timeline);
        let before = shell.state["project"].clone();
        let revision = shell.state["revision"].clone();
        assert!(key(&mut shell, Key::Enter).is_empty());
        assert!(open(&mut shell).is_empty());
        assert_eq!(shell.panels.menu, Some(1));
        assert!(shell.panels.layer_actions.text_in(Some(Panel::Timeline)));
        assert!(choose(&mut shell, 3).is_empty());
        assert_eq!(&*shell.text_clipboard.read().unwrap().unwrap(), "Original");
        assert!(open(&mut shell).is_empty());
        assert!(choose(&mut shell, 0).is_empty());
        shell.text_clipboard.store("菜单粘贴 Text").unwrap();
        assert!(open(&mut shell).is_empty());
        assert!(choose(&mut shell, 4).is_empty());
        assert!(open(&mut shell).is_empty());
        assert!(choose(&mut shell, 0).is_empty());
        assert!(open(&mut shell).is_empty());
        assert!(choose(&mut shell, 1).is_empty());
        assert!(open(&mut shell).is_empty());
        assert!(key(&mut shell, Key::Escape).is_empty());
        assert!(shell.panels.layer_actions.text_in(Some(Panel::Timeline)));
        assert_eq!(shell.engine.state().unwrap()["revision"], revision);
        assert_eq!(shell.engine.state().unwrap()["project"], before);
        let mut actions = key(&mut shell, Key::Enter);
        assert!(
            matches!(actions.as_slice(), [Action::Edit(motion_core::Command::InComposition { command, .. })]
            if matches!(command.as_ref(), motion_core::Command::Rename {object:7,name} if name == "菜单粘贴 Text")),
            "{actions:?}"
        );
        shell.action(actions.pop().unwrap(), None).unwrap();
        let (_, reply) = shell.editing.take().unwrap();
        shell.state = reply
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap();
        shell.engine.save().unwrap();
        assert_eq!(
            serde_json::to_value(motion_core::storage::load(directory.path()).unwrap()).unwrap(),
            shell.state["project"]
        );
        assert_eq!(shell.engine.history(0).unwrap()["project"], before);
    }
}

#[test]
fn text_menu_clipboard_search_and_stale_context_do_not_fall_back_to_document_actions() {
    let directory = tempfile::tempdir().unwrap();
    project(directory.path(), false);
    let mut shell = shell(directory.path(), Locale::Zh);
    shell.panels.focused = Some(Panel::Timeline);
    shell.panels.timeline.timeline_search = true;
    shell.panels.timeline.timeline_query = "Original".into();
    shell
        .panels
        .timeline
        .timeline_search_editor
        .begin("Original");
    let before = shell.engine.state().unwrap()["project"].clone();
    assert!(open(&mut shell).is_empty());
    assert!(choose(&mut shell, 3).is_empty());
    assert_eq!(shell.panels.timeline.timeline_query, "");
    assert!(open(&mut shell).is_empty());
    assert!(choose(&mut shell, 0).is_empty());
    assert_eq!(shell.panels.timeline.timeline_query, "Original");
    assert!(open(&mut shell).is_empty());
    shell
        .panels
        .timeline
        .timeline_search_editor
        .begin("new draft");
    shell.panels.timeline.timeline_query = "new draft".into();
    shell
        .text_clipboard
        .store("must not replace new draft")
        .unwrap();
    assert!(choose(&mut shell, 4).is_empty());
    assert_eq!(shell.panels.timeline.timeline_query, "new draft");
    assert!(key(&mut shell, Key::Escape).is_empty());
    assert!(shell.panels.timeline.timeline_search);
    assert!(open(&mut shell).is_empty());
    let revision = shell.state["revision"].as_u64().unwrap();
    shell.state["revision"] = (revision + 1).into();
    assert!(choose(&mut shell, 4).is_empty());
    assert_eq!(shell.panels.timeline.timeline_query, "new draft");
    assert_eq!(shell.engine.state().unwrap()["project"], before);
}

#[test]
fn text_menu_clipboard_file_switch_keeps_outside_commit_and_focus_loss_cancels() {
    let directory = tempfile::tempdir().unwrap();
    project(directory.path(), false);
    let mut shell = shell(directory.path(), Locale::En);
    shell.panels.focused = Some(Panel::Timeline);
    let before = shell.engine.state().unwrap()["project"].clone();
    assert!(key(&mut shell, Key::Enter).is_empty());
    assert!(draw(
        &mut shell,
        Input {
            text: "File switch".into(),
            ..Default::default()
        }
    )
    .is_empty());
    assert!(open(&mut shell).is_empty());
    let actions = draw(
        &mut shell,
        click([12., metrics::TITLE_BAR + metrics::MENU_BAR * 0.5]),
    );
    assert!(
        matches!(actions.as_slice(), [Action::Edit(motion_core::Command::InComposition { command, .. })]
        if matches!(command.as_ref(), motion_core::Command::Rename { object: 7, name } if name == "File switch")),
        "{actions:?}"
    );
    assert_eq!(shell.panels.menu, Some(0));
    assert!(!shell.panels.layer_actions.text_in(Some(Panel::Timeline)));
    // Drop the returned edit deliberately: this half verifies routing, not commit.
    assert_eq!(shell.engine.state().unwrap()["project"], before);
    assert!(key(&mut shell, Key::Escape).is_empty());
    assert!(key(&mut shell, Key::Enter).is_empty());
    assert!(open(&mut shell).is_empty());
    assert!(draw(
        &mut shell,
        Input {
            focus_lost: true,
            ..Default::default()
        }
    )
    .is_empty());
    assert!(shell.panels.menu.is_none());
    assert!(!shell.panels.layer_actions.text_in(Some(Panel::Timeline)));
    assert_eq!(shell.engine.state().unwrap()["project"], before);
}
