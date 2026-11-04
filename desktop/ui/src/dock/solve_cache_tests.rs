use super::*;
use crate::{
    input::{Input, MouseButton},
    workspace::{Change, Workspace},
};
use std::cell::Cell;

thread_local! {
    static SOLVES: Cell<Option<usize>> = const { Cell::new(None) };
}

pub(super) fn record_solve() {
    SOLVES.with(|slot| {
        if let Some(count) = slot.get() {
            slot.set(Some(count.saturating_add(1)));
        }
    });
}

pub(super) fn measure<T>(run: impl FnOnce() -> T) -> (T, usize) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            SOLVES.with(|slot| slot.set(None));
        }
    }
    SOLVES.with(|slot| assert!(slot.replace(Some(0)).is_none()));
    let _reset = Reset;
    let value = run();
    let count = SOLVES.with(|slot| slot.get().unwrap());
    (value, count)
}

pub(super) fn stamp(rows: &[Placement]) -> Vec<(u32, [u32; 4])> {
    rows.iter()
        .map(|p| {
            (
                p.id.0,
                [
                    p.area.min[0].to_bits(),
                    p.area.min[1].to_bits(),
                    p.area.max[0].to_bits(),
                    p.area.max[1].to_bits(),
                ],
            )
        })
        .collect()
}

pub(super) fn original(root: &Node, area: Rect) -> Vec<Placement> {
    let mut rows = vec![];
    // Direct original recursion is independent of solve's cache and counter.
    solve_into(root, area, &mut rows);
    rows
}

fn pair() -> Node {
    Node::Split {
        edge: Edge::Left,
        split: 0.5,
        first: Box::new(Node::dock(DockId(1))),
        second: Box::new(Node::dock(DockId(2))),
    }
}

fn press(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        pressed: Some((MouseButton::Left, point)),
        held: Some(MouseButton::Left),
        ..Default::default()
    }
}

#[test]
fn actual_workspace_body_press_and_paint_queries_share_one_tree_solve() {
    for size in [[900., 600.], [1600., 1000.]] {
        let area = Rect::new(23., 81., size[0], size[1]);
        let mut workspace = Workspace::from_root(pair());
        let before = serde_json::to_value(&workspace).unwrap();
        let (_, count) = measure(|| {
            let rows = solve(&workspace.root, area);
            assert_eq!(stamp(&rows), stamp(&original(&workspace.root, area)));
            let point = [
                rows[0].area.min[0] + 20.,
                rows[0].area.min[1] + metrics::TAB_BAR + 20.,
            ];
            assert!(workspace.interact(&press(point), area).is_none());
            for _ in 0..4 {
                assert_eq!(
                    stamp(&solve(&workspace.root, area)),
                    stamp(&original(&workspace.root, area))
                );
            }
        });
        assert_eq!(
            count, 1,
            "actual workspace hit testing must share the paint query's solve"
        );
        assert_eq!(serde_json::to_value(&workspace).unwrap(), before);
    }
}

#[test]
fn actual_title_drag_reuses_old_geometry_and_solves_the_changed_tree_once() {
    let area = Rect::new(31., 87., 1200., 800.);
    let mut workspace = Workspace::from_root(pair());
    let rows = solve(&workspace.root, area);
    let start = [rows[0].area.min[0] + 15., rows[0].area.min[1] + 10.];
    let end = [
        rows[1].area.min[0] + rows[1].area.width() * 0.5,
        rows[1].area.min[1] + rows[1].area.height() * 0.5,
    ];
    let (_, pressed) = measure(|| assert!(workspace.interact(&press(start), area).is_none()));
    assert_eq!(pressed, 0, "warm title hit testing must not solve again");
    let (_, released) = measure(|| {
        assert!(matches!(
            workspace.interact(
                &Input {
                    mouse: end,
                    released: Some((MouseButton::Left, end)),
                    ..Default::default()
                },
                area
            ),
            Some(Change::Changed)
        ));
        assert_eq!(
            stamp(&solve(&workspace.root, area)),
            stamp(&original(&workspace.root, area))
        );
        assert_eq!(
            stamp(&solve(&workspace.root, area)),
            stamp(&original(&workspace.root, area))
        );
    });
    assert_eq!(released, 1, "only the changed tree needs a new solve");
    assert_eq!(workspace.owned_panels().len(), 2);
    assert_eq!(solve(&workspace.root, area).len(), 1);
}

#[test]
fn actual_splitter_move_solves_only_its_new_geometry() {
    let area = Rect::new(47., 93., 1000., 700.);
    let mut workspace = Workspace::from_root(pair());
    let rows = solve(&workspace.root, area);
    let start = [rows[0].area.max[0], area.min[1] + area.height() * 0.5];
    assert!(matches!(
        workspace.interact(&press(start), area),
        Some(Change::Changed)
    ));
    let point = [start[0] + 70., start[1]];
    let (_, count) = measure(|| {
        assert!(matches!(
            workspace.interact(
                &Input {
                    mouse: point,
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
                area
            ),
            Some(Change::Changed)
        ));
        for _ in 0..5 {
            assert_eq!(
                stamp(&solve(&workspace.root, area)),
                stamp(&original(&workspace.root, area))
            );
        }
    });
    assert_eq!(count, 1);
    assert_ne!(stamp(&rows), stamp(&solve(&workspace.root, area)));
}

#[test]
fn all_geometry_fields_invalidate_without_changing_the_original_results() {
    let mut layouts = vec![
        editor_layout(),
        pair(),
        Node::Fixed {
            edge: Edge::Top,
            size: 0.,
        },
        Node::Dock {
            id: DockId(4),
            tabs: vec![DockId(4), DockId(5)],
            active: 0,
        },
        Node::Dock {
            id: DockId(4),
            tabs: vec![DockId(4), DockId(5)],
            active: 1,
        },
        Node::Dock {
            id: DockId(4),
            tabs: vec![],
            active: 99,
        },
    ];
    for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
        for split in [-0., 0.2, 0.8, f32::NAN, f32::INFINITY] {
            layouts.push(Node::Split {
                edge,
                split,
                first: Box::new(Node::dock(DockId(7))),
                second: Box::new(Node::dock(DockId(8))),
            });
        }
    }
    let mut compared = 0;
    for root in layouts {
        for area in [
            Rect::new(0., 0., 900., 600.),
            Rect::new(10., 20., 900., 600.),
            Rect::new(10., 20., 1600., 1000.),
            Rect::new(-0., 0., 0., 0.),
            Rect::new(f32::NAN, 0., 900., 600.),
        ] {
            let expected = stamp(&original(&root, area));
            for _ in 0..3 {
                assert_eq!(stamp(&solve(&root, area)), expected);
                compared += 1;
            }
        }
    }
    assert_eq!(compared, 390);
}

#[test]
fn caller_edits_and_same_allocation_tree_edits_never_poison_results() {
    let mut root = pair();
    let area = Rect::new(17., 29., 950., 650.);
    let mut rows = solve(&root, area);
    rows[0].id = DockId(999);
    rows[0].area.min = [-900.; 2];
    assert_eq!(stamp(&solve(&root, area)), stamp(&original(&root, area)));
    if let Node::Split {
        edge,
        split,
        first,
        second,
    } = &mut root
    {
        *edge = Edge::Bottom;
        *split = 0.27;
        std::mem::swap(first, second);
    }
    assert_eq!(stamp(&solve(&root, area)), stamp(&original(&root, area)));
    if let Node::Split { first, .. } = &mut root {
        **first = Node::Dock {
            id: DockId(35),
            tabs: vec![DockId(35), DockId(36)],
            active: 1,
        };
    }
    assert_eq!(stamp(&solve(&root, area)), stamp(&original(&root, area)));
    assert_eq!(rows[0].id, DockId(999));
}
