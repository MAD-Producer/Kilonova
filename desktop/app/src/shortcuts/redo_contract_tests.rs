use super::*;
use crate::shortcuts::bindings::Bindings;
use motion_ui::input::KeyChord;

#[test]
fn redo_and_new_solid_have_distinct_default_chords_in_all_scopes() {
    let ctrl = Modifiers {
        control: true,
        ..Default::default()
    };
    for scope in [Scope::Application, Scope::Animation, Scope::Nodes] {
        assert_eq!(resolve(scope, Key::Y, ctrl, false), Some(Command::Redo));
        assert_eq!(resolve(scope, Key::Y, ctrl, true), None);
        assert_eq!(
            resolve(
                scope,
                Key::Z,
                Modifiers {
                    shift: true,
                    ..ctrl
                },
                false
            ),
            Some(Command::Redo)
        );
    }
    assert_eq!(
        resolve(
            Scope::Application,
            Key::Y,
            Modifiers {
                shift: true,
                ..ctrl
            },
            false
        ),
        Some(Command::Solid)
    );
    let bindings = Bindings::default();
    bindings.validate().unwrap();
    assert_eq!(
        bindings.hint(Scope::Application, Command::Redo),
        "Ctrl+Shift+Z / Ctrl+Y"
    );
    assert_eq!(
        bindings.hint(Scope::Application, Command::Solid),
        "Ctrl+Shift+Y"
    );
}

#[test]
fn explicit_saved_y_bindings_keep_their_owner_and_round_trip() {
    for (command, shift, expected, unbound) in [
        (Command::Solid, false, Command::Solid, Command::Redo),
        (Command::Redo, true, Command::Redo, Command::Solid),
    ] {
        let raw = serde_json::json!({"overrides":[{"scope":"application","command":command,
            "chord":KeyChord::new(Key::Y,true,shift,false)}]});
        let bindings: Bindings = serde_json::from_value(raw).unwrap();
        assert_eq!(
            bindings.resolve(
                Scope::Application,
                Key::Y,
                Modifiers {
                    control: true,
                    shift,
                    alt: false
                },
                false
            ),
            Some(expected)
        );
        assert_eq!(bindings.hint(Scope::Application, unbound), "");
        let restored: Bindings =
            serde_json::from_slice(&serde_json::to_vec(&bindings).unwrap()).unwrap();
        assert_eq!(restored, bindings);
        let mut reset = bindings.clone();
        assert!(reset.reset_command(Scope::Application, unbound).is_err());
        assert_eq!(reset, bindings, "reset cannot steal an explicit assignment");
    }
}
