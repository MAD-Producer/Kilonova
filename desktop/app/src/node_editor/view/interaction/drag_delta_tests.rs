use super::*;
use crate::node_editor::tests::simple;

#[test]
fn stationary_node_drag_keeps_exact_positions_and_history() {
    let canvas = Rect::new(17., 33., 900., 600.);
    for zoom in [0.33, 0.55, 0.85, 1.25, 2.75] {
        for mouse in [[235.73, 180.29], [101.25, 91.7], [701.3, 504.9]] {
            for release in [false, true] {
                let mut editor = simple();
                editor.view.zoom = zoom;
                let initial = editor.snapshot();
                let before = editor.state(false);
                let positions: Vec<_> = editor.document.nodes.iter()
                    .map(|n| (n.id.clone(), n.position)).collect();
                let origin = editor.view.screen_to_world(mouse, canvas);
                // Repeated held frames must not turn a click into an edit.
                for _ in 0..5 {
                    editor.drag = None;
                    editor.update_node_drag(origin, positions.clone(), before.clone(), canvas,
                        &Input { mouse, held: Some(MouseButton::Left), ..Default::default() });
                    assert_eq!(editor.document, initial.document, "stationary zoom={zoom} mouse={mouse:?}");
                }
                editor.drag = None;
                let input = if release {
                    Input { mouse, released: Some((MouseButton::Left, mouse)), ..Default::default() }
                } else { Input { mouse, ..Default::default() } };
                editor.update_node_drag(origin, positions, before, canvas, &input);
                assert_eq!(format!("{:?}", editor.snapshot()), format!("{initial:?}"));
                assert!(editor.drag.is_none());
                assert!(editor.undo.is_empty());
                assert!(editor.redo.is_empty());
            }
        }
    }
}

#[test]
fn node_drag_return_to_origin_discards_preview_without_an_undo_step() {
    let mut editor = simple();
    let canvas = Rect::new(17., 33., 900., 600.);
    editor.view.zoom = 0.55;
    let initial = editor.snapshot();
    let before = editor.state(false);
    let positions: Vec<_> = editor.document.nodes.iter()
        .map(|n| (n.id.clone(), n.position)).collect();
    let start = [235.73, 180.29];
    let origin = editor.view.screen_to_world(start, canvas);
    let moved = [start[0] + 55., start[1] - 22.];
    editor.update_node_drag(origin, positions.clone(), before.clone(), canvas,
        &Input { mouse: moved, held: Some(MouseButton::Left), ..Default::default() });
    assert_ne!(editor.document, initial.document);
    assert!(editor.undo.is_empty());
    editor.drag = None;
    editor.update_node_drag(origin, positions, before, canvas,
        &Input { mouse: start, released: Some((MouseButton::Left, start)), ..Default::default() });
    assert_eq!(format!("{:?}", editor.snapshot()), format!("{initial:?}"));
    assert!(editor.undo.is_empty());
    assert!(editor.redo.is_empty());
}

#[test]
fn actual_node_movement_commits_one_reversible_edit() {
    let mut editor = simple();
    editor.view.zoom = 1.;
    let canvas = Rect::new(17., 33., 900., 600.);
    let initial = editor.document.clone();
    let before = editor.state(false);
    let positions: Vec<_> = editor.document.nodes.iter()
        .map(|n| (n.id.clone(), n.position)).collect();
    let start = [200., 180.];
    let origin = editor.view.screen_to_world(start, canvas);
    let end = [255., 158.];
    editor.update_node_drag(origin, positions, before, canvas,
        &Input { mouse: end, released: Some((MouseButton::Left, end)), ..Default::default() });
    for (old, moved) in initial.nodes.iter().zip(&editor.document.nodes) {
        assert_eq!(moved.position, [old.position[0] + 55., old.position[1] - 22.]);
    }
    let moved = editor.document.clone();
    assert_eq!(editor.undo.len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.document, initial);
    assert!(editor.redo());
    assert_eq!(editor.document, moved);
}
