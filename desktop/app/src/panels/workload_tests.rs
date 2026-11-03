//! Actual shell drawing and frame assembly; no native window or GPU submission.
use super::*;
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default, serde::Serialize, PartialEq, Eq)]
struct Counts {
    solves: usize,
    chrome: usize,
    cards: usize,
}

thread_local! {
    static COUNTS: Cell<Option<Counts>> = const { Cell::new(None) };
}

fn record(update: impl FnOnce(&mut Counts)) {
    COUNTS.with(|slot| {
        if let Some(mut counts) = slot.get() {
            update(&mut counts);
            slot.set(Some(counts));
        }
    });
}
pub(super) fn record_solve() {
    record(|counts| counts.solves = counts.solves.saturating_add(1));
}
pub(super) fn record_chrome() {
    record(|counts| counts.chrome = counts.chrome.saturating_add(1));
}
pub(crate) fn record_cards() {
    record(|counts| counts.cards = counts.cards.saturating_add(1));
}

#[derive(Debug, serde::Serialize)]
struct FrameCost {
    counts: Counts,
    notifications: usize,
    shapes: usize,
    glyphs: usize,
    order: usize,
}

fn frame(shell: &mut crate::Shell, size: [f32; 2], input: Input) -> FrameCost {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            COUNTS.with(|slot| slot.set(None));
        }
    }
    COUNTS.with(|slot| assert!(slot.replace(Some(Counts::default())).is_none()));
    let reset = Reset;
    shell.input = input;
    // These are the actual geometry queries used by polling/hit testing.
    let _ = composition_view_styled(
        shell.workspace.display_root(),
        size,
        &PaintList::default().styles(),
    );
    let _ = node_view(shell.workspace.display_root(), size);
    let (paint, actions) = shell.draw_commands(size);
    let ui = shell.ui_frame(paint, size, false);
    let counts = COUNTS.with(|slot| slot.get().unwrap());
    drop(reset);
    let cost = FrameCost {
        counts,
        notifications: actions.len(),
        shapes: ui.paint.shapes.len(),
        glyphs: ui.paint.glyphs.len(),
        order: ui.paint.order.len(),
    };
    shell.engine.recycle_paint(ui.paint);
    // Release publishes a real geometry-change notification. Consume it through
    // the actual shell handler, outside the UI draw/assembly measurement.
    for action in actions {
        match action {
            Action::Node(
                action @ crate::node_editor::Action::Changed {
                    semantic: false, ..
                },
            ) => {
                shell.node_action(action).unwrap();
            }
            other => panic!("unexpected action in the geometry-only fixture: {other:?}"),
        }
    }
    cost
}

fn press(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        pressed: Some((MouseButton::Left, point)),
        held: Some(MouseButton::Left),
        ..Default::default()
    }
}
fn release(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        released: Some((MouseButton::Left, point)),
        ..Default::default()
    }
}
fn node_layout(multiple: bool) -> Node {
    if !multiple {
        return Node::dock(Panel::Nodes.dock());
    }
    Node::Split {
        edge: dock::Edge::Top,
        split: 0.76,
        first: Box::new(Node::Split {
            edge: dock::Edge::Left,
            split: 0.78,
            first: Box::new(Node::dock(Panel::Nodes.dock())),
            second: Box::new(Node::dock(Panel::Properties.dock())),
        }),
        second: Box::new(Node::dock(Panel::Timeline.dock())),
    }
}

#[test]
fn complete_shell_drag_frames_match_static_work_and_solve_once_for_each_stable_layout() {
    let folder = tempfile::tempdir().unwrap();
    let mut shell =
        crate::Shell::new(folder.path().join("project"), None, Some(Locale::En)).unwrap();
    shell.home = None;
    let mut samples = Vec::new();
    for locale in [Locale::En, Locale::Zh] {
        for size in [[900., 900.], [1600., 1000.]] {
            for multiple in [false, true] {
                shell.workspace.locale = locale;
                shell.workspace.root = node_layout(multiple);
                shell.panels.focused = Some(Panel::Nodes);
                let mut graph = motion_graph::demo_graph();
                for (index, node) in graph.nodes.iter_mut().enumerate() {
                    node.position = if node.id == "zero" {
                        [0., 0.]
                    } else {
                        [260. + (index % 3) as f32 * 250., (index / 3) as f32 * 220.]
                    };
                }
                motion_graph::validate(&graph, &motion_graph::CompileOptions::default()).unwrap();
                shell.panels.node_editor = crate::node_editor::NodeEditor::new(graph);
                FRAME_CACHE.with(|slot| *slot.borrow_mut() = frame_cache::Cache::new());
                let cold = frame(&mut shell, size, Input::default());
                assert_eq!(
                    cold.counts.solves, 1,
                    "cold layout={multiple} size={size:?}"
                );
                assert_eq!(cold.counts.chrome, 1);
                assert_eq!(cold.counts.cards, 1);
                let start = shell.panels.node_editor.node_header_center("zero").unwrap();
                frame(&mut shell, size, press(start));
                frame(&mut shell, size, release(start));
                let before = shell.panels.node_editor.snapshot();
                let static_cost = frame(
                    &mut shell,
                    size,
                    Input {
                        mouse: start,
                        ..Default::default()
                    },
                );
                assert_eq!(static_cost.notifications, 0);
                assert_eq!(
                    static_cost.counts,
                    Counts {
                        solves: 0,
                        chrome: 1,
                        cards: 1
                    }
                );
                frame(&mut shell, size, press(start));
                for index in 1..=6 {
                    let point = [start[0] + index as f32 * 3., start[1] + index as f32 * 2.];
                    let drag = frame(
                        &mut shell,
                        size,
                        Input {
                            mouse: point,
                            held: Some(MouseButton::Left),
                            ..Default::default()
                        },
                    );
                    assert_eq!(drag.notifications, 0);
                    assert_eq!(
                        drag.counts, static_cost.counts,
                        "held drag {locale:?} {size:?} multiple={multiple}"
                    );
                    assert_eq!(
                        (drag.shapes, drag.glyphs, drag.order),
                        (static_cost.shapes, static_cost.glyphs, static_cost.order),
                        "actual whole-shell paint workload {locale:?} {size:?} multiple={multiple}"
                    );
                    samples.push(serde_json::json!({"locale":format!("{locale:?}"),"size":size,"multiple":multiple,"frame":index,"cold":cold,"static":static_cost,"drag":drag}));
                }
                assert_ne!(
                    shell.panels.node_editor.snapshot().document,
                    before.document
                );
                frame(&mut shell, size, release([start[0] + 18., start[1] + 12.]));
                assert!(shell.panels.node_editor.undo());
                assert_eq!(
                    shell.panels.node_editor.snapshot().document,
                    before.document
                );
            }
        }
    }
    assert_eq!(samples.len(), 48);
    if let Some(path) = std::env::var_os("MOTION_WORKSPACE_COST_EVIDENCE") {
        std::fs::write(path, serde_json::to_vec_pretty(&samples).unwrap()).unwrap();
    }
    println!("whole-shell steady drag samples={} (cold solves=1; warmed solves=0; chrome=1; cards=1; exact shape/glyph/order counts)", samples.len());
}
