use super::*;

fn prepare(zoom: f32, multiple: bool) -> NodeEditor {
    let mut editor = simple();
    editor.inspector_open = false;
    editor.view.zoom = zoom;
    editor.focus_panel();
    editor.select(Some("number"), false);
    if multiple {
        editor.select(Some("math"), true);
    }
    editor
}

fn scenario(
    editor: &mut NodeEditor,
    changed: bool,
    held: Option<MouseButton>,
    mut step: impl FnMut(&mut NodeEditor, Input, &str),
) {
    step(editor, Input::default(), "initial");
    let before = editor.snapshot();
    let selection = editor.selected.clone();
    let card = editor
        .cards(editor.last_canvas)
        .into_iter()
        .find(|card| card.id == "number")
        .unwrap();
    let start = [
        card.rect.min[0] + NODE_WIDTH * editor.view.zoom * 0.5,
        card.rect.min[1] + 8. * editor.view.zoom,
    ];
    let end = [start[0] + 31.5, start[1] + 17.25];
    step(editor, press(start, MouseButton::Left), "press");
    assert!(matches!(editor.drag, Some(Drag::Nodes { .. })));
    step(
        editor,
        Input {
            mouse: if changed { end } else { start },
            held: Some(MouseButton::Left),
            ..Default::default()
        },
        "move",
    );
    assert_eq!(
        editor.document == before.document,
        !changed,
        "held movement changed={changed} zoom={} before={:?} after={:?}",
        editor.view.zoom,
        before.document.nodes,
        editor.document.nodes
    );
    assert_eq!(editor.revision, before.revision);
    // Even with no preceding movement, the original handler applies this
    // pointer position before restoring and increments the revision once.
    step(
        editor,
        Input {
            mouse: end,
            held,
            wheel: [5., 8.],
            ..Default::default()
        },
        "lost",
    );
    assert_eq!(editor.document, before.document);
    assert_eq!(editor.revision, before.revision.wrapping_add(1));
    assert_eq!(editor.semantic_revision, before.semantic_revision);
    assert_eq!(editor.preview_revision, before.preview_revision);
    assert_eq!(editor.selected, selection);
    assert!(editor.drag.is_none() && editor.undo.is_empty() && editor.redo.is_empty());
    step(editor, Input::default(), "settled");
    assert_eq!(editor.document, before.document);
}

#[test]
fn lost_held_restoration_builds_only_the_final_cards_and_preserves_revision_semantics() {
    let mut counts = vec![];
    for language in [Language::English, Language::Chinese] {
        let mut atlas =
            TextAtlas::from_system_language(12., language == Language::Chinese).unwrap();
        for size in [[1200., 800.], [1600., 1000.]] {
            let area = Rect::new(13., 17., size[0], size[1]);
            for zoom in [0.5, 1.25] {
                for multiple in [false, true] {
                    for changed in [false, true] {
                        for held in [None, Some(MouseButton::Right)] {
                            let mut editor = prepare(zoom, multiple);
                            let view = editor.view;
                            scenario(&mut editor, changed, held, |editor, input, phase| {
                                let (builds, ports) =
                                    draw_counts(editor, &mut atlas, area, input, language);
                                counts.push((format!("{language:?}-{size:?}-{zoom}-{multiple}-{changed}-{held:?}-{phase}"), builds, ports, editor.document.nodes.len()));
                            });
                            assert_eq!(
                                (editor.view.pan, editor.view.zoom),
                                (view.pan, view.zoom),
                                "wheel remains suppressed while the cancelled drag owns input"
                            );
                        }
                    }
                }
            }
        }
    }
    assert_eq!(counts.len(), 320);
    assert!(
        counts
            .iter()
            .all(|(_, builds, ports, nodes)| *builds == 1 && ports == nodes),
        "one final card build after restoration: {counts:?}"
    );
}

fn drag_state(editor: &NodeEditor) -> String {
    match &editor.drag {
        Some(Drag::Nodes {
            origin,
            positions,
            before,
        }) => format!(
            "{:?}",
            (
                origin,
                positions,
                &before.document,
                &before.options,
                before.semantic,
                before.view,
                &before.selection,
                &before.primary
            )
        ),
        None => "none".into(),
        _ => panic!("unexpected restoration drag"),
    }
}

#[test]
fn record_complete_restoration_frames_for_original_path_comparison() {
    let mut evidence = std::collections::BTreeMap::new();
    for language in [Language::English, Language::Chinese] {
        let mut atlas =
            TextAtlas::from_system_language(12., language == Language::Chinese).unwrap();
        for size in [[1200., 800.], [1600., 1000.]] {
            let area = Rect::new(13., 17., size[0], size[1]);
            for zoom in [0.5, 1.25] {
                for multiple in [false, true] {
                    for changed in [false, true] {
                        for held in [None, Some(MouseButton::Right)] {
                            let mut editor = prepare(zoom, multiple);
                            scenario(&mut editor, changed, held, |editor, input, phase| {
                                let mut paint = PaintList::default();
                                let response =
                                    editor.draw(&mut paint, &mut atlas, area, &input, language);
                                let record = format!(
                                    "{:?}",
                                    (
                                        (paint.shapes, paint.colors, paint.glyphs, paint.order),
                                        (
                                            response.actions,
                                            response.wants_pointer,
                                            response.wants_keyboard,
                                            response.changed,
                                            response.semantic_changed
                                        ),
                                        editor.snapshot(),
                                        (
                                            &editor.selected,
                                            &editor.primary,
                                            editor.view,
                                            &editor.notice,
                                            &editor.diagnostic
                                        ),
                                        drag_state(editor),
                                        editor.undo.len(),
                                        editor.redo.len()
                                    )
                                );
                                assert!(evidence.insert(format!("{language:?}-{size:?}-{zoom}-{multiple}-{changed}-{held:?}-{phase}"), record).is_none());
                            });
                        }
                    }
                }
            }
        }
    }
    assert_eq!(evidence.len(), 320);
    if let Some(path) = std::env::var_os("MOTION_NODE_RESTORE_EVIDENCE") {
        std::fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    }
}

#[test]
fn early_settlement_leaves_presses_held_drags_and_explicit_releases_untouched() {
    let canvas = Rect::new(13., 55., 1200., 700.);
    for input in [
        press([200., 300.], MouseButton::Left),
        press([200., 300.], MouseButton::Right),
        press([200., 300.], MouseButton::Middle),
        Input {
            held: Some(MouseButton::Left),
            ..Default::default()
        },
        release([200., 300.], MouseButton::Left),
    ] {
        let mut editor = prepare(1., true);
        editor.drag = Some(Drag::Nodes {
            origin: [20., 260.],
            positions: vec![("number".into(), [20., 260.])],
            before: editor.state(false),
        });
        let before = format!("{:?}", editor.snapshot());
        let drag = drag_state(&editor);
        assert!(!editor.settle_node_cancellation(canvas, &input));
        assert_eq!(format!("{:?}", editor.snapshot()), before);
        assert_eq!(drag_state(&editor), drag);
        assert!(editor.undo.is_empty() && editor.redo.is_empty());
    }
    let mut editor = prepare(1., false);
    editor.drag = Some(Drag::WireInput {
        target: PortRef::new("math", "a"),
    });
    assert!(!editor.settle_node_cancellation(canvas, &Input::default()));
    assert!(matches!(editor.drag, Some(Drag::WireInput { .. })));
}
