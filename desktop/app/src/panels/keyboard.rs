//! Focus routing and application shortcut actions.
use super::{
    helpers::last_frame,
    types::{Action, EffectSearch, Panels},
};
use crate::shortcuts::{self, Command, Scope};
#[cfg(test)]
use crate::shortcuts::{EFFECT_SEARCH, EFFECT_SEARCH_ALT};
use motion_core::Command as Edit;
use motion_ui::{
    dock::Panel,
    input::{Input, Key, KeyChord},
};
use serde_json::Value;

fn text_active(panels: &Panels) -> bool {
    editor_active(panels)
        || matches!(panels.focused, Some(Panel::Effects | Panel::Expression))
}

fn editor_active(panels: &Panels) -> bool {
    panels.composition.zoom.interacting()
        || panels.timeline_columns.editing()
        || panels.properties.field.is_some()
        || panels.project.search
        || panels.timeline.timeline_search
        || panels.timeline.moving.is_some()
        || panels.timeline.trimming.is_some()
        || panels.timeline.key_drag.is_some()
        || panels.timeline.timing_drag.is_some()
        || panels.timeline.graph_drag.is_some()
        || panels.timeline.graph_box.editing()
        || panels.timeline.horizontal_drag.is_some()
        || panels.timeline.scrubbing
        || panels.effect_controls.editing()
        || panels.properties.effects.editing()
        || panels.effect_timeline.editing()
        || panels.properties.anchor.editing()
        || panels.timeline_anchor.editing()
        || panels.expressions.editing()
        || panels.auxiliary.editing()
        || panels.animation_controls.editing()
        || panels.camera_controls.editing()
        || panels.audio_controls.editing()
        || panels.path_controls.wants_keyboard()
        || panels.node_editor.text_editing()
}

/// Layout shortcuts must defer to the same editors as document shortcuts.
pub(crate) fn layout_shortcuts_blocked(panels: &Panels) -> bool {
    editor_active(panels)
        || (panels.focused == Some(Panel::Effects) && panels.browser.library_search_editing())
        || panels.transform_controls.gesture_active()
        || panels.timeline.reorder.is_some()
        || panels.composition.pan.is_some()
        || panels.composition.orbit.is_some()
        || panels.parent_controls.open
        || panels.node_editor.editing()
        || panels.node_editor.popup_open()
        || panels.menu.is_some()
        || panels.panel_menu.is_some()
        || panels.project.context_menu.is_some()
        || panels.timeline.timeline_context_menu.is_some()
        || panels.effect_search.is_some()
        || panels.layer_actions.modal_in(None)
        || panels.layer_actions.modal_in(Some(Panel::Timeline))
        || panels.layer_actions.text_in(None)
        || panels.layer_actions.text_in(Some(Panel::Timeline))
}

/// The modal also owns its closing frame, preventing input from reaching the canvas.
pub fn effect_search_close_pressed(input: &Input) -> bool {
    shortcuts::effect_search_pressed(input) || KeyChord::new(Key::Escape, false, false, false).pressed(input)
}

pub fn route(
    input: &Input,
    panels: &mut Panels,
    state: &Value,
    node_context: bool,
    owner: Option<Panel>,
    out: &mut Vec<Action>,
) -> bool {
    if input.focus_lost || input.composing || input.ime_consumed
        || input.pressed.is_some() || input.wheel != [0.; 2] || text_active(panels) || node_context
        || panels.menu.is_some() || panels.panel_menu.is_some()
        || panels.timeline.timeline_context_menu.is_some()
        || panels.project.context_menu.is_some() || panels.effect_search.is_some()
        || panels.node_editor.popup_open() || panels.layer_actions.modal_in(owner)
        || panels.layer_actions.text_in(owner) || panels.layer_actions.modal_in(Some(Panel::Timeline))
        || panels.layer_actions.text_in(Some(Panel::Timeline))
    {
        panels.property_reveal.interrupt();
    }
    super::property_focus::settle(panels);
    if let Some(search) = &panels.effect_search {
        let owned = search.owner == owner;
        if effect_search_close_pressed(input) || owned && input.focus_lost {
            panels.effect_search = None;
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
            if text_active(panels) && !modifiers.control {continue;}
            let request = match panels.shortcut_bindings.resolve(Scope::Nodes, key, modifiers, repeat) {
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
        && !text_active(panels)
        && input.key_presses().any(|(k,m,r)| panels.shortcut_bindings.resolve(Scope::Application,k,m,r)==Some(Command::ReloadStyle))
    {
        out.push(Action::ReloadStyle);
        return true;
    }
    if text_active(panels) || input.composing || input.ime_consumed {
        return false;
    }
    // Legacy property handlers still consume J/K/Delete-shaped commands; adapt only
    // their input, after focus/IME ownership, using the effective user mapping.
    let adapted = panels.shortcut_bindings.property_input(input);
    let property_input = adapted.as_ref().unwrap_or(input);
    let effects_owned = super::property_focus::navigation_scope(panels.focused)
        && (panels.effect_controls.has_key_channel() || panels.properties.effects.has_key_channel());
    let effects_navigated = effects_owned && if panels.effect_controls.has_key_channel() {
        panels.effect_controls.navigate_keys(state, panels.selected, property_input, out)
    } else {panels.properties.effects.navigate_keys(state, panels.selected, property_input, out)};
    if effects_navigated || effects_owned && property_input.key_presses().any(|(key, modifiers, _)|
        matches!(key, Key::J | Key::K) && !modifiers.control && !modifiers.alt && !modifiers.shift) {
        return true;
    }
    let property_navigation = super::property_focus::navigation_scope(panels.focused)
        && property_input.key_presses().any(|(key, modifiers, _)| matches!(key, Key::J | Key::K)
            && !modifiers.control && !modifiers.alt && !modifiers.shift);
    if (property_navigation || matches!(
        panels.focused,
        Some(Panel::Masks | Panel::Properties | Panel::Vector | Panel::Timeline | Panel::Curves)
    )) && panels
        .path_controls
        .mask_key_shortcuts(state, panels.selected, property_input, out)
    {
        return true;
    }
    if node_context && !panels.node_editor.editing() {
        for (key, modifiers, repeat) in input.key_presses() {
            let command = panels.shortcut_bindings.resolve(Scope::Nodes, key, modifiers, repeat);
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
    if shortcuts::effect_search_pressed(input) && !panels.node_editor.editing() {
        panels.effect_search = Some(EffectSearch {
            position: input.mouse,
            query: String::new(),
            selected_index: 0,
            owner,
            continuous: false,
        });
        return true;
    }
    if panels.menu.is_none()
        && panels.panel_menu.is_none()
        && panels.timeline.timeline_context_menu.is_none()
    {
        if node_context {
            if !panels.node_editor.editing() {
                if input.key_presses().any(|(k,m,r)| panels.shortcut_bindings.resolve(Scope::Nodes,k,m,r)==Some(Command::Play)) {
                    panels.node_preview.toggle();
                    return true;
                }
                handle_context_shortcuts(input, panels, state, out, true, owner);
            }
        } else {
            handle_context_shortcuts(input, panels, state, out, false, owner);
        }
    }
    false
}

#[cfg(test)]
pub fn handle_shortcuts(input: &Input, panels: &mut Panels, state: &Value, out: &mut Vec<Action>) {
    handle_context_shortcuts(input, panels, state, out, false, None);
}

fn handle_context_shortcuts(
    input: &Input,
    panels: &mut Panels,
    state: &Value,
    out: &mut Vec<Action>,
    node_context: bool,
    owner: Option<Panel>,
) {
    let mut frame = state["frame"].as_f64().unwrap_or(0.0);
    for (key, modifiers, repeat) in input.key_presses() {
        let property_reveal = panels.shortcut_bindings.resolve(
            Scope::Animation, key, modifiers, false,
        ) == Some(Command::AnimatedProperties);
        if property_reveal && repeat {
            panels.property_reveal.interrupt();
            continue;
        }
        if !property_reveal {
            panels.property_reveal.interrupt();
        }
        // Document edits and canvas navigation remain owned by the node scope.
        // These application actions have no competing node action.
        if node_context {
            let allowed = match panels.shortcut_bindings.resolve(Scope::Application, key, modifiers, repeat) {
                Some(
                    Command::NewProject
                    | Command::NewComposition
                    | Command::Import
                    | Command::Settings
                    | Command::Preferences
                    | Command::Export
                    | Command::Solid
                    | Command::Text
                    | Command::Null
                    | Command::Adjustment
                    | Command::LastFrame,
                ) => true,
                Some(Command::PreviousFrame | Command::NextFrame) => {
                    !matches!(key, Key::Left | Key::Right) || modifiers.control
                }
                _ => false,
            };
            if !allowed {
                continue;
            }
        }
        if panels.shortcut_bindings.resolve(Scope::Application,key,modifiers,repeat)
            == Some(Command::SelectAll)
            && super::graph_box::select_all(panels, state)
        {
            continue;
        }
        let key_owned = matches!(panels.focused, Some(Panel::Timeline | Panel::Curves))
            && panels.animation_controls.user_key().is_some()
            || matches!(panels.focused, Some(Panel::Timeline | Panel::Curves | Panel::Properties
                | Panel::EffectControls | Panel::Masks | Panel::Vector | Panel::Audio))
                && panels.animation_controls.track_selection(state, panels.selected)
                    == super::animation_controls::TrackSelection::Invalid;
        if let Some(mut command) = panels.shortcut_bindings.resolve(Scope::Animation, key, modifiers, repeat) {
            if command == Command::AnimatedProperties && matches!(panels.focused,
                Some(Panel::Timeline | Panel::Curves | Panel::Properties | Panel::Composition)) {
                use std::hash::{Hash, Hasher};
                let mut context = std::collections::hash_map::DefaultHasher::new();
                state["root"].as_str().hash(&mut context);
                state["composition"].as_str().hash(&mut context);
                panels.selected.hash(&mut context);
                panels.selected_layers.hash(&mut context);
                panels.focused.map(|panel| panel.dock().0).hash(&mut context);
                owner.map(|panel| panel.dock().0).hash(&mut context);
                if panels.property_reveal.press(context.finish(), std::time::Instant::now()) {
                    command = Command::ModifiedProperties;
                }
            }

            if !input.focus_lost && !layout_shortcuts_blocked(panels)
                && matches!(panels.focused, Some(Panel::Timeline | Panel::Composition)) {
                use super::layer_navigation::Order;
                let order = match command {
                    Command::OrderLayersUp => Some(Order::Up),
                    Command::OrderLayersDown => Some(Order::Down),
                    Command::OrderLayersTop => Some(Order::Top),
                    Command::OrderLayersBottom => Some(Order::Bottom),
                    _ => None,
                };
                if let Some(order) = order {
                    out.push(Action::OrderLayers { objects: panels.selection_ids(), order });
                    continue;
                }
                if panels.focused == Some(Panel::Timeline)
                    && matches!(command, Command::SelectLayerAbove | Command::SelectLayerBelow)
                {
                    super::layer_navigation::navigate(panels, state,
                        command == Command::SelectLayerBelow, modifiers.shift);
                    continue;
                }
            }
            if super::timeline_zoom::shortcut(command, input, panels, state) {
                continue;
            }
            if super::audio::shortcut(command, panels, state, out) {
                continue;
            }
            if command == Command::Delete && super::graph_box::delete(panels, state, out) {
                continue;
            }
            if super::timeline_shortcuts::handle(command, modifiers, panels, state, &mut frame, out)
                || key_owned && command == Command::Delete
            {
                continue;
            }
        }
        let Some(command) = panels.shortcut_bindings.resolve(Scope::Application, key, modifiers, repeat) else {
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
            Command::Preferences => Action::Preferences,
            Command::Export if cfg!(feature = "ffmpeg") => Action::ExportMovie,
            Command::Export => {
                static REPORTED: std::sync::Once = std::sync::Once::new();
                REPORTED.call_once(|| {
                    log::warn!("shortcut ignored operation=export reason=ffmpeg_feature_disabled")
                });
                continue;
            }
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
    fn search_state(owner: Option<Panel>) -> EffectSearch {
        EffectSearch {
            position: [20., 20.],
            query: "glow".into(),
            selected_index: 0,
            owner,
            continuous: false,
        }
    }
    #[test]
    fn effect_search_blocks_only_its_owner_and_other_window_can_save() {
        for (owner, other) in [
            (Some(Panel::NodeProperties), None),
            (None, Some(Panel::Info)),
        ] {
            let mut panels = Panels {
                effect_search: Some(search_state(owner)),
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
            assert_eq!(panels.effect_search.as_ref().unwrap().owner, owner);
        }
    }
    #[test]
    fn escape_and_toggle_from_either_window_close_and_consume_the_frame() {
        for (owner, other) in [
            (Some(Panel::NodeProperties), None),
            (None, Some(Panel::Info)),
        ] {
            for chord in [KeyChord::new(Key::Escape, false, false, false), EFFECT_SEARCH, EFFECT_SEARCH_ALT] {
                let mut panels = Panels {
                    effect_search: Some(search_state(owner)),
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
                assert!(panels.effect_search.is_none());
                assert!(out.is_empty());
            }
        }
    }
    #[test]
    fn unrelated_focus_loss_keeps_effect_search_and_owner_focus_loss_closes_it() {
        let owner = Some(Panel::NodeProperties);
        let mut panels = Panels {
            effect_search: Some(search_state(owner)),
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
        assert!(panels.effect_search.is_some());
        assert!(route(
            &input,
            &mut panels,
            &Value::Null,
            false,
            owner,
            &mut out
        ));
        assert!(panels.effect_search.is_none());
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
            keys: vec![(Key::Space, EFFECT_SEARCH.modifiers)],
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
            panels.effect_search.as_ref().unwrap().owner,
            Some(Panel::NodeProperties)
        );
        route(&input, &mut panels, &Value::Null, true, None, &mut out);
        assert!(panels.effect_search.is_none());
        assert!(out.is_empty());
    }
    #[test]
    fn modal_has_priority_in_nodes_and_its_closing_frame_is_consumed() {
        let mut panels = Panels::default();
        panels.node_editor.focus_panel();
        let input = Input {
            keys: vec![(Key::Space, EFFECT_SEARCH.modifiers)],
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
        assert!(panels.effect_search.is_some());
        assert!(route(
            &input,
            &mut panels,
            &Value::Null,
            true,
            None,
            &mut out
        ));
        assert!(panels.effect_search.is_none());
        assert!(out.is_empty());
    }
    #[test]
    fn search_and_repeat_do_not_open_modal() {
        let mut panels = Panels::default();
        panels.project.search = true;
        let mut out = vec![];
        let mut input = Input {
            keys: vec![(Key::Space, EFFECT_SEARCH.modifiers)],
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
        assert!(panels.effect_search.is_none());
        panels.project.search = false;
        input.key_repeats = vec![true];
        route(&input, &mut panels, &Value::Null, false, None, &mut out);
        assert!(panels.effect_search.is_none());
        assert!(out.is_empty());
    }
}

#[cfg(test)]
#[path = "keyboard_node_tests.rs"]
mod node_scope_tests;

#[cfg(test)]
#[path = "keyboard_search_tests.rs"]
mod search_chord_tests;
