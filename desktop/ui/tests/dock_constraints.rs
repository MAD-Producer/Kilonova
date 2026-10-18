use motion_ui::{
    dock::{self, DockId, Edge, Node},
    theme::metrics,
    workspace::Workspace,
    Input, MouseButton, Rect,
};

fn pair(edge: Edge, split: f32) -> Node {
    Node::Split {
        edge,
        split,
        first: Box::new(Node::dock(DockId(1))),
        second: Box::new(Node::dock(DockId(2))),
    }
}

fn center(rect: Rect) -> [f32; 2] {
    [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]
}

fn drag(workspace: &mut Workspace, area: Rect, start: [f32; 2], end: [f32; 2]) {
    assert!(workspace
        .interact(
            &Input {
                mouse: start,
                pressed: Some((MouseButton::Left, start)),
                held: Some(MouseButton::Left),
                ..Default::default()
            },
            area
        )
        .is_some());
    assert!(workspace
        .interact(
            &Input {
                mouse: end,
                held: Some(MouseButton::Left),
                ..Default::default()
            },
            area
        )
        .is_some());
    assert!(workspace
        .interact(
            &Input {
                mouse: end,
                released: Some((MouseButton::Left, end)),
                ..Default::default()
            },
            area
        )
        .is_some());
}

#[test]
fn rejected_split_preserves_the_entire_existing_tree() {
    for ratio in [0.02, 0.04, 0.96, 0.98] {
        let mut root = pair(Edge::Right, ratio);
        let before = serde_json::to_value(&root).unwrap();
        assert!(!root.split(Edge::Top, DockId(3)));
        assert_eq!(serde_json::to_value(&root).unwrap(), before);
    }
}

#[test]
fn pixel_minima_do_not_grow_with_large_window_percentages() {
    for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
        for size in [900., 1600., 4000., 8000.] {
            let area = Rect::new(100., 200., size, size);
            let minimum = if edge.is_horizontal() {
                metrics::MIN_DOCK
            } else {
                metrics::MIN_TIMELINE
            };
            assert!((dock::clamp_split(edge, 0., area) * size - minimum).abs() < 0.001);
            assert!(((1. - dock::clamp_split(edge, 1., area)) * size - minimum).abs() < 0.001);
        }
    }
}

#[test]
fn dragging_all_edges_uses_the_same_limits_as_the_public_helper() {
    for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
        for size in [180., 900., 1600., 4000.] {
            for upper in [false, true] {
                let area = Rect::new(100., 200., size, size);
                let mut workspace = Workspace::from_root(pair(edge, 0.5));
                let handle = dock::splitters(&workspace.root, area)[0].area;
                let mut end = center(area);
                let axis = usize::from(!edge.is_horizontal());
                end[axis] = if upper {
                    area.max[axis] + 100.
                } else {
                    area.min[axis] - 100.
                };
                drag(&mut workspace, area, center(handle), end);
                let proposed = if upper ^ matches!(edge, Edge::Right | Edge::Bottom) {
                    1.
                } else {
                    0.
                };
                let expected = dock::clamp_split(edge, proposed, area);
                let Node::Split { split, .. } = workspace.root else {
                    panic!("split disappeared")
                };
                assert!(
                    (split - expected).abs() < 0.000001,
                    "{edge:?} size={size} actual={split} expected={expected}"
                );
            }
        }
    }
}

#[test]
fn narrow_saved_splits_remain_valid_and_their_nested_handles_follow_the_drawn_layout() {
    let area = Rect::new(100., 200., 4000., 2400.);
    let mut root = pair(Edge::Left, 0.04);
    let Node::Split { first, .. } = &mut root else {
        unreachable!()
    };
    *first = Box::new(Node::Split {
        edge: Edge::Top,
        split: 0.5,
        first: Box::new(Node::dock(DockId(1))),
        second: Box::new(Node::dock(DockId(3))),
    });
    let mut workspace = Workspace::from_root(root);
    assert!(workspace.valid());
    let encoded = serde_json::to_vec(&workspace).unwrap();
    workspace = serde_json::from_slice(&encoded).unwrap();
    assert!(workspace.valid());
    let handle = dock::splitters(&workspace.root, area)[1].area;
    // The nested splitter ends at x=260. A 5% clamp would incorrectly let
    // its hit region extend to x=300, into the neighbouring dock.
    let outside = [280., center(handle)[1]];
    assert!(workspace
        .interact(
            &Input {
                mouse: outside,
                pressed: Some((MouseButton::Left, outside)),
                ..Default::default()
            },
            area
        )
        .is_none());
    let start = center(handle);
    drag(&mut workspace, area, start, [start[0], area.min[1] + 800.]);
    let placements = dock::solve(&workspace.root, area);
    assert!(
        (placements
            .iter()
            .find(|p| p.id == DockId(1))
            .unwrap()
            .area
            .height()
            - 800.)
            .abs()
            < 0.001
    );
}

#[test]
fn nonfinite_split_proposals_and_degenerate_extents_have_a_finite_fallback() {
    for edge in [Edge::Left, Edge::Top] {
        for ratio in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                dock::clamp_split(edge, ratio, Rect::new(0., 0., 4000., 4000.)),
                0.5
            );
        }
        for size in [0., -1., f32::NAN, f32::INFINITY] {
            assert_eq!(
                dock::clamp_split(edge, 0.2, Rect::new(0., 0., size, size)),
                0.5
            );
        }
    }
}
