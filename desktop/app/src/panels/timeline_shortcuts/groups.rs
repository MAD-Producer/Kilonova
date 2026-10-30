//! Disclosure changes only window-owned state, including on locked layers.
use super::*;

pub(super) fn toggle(panels: &mut Panels, state: &Value, effects: bool) {
    let objects: Vec<_> = panels
        .selection_ids()
        .into_iter()
        .filter(|object| {
            visual_layer(state, *object).is_some_and(|layer| {
                layer["content"]["kind"] != "camera"
                    && layer[if effects { "effects" } else { "masks" }]
                        .as_array()
                        .is_some_and(|items| !items.is_empty())
            })
        })
        .collect();
    if objects.is_empty() {
        return;
    }
    // A property/animated filter or a collapsed item can hide the requested
    // channels even when the group's disclosure flag is open.
    let all_open = objects.iter().all(|object| {
        panels.timeline.expanded.contains(object)
            && panels
                .timeline
                .filters
                .get(object)
                .copied()
                .unwrap_or_default()
                == PropertyFilter::All
            && if effects {
                panels.timeline.expanded_effects.contains(object)
                    && !panels
                        .timeline
                        .collapsed_effect_items
                        .iter()
                        .any(|(id, _)| id == object)
            } else {
                !panels.timeline.collapsed_masks.contains(object)
                    && !panels
                        .timeline
                        .collapsed_mask_items
                        .iter()
                        .any(|(id, _)| id == object)
            }
    });
    for object in &objects {
        if all_open {
            if effects {
                panels.timeline.expanded_effects.remove(object);
            } else {
                panels.timeline.collapsed_masks.insert(*object);
            }
        } else {
            panels.timeline.expanded.insert(*object);
            panels.timeline.filters.remove(object);
            if effects {
                panels.timeline.expanded_effects.insert(*object);
                panels
                    .timeline
                    .collapsed_effect_items
                    .retain(|(id, _)| id != object);
            } else {
                panels.timeline.collapsed_masks.remove(object);
                panels
                    .timeline
                    .collapsed_mask_items
                    .retain(|(id, _)| id != object);
            }
        }
    }
    if !all_open {
        panels.timeline.reveal_layer = objects.first().copied();
    }
}
