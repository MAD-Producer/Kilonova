//! One all-or-nothing eligibility rule for both effect-removal menu hosts.
use super::*;
use motion_core::EffectAction;

pub(super) fn commands<'a>(
    state: &'a Value,
    objects: &'a [u64],
) -> Option<impl Iterator<Item = Command> + 'a> {
    if state["observing"] == true {
        return None;
    }
    let layers = state["project"]["layers"].as_array()?;
    for (index, object) in objects.iter().enumerate() {
        if objects[..index].contains(object) {
            return None;
        }
        let layer = layers
            .iter()
            .find(|layer| layer["id"].as_u64() == Some(*object))?;
        if layer["locked"] == true {
            return None;
        }
        if !layer["effects"].is_null() {
            let effects = layer["effects"].as_array()?;
            if effects.iter().any(|effect| effect["id"].as_u64().is_none()) {
                return None;
            }
        }
    }
    Some(objects.iter().flat_map(move |object| {
        layers
            .iter()
            .find(|layer| layer["id"].as_u64() == Some(*object))
            .and_then(|layer| layer["effects"].as_array())
            .into_iter()
            .flatten()
            .filter_map(move |effect| {
                effect["id"].as_u64().map(|effect| Command::Effect {
                    object: *object,
                    action: EffectAction::Remove { effect },
                })
            })
    }))
}

pub(super) fn menu_entry(state: &Value, panels: &Panels) -> (String, Action, bool) {
    let objects = panels.selection_ids();
    let Some(commands) = commands(state, &objects) else {
        return (
            if panels.locale == Locale::Zh {
                "移除所选图层全部效果"
            } else {
                "Remove selected layers' effects"
            }
            .into(),
            Action::EditBatch(Vec::new()),
            false,
        );
    };
    let commands = commands.collect::<Vec<_>>();
    let count = commands.len();
    let label = if panels.locale == Locale::Zh {
        format!("移除所选{}层全部{}项效果", objects.len(), count)
    } else {
        format!(
            "Remove all: {} selected layers / {count} effects",
            objects.len()
        )
    };
    (label, Action::EditBatch(commands), count > 0)
}
