//! Keyboard shortcut handling
use motion_core::Command;
use motion_ui::input::{Input, Key};
use serde_json::Value;

use super::{
    helpers::last_frame,
    types::{Action, FxConsole, Panels},
};

/// Process keyboard shortcuts
pub fn handle_shortcuts(input: &Input, panels: &mut Panels, state: &Value, out: &mut Vec<Action>) {
    for (key, modifiers) in &input.keys {
        match (key, modifiers.control, modifiers.shift) {
            // File operations
            (Key::N, true, false) => out.push(Action::New),
            (Key::O, true, false) => out.push(Action::Open),
            (Key::I, true, false) => out.push(Action::ImportMedia),
            (Key::S, true, false) => out.push(Action::Save),

            // History (Undo/Redo)
            (Key::Z, true, false) => out.push(Action::History(0)),
            (Key::Z, true, true) => out.push(Action::History(1)),
            (Key::Y, true, false) if modifiers.alt => out.push(Action::AddAdjustment),
            (Key::Y, true, false) => out.push(Action::History(1)),

            // Playback and FX Console
            (Key::Space, _, _) => {
                if modifiers.control {
                    if panels.fx_console.is_some() {
                        panels.fx_console = None;
                    } else {
                        panels.fx_console = Some(FxConsole {
                            position: input.mouse,
                            query: String::new(),
                            selected_index: 0,
                        });
                    }
                } else {
                    out.push(Action::TogglePlay);
                }
            }

            // Frame navigation
            (Key::Left, false, _) => out.push(Action::Seek(
                (state["frame"].as_f64().unwrap_or(0.0) - 1.0).max(0.0),
            )),
            (Key::Right, false, _) => out.push(Action::Seek(
                (state["frame"].as_f64().unwrap_or(0.0) + 1.0).min(last_frame(state)),
            )),
            (Key::Home, false, _) => out.push(Action::Seek(0.0)),
            (Key::End, false, _) => out.push(Action::Seek(last_frame(state))),

            // Layer operations
            (Key::D, true, false) => {
                // Duplicate layer
                if let Some(object) = panels.selected.filter(|id| *id != 0) {
                    out.push(Action::Edit(Command::Duplicate { object }));
                }
            }
            (Key::D, true, true) => {
                // Split layer at current time
                if let Some(object) = panels.selected.filter(|id| *id != 0) {
                    out.push(Action::Edit(Command::SplitLayerClip {
                        object,
                        frame: state["frame"].as_f64().unwrap_or(0.0).floor() as u32,
                    }));
                }
            }
            (Key::Delete, false, _) | (Key::Backspace, false, _) => {
                // Delete selected layer
                if let Some(object) = panels.selected.filter(|id| *id != 0) {
                    out.push(Action::Edit(Command::Delete { object }));
                }
            }

            // Add new layers (AE-style)
            (Key::T, true, true) => out.push(Action::NewText),
            (Key::Y, true, true) => {
                // Ctrl+Shift+Y: New solid layer
                out.push(Action::AddSolid);
            }
            (Key::N, true, true) => {
                // Ctrl+Shift+N: New null object
                out.push(Action::AddNull);
            }

            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adjustment_shortcut_does_not_trigger_redo() {
        let mut panels = Panels::default();
        let mut actions = vec![];
        let modifiers = motion_ui::input::Modifiers {
            control: true,
            alt: true,
            ..Default::default()
        };
        handle_shortcuts(
            &Input {
                keys: vec![(Key::Y, modifiers)],
                ..Default::default()
            },
            &mut panels,
            &Value::Null,
            &mut actions,
        );
        assert!(matches!(actions.as_slice(), [Action::AddAdjustment]));
    }
    #[test]
    fn ctrl_space_opens_modal_without_playback_and_shift_y_creates_solid() {
        let mut panels = Panels::default();
        let mut actions = vec![];
        let modifiers = motion_ui::input::Modifiers {
            control: true,
            ..Default::default()
        };
        handle_shortcuts(
            &Input {
                keys: vec![(Key::Space, modifiers)],
                ..Default::default()
            },
            &mut panels,
            &Value::Null,
            &mut actions,
        );
        assert!(panels.fx_console.is_some());
        assert!(actions.is_empty());
        handle_shortcuts(
            &Input {
                keys: vec![(
                    Key::Y,
                    motion_ui::input::Modifiers {
                        shift: true,
                        ..modifiers
                    },
                )],
                ..Default::default()
            },
            &mut panels,
            &Value::Null,
            &mut actions,
        );
        assert!(matches!(actions.as_slice(), [Action::AddSolid]));
    }
}
