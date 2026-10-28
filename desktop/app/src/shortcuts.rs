//! Default desktop chords shared by application and node contexts.
use motion_ui::input::{Key, KeyChord, Modifiers};
pub(crate) mod bindings;

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Application,
    Animation,
    Nodes,
    Viewer,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    ReloadStyle,
    TogglePanelMaximize,
    ToolSelect,
    ToolPen,
    ToolAnchor,
    ToolRotate,
    ToolShape,
    ToolText,
    ToolHand,
    ToolZoom,
    ToolCamera,
    ToggleViewerHandles,
    NewProject,
    NewComposition,
    Open,
    Save,
    SaveAs,
    Import,
    Settings,
    Preferences,
    Export,
    Undo,
    Redo,
    Play,
    AudioPreview,
    PreviousFrame,
    NextFrame,
    FirstFrame,
    LastFrame,
    GoToIn,
    GoToOut,
    WorkAreaIn,
    WorkAreaOut,
    GoToWorkAreaIn,
    GoToWorkAreaOut,
    MoveInToPlayhead,
    MoveOutToPlayhead,
    TrimInToPlayhead,
    TrimOutToPlayhead,
    ToggleCompositeColumns,
    ToggleParentColumns,
    CopyNodes,
    CopySelection,
    CutSelection,
    PasteSelection,
    PasteInPlace,
    CutNodes,
    PasteNodes,
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
    AddTextureMapping,
    ResetZoom,
    ZoomIn,
    ZoomOut,
    ToggleFrameView,
    ToggleNodeGrid,
    ToggleNodeMinimap,
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    SelectLayerAbove,
    SelectLayerBelow,
    OrderLayersUp,
    OrderLayersDown,
    OrderLayersTop,
    OrderLayersBottom,
    Arrange,
    Group,
    Ungroup,
    EnterGroup,
    LeaveGroup,
    Reveal(motion_core::Property),
    AnimatedProperties,
    ModifiedProperties,
    PreviousKey,
    NextKey,
    NudgeKeyEarlier,
    NudgeKeyLater,
    NudgeKeysEarlierTen,
    NudgeKeysLaterTen,
    ToggleGraph,
    EaseBoth,
    EaseIncoming,
    EaseOutgoing,
    KeyframeTiming,
    ToggleKey(motion_core::Property),
}

pub const EFFECT_SEARCH: KeyChord = KeyChord::new(Key::Space, true, false, false);
pub const EFFECT_SEARCH_ALT: KeyChord = KeyChord::new(Key::Space, true, false, true);

/// Both structural search chords share repeat and IME suppression.
pub fn effect_search_pressed(input: &impl motion_ui::InputSource) -> bool {
    EFFECT_SEARCH.pressed(input) || EFFECT_SEARCH_ALT.pressed(input)
}

/// Modifiers match exactly. Only navigation and micro-movement accept OS repeat.
pub fn resolve(scope: Scope, key: Key, m: Modifiers, repeat: bool) -> Option<Command> {
    use Command::*;
    use Scope::*;
    let command = match (scope, key, m.control, m.shift, m.alt) {
        (Animation, Key::C, true, false, false) => CopySelection,
        (Animation, Key::X, true, false, false) => CutSelection,
        (Animation, Key::V, true, false, false) => PasteSelection,
        (Animation, Key::V, true, true, false) => PasteInPlace,
        (Animation, Key::Up, true, _, false) => SelectLayerAbove,
        (Animation, Key::Down, true, _, false) => SelectLayerBelow,
        (Animation, Key::Up, true, false, true) => OrderLayersUp,
        (Animation, Key::Down, true, false, true) => OrderLayersDown,
        (Animation, Key::Up, true, true, true) => OrderLayersTop,
        (Animation, Key::Down, true, true, true) => OrderLayersBottom,
        (Animation, Key::A, false, _, false) => Reveal(motion_core::Property::Anchor),
        (Animation, Key::P, false, _, false) => Reveal(motion_core::Property::Position),
        (Animation, Key::S, false, _, false) => Reveal(motion_core::Property::Scale),
        (Animation, Key::R, false, _, false) => Reveal(motion_core::Property::Rotation),
        (Animation, Key::T, false, _, false) => Reveal(motion_core::Property::Opacity),
        (Animation, Key::U, false, false, false) => AnimatedProperties,
        (Animation, Key::Equal, false, _, false) => ZoomIn,
        (Animation, Key::Minus, false, false, false) => ZoomOut,
        (Animation, Key::Semicolon, false, false, false) => ToggleFrameView,
        (Animation, Key::J, false, false, false) => PreviousKey,
        (Animation, Key::K, false, false, false) => NextKey,
        (Animation, Key::Left, false, false, true) => NudgeKeyEarlier,
        (Animation, Key::Right, false, false, true) => NudgeKeyLater,
        (Animation, Key::Left, false, true, true) => NudgeKeysEarlierTen,
        (Animation, Key::Right, false, true, true) => NudgeKeysLaterTen,
        (Animation, Key::I, false, false, false) => GoToIn,
        (Animation, Key::O, false, false, false) => GoToOut,
        (Animation, Key::B, false, false, false) => WorkAreaIn,
        (Animation, Key::N, false, false, false) => WorkAreaOut,
        (Animation, Key::Home, false, true, false) => GoToWorkAreaIn,
        (Animation, Key::End, false, true, false) => GoToWorkAreaOut,
        (Animation, Key::BracketLeft, false, false, false) => MoveInToPlayhead,
        (Animation, Key::BracketRight, false, false, false) => MoveOutToPlayhead,
        (Animation, Key::BracketLeft, false, false, true) => TrimInToPlayhead,
        (Animation, Key::BracketRight, false, false, true) => TrimOutToPlayhead,
        (Animation, Key::F4, false, false, false) => ToggleCompositeColumns,
        (Animation, Key::F4, false, true, false) => ToggleParentColumns,
        (Animation, Key::F3, false, true, false) => ToggleGraph,
        (Animation, Key::F9, false, false, false) => EaseBoth,
        (Animation, Key::F9, false, true, false) => EaseIncoming,
        (Animation, Key::F9, true, true, false) => EaseOutgoing,
        (Animation, Key::K, true, false, true) => KeyframeTiming,
        (Animation, Key::P, false, true, true) => ToggleKey(motion_core::Property::Position),
        (Animation, Key::S, false, true, true) => ToggleKey(motion_core::Property::Scale),
        (Animation, Key::R, false, true, true) => ToggleKey(motion_core::Property::Rotation),
        (Animation, Key::T, false, true, true) => ToggleKey(motion_core::Property::Opacity),
        (Viewer, Key::V, false, false, false) => ToolSelect,
        (Viewer, Key::G, false, false, false) => ToolPen,
        (Viewer, Key::Y, false, false, false) => ToolAnchor,
        (Viewer, Key::W, false, false, false) => ToolRotate,
        (Viewer, Key::Q, false, false, false) => ToolShape,
        (Viewer, Key::T, false, false, false) => ToolText,
        (Viewer, Key::H, false, false, false) => ToolHand,
        (Viewer, Key::H, true, true, false) => ToggleViewerHandles,
        (Viewer, Key::Z, false, false, false) => ToolZoom,
        (Viewer, Key::C, false, false, false) => ToolCamera,
        (Viewer, _, _, _, _) => return None,
        (Application, Key::R, true, false, false) => ReloadStyle,
        (Application, Key::Backquote, false, false, false) => TogglePanelMaximize,
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
        (Application, Key::Comma, true, false, false) => Preferences,
        (Application, Key::M, true, false, true) => Export,
        (Application, Key::Space, false, false, false) => Play,
        (Application, Key::NumpadDecimal, false, false, false) => AudioPreview,
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
        (Application, Key::A, true, false, false) => SelectAll,
        (Application, Key::A, true, true, false) => DeselectAll,
        (Nodes, Key::Space, false, false, false) => Play,
        (Nodes, Key::S, true, true, false) => SaveAs,
        (Nodes, Key::A, true, false, false) => SelectAll,
        (Nodes, Key::A, true, true, false) => DeselectAll,
        (Nodes, Key::C, true, false, false) => CopyNodes,
        (Nodes, Key::X, true, false, false) => CutNodes,
        (Nodes, Key::V, true, false, false) => PasteNodes,
        (Nodes, Key::X, false, false, false) => Delete,
        (Nodes, Key::Home, false, false, false) => FitAll,
        (Nodes, Key::F, false, false, false) => FitSelection,
        (Nodes, Key::A, false, true, false) => AddNode,
        (Nodes, Key::T, true, false, false) => AddTextureMapping,
        (Nodes, Key::Num0, true, false, false) => ResetZoom,
        (Nodes, Key::Equal, false, true, false) => Arrange,
        (Nodes, Key::G, true, false, false) => Group,
        (Nodes, Key::G, false, false, false) => ToggleNodeGrid,
        (Nodes, Key::N, false, false, false) => ToggleNodeMinimap,
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
                | NudgeKeyEarlier | NudgeKeyLater | NudgeKeysEarlierTen | NudgeKeysLaterTen
                | SelectLayerAbove
                | SelectLayerBelow
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

#[cfg(test)]
#[path = "shortcuts_documentation_tests.rs"]
mod documentation_tests;
