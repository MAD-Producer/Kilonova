use super::*;
use crate::panels::{keyboard, Panel};
use motion_ui::input::{Input, Key, Modifiers};

#[cfg(target_os = "windows")]
mod native;

fn state(count: u64) -> Value {
    serde_json::json!({"frame":12,"project":{"frames":120,"layers":(1..=count)
        .map(|id|serde_json::json!({"id":id,"name":format!("Layer {}",id%2),"locked":false}))
        .collect::<Vec<_>>()}})
}
fn apply(mut ids: Vec<u64>, commands: Vec<Command>) -> Vec<u64> {
    for command in commands {
        let Command::Reorder { object, index } = command else {
            panic!("expected reorder")
        };
        let old = ids.iter().position(|id| *id == object).unwrap();
        ids.remove(old);
        ids.insert(index, object);
    }
    ids
}

#[test]
fn every_small_selection_preserves_both_relative_orders_and_moves_one_neighbor_or_to_edge() {
    for count in 1..=7 {
        let state = state(count);
        let original: Vec<_> = (1..=count).collect();
        for bits in 1..1u64 << count {
            let selected: Vec<_> = original
                .iter()
                .copied()
                .filter(|id| bits & (1 << (id - 1)) != 0)
                .collect();
            let others: Vec<_> = original
                .iter()
                .copied()
                .filter(|id| !selected.contains(id))
                .collect();
            for order in [Order::Up, Order::Down, Order::Top, Order::Bottom] {
                let commands = plan(&state, &selected, order).unwrap();
                assert!(commands.iter().all(|command| matches!(command, Command::Reorder {object,..} if selected.contains(object))));
                let actual = apply(original.clone(), commands);
                assert_eq!(
                    actual
                        .iter()
                        .copied()
                        .filter(|id| selected.contains(id))
                        .collect::<Vec<_>>(),
                    selected
                );
                assert_eq!(
                    actual
                        .iter()
                        .copied()
                        .filter(|id| !selected.contains(id))
                        .collect::<Vec<_>>(),
                    others
                );
                for (a, id) in selected.iter().enumerate() {
                    let old_below = original
                        .iter()
                        .take_while(|item| *item != id)
                        .filter(|item| !selected.contains(item))
                        .count();
                    let new_below = actual
                        .iter()
                        .take_while(|item| *item != id)
                        .filter(|item| !selected.contains(item))
                        .count();
                    match order {
                        Order::Up => assert_eq!(new_below, (old_below + 1).min(others.len())),
                        Order::Down => assert_eq!(new_below, old_below.saturating_sub(1)),
                        Order::Top => assert_eq!(actual[others.len() + a], *id),
                        Order::Bottom => assert_eq!(actual[a], *id),
                    }
                }
            }
        }
    }
}

#[test]
fn navigation_uses_filtered_layer_headers_and_range_anchor_survives_reversal() {
    let state = state(7);
    let mut panels = Panels::default();
    panels.timeline.timeline_query = "LAYER 1".into();
    panels.timeline.expanded.extend([7, 5, 3]);
    navigate(&mut panels, &state, true, false);
    assert_eq!(panels.selected, Some(7));
    navigate(&mut panels, &state, true, true);
    navigate(&mut panels, &state, true, true);
    assert_eq!(panels.selected, Some(3));
    assert_eq!(panels.selected_layers, [7, 5, 3]);
    assert_eq!(panels.selection_anchor, Some(7));
    navigate(&mut panels, &state, false, true);
    assert_eq!(panels.selected_layers, [7, 5]);
    assert_eq!(panels.timeline.reveal_layer, Some(5));
    navigate(&mut panels, &state, false, false);
    assert_eq!(panels.selection_ids(), [7]);
    navigate(&mut panels, &state, false, false);
    assert_eq!(panels.selection_ids(), [7]);
    assert_eq!(panels.timeline.expanded.len(), 3);
    panels.select_only(Some(6));
    navigate(&mut panels, &state, true, true);
    assert_eq!(panels.selection_ids(), [7]);
    panels.timeline.timeline_query = "no match".into();
    navigate(&mut panels, &state, true, false);
    assert_eq!(panels.selection_ids(), [7]);
}

#[test]
fn read_only_locked_stale_empty_and_order_boundaries_do_not_create_partial_batches() {
    let mut value = state(4);
    assert!(matches!(
        plan(&value, &[], Order::Top),
        Err(Unavailable::Empty)
    ));
    assert!(matches!(
        plan(&value, &[9], Order::Top),
        Err(Unavailable::Stale)
    ));
    assert!(plan(&value, &[4], Order::Top).unwrap().is_empty());
    assert!(plan(&value, &[1, 2], Order::Bottom).unwrap().is_empty());
    value["project"]["layers"][1]["locked"] = true.into();
    assert!(matches!(
        plan(&value, &[1, 2], Order::Top),
        Err(Unavailable::Locked)
    ));
    assert_eq!(
        apply(vec![1, 2, 3, 4], plan(&value, &[1], Order::Top).unwrap()),
        [2, 3, 4, 1]
    );
    value["observing"] = true.into();
    assert!(matches!(
        plan(&value, &[1], Order::Top),
        Err(Unavailable::ReadOnly)
    ));
    assert!(matches!(
        plan(&serde_json::json!({}), &[1], Order::Top),
        Err(Unavailable::InvalidSnapshot)
    ));
}

#[test]
fn keyboard_keeps_time_navigation_text_and_node_focus_separate_in_main_and_float() {
    for owner in [None, Some(Panel::Timeline)] {
        for locale in [Locale::En, Locale::Zh] {
            let mut panels = Panels {
                focused: Some(Panel::Timeline),
                locale,
                ..Default::default()
            };
            panels.select_only(Some(4));
            let value = state(5);
            let input = Input {
                keys: vec![(
                    Key::Down,
                    Modifiers {
                        control: true,
                        ..Default::default()
                    },
                )],
                ..Default::default()
            };
            let mut out = vec![];
            keyboard::route(&input, &mut panels, &value, false, owner, &mut out);
            assert_eq!(panels.selected, Some(3));
            assert!(out.is_empty());
            let mut input = input.clone();
            input.keys[0].1.alt = true;
            keyboard::route(&input, &mut panels, &value, false, owner, &mut out);
            assert!(
                matches!(out.as_slice(),[Action::OrderLayers {objects,order:Order::Down}] if objects==&vec![3])
            );
            out.clear();
            panels.timeline.timeline_search = true;
            keyboard::route(&input, &mut panels, &value, false, owner, &mut out);
            assert!(out.is_empty());
            panels.timeline.timeline_search = false;
            keyboard::route(&input, &mut panels, &value, true, owner, &mut out);
            assert!(out.is_empty());
            assert_eq!(panels.selected, Some(3));
        }
    }
}

#[test]
fn real_batch_is_one_undo_preserves_selection_order_and_survives_save_reopen() {
    let mut project = motion_core::Project::new(320, 180, 30, 120).unwrap();
    for id in 1..=5 {
        project.layers.push(motion_core::Layer::solid(
            id, "Layer", [16.; 2], [0.; 3], [1.; 4],
        ));
    }
    let mut engine = motion_core::Engine::new(project).unwrap();
    let before = engine.project().clone();
    let before_history = engine.history_diagnostics();
    let value = serde_json::json!({"project":engine.project()});
    // The primary can be in any selection position; stack order governs output.
    engine
        .apply_batch(plan(&value, &[3, 1], Order::Top).unwrap())
        .unwrap();
    assert_eq!(
        engine
            .project()
            .layers
            .iter()
            .map(|layer| layer.id)
            .collect::<Vec<_>>(),
        [2, 4, 5, 1, 3]
    );
    assert_eq!(
        engine.history_diagnostics()["undoEntries"]
            .as_u64()
            .unwrap(),
        before_history["undoEntries"].as_u64().unwrap() + 1
    );
    let after = engine.project().clone();
    let directory = tempfile::tempdir().unwrap();
    motion_core::storage::save(directory.path(), &after).unwrap();
    assert_eq!(motion_core::storage::load(directory.path()).unwrap(), after);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &before);
    assert!(engine.redo().unwrap());
    assert_eq!(engine.project(), &after);
    let mut commands = plan(
        &serde_json::json!({"project":engine.project()}),
        &[1, 3],
        Order::Bottom,
    )
    .unwrap();
    commands.push(Command::Reorder {
        object: 99,
        index: 0,
    });
    assert!(engine.apply_batch(commands).is_err());
    assert_eq!(
        engine.project(),
        &after,
        "failed batch must not leave the first layer moved"
    );
}

#[test]
fn navigation_reveals_headers_with_expanded_properties_without_modifying_the_document() {
    let mut project = motion_core::Project::new(320, 180, 30, 120).unwrap();
    for id in 1..=12 {
        project.layers.push(motion_core::Layer::solid(
            id, "Layer", [16.; 2], [0.; 3], [1.; 4],
        ));
    }
    let before = project.clone();
    let value = serde_json::json!({"project":project,"revision":1,"frame":0});
    let mut panels = Panels::default();
    panels.timeline.expanded.extend(1..=12);
    panels.select_only(Some(2));
    navigate(&mut panels, &value, true, false);
    assert_eq!(panels.selected, Some(1));
    let mut atlas = motion_ui::text::TextAtlas::from_system(12.).unwrap();
    let mut render = |panels: &mut Panels| {
        let mut paint = motion_ui::paint::PaintList::default();
        let mut actions = vec![];
        crate::panels::timeline::render(
            &mut paint,
            &mut atlas,
            motion_ui::Rect::new(0., 0., 1200., 320.),
            panels,
            &value,
            &Input::default(),
            &mut actions,
        );
        assert!(actions.is_empty());
    };
    render(&mut panels);
    assert!(panels.timeline.scroll > 0.);
    assert!(panels.timeline.reveal_layer.is_none());
    panels.select_only(Some(11));
    navigate(&mut panels, &value, false, false);
    render(&mut panels);
    assert_eq!(panels.selected, Some(12));
    assert_eq!(panels.timeline.scroll, 0.);
    assert_eq!(
        serde_json::from_value::<motion_core::Project>(value["project"].clone()).unwrap(),
        before
    );
    panels.selection_context = Some(("old".into(), "comp-main".into()));
    panels.timeline.reveal_layer = Some(12);
    panels.normalize_selection(
        &serde_json::json!({"root":"new","composition":"comp-main","project":before}),
    );
    assert!(panels.timeline.reveal_layer.is_none());
}
