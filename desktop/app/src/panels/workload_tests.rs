//! Actual shell drawing and frame assembly; no native window or GPU submission.
use super::*;
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default, serde::Serialize, PartialEq, Eq)]
struct Counts {
    // Counts calculate_frames calls, not every low-level dock solver caller.
    frame_builds: usize,
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
pub(super) fn record_frame_build() {
    record(|counts| counts.frame_builds = counts.frame_builds.saturating_add(1));
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
                    cold.counts.frame_builds, 1,
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
                        frame_builds: 0,
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
    println!("whole-shell steady drag samples={} (cold frame_builds=1; warmed frame_builds=0; chrome=1; cards=1; exact shape/glyph/order counts)", samples.len());
}

fn layout_transition(
    shell: &mut crate::Shell,
    size: [f32; 2],
    label: &str,
    records: &mut Vec<serde_json::Value>,
) {
    // The independent old solver does not warm the cache under test.
    let expected = calculate_frames(shell.workspace.display_root(), size);
    let changed = frame(shell, size, Input::default());
    assert_eq!(changed.counts.frame_builds, 1, "changed layout: {label}");
    assert_eq!(changed.counts.chrome, 1, "changed layout: {label}");
    assert_eq!(changed.notifications, 0, "changed layout: {label}");
    let cached = frames(shell.workspace.display_root(), size);
    assert_eq!(cached.len(), expected.len(), "{label}");
    for (actual, original) in cached.iter().zip(&expected) {
        assert_eq!(actual.panel, original.panel, "{label}");
        assert_eq!(actual.tabs, original.tabs, "{label}");
        assert_eq!(actual.content, original.content, "{label}");
    }
    let steady = frame(shell, size, Input::default());
    assert_eq!(steady.counts.frame_builds, 0, "unchanged layout: {label}");
    assert_eq!(steady.counts.chrome, 1, "unchanged layout: {label}");
    assert_eq!(steady.notifications, 0, "unchanged layout: {label}");
    records.push(serde_json::json!({"label":label,"size":size,"changed":changed,"steady":steady}));
}

fn activate_next_tab(node: &mut Node) -> bool {
    match node {
        Node::Dock { tabs, active, .. } if tabs.len() > 1 => {
            *active = (*active + 1) % tabs.len();
            true
        }
        Node::Split { first, second, .. } => activate_next_tab(first) || activate_next_tab(second),
        _ => false,
    }
}

#[test]
fn real_presets_resize_maximize_and_tab_changes_invalidate_once_without_stale_geometry() {
    let folder = tempfile::tempdir().unwrap();
    let mut shell =
        crate::Shell::new(folder.path().join("project"), None, Some(Locale::En)).unwrap();
    shell.home = None;
    FRAME_CACHE.with(|slot| *slot.borrow_mut() = frame_cache::Cache::new());
    let before = shell.state.clone();
    let mut records = Vec::new();
    for locale in [Locale::En, Locale::Zh] {
        shell.workspace.locale = locale;
        for preset in crate::workspace::Preset::ALL {
            shell.workspace.select_preset(preset);
            assert!(shell.workspace.valid());
            let label = format!("{locale:?}/{preset:?}");
            layout_transition(&mut shell, [900., 900.], &label, &mut records);
            layout_transition(
                &mut shell,
                [1600., 1000.],
                &format!("{label}/resize"),
                &mut records,
            );
            assert!(shell
                .workspace
                .toggle_panel_maximize(Panel::Composition.dock()));
            layout_transition(
                &mut shell,
                [1600., 1000.],
                &format!("{label}/maximize"),
                &mut records,
            );
            assert!(shell.workspace.restore_panel());
            layout_transition(
                &mut shell,
                [1600., 1000.],
                &format!("{label}/restore"),
                &mut records,
            );
            if activate_next_tab(&mut shell.workspace.root) {
                layout_transition(
                    &mut shell,
                    [1600., 1000.],
                    &format!("{label}/tab"),
                    &mut records,
                );
            }
        }
    }
    assert_eq!(
        shell.state, before,
        "layout changes must not edit the project"
    );
    if let Some(path) = std::env::var_os("MOTION_LAYOUT_COST_EVIDENCE") {
        std::fs::write(path, serde_json::to_vec_pretty(&records).unwrap()).unwrap();
    }
    println!("real preset layout transitions={} (changed frame_builds=1; steady frame_builds=0; chrome=1; exact original geometry)", records.len());
}
