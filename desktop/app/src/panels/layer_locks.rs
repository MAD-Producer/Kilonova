//! Lock changes preserve visibility and submit one atomic, composition-scoped batch.
use super::*;

pub(super) fn changes(state: &Value, ids: Option<&[u64]>, locked: bool) -> Vec<Command> {
    if state["observing"] == true {
        return vec![];
    }
    let composition = state["composition"]
        .as_str()
        .or_else(|| state["project"]["composition_id"].as_str());
    state["project"]["layers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|layer| {
            layer["id"]
                .as_u64()
                .is_some_and(|id| ids.is_none_or(|ids| ids.contains(&id)))
        })
        .filter(|layer| (layer["locked"] == true) != locked)
        .filter_map(|layer| {
            let command = Command::Flags {
                object: layer["id"].as_u64()?,
                visible: layer["visible"] != false,
                locked,
            };
            Some(if let Some(composition) = composition {
                Command::InComposition {
                    composition: composition.into(),
                    command: Box::new(command),
                }
            } else {
                command
            })
        })
        .collect()
}

/// A mixed selection locks every selected layer. An entirely locked one unlocks.
pub(super) fn selection(state: &Value, panels: &Panels) -> (bool, Vec<Command>) {
    let ids = panels.selection_ids();
    let lock = state["project"]["layers"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|layer| {
            layer["id"].as_u64().is_some_and(|id| ids.contains(&id)) && layer["locked"] != true
        });
    (lock, changes(state, Some(&ids), lock))
}
