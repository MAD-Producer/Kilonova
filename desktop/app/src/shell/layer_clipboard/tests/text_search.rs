//! Search drafts use production chrome and never enqueue document clipboard edits.
use super::*;
use crate::panels;
use motion_ui::{dock::Panel, Input, Key};

fn key(key: Key, control: bool) -> Input {
    Input {
        keys: vec![(
            key,
            motion_ui::input::Modifiers {
                control,
                ..Default::default()
            },
        )],
        ..Default::default()
    }
}
fn draw(shell: &mut Shell, input: Input) {
    shell.input = input;
    let (_, actions) = shell.draw_commands([1280., 800.]);
    assert!(
        actions.is_empty(),
        "search escaped into document actions: {actions:?}"
    );
    assert!(!shell.layer_clipboard.busy());
}
fn query(shell: &Shell, effects: bool) -> &str {
    if effects {
        &shell.panels.effect_search.as_ref().unwrap().query
    } else {
        &shell.panels.timeline.timeline_query
    }
}

#[test]
fn search_clipboard_cut_paste_and_history_leave_document_and_layer_cache_untouched() {
    for locale in [Locale::En, Locale::Zh] {
        for effects in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            project(directory.path(), false);
            let mut shell = shell(directory.path(), locale);
            shell.panels.focused = Some(Panel::Timeline);
            run(&mut shell, Intent::Copy);
            let before = shell.engine.state().unwrap();
            let prefix = "搜索 👩🏽‍🚀";
            if effects {
                shell.panels.effect_search = Some(panels::EffectSearch {
                    query: prefix.into(),
                    position: [600., 300.],
                    ..Default::default()
                });
            } else {
                shell.panels.timeline.timeline_query = prefix.into();
                shell.panels.timeline.timeline_search = true;
            }
            shell.text_clipboard.store("追加").unwrap();
            draw(&mut shell, key(Key::V, true));
            let pasted = format!("{prefix}追加");
            assert_eq!(query(&shell, effects), pasted);
            draw(&mut shell, key(Key::A, true));
            let mut repeated_cut = key(Key::X, true);
            repeated_cut.key_repeats.push(true);
            draw(&mut shell, repeated_cut);
            assert_eq!(query(&shell, effects), pasted);
            draw(&mut shell, key(Key::X, true));
            assert_eq!(query(&shell, effects), "");
            assert_eq!(&*shell.text_clipboard.read().unwrap().unwrap(), pasted);
            draw(&mut shell, key(Key::Z, true));
            assert_eq!(query(&shell, effects), pasted);
            draw(&mut shell, key(Key::Y, true));
            assert_eq!(query(&shell, effects), "");
            draw(&mut shell, key(Key::Z, true));
            // One backspace removes the whole extended grapheme, not a code point.
            draw(&mut shell, key(Key::End, false));
            draw(&mut shell, key(Key::Backspace, false));
            draw(&mut shell, key(Key::Backspace, false));
            draw(&mut shell, key(Key::Backspace, false));
            assert_eq!(query(&shell, effects), "搜索 ");
            draw(&mut shell, key(Key::Z, true));
            assert_eq!(query(&shell, effects), prefix);
            let mut ime_cut = key(Key::X, true);
            ime_cut.ime_consumed = true;
            draw(&mut shell, ime_cut);
            assert_eq!(query(&shell, effects), prefix);
            draw(&mut shell, key(Key::Escape, false));
            assert!(shell.panels.effect_search.is_none());
            assert!(!shell.panels.timeline.timeline_search);
            assert_eq!(shell.engine.state().unwrap()["project"], before["project"]);
            assert_eq!(
                shell.engine.state().unwrap()["revision"],
                before["revision"]
            );
            assert!(shell.layer_clipboard.available());
            shell.state = shell.engine.seek(40.).unwrap();
            run(&mut shell, Intent::Paste { in_place: false });
            assert_eq!(shell.state["project"]["layers"][1]["name"], "Original");
        }
    }
}

#[test]
fn search_clipboard_ime_commit_and_focus_loss_preserve_live_query_without_host_edits() {
    let directory = tempfile::tempdir().unwrap();
    project(directory.path(), false);
    let mut shell = shell(directory.path(), Locale::Zh);
    shell.panels.focused = Some(Panel::Timeline);
    shell.panels.timeline.timeline_query = "前缀".into();
    shell.panels.timeline.timeline_search = true;
    let mut ime = key(Key::Enter, false);
    ime.ime_consumed = true;
    ime.text = "中文".into();
    draw(&mut shell, ime);
    assert!(shell.panels.timeline.timeline_search);
    assert_eq!(shell.panels.timeline.timeline_query, "前缀中文");
    draw(&mut shell, key(Key::Enter, false));
    assert!(!shell.panels.timeline.timeline_search);
    // Re-entering a search retains its live query and starts at the end.
    shell.panels.timeline.timeline_search = true;
    draw(
        &mut shell,
        Input {
            text: "后缀".into(),
            ..Default::default()
        },
    );
    assert_eq!(shell.panels.timeline.timeline_query, "前缀中文后缀");
    draw(
        &mut shell,
        Input {
            focus_lost: true,
            ..Default::default()
        },
    );
    assert!(!shell.panels.timeline.timeline_search);
    assert_eq!(shell.panels.timeline.timeline_query, "前缀中文后缀");
    shell.panels.effect_search = Some(panels::EffectSearch {
        query: "Blur".into(),
        position: [600., 300.],
        ..Default::default()
    });
    draw(&mut shell, Input::default());
    draw(
        &mut shell,
        Input {
            focus_lost: true,
            ..Default::default()
        },
    );
    assert!(shell.panels.effect_search.is_none());
}
