//! Follow timing edits from existing single-key/group controllers. This does
//! not implement multi-key dragging; the collection only follows keys actually
//! moved by their commands, and rolls back with the existing gesture.
use super::*;
use crate::panels::{Action, Panels};
use motion_core::{
    masks::MaskAction,
    vector::{groups::KeyEdit, VectorAction},
    AudioLevelsAction, Command, EffectAction,
};

fn moved(command: &Command, target: &KeyframeTarget) -> Option<(u32, u32)> {
    Some(match (command, target) {
        (
            Command::MoveKey {
                object,
                property,
                axis,
                from,
                to,
            },
            KeyframeTarget::Property {
                object: o,
                property: p,
                axis: a,
            },
        ) if (object, property, axis) == (o, p, a) => (*from, *to),
        (
            Command::Mask {
                object,
                action:
                    MaskAction::MoveKey {
                        mask,
                        property,
                        from,
                        to,
                    },
            },
            KeyframeTarget::Mask {
                object: o,
                mask: m,
                property: p,
            },
        ) if (object, mask, property) == (o, m, p) => (*from, *to),
        (
            Command::Vector {
                object,
                action:
                    VectorAction::EditGroupKey {
                        item,
                        parameter,
                        edit: KeyEdit::Move { from, to },
                    },
            },
            KeyframeTarget::Vector {
                object: o,
                item: i,
                parameter: p,
            },
        ) if object == o && parameter == p && i.is_none_or(|i| i == *item) => (*from, *to),
        (
            Command::Effect {
                object,
                action:
                    EffectAction::MoveKey {
                        effect,
                        param,
                        from,
                        to,
                    },
            },
            KeyframeTarget::Effect {
                object: o,
                effect: e,
                param: p,
            },
        ) if (object, effect, param) == (o, e, p) => (*from, *to),
        (
            Command::AudioLevels {
                object,
                action: AudioLevelsAction::MoveKey { channel, from, to },
            },
            KeyframeTarget::Audio {
                object: o,
                channel: c,
            },
        ) if (object, channel) == (o, c) => (*from, *to),
        _ => return None,
    })
}
fn command(set: &mut Set, edit: &Command) {
    let mut changed = false;
    for channel in set.channels.values_mut() {
        if let Some((from, to)) = channel.target.as_ref().and_then(|t| moved(edit, t)) {
            if channel.frames.remove(&from) {
                channel.frames.insert(to);
                changed = true;
            }
        }
    }
    if let Some(primary) = &mut set.primary {
        if let Some((from, to)) = moved(edit, &primary.target) {
            if primary.frame == from {
                primary.frame = to;
            }
        }
    }
    if changed {
        set.validated.set(None);
        set.intent = true;
    }
}
pub(super) fn follow(panels: &mut Panels, state: &Value, actions: &[Action]) {
    if !panels.selected_keys.owns_keys() {
        panels.key_gestures.transaction = None;
        return;
    }
    for action in actions {
        match action {
            Action::History(2) if panels.selected_keys.scope == Scope::of(state) => {
                if panels.key_gestures.transaction.is_none() {
                    panels.key_gestures.transaction = Some(panels.selected_keys.clone());
                }
            }
            Action::History(3) => panels.key_gestures.transaction = None,
            Action::History(4) => {
                if let Some(mut before) = panels.key_gestures.transaction.take() {
                    before.intent = true;
                    panels.selected_keys = before;
                }
            }
            Action::Edit(edit) if panels.selected_keys.scope == Scope::of(state) => {
                command(&mut panels.selected_keys, edit)
            }
            Action::EditBatch(edits) if panels.selected_keys.scope == Scope::of(state) => {
                for edit in edits {
                    command(&mut panels.selected_keys, edit);
                }
            }
            _ => {}
        }
    }
}
