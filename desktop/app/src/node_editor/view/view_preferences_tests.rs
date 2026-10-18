use super::super::tests::{frame, press, release, simple};
use super::*;
use motion_ui::input::Modifiers;

fn key(key: Key) -> Input {
    Input {
        keys: vec![(key, Modifiers::default())],
        ..Default::default()
    }
}
fn center(rect: Rect) -> [f32; 2] {
    [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]
}

#[test]
fn node_view_shortcuts_are_local_exact_and_not_repeated() {
    use crate::shortcuts::{resolve, Command, Scope};
    for (key, command) in [
        (Key::G, Command::ToggleNodeGrid),
        (Key::N, Command::ToggleNodeMinimap),
    ] {
        assert_eq!(
            resolve(Scope::Nodes, key, Modifiers::default(), false),
            Some(command)
        );
        assert_eq!(resolve(Scope::Nodes, key, Modifiers::default(), true), None);
        assert_eq!(
            resolve(Scope::Application, key, Modifiers::default(), false),
            None
        );
        assert_eq!(
            resolve(Scope::Animation, key, Modifiers::default(), false),
            None
        );
    }
    assert_eq!(
        resolve(
            Scope::Nodes,
            Key::G,
            Modifiers {
                control: true,
                ..Default::default()
            },
            false
        ),
        Some(Command::Group)
    );
    assert_eq!(
        resolve(
            Scope::Nodes,
            Key::G,
            Modifiers {
                alt: true,
                ..Default::default()
            },
            false
        ),
        Some(Command::Ungroup)
    );
}

#[test]
fn toggles_leave_graph_history_compile_and_permanent_output_unchanged() {
    let mut editor = NodeEditor::default();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    editor.focus_panel();
    let document = editor.root_document();
    let revision = editor.revision;
    let stamp = editor.stamp();
    let history = (editor.undo.len(), editor.redo.len());
    let compiled = editor.snapshot().compile().unwrap();
    for k in [Key::G, Key::N] {
        let response = frame(&mut editor, &mut atlas, key(k));
        assert!(!response.changed && !response.semantic_changed);
        assert!(matches!(
            response.actions.as_slice(),
            [Action::ViewPreferencesChanged(_)]
        ));
    }
    assert_eq!(editor.root_document(), document);
    assert_eq!(
        editor.view_preferences,
        ViewPreferences {
            grid: false,
            minimap: true
        }
    );
    assert_eq!(editor.revision, revision);
    assert_eq!(editor.stamp(), stamp);
    assert_eq!((editor.undo.len(), editor.redo.len()), history);
    assert_eq!(editor.snapshot().compile().unwrap().source, compiled.source);
    let preferences = editor.view_preferences;
    editor.replace_document(demo_graph(), None);
    assert_eq!(editor.view_preferences, preferences);
}

#[test]
fn view_keys_respect_text_ime_focus_popups_and_active_gestures() {
    let mut editor = simple();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let prefs = editor.view_preferences;
    frame(&mut editor, &mut atlas, key(Key::G)); // unfocused
    assert_eq!(editor.view_preferences, prefs);
    editor.focus_panel();
    for input in [
        Input {
            composing: true,
            ..key(Key::G)
        },
        Input {
            ime_consumed: true,
            ..key(Key::N)
        },
    ] {
        frame(&mut editor, &mut atlas, input);
        assert_eq!(editor.view_preferences, prefs);
    }
    editor.search_focus = true;
    frame(&mut editor, &mut atlas, key(Key::G));
    assert_eq!(editor.view_preferences, prefs);
    editor.search_focus = false;
    editor.field = Some(Field {
        binding: Binding::GraphName(false),
        text: "name".into(),
        cursor: 4,
        replace: false,
        error: None,
    });
    frame(&mut editor, &mut atlas, key(Key::N));
    assert_eq!(editor.view_preferences, prefs);
    editor.field = None;
    editor.context_menu = Some(ContextMenu {
        position: [300., 200.],
        kind: MenuKind::View,
        selected: 0,
    });
    frame(&mut editor, &mut atlas, key(Key::G));
    assert_eq!(editor.view_preferences, prefs);
    frame(&mut editor, &mut atlas, key(Key::Escape));
    assert!(editor.context_menu.is_none());
    editor.drag = Some(Drag::Pan {
        origin: [0.; 2],
        pan: editor.view.pan,
        button: MouseButton::Middle,
    });
    frame(
        &mut editor,
        &mut atlas,
        Input {
            held: Some(MouseButton::Middle),
            ..key(Key::N)
        },
    );
    assert_eq!(editor.view_preferences, prefs);
}

#[test]
fn minimap_transform_tracks_nonzero_origins_pan_zoom_and_empty_graphs() {
    let mut editor = simple();
    editor.view_preferences.minimap = true;
    let styles = PaintList::default().styles();
    let canvas = Rect::new(105., 83., 880., 560.);
    for zoom in [1.0e-6, 0.2, 0.85, 2.5] {
        editor.view = CanvasView {
            pan: [-730., 280.],
            zoom,
        };
        let map = editor.minimap(canvas, &styles).unwrap();
        let expected = Rect {
            min: editor.view.screen_to_world(canvas.min, canvas),
            max: editor.view.screen_to_world(canvas.max, canvas),
        };
        assert_eq!(map.viewport, expected);
        let projected = map.project_rect(expected);
        assert!(projected.min[0] >= map.area.min[0] && projected.max[0] <= map.area.max[0]);
        assert!(projected.min[1] >= map.area.min[1] && projected.max[1] <= map.area.max[1]);
    }
    editor.document.nodes.clear();
    assert!(editor.minimap(canvas, &styles).is_some());
    assert!(editor
        .minimap(Rect::new(0., 0., 100., 80.), &styles)
        .is_none());
    editor.view.zoom = f32::NAN;
    assert!(editor.minimap(canvas, &styles).is_none());
}

#[test]
fn minimap_navigation_is_view_only_and_escape_focus_loss_or_lost_release_restore_pan() {
    let mut editor = simple();
    editor.view_preferences.minimap = true;
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let styles = PaintList::default().styles();
    let canvas = editor.last_canvas;
    let before = editor.view.pan;
    let document = editor.document.clone();
    let selected = editor.selected.clone();
    let revision = editor.revision;
    let point = editor
        .minimap(canvas, &styles)
        .unwrap()
        .project([410., 100.]);
    frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
    assert_ne!(editor.view.pan, before);
    assert!(editor.minimap_drag.is_some());
    frame(&mut editor, &mut atlas, release(point, MouseButton::Left));
    assert!(editor.minimap_drag.is_none());
    let expected = editor.view.screen_to_world(center(canvas), canvas);
    assert!((expected[0] - 410.).abs() < 0.01 && (expected[1] - 100.).abs() < 0.01);
    assert_eq!(editor.document, document);
    assert_eq!(editor.selected, selected);
    assert_eq!(editor.revision, revision);
    assert!(editor.undo.is_empty());
    let mapping = editor.minimap(canvas, &styles).unwrap();
    let start = mapping.project([410., 100.]);
    let destination = mapping.project([510., 180.]);
    frame(&mut editor, &mut atlas, press(start, MouseButton::Left));
    frame(
        &mut editor,
        &mut atlas,
        Input {
            mouse: destination,
            held: Some(MouseButton::Left),
            ..Default::default()
        },
    );
    let center_after_drag = editor.view.screen_to_world(center(canvas), canvas);
    assert!((center_after_drag[0] - 510.).abs() < 0.01);
    assert!((center_after_drag[1] - 180.).abs() < 0.01);
    frame(
        &mut editor,
        &mut atlas,
        release(destination, MouseButton::Left),
    );
    assert!(editor.minimap_drag.is_none());
    assert_eq!(editor.document, document);
    assert!(editor.undo.is_empty());
    for cancel in [
        key(Key::Escape),
        Input {
            focus_lost: true,
            ..Default::default()
        },
        Input::default(),
    ] {
        let before = editor.view.pan;
        let map = editor.minimap(canvas, &styles).unwrap();
        let point = [map.area.min[0] + 12., map.area.min[1] + 12.];
        frame(&mut editor, &mut atlas, press(point, MouseButton::Left));
        frame(&mut editor, &mut atlas, cancel);
        assert!(editor.minimap_drag.is_none());
        assert_eq!(editor.view.pan, before);
    }
}

#[test]
fn view_and_canvas_menus_share_checked_actions_and_cancel_without_toggling() {
    for language in [Language::Chinese, Language::English] {
        for kind in [MenuKind::View, MenuKind::Canvas] {
            let mut editor = simple();
            let mut atlas = TextAtlas::from_system(13.).unwrap();
            frame(&mut editor, &mut atlas, Input::default());
            editor.context_menu = Some(ContextMenu {
                position: [300., 200.],
                kind: kind.clone(),
                selected: 0,
            });
            let mut paint = PaintList::default();
            editor.paint_context_menu(
                &mut paint,
                &mut atlas,
                Rect::new(0., 0., 1200., 800.),
                &key(Key::Escape),
                language,
            );
            assert!(editor.context_menu.is_none());
            assert!(editor.view_preferences.grid);
            let selection = if matches!(kind, MenuKind::Canvas) {
                2
            } else {
                0
            };
            editor.context_menu = Some(ContextMenu {
                position: [300., 200.],
                kind,
                selected: selection,
            });
            editor.paint_context_menu(
                &mut paint,
                &mut atlas,
                Rect::new(0., 0., 1200., 800.),
                &key(Key::Enter),
                language,
            );
            assert!(!editor.view_preferences.grid);
            assert_eq!(editor.revision, 0);
        }
    }
}
