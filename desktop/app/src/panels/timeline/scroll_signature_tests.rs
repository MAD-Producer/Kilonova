use super::*;

fn signature(state: &types::TimelineState) -> u64 {
    scroll_extent_signature(&("project".into(), "main".into(), 1), state, "", 180., 22.)
}

#[test]
fn group_lengths_separate_equal_flat_id_streams() {
    let mut effects = types::TimelineState::default();
    effects.expanded_effects.extend([7, 8]);
    let ids: Vec<_> = effects.expanded_effects.iter().copied().collect();
    let mut items = types::TimelineState::default();
    items.collapsed_effect_items.insert((ids[0], ids[1]));
    assert_ne!(signature(&effects), signature(&items));

    let mut effects = types::TimelineState::default();
    effects.expanded_effects.insert(7);
    let mut audio = types::TimelineState::default();
    audio.collapsed_audio.insert(7);
    assert_ne!(signature(&effects), signature(&audio));
}

#[test]
fn all_row_groups_and_viewport_identity_inputs_invalidate_the_signature() {
    let empty = types::TimelineState::default();
    let baseline = signature(&empty);
    for changed in 0..10 {
        let mut state = types::TimelineState::default();
        match changed {
            0 => {
                state.expanded.insert(7);
            }
            1 => {
                state.collapsed_transforms.insert(7);
            }
            2 => {
                state.expanded_effects.insert(7);
            }
            3 => {
                state.collapsed_effect_items.insert((7, 1));
            }
            4 => {
                state.collapsed_audio.insert(7);
            }
            5 => {
                state.collapsed_masks.insert(7);
            }
            6 => {
                state.collapsed_mask_items.insert((7, 1));
            }
            7 => {
                state.vector_open.insert((7, 1, 0));
            }
            8 => {
                state.filters.insert(7, types::PropertyFilter::Animated);
            }
            _ => {
                state
                    .expression_rows
                    .push(motion_core::ExpressionTarget::Property {
                        object: 7,
                        property: Property::Position,
                        axis: None,
                    });
            }
        }
        assert_ne!(signature(&state), baseline, "input={changed}");
        assert_eq!(signature(&state), signature(&state));
    }
    for (root, comp, revision, query, area, row) in [
        ("other", "main", 1, "", 180., 22.),
        ("project", "other", 1, "", 180., 22.),
        ("project", "main", 2, "", 180., 22.),
        ("project", "main", 1, "title", 180., 22.),
        ("project", "main", 1, "", 240., 22.),
        ("project", "main", 1, "", 180., 32.),
    ] {
        assert_ne!(
            scroll_extent_signature(
                &(root.into(), comp.into(), revision),
                &empty,
                query,
                area,
                row
            ),
            baseline
        );
    }
}
