use super::*;
use std::collections::BTreeSet;
#[test]
fn default_binding_reference_matches_live_resolver() {
    let keys = [
        Key::B,Key::C,Key::E,Key::H,Key::L,Key::Q,Key::V,Key::W,
        Key::Num1,Key::Num2,Key::Num3,Key::Num4,Key::Num5,Key::Num6,Key::Num7,Key::Num8,Key::Num9,
        Key::F1,Key::F2,Key::F5,Key::F6,Key::F7,Key::F8,Key::F10,Key::F11,Key::F12,
        Key::A,
        Key::F,
        Key::G,
        Key::T,
        Key::Num0,
        Key::N,
        Key::O,
        Key::I,
        Key::S,
        Key::Z,
        Key::Y,
        Key::D,
        Key::K,
        Key::P,
        Key::R,
        Key::U,
        Key::J,
        Key::F3,
        Key::F9,
        Key::M,
        Key::X,
        Key::Equal,
        Key::Minus,
        Key::Semicolon,
        Key::Backquote,
        Key::PageUp,
        Key::PageDown,
        Key::Escape,
        Key::Enter,
        Key::Tab,
        Key::Space,
        Key::Delete,
        Key::Backspace,
        Key::Left,
        Key::Right,
        Key::Up,
        Key::Down,
        Key::Home,
        Key::End,
        Key::Comma,
        Key::Period,
        Key::BracketLeft,
        Key::BracketRight,
        Key::F4,
        Key::Shift,
        Key::Control,
        Key::Alt,
    ];
    // A new input key must join this enumeration before its bindings can go undocumented.
    let declared: BTreeSet<_> = include_str!("../../ui/src/input.rs")
        .split("pub enum Key {")
        .nth(1)
        .unwrap()
        .split('}')
        .next()
        .unwrap()
        .lines()
        .map(|s| s.trim().trim_end_matches(','))
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let enumerated: BTreeSet<_> = keys.iter().map(|k| format!("{k:?}")).collect();
    assert_eq!(declared, enumerated);
    let mut text=String::from("# 默认键位解析目录\n\n此表由真实 `shortcuts::resolve` 生成；文本、IME、菜单、手势和面板路由还会限制可达性。完整行为见 [快捷键与作用域](KEYBOARD-SHORTCUTS.md)。\n\n| Scope | Key | Control | Shift | Alt | Command | OS repeat |\n| --- | --- | --- | --- | --- | --- | --- |\n");
    for scope in [Scope::Application, Scope::Animation, Scope::Nodes, Scope::Viewer] {
        for key in keys {
            for control in [false, true] {
                for shift in [false, true] {
                    for alt in [false, true] {
                        let modifiers = Modifiers {
                            control,
                            shift,
                            alt,
                        };
                        if let Some(command) = resolve(scope, key, modifiers, false) {
                            text.push_str(&format!("| {scope:?} | {key:?} | {control} | {shift} | {alt} | {command:?} | {} |\n",resolve(scope,key,modifiers,true).is_some()));
                        }
                    }
                }
            }
        }
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../DEFAULT-KEY-BINDINGS.md");
    if std::env::var_os("MOTION_UPDATE_SHORTCUT_DOCS").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        text,
        "default binding reference drift; regenerate using CONTRIBUTING.md"
    );
    let panel_doc = include_str!("../../PANELS.md");
    let expected = format!(
        "{} 个可编辑源图示例",
        motion_graph::builtin_examples().len()
    );
    assert!(panel_doc.contains(&expected));
    assert!(include_str!("../../README.md").contains("`desktop-layout-v2.json`"));
}
