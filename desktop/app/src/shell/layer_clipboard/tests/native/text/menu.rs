//! Real Edit menu clicks retain the draft while the popup owns pointer input.
use super::*;
use crate::shell::layer_clipboard::tests::text_menu as menu;
use std::path::Path;

pub(super) fn run(
    events: &mut EventLoop<()>,
    shell: &mut Shell,
    size: [f32; 2],
    scale: f32,
    name: &str,
    root: &Path,
) {
    shell.panels.focused = Some(Panel::Timeline);
    shell.panels.select_only(Some(7));
    let before = shell.engine.state().unwrap();
    assert_quiet(shell, size, 0, enter());
    assert!(menu::open(shell).is_empty());
    assert_eq!(shell.panels.menu, Some(1));
    capture(
        events,
        shell,
        size,
        scale,
        0,
        root.join(format!("{name}-rename-text-menu.png")),
    );
    assert!(menu::choose(shell, 3).is_empty());
    assert_eq!(
        &*shell.text_clipboard.read().unwrap().unwrap(),
        "跨窗文本 Text"
    );
    assert!(menu::open(shell).is_empty());
    capture(
        events,
        shell,
        size,
        scale,
        0,
        root.join(format!("{name}-cut-text-menu.png")),
    );
    assert!(menu::choose(shell, 0).is_empty());
    shell.text_clipboard.store("菜单粘贴 Text").unwrap();
    assert!(menu::open(shell).is_empty());
    assert!(menu::choose(shell, 4).is_empty());
    assert!(menu::open(shell).is_empty());
    capture(
        events,
        shell,
        size,
        scale,
        0,
        root.join(format!("{name}-paste-text-menu.png")),
    );
    assert_quiet(
        shell,
        size,
        0,
        Input {
            keys: vec![(Key::Escape, Default::default())],
            ..Default::default()
        },
    );
    assert!(shell.panels.layer_actions.text_in(Some(Panel::Timeline)));
    let current = shell.engine.state().unwrap();
    assert_eq!(current["project"], before["project"]);
    assert_eq!(current["revision"], before["revision"]);
    let (_, actions) = draw(shell, size, 0, enter());
    assert!(
        matches!(actions.as_slice(), [Action::Edit(motion_core::Command::InComposition { command, .. })]
        if matches!(command.as_ref(), motion_core::Command::Rename { object: 7, name } if name == "菜单粘贴 Text")),
        "{actions:?}"
    );
    shell
        .action(actions.into_iter().next().unwrap(), None)
        .unwrap();
    let (_, reply) = shell.editing.take().unwrap();
    shell.state = native_test_support::receive(events, &reply, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    shell.engine.save().unwrap();
    assert_eq!(
        serde_json::to_value(
            motion_core::storage::load(Path::new(shell.state["root"].as_str().unwrap())).unwrap()
        )
        .unwrap(),
        shell.state["project"]
    );
    shell.state = shell.engine.history(0).unwrap();
    assert_eq!(shell.state["project"], before["project"]);
}
