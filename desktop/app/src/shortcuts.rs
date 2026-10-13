//! Default desktop chords shared by application and node contexts.
use motion_ui::input::{Key, KeyChord, Modifiers};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    Application,
    Animation,
    Nodes,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Command {
    ReloadStyle,
    NewProject,
    NewComposition,
    Open,
    Save,
    SaveAs,
    Import,
    Settings,
    Export,
    Undo,
    Redo,
    Play,
    PreviousFrame,
    NextFrame,
    FirstFrame,
    LastFrame,
    Duplicate,
    Split,
    Delete,
    Solid,
    Text,
    Null,
    Adjustment,
    SelectAll,
    DeselectAll,
    FitAll,
    FitSelection,
    AddNode,
    ResetZoom,
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    Arrange,
    Group,
    Ungroup,
    EnterGroup,
    LeaveGroup,
    Reveal(motion_core::Property),
    AnimatedProperties,
    PreviousKey,
    NextKey,
    ToggleGraph,
    EaseBoth,
    EaseIncoming,
    EaseOutgoing,
    ToggleKey(motion_core::Property),
}

pub const FX_CONSOLE: KeyChord = KeyChord::new(Key::Space, true, false, false);

/// Modifiers match exactly. Only navigation and micro-movement accept OS repeat.
pub fn resolve(scope: Scope, key: Key, m: Modifiers, repeat: bool) -> Option<Command> {
    use Command::*;
    use Scope::*;
    let command = match (scope, key, m.control, m.shift, m.alt) {
        (Animation, Key::P, false, _, false) => Reveal(motion_core::Property::Position),
        (Animation, Key::S, false, _, false) => Reveal(motion_core::Property::Scale),
        (Animation, Key::R, false, _, false) => Reveal(motion_core::Property::Rotation),
        (Animation, Key::T, false, _, false) => Reveal(motion_core::Property::Opacity),
        (Animation, Key::U, false, false, false) => AnimatedProperties,
        (Animation, Key::J, false, false, false) => PreviousKey,
        (Animation, Key::K, false, false, false) => NextKey,
        (Animation, Key::F3, false, true, false) => ToggleGraph,
        (Animation, Key::F9, false, false, false) => EaseBoth,
        (Animation, Key::F9, false, true, false) => EaseIncoming,
        (Animation, Key::F9, true, true, false) => EaseOutgoing,
        (Animation, Key::P, false, true, true) => ToggleKey(motion_core::Property::Position),
        (Animation, Key::S, false, true, true) => ToggleKey(motion_core::Property::Scale),
        (Animation, Key::R, false, true, true) => ToggleKey(motion_core::Property::Rotation),
        (Animation, Key::T, false, true, true) => ToggleKey(motion_core::Property::Opacity),
        (Application, Key::R, true, false, false) => ReloadStyle,
        (_, Key::O, true, false, false) => Open,
        (_, Key::S, true, false, false) => Save,
        (_, Key::Z, true, false, false) => Undo,
        (_, Key::Z, true, true, false) => Redo,
        (_, Key::D, true, false, false) => Duplicate,
        (_, Key::Delete | Key::Backspace, false, false, false) => Delete,
        (Application, Key::N, true, false, true) => NewProject,
        (Application, Key::N, true, false, false) => NewComposition,
        (Application, Key::I, true, false, false) => Import,
        (Application, Key::K, true, false, false) => Settings,
        (Application, Key::M, true, false, true) => Export,
        (Application, Key::Space, false, false, false) => Play,
        (Application, Key::Home, false, false, false) => FirstFrame,
        (Application, Key::End, false, false, false) => LastFrame,
        (Application, Key::PageUp, false, _, false) | (Application, Key::Left, _, _, false) => {
            PreviousFrame
        }
        (Application, Key::PageDown, false, _, false) | (Application, Key::Right, _, _, false) => {
            NextFrame
        }
        (Application, Key::D, true, true, false) => Split,
        (Application, Key::Y, true, false, false) => Solid,
        (Application, Key::T, true, true, true) => Text,
        (Application, Key::Y, true, true, true) => Null,
        (Application, Key::Y, true, false, true) => Adjustment,
        (Nodes, Key::S, true, true, false) => SaveAs,
        (Nodes, Key::A, true, false, false) => SelectAll,
        (Nodes, Key::A, true, true, false) => DeselectAll,
        (Nodes, Key::X, false, false, false) => Delete,
        (Nodes, Key::Home, false, false, false) => FitAll,
        (Nodes, Key::F, false, false, false) => FitSelection,
        (Nodes, Key::A, false, true, false) => AddNode,
        (Nodes, Key::Num0, true, false, false) => ResetZoom,
        (Nodes, Key::Equal, false, true, false) => Arrange,
        (Nodes, Key::G, true, false, false) => Group,
        (Nodes, Key::G, false, false, true) => Ungroup,
        (Nodes, Key::Tab, false, false, false) => EnterGroup,
        (Nodes, Key::Tab, true, false, false) => LeaveGroup,
        (Nodes, Key::Left, false, _, false) => MoveLeft,
        (Nodes, Key::Right, false, _, false) => MoveRight,
        (Nodes, Key::Up, false, _, false) => MoveUp,
        (Nodes, Key::Down, false, _, false) => MoveDown,
        _ => return None,
    };
    (!repeat
        || matches!(
            command,
            PreviousFrame
                | NextFrame
                | MoveLeft
                | MoveRight
                | MoveUp
                | MoveDown
                | PreviousKey
                | NextKey
        ))
    .then_some(command)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modifiers_and_repeat_do_not_alias_other_commands() {
        let ctrl = Modifiers {
            control: true,
            ..Default::default()
        };
        assert_eq!(
            resolve(Scope::Application, Key::Y, ctrl, false),
            Some(Command::Solid)
        );
        assert_eq!(
            resolve(
                Scope::Application,
                Key::Y,
                Modifiers { alt: true, ..ctrl },
                false
            ),
            Some(Command::Adjustment)
        );
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
            None
        );
        assert_eq!(
            resolve(Scope::Nodes, Key::D, Modifiers { alt: true, ..ctrl }, false),
            None
        );
        assert_eq!(resolve(Scope::Nodes, Key::D, ctrl, true), None);
        assert_eq!(
            resolve(Scope::Nodes, Key::Left, Modifiers::default(), true),
            Some(Command::MoveLeft)
        );
        assert_eq!(
            resolve(
                Scope::Application,
                Key::Space,
                Modifiers {
                    alt: true,
                    ..Default::default()
                },
                false
            ),
            None
        );
    }
}
