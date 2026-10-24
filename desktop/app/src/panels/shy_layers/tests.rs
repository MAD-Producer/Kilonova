use super::*;
use motion_core::{Engine, Layer, Project};

#[cfg(target_os = "windows")]
mod native;

fn fixture() -> Engine {
    let mut p = Project::new(640, 360, 30, 120).unwrap();
    for id in 1..=5 {
        let mut layer = Layer::solid(
            id,
            &format!("Layer {id}"),
            [60.; 2],
            [320., 180., 0.],
            [0.2, 0.5, 0.8, 1.],
        );
        layer.shy = id == 2 || id == 4;
        p.layers.push(layer);
    }
    Engine::new(p).unwrap()
}
fn snapshot(engine: &Engine) -> Value {
    serde_json::json!({"root":"shy-test","composition":"comp-main","revision":engine.revision(),"frame":0.,"project":engine.project(),"timeline_layers":engine.project().timeline_layers(0.).unwrap()})
}
fn press(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        pressed: Some((MouseButton::Left, point)),
        ..Default::default()
    }
}

#[test]
fn hidden_selection_navigation_range_search_and_reorder_use_actual_layer_ids() {
    let engine = fixture();
    let state = snapshot(&engine);
    let mut panels = Panels::default();
    panels.timeline.hide_shy = true;
    panels.select_only(Some(4));
    panels.selected_layers = vec![3, 4, 2];
    panels.selection_anchor = Some(2);
    reconcile_selection(&mut panels, &state);
    assert_eq!(panels.selected, Some(3));
    assert_eq!(panels.selected_layers, [3]);
    assert_eq!(panels.selection_anchor, Some(3));
    super::super::layer_navigation::navigate(&mut panels, &state, true, true);
    assert_eq!(panels.selected, Some(1));
    assert_eq!(panels.selected_layers, [3, 1]);
    panels.timeline.timeline_query = "Layer 2".into();
    super::super::layer_navigation::navigate(&mut panels, &state, true, false);
    assert_eq!(panels.selected, Some(1));
    panels.timeline.hide_shy = false;
    super::super::layer_navigation::navigate(&mut panels, &state, true, false);
    assert_eq!(panels.selected, Some(2));
    let index = reorder_index(&[5, 4, 3, 2, 1], &[5, 3, 1], 1, 1).unwrap();
    let mut storage = vec![1, 2, 3, 4, 5];
    let moving = storage.remove(0);
    storage.insert(index, moving);
    assert_eq!(storage, [2, 3, 1, 4, 5]); // above 3, hidden 2/4 retain order
    assert_eq!(reorder_index(&[5, 4, 3, 2, 1], &[5, 3, 1], 5, 3), Some(0));
    assert_eq!(reorder_index(&[5, 4, 3, 2, 1], &[5, 3, 1], 3, 1), None);
}

#[test]
fn canvas_can_select_shy_layers_but_timeline_select_all_only_addresses_visible_search_results() {
    let state = snapshot(&fixture());
    let mut panels = Panels::default();
    panels.timeline.hide_shy = true;
    panels.focused = Some(Panel::Composition);
    panels.select_only(Some(2));
    assert!(!reconcile_selection(&mut panels, &state));
    assert_eq!(panels.selected, Some(2));
    panels.focused = Some(Panel::Timeline);
    panels.timeline.timeline_query = "Layer 3".into();
    let input = Input {
        keys: vec![(
            Key::A,
            motion_ui::input::Modifiers {
                control: true,
                ..Default::default()
            },
        )],
        ..Default::default()
    };
    let mut actions = vec![];
    super::super::keyboard::handle_shortcuts(&input, &mut panels, &state, &mut actions);
    assert!(actions.is_empty());
    assert_eq!(panels.selected_layers, [3]);
    panels.timeline.timeline_query.clear();
    super::super::keyboard::handle_shortcuts(&input, &mut panels, &state, &mut actions);
    assert_eq!(panels.selected_layers, [1, 3, 5]);
    panels.focused = Some(Panel::Composition);
    super::super::keyboard::handle_shortcuts(&input, &mut panels, &state, &mut actions);
    assert_eq!(panels.selected_layers, [1, 2, 3, 4, 5]);
}

#[test]
fn shy_row_dispatches_one_checked_edit_and_hidden_extent_recovers_on_show() {
    let mut engine = fixture();
    let mut state = snapshot(&engine);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut panels = Panels::default();
    let area = Rect::new(0., 0., 1200., 300.);
    let render = |panels: &mut Panels, atlas: &mut TextAtlas, state: &Value, input: &Input| {
        let mut list = PaintList::default();
        let mut out = vec![];
        super::super::timeline::render(&mut list, atlas, area, panels, state, input, &mut out);
        (list, out)
    };
    let (list, _) = render(&mut panels, &mut atlas, &state, &Input::default());
    let columns = ColumnRects::with_sampling(
        &list.styles(),
        0.,
        624.,
        panels.timeline.column_visibility,
        panels.timeline.sampling_visibility,
        false,
    );
    let shy = columns.shy.unwrap();
    let styles = list.styles();
    let point = [shy.x + shy.width * 0.5,
        super::super::timeline::header_height(&styles) + styles.metric("row.height", 22.) * 0.5]; // top row is layer 5
    let (_, actions) = render(&mut panels, &mut atlas, &state, &press(point));
    assert!(matches!(
        actions.as_slice(),
        [Action::EditChecked {
            command: Command::SetLayerShy {
                object: 5,
                shy: true
            },
            revision: 0
        }]
    ));
    for action in actions {
        if let Action::EditChecked { command, .. } = action {
            engine.apply(command).unwrap();
        }
    }
    state = snapshot(&engine);
    panels.timeline.hide_shy = true;
    panels.select_only(Some(5));
    render(&mut panels, &mut atlas, &state, &Input::default());
    assert_eq!(panels.selected, None);
    let hidden = panels.timeline.scroll_extent_cache.unwrap().0;
    panels.timeline.hide_shy = false;
    render(&mut panels, &mut atlas, &state, &Input::default());
    assert_ne!(panels.timeline.scroll_extent_cache.unwrap().0, hidden);
    assert!(engine.undo().unwrap());
    assert!(!engine.project().layers[4].shy);
    let mut workspace = crate::workspace::Workspace::default();
    workspace.timeline_hide_shy = true;
    let encoded = serde_json::to_vec(&workspace).unwrap();
    assert!(
        serde_json::from_slice::<crate::workspace::Workspace>(&encoded)
            .unwrap()
            .timeline_hide_shy
    );
}
