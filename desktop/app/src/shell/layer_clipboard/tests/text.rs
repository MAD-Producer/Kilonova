use super::*;
use motion_ui::{dock::Panel, widgets::TextEditor, Input, Key};
use std::sync::Arc;

fn chord(key: Key) -> Input {
    Input {
        keys: vec![(
            key,
            motion_ui::input::Modifiers {
                control: true,
                ..Default::default()
            },
        )],
        ..Default::default()
    }
}
fn draw(shell: &mut Shell, input: Input) -> Vec<Action> {
    shell.input = input;
    shell.draw_commands([1280., 800.]).1
}
fn enter(shell: &mut Shell) -> Vec<Action> {
    draw(
        shell,
        Input {
            keys: vec![(Key::Enter, Default::default())],
            ..Default::default()
        },
    )
}

#[test]
fn text_clipboard_draft_cut_undo_and_layer_cache_remain_independent_in_actual_chrome() {
    let root = tempfile::tempdir().unwrap();
    project(root.path(), false);
    let mut shell = shell(root.path(), Locale::Zh);
    shell.panels.focused = Some(Panel::Timeline);
    run(&mut shell, Intent::Copy);
    let before = shell.state.clone();
    assert!(enter(&mut shell).is_empty());
    assert!(shell.panels.layer_actions.text_in(Some(Panel::Timeline)));
    assert!(draw(&mut shell, chord(Key::X)).is_empty());
    assert_eq!(&*shell.text_clipboard.read().unwrap().unwrap(), "Original");
    assert!(!shell.layer_clipboard.busy());
    assert!(shell.layer_clipboard.available());
    assert!(draw(&mut shell, chord(Key::Z)).is_empty());
    assert!(
        enter(&mut shell).is_empty(),
        "undo must restore the original name before commit"
    );
    assert!(!shell.panels.layer_actions.text_in(Some(Panel::Timeline)));
    assert_eq!(shell.engine.state().unwrap()["project"], before["project"]);
    assert_eq!(
        shell.engine.state().unwrap()["revision"],
        before["revision"]
    );
    shell.text_clipboard.store("新名称 👩🏽‍🚀").unwrap();
    assert!(enter(&mut shell).is_empty());
    assert!(draw(&mut shell, chord(Key::V)).is_empty());
    let mut actions = enter(&mut shell);
    assert!(
        matches!(actions.as_slice(), [Action::Edit(motion_core::Command::InComposition { composition, command })]
            if composition == shell.state["project"]["composition_id"].as_str().unwrap()
            && matches!(command.as_ref(), motion_core::Command::Rename { object: 7, name } if name == "新名称 👩🏽‍🚀")),
        "{actions:?}"
    );
    shell.action(actions.pop().unwrap(), None).unwrap();
    let (_, reply) = shell.editing.take().unwrap();
    shell.state = reply
        .recv_timeout(Duration::from_secs(30))
        .unwrap()
        .unwrap();
    assert_eq!(shell.state["project"]["layers"][0]["name"], "新名称 👩🏽‍🚀");
    assert_eq!(
        shell.engine.history(0).unwrap()["project"],
        before["project"]
    );
    shell.engine.save().unwrap();
    assert_eq!(
        serde_json::to_value(motion_core::storage::load(root.path()).unwrap()).unwrap(),
        before["project"]
    );
    // The frozen layer cache predates the text rename and still owns its data.
    shell.state = shell.engine.seek(40.).unwrap();
    run(&mut shell, Intent::Paste { in_place: false });
    assert_eq!(shell.state["project"]["layers"][1]["name"], "Original");
    assert_eq!(&*shell.text_clipboard.read().unwrap().unwrap(), "新名称 👩🏽‍🚀");
}

#[test]
fn text_clipboard_survives_a_real_project_replacement_without_document_aliases() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    project(source.path(), false);
    project(target.path(), false);
    let mut shell = shell(source.path(), Locale::En);
    shell.panels.focused = Some(Panel::Timeline);
    assert!(enter(&mut shell).is_empty());
    assert!(draw(&mut shell, chord(Key::C)).is_empty());
    assert_eq!(&*shell.text_clipboard.read().unwrap().unwrap(), "Original");
    let mut escape = Input::default();
    escape.keys.push((Key::Escape, Default::default()));
    assert!(draw(&mut shell, escape).is_empty());
    let cache = shell.text_clipboard.clone();
    shell
        .request_project_change(crate::project_lifecycle::Target::Open(target.path().into()))
        .unwrap();
    let began = Instant::now();
    while shell.project_pending.is_some() {
        assert!(began.elapsed() < Duration::from_secs(20));
        shell.poll_project_change();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(shell.project_prompt.is_none());
    assert!(Arc::ptr_eq(&cache, &shell.text_clipboard));
    assert_eq!(&*shell.text_clipboard.read().unwrap().unwrap(), "Original");
    assert_eq!(
        std::path::Path::new(shell.state["root"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        target.path().canonicalize().unwrap()
    );
    assert!(!shell.layer_clipboard.available() && !shell.layer_clipboard.keyframes_available());
    assert!(!shell.panels.layer_actions.text_in(Some(Panel::Timeline)));
    assert_eq!(shell.state["revision"], 0);
}

#[test]
fn text_clipboard_failures_use_default_file_stderr_without_leaking_user_text() {
    const NAME: &str = "shell::layer_clipboard::tests::text::text_clipboard_failures_use_default_file_stderr_without_leaking_user_text";
    const CHILD: &str = "MOTION_TEXT_CLIPBOARD_CHANNEL_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let cache = Arc::new(motion_ui::text_clipboard::TextClipboard::default());
        assert!(cache.read().unwrap().is_none());
        cache.store("private-user-text-sentinel").unwrap();
        let mut editor = TextEditor::default();
        editor.begin("private-user-text-sentinel");
        editor.input(&chord(Key::X));
        assert_eq!(editor.text, "private-user-text-sentinel");
        assert!(editor.error.as_deref().unwrap().contains("unavailable"));
        assert!(cache
            .store(&"q".repeat(motion_ui::text_clipboard::MAX_BYTES + 1))
            .is_err());
        assert_eq!(
            &*cache.read().unwrap().unwrap(),
            "private-user-text-sentinel"
        );
        let mut empty = TextEditor::default();
        empty.begin("untouched");
        let mut paste = chord(Key::V);
        paste.text_clipboard = Some(Arc::new(Default::default()));
        empty.input(&paste);
        assert_eq!(empty.text, "untouched");
        assert!(empty.error.is_none());
        crate::logging::flush();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_TEXT_CLIPBOARD_CHANNEL_ARTIFACTS")
        .map(std::path::PathBuf::from)
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
    for cause in ["text clipboard is unavailable", "exceeds the 16 MiB limit"] {
        assert!(file.contains(cause) && stderr.contains(cause), "{cause}");
        assert!(!stdout.contains(cause));
    }
    assert!(file.contains("logging initialized level=INFO"));
    assert!(
        file.contains("text clipboard start operation=copy")
            && !stderr.contains("text clipboard start")
    );
    assert!(
        !file.contains("private-user-text-sentinel")
            && !stderr.contains("private-user-text-sentinel")
    );
}
