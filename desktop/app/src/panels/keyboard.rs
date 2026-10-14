//! Focus routing and application shortcut actions.
use super::{
    helpers::last_frame,
    types::{Action, FxConsole, Panels},
};
use crate::shortcuts::{self, Command, Scope, FX_CONSOLE};
use motion_core::Command as Edit;
use motion_ui::{
    dock::Panel,
    input::{Input, Key, KeyChord},
};
use serde_json::Value;

fn text_active(panels: &Panels) -> bool {
    panels.timeline_columns.editing()
        || panels.properties.field.is_some()
        || (panels.focused == Some(Panel::Properties) && panels.properties.checkbox_focused)
        || panels.project.search
        || panels.timeline.timeline_search
        || panels.timeline.moving.is_some()
        || panels.timeline.trimming.is_some()
        || panels.timeline.key_drag.is_some()
        || panels.timeline.graph_drag.is_some()
        || panels.timeline.scrubbing
        || panels.effect_controls.editing()
        || panels.animation_controls.editing()
        || panels.camera_controls.editing()
        || panels.path_controls.wants_keyboard()
        || panels.node_editor.text_editing()
        || matches!(panels.focused, Some(Panel::Effects | Panel::Expression))
}

/// The modal also owns its closing frame, preventing input from reaching the canvas.
pub fn console_close_pressed(input: &Input) -> bool {
    FX_CONSOLE.pressed(input) || KeyChord::new(Key::Escape, false, false, false).pressed(input)
}

pub fn route(
    input: &Input,
    panels: &mut Panels,
    state: &Value,
    node_context: bool,
    owner: Option<Panel>,
    out: &mut Vec<Action>,
) -> bool {
    if let Some(console) = &panels.fx_console {
        let owned = console.owner == owner;
        if console_close_pressed(input) || owned && input.focus_lost {
            panels.fx_console = None;
            return true;
        }
        if owned {
            return true;
        }
    }
    let docked_timeline_modal = owner.is_none()
        && (panels.layer_actions.modal_in(Some(Panel::Timeline))
            || panels.layer_actions.text_in(Some(Panel::Timeline)));
    if panels.layer_actions.modal_in(owner)
        || panels.layer_actions.text_in(owner)
        || docked_timeline_modal
    {
        return true;
    }
    if panels.node_editor.popup_open()
        || panels.menu.is_some()
        || panels.panel_menu.is_some()
        || panels.project.context_menu.is_some()
        || panels.timeline.timeline_context_menu.is_some()
    {
        return false;
    }
    // Node properties and preview can be floated or shown without the canvas.
    // File commands still commit the shared node document's active field once.
    if node_context {
        for (key, modifiers, repeat) in input.key_presses() {
            let request = match shortcuts::resolve(Scope::Nodes, key, modifiers, repeat) {
                Some(Command::Open) => crate::node_editor::Request::Open,
                Some(Command::Save) => crate::node_editor::Request::Save,
                Some(Command::SaveAs) => crate::node_editor::Request::SaveAs,
                _ => continue,
            };
            let before = panels.node_editor.snapshot();
            if let Ok(action) = panels.node_editor.request(request) {
                let after = panels.node_editor.snapshot();
                if before.revision != after.revision {
                    out.push(Action::Node(crate::node_editor::Action::Changed {
                        revision: after.revision,
                        semantic_revision: after.semantic_revision,
                        semantic: before.semantic_revision != after.semantic_revision,
                    }));
                }
                out.push(Action::Node(action));
            }
            return true;
        }
    }
    if !input.composing
        && !input.ime_consumed
        && KeyChord::new(Key::R, true, false, false).pressed(input)
    {
        out.push(Action::ReloadStyle);
        return true;
    }
    if text_active(panels) || input.composing || input.ime_consumed {
        return false;
    }
    if node_context && !panels.node_editor.editing() {
        for (key, modifiers, repeat) in input.key_presses() {
            let command = shortcuts::resolve(Scope::Nodes, key, modifiers, repeat);
            if !matches!(command, Some(Command::Undo | Command::Redo)) {
                continue;
            }
            let before = panels.node_editor.snapshot();
            let changed = if command == Some(Command::Undo) {
                panels.node_editor.undo()
            } else {
                panels.node_editor.redo()
            };
            if changed {
                let after = panels.node_editor.snapshot();
                out.push(Action::Node(crate::node_editor::Action::Changed {
                    revision: after.revision,
                    semantic_revision: after.semantic_revision,
                    semantic: before.semantic_revision != after.semantic_revision,
                }));
            }
            return true;
        }
    }
    if FX_CONSOLE.pressed(input) && !panels.node_editor.editing() {
        panels.fx_console = Some(FxConsole {
            position: input.mouse,
            query: String::new(),
            selected_index: 0,
            owner,
        });
        return true;
    }
    if !node_context
        && panels.menu.is_none()
        && panels.panel_menu.is_none()
        && panels.timeline.timeline_context_menu.is_none()
    {
        handle_shortcuts(input, panels, state, out);
    }
    false
}

pub fn handle_shortcuts(input: &Input, panels: &mut Panels, state: &Value, out: &mut Vec<Action>) {
    let mut frame = state["frame"].as_f64().unwrap_or(0.0);
    for (key, modifiers, repeat) in input.key_presses() {
        let key_owned = matches!(panels.focused, Some(Panel::Timeline | Panel::Curves))
            && panels.animation_controls.user_key().is_some();
        if let Some(command) = shortcuts::resolve(Scope::Animation, key, modifiers, repeat) {
            if super::timeline_shortcuts::handle(command, modifiers, panels, state, &mut frame, out)
                || key_owned && command == Command::Delete
            {
                continue;
            }
        }
        let Some(command) = shortcuts::resolve(Scope::Application, key, modifiers, repeat) else {
            continue;
        };
        let selected_ids = panels.editable_selection(state);
        let selected = panels.selected.filter(|id| selected_ids.contains(id));
        let action = match command {
            Command::ReloadStyle => Action::ReloadStyle,
            Command::NewProject => Action::New,
            Command::NewComposition => Action::NewComposition,
            Command::Open => Action::Open,
            Command::Save => Action::Save,
            Command::Import => Action::ImportMedia,
            Command::Settings => Action::CompositionSettings,
            Command::Export if cfg!(feature = "ffmpeg") => Action::ExportMovie,
            Command::Undo => Action::History(0),
            Command::Redo => Action::History(1),
            Command::Play => Action::TogglePlay,
            Command::Solid => Action::AddSolid,
            Command::Text => Action::NewText,
            Command::Null => Action::AddNull,
            Command::Adjustment => Action::AddAdjustment,
            Command::PreviousFrame | Command::NextFrame => {
                if matches!(key, Key::Left | Key::Right)
                    && !modifiers.control
                    && panels.focused != Some(Panel::Timeline)
                {
                    continue;
                }
                let direction = if command == Command::PreviousFrame {
                    -1.
                } else {
                    1.
                };
                frame = (frame + direction * if modifiers.shift { 10. } else { 1. })
                    .clamp(0., last_frame(state));
                Action::Seek(frame)
            }
            Command::FirstFrame => {
                frame = 0.;
                Action::Seek(frame)
            }
            Command::LastFrame => {
                frame = last_frame(state);
                Action::Seek(frame)
            }
            Command::Duplicate => {
                if panels.focused == Some(Panel::Curves)
                    || matches!(panels.focused, Some(Panel::Timeline))
                        && (panels.timeline.graph_mode
                            || panels.animation_controls.user_key().is_some())
                {
                    continue;
                }
                let Some(object) = selected_ids.first().copied() else {
                    continue;
                };
                if selected_ids.len() > 1 {
                    Action::EditBatch(
                        selected_ids
                            .iter()
                            .copied()
                            .map(|object| Edit::Duplicate { object })
                            .collect(),
                    )
                } else {
                    Action::Edit(Edit::Duplicate { object })
                }
            }
            Command::Split => {
                let Some(object) = selected else {
                    continue;
                };
                Action::Edit(Edit::SplitLayerClip {
                    object,
                    frame: frame.floor() as u32,
                })
            }
            Command::Delete => {
                let Some(object) = selected_ids.first().copied() else {
                    continue;
                };
                if selected_ids.len() > 1 {
                    Action::EditBatch(
                        selected_ids
                            .iter()
                            .copied()
                            .map(|object| Edit::Delete { object })
                            .collect(),
                    )
                } else {
                    Action::Edit(Edit::Delete { object })
                }
            }
            Command::SelectAll | Command::DeselectAll
                if matches!(panels.focused, Some(Panel::Timeline | Panel::Composition))
                    && (panels.focused != Some(Panel::Timeline)
                        || (!panels.timeline.graph_mode
                            && panels.animation_controls.user_key().is_none())) =>
            {
                if command == Command::DeselectAll {
                    panels.select_only(None);
                } else {
                    let primary = panels.selected;
                    panels.selected_layers = state["project"]["layers"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|layer| layer["id"].as_u64())
                        .filter(|id| *id != 0)
                        .collect();
                    panels.selected = primary
                        .filter(|id| panels.selected_layers.contains(id))
                        .or_else(|| panels.selected_layers.first().copied());
                    panels.selection_anchor = panels.selected;
                }
                continue;
            }
            _ => continue,
        };
        out.push(action);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn console(owner: Option<Panel>) -> FxConsole {
        FxConsole {
            position: [20., 20.],
            query: "glow".into(),
            selected_index: 0,
            owner,
        }
    }
    #[test]
    fn console_blocks_only_its_owner_and_other_window_can_save() {
        for (owner, other) in [
            (Some(Panel::NodeProperties), None),
            (None, Some(Panel::Info)),
        ] {
            let mut panels = Panels {
                fx_console: Some(console(owner)),
                ..Default::default()
            };
            let mut out = vec![];
            assert!(route(
                &Input::default(),
                &mut panels,
                &Value::Null,
                false,
                owner,
                &mut out
            ));
            assert!(!route(
                &Input::default(),
                &mut panels,
                &Value::Null,
                false,
                other,
                &mut out
            ));
            let input = Input {
                keys: vec![(
                    Key::S,
                    motion_ui::input::Modifiers {
                        control: true,
                        ..Default::default()
                    },
                )],
                ..Default::default()
            };
            assert!(!route(
                &input,
                &mut panels,
                &Value::Null,
                false,
                other,
                &mut out
            ));
            assert!(matches!(out.as_slice(), [Action::Save]));
            assert_eq!(panels.fx_console.as_ref().unwrap().owner, owner);
        }
    }
    #[test]
    fn escape_and_toggle_from_either_window_close_and_consume_the_frame() {
        for (owner, other) in [
            (Some(Panel::NodeProperties), None),
            (None, Some(Panel::Info)),
        ] {
            for chord in [KeyChord::new(Key::Escape, false, false, false), FX_CONSOLE] {
                let mut panels = Panels {
                    fx_console: Some(console(owner)),
                    ..Default::default()
                };
                let input = Input {
                    keys: vec![
                        (chord.key, chord.modifiers),
                        (Key::Delete, Default::default()),
                    ],
                    ..Default::default()
                };
                let mut out = vec![];
                assert!(route(
                    &input,
                    &mut panels,
                    &Value::Null,
                    false,
                    other,
                    &mut out
                ));
                assert!(panels.fx_console.is_none());
                assert!(out.is_empty());
            }
        }
    }
    #[test]
    fn unrelated_focus_loss_keeps_console_and_owner_focus_loss_closes_it() {
        let owner = Some(Panel::NodeProperties);
        let mut panels = Panels {
            fx_console: Some(console(owner)),
            ..Default::default()
        };
        let input = Input {
            focus_lost: true,
            ..Default::default()
        };
        let mut out = vec![];
        assert!(!route(
            &input,
            &mut panels,
            &Value::Null,
            false,
            None,
            &mut out
        ));
        assert!(panels.fx_console.is_some());
        assert!(route(
            &input,
            &mut panels,
            &Value::Null,
            false,
            owner,
            &mut out
        ));
        assert!(panels.fx_console.is_none());
        assert!(out.is_empty());
    }
    #[test]
    fn node_preview_history_uses_the_node_document_without_a_canvas() {
        let mut panels = Panels::default();
        panels.focused = Some(Panel::NodePreview);
        let before = panels.node_editor.document().clone();
        panels
            .node_editor
            .add_node(motion_graph::NodeOp::Float { value: 1. }, [0., 0.])
            .unwrap();
        let mut out = vec![];
        let input = Input {
            keys: vec![(
                Key::Z,
                motion_ui::input::Modifiers {
                    control: true,
                    ..Default::default()
                },
            )],
            ..Default::default()
        };
        assert!(route(
            &input,
            &mut panels,
            &Value::Null,
            true,
            Some(Panel::NodePreview),
            &mut out
        ));
        assert_eq!(panels.node_editor.document(), &before);
        assert!(matches!(
            out.as_slice(),
            [Action::Node(crate::node_editor::Action::Changed {
                semantic: true,
                ..
            })]
        ));
    }
    #[test]
    fn locked_layers_cannot_edit_and_repeated_frame_steps_accumulate() {
        let mut panels = Panels {
            selected: Some(1),
            ..Default::default()
        };
        let state = serde_json::json!({"frame":20,"project":{"frames":120,"layers":[{"id":1,"locked":true}]}});
        let ctrl = motion_ui::input::Modifiers {
            control: true,
            ..Default::default()
        };
        let input = Input {
            keys: vec![
                (Key::D, ctrl),
                (Key::Delete, Default::default()),
                (Key::Right, ctrl),
                (Key::Right, ctrl),
                (
                    Key::PageUp,
                    motion_ui::input::Modifiers {
                        shift: true,
                        ..Default::default()
                    },
                ),
            ],
            key_repeats: vec![false, false, false, true, false],
            ..Default::default()
        };
        let mut out = vec![];
        handle_shortcuts(&input, &mut panels, &state, &mut out);
        assert!(matches!(
            out.as_slice(),
            [Action::Seek(21.), Action::Seek(22.), Action::Seek(12.)]
        ));
        let input = Input {
            keys: vec![
                (Key::N, ctrl),
                (Key::N, motion_ui::input::Modifiers { alt: true, ..ctrl }),
                (Key::Y, ctrl),
                (
                    Key::Y,
                    motion_ui::input::Modifiers {
                        alt: true,
                        shift: true,
                        ..ctrl
                    },
                ),
                (
                    Key::T,
                    motion_ui::input::Modifiers {
                        alt: true,
                        shift: true,
                        ..ctrl
                    },
                ),
            ],
            ..Default::default()
        };
        out.clear();
        handle_shortcuts(&input, &mut panels, &state, &mut out);
        assert!(matches!(
            out.as_slice(),
            [
                Action::NewComposition,
                Action::New,
                Action::AddSolid,
                Action::AddNull,
                Action::NewText
            ]
        ));
    }

    #[test]
    fn duplicate_and_delete_use_one_batch_for_multiple_selected_layers() {
        let mut panels = Panels {
            selected: Some(2),
            selected_layers: vec![1, 2],
            ..Default::default()
        };
        let state = serde_json::json!({
            "frame": 0,
            "project": {"frames": 120, "layers": [
                {"id": 1, "locked": false},
                {"id": 2, "locked": false}
            ]}
        });
        let mut out = vec![];
        handle_shortcuts(
            &Input {
                keys: vec![(
                    Key::D,
                    motion_ui::input::Modifiers {
                        control: true,
                        ..Default::default()
                    },
                )],
                ..Default::default()
            },
            &mut panels,
            &state,
            &mut out,
        );
        assert!(matches!(
            out.as_slice(),
            [Action::EditBatch(commands)] if commands.len() == 2
        ));
        out.clear();
        handle_shortcuts(
            &Input {
                keys: vec![(Key::Delete, Default::default())],
                ..Default::default()
            },
            &mut panels,
            &state,
            &mut out,
        );
        assert!(matches!(
            out.as_slice(),
            [Action::EditBatch(commands)] if commands.len() == 2
        ));
    }
    #[test]
    fn node_properties_file_commands_and_floating_modal_keep_their_owner() {
        let mut panels = Panels::default();
        panels.focused = Some(Panel::NodeProperties);
        let mut out = vec![];
        let input = Input {
            keys: vec![(
                Key::S,
                motion_ui::input::Modifiers {
                    control: true,
                    ..Default::default()
                },
            )],
            ..Default::default()
        };
        assert!(route(
            &input,
            &mut panels,
            &Value::Null,
            true,
            Some(Panel::NodeProperties),
            &mut out
        ));
        assert!(matches!(
            out.as_slice(),
            [Action::Node(crate::node_editor::Action::Save(_))]
        ));
        out.clear();
        let input = Input {
            keys: vec![(Key::Space, FX_CONSOLE.modifiers)],
            ..Default::default()
        };
        route(
            &input,
            &mut panels,
            &Value::Null,
            true,
            Some(Panel::NodeProperties),
            &mut out,
        );
        assert_eq!(
            panels.fx_console.as_ref().unwrap().owner,
            Some(Panel::NodeProperties)
        );
        route(&input, &mut panels, &Value::Null, true, None, &mut out);
        assert!(panels.fx_console.is_none());
        assert!(out.is_empty());
    }
    #[test]
    fn modal_has_priority_in_nodes_and_its_closing_frame_is_consumed() {
        let mut panels = Panels::default();
        panels.node_editor.focus_panel();
        let input = Input {
            keys: vec![(Key::Space, FX_CONSOLE.modifiers)],
            ..Default::default()
        };
        let mut out = vec![];
        assert!(route(
            &input,
            &mut panels,
            &Value::Null,
            true,
            None,
            &mut out
        ));
        assert!(panels.fx_console.is_some());
        assert!(route(
            &input,
            &mut panels,
            &Value::Null,
            true,
            None,
            &mut out
        ));
        assert!(panels.fx_console.is_none());
        assert!(out.is_empty());
    }
    #[test]
    fn search_and_repeat_do_not_open_modal() {
        let mut panels = Panels::default();
        panels.project.search = true;
        let mut out = vec![];
        let mut input = Input {
            keys: vec![(Key::Space, FX_CONSOLE.modifiers)],
            ..Default::default()
        };
        assert!(!route(
            &input,
            &mut panels,
            &Value::Null,
            false,
            None,
            &mut out
        ));
        assert!(panels.fx_console.is_none());
        panels.project.search = false;
        input.key_repeats = vec![true];
        route(&input, &mut panels, &Value::Null, false, None, &mut out);
        assert!(panels.fx_console.is_none());
        assert!(out.is_empty());
    }
}
