use super::*;

fn prepare(editor: &mut NodeEditor, atlas: &mut TextAtlas, area: Rect, language: Language) {
    editor.view_preferences.minimap = false;
    draw(editor, atlas, area, Input::default(), language);
}

#[test]
fn wheel_pan_and_minimap_changes_project_existing_metadata_once() {
    for language in [Language::English, Language::Chinese] {
        let mut atlas =
            TextAtlas::from_system_language(12., language == Language::Chinese).unwrap();
        for size in [[900., 600.], [1600., 1000.]] {
            let area = Rect::new(0., 0., size[0], size[1]);
            let mut editor = simple();
            prepare(&mut editor, &mut atlas, area, language);
            let before = editor.snapshot();
            let point = center(editor.last_canvas);
            for modifiers in [
                Modifiers::default(),
                Modifiers {
                    control: true,
                    ..Default::default()
                },
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
            ] {
                let view = editor.view;
                assert_eq!(
                    draw(
                        &mut editor,
                        &mut atlas,
                        area,
                        Input {
                            mouse: point,
                            wheel: [8., 25.],
                            modifiers,
                            ..Default::default()
                        },
                        language
                    ),
                    1,
                    "viewport projection must not resolve ports twice"
                );
                assert_ne!((editor.view.pan, editor.view.zoom), (view.pan, view.zoom));
            }
            for input in [
                press(point, MouseButton::Middle),
                Input {
                    mouse: [point[0] + 50., point[1] - 25.],
                    held: Some(MouseButton::Middle),
                    ..Default::default()
                },
                release([point[0] + 50., point[1] - 25.], MouseButton::Middle),
            ] {
                assert_eq!(draw(&mut editor, &mut atlas, area, input, language), 1);
            }
            editor.view_preferences.minimap = true;
            let styles = PaintList::default().styles();
            let map = editor.minimap(editor.last_canvas, &styles).unwrap();
            let start = [map.area.min[0] + 12., map.area.min[1] + 12.];
            let view = editor.view;
            assert_eq!(
                draw(
                    &mut editor,
                    &mut atlas,
                    area,
                    press(start, MouseButton::Left),
                    language
                ),
                1
            );
            assert_ne!(editor.view.pan, view.pan);
            assert_eq!(
                draw(
                    &mut editor,
                    &mut atlas,
                    area,
                    Input {
                        mouse: center(map.area),
                        held: Some(MouseButton::Left),
                        ..Default::default()
                    },
                    language
                ),
                1
            );
            assert_eq!(
                draw(
                    &mut editor,
                    &mut atlas,
                    area,
                    Input {
                        focus_lost: true,
                        ..Default::default()
                    },
                    language
                ),
                1
            );
            assert_eq!((editor.view.pan, editor.view.zoom), (view.pan, view.zoom));
            assert_eq!(editor.document, before.document);
            assert_eq!(editor.revision, before.revision);
            assert!(editor.undo.is_empty());
        }
    }
}

#[test]
fn initial_pointer_batch_release_and_restoration_use_current_geometry_once() {
    for language in [Language::English, Language::Chinese] {
        let mut atlas =
            TextAtlas::from_system_language(12., language == Language::Chinese).unwrap();
        for cancel in [false, true] {
            let mut editor = simple();
            let area = Rect::new(0., 0., 1600., 1000.);
            prepare(&mut editor, &mut atlas, area, language);
            let before = editor.snapshot();
            let card = editor
                .cards(editor.last_canvas)
                .into_iter()
                .find(|c| c.id == "number")
                .unwrap();
            let start = [card.rect.min[0] + 40., card.rect.min[1] + 8.];
            let end = [start[0] + 35., start[1] + 20.];
            let mut input = press(start, MouseButton::Left);
            input.mouse = end;
            assert_eq!(draw(&mut editor, &mut atlas, area, input, language), 1);
            assert_ne!(editor.document, before.document);
            assert_eq!(editor.revision, before.revision);
            assert_eq!(editor.semantic_revision, before.semantic_revision);
            assert!(editor.undo.is_empty());
            if cancel {
                // Restore before building the final cards. The old identities
                // remain invalid; this path no longer creates abandoned cards.
                assert_eq!(
                    draw(
                        &mut editor,
                        &mut atlas,
                        area,
                        Input {
                            mouse: end,
                            ..Default::default()
                        },
                        language
                    ),
                    1
                );
                assert_eq!(editor.document, before.document);
                assert!(editor.undo.is_empty());
            } else {
                assert_eq!(
                    draw(
                        &mut editor,
                        &mut atlas,
                        area,
                        release(end, MouseButton::Left),
                        language
                    ),
                    1
                );
                assert_eq!(editor.semantic_revision, before.semantic_revision);
                assert_eq!(editor.undo.len(), 1);
                assert!(editor.undo());
                assert_eq!(editor.document, before.document);
            }
        }
    }
}

#[test]
fn record_complete_navigation_and_release_paint_for_original_path_comparison() {
    let mut evidence = std::collections::BTreeMap::new();
    for language in [Language::English, Language::Chinese] {
        let mut atlas =
            TextAtlas::from_system_language(12., language == Language::Chinese).unwrap();
        for size in [[900., 600.], [1600., 1000.]] {
            for zoom in [0.33, 0.85, 2.75] {
                for collapsed in [false, true] {
                    let mut editor = simple();
                    let area = Rect::new(0., 0., size[0], size[1]);
                    prepare(&mut editor, &mut atlas, area, language);
                    editor.view.zoom = zoom;
                    let node = editor
                        .document
                        .nodes
                        .iter_mut()
                        .find(|n| n.id == "number")
                        .unwrap();
                    node.collapsed = collapsed;
                    editor.view.pan = [
                        100. - node.position[0] * zoom,
                        100. - node.position[1] * zoom,
                    ];
                    let before = editor.snapshot();
                    let point = center(editor.last_canvas);
                    let mut frames = Vec::new();
                    let mut record = |editor: &mut NodeEditor, input: Input| {
                        let mut paint = PaintList::default();
                        let response = editor.draw(&mut paint, &mut atlas, area, &input, language);
                        assert!(!paint.shapes.is_empty() && !paint.glyphs.is_empty());
                        frames.push(format!(
                            "{:?}",
                            (
                                paint.shapes,
                                paint.colors,
                                paint.glyphs,
                                paint.order,
                                response.actions,
                                response.wants_pointer,
                                response.wants_keyboard,
                                response.changed,
                                response.semantic_changed,
                                editor.snapshot(),
                                editor.undo.len()
                            )
                        ));
                    };
                    for modifiers in [
                        Modifiers::default(),
                        Modifiers {
                            control: true,
                            ..Default::default()
                        },
                        Modifiers {
                            shift: true,
                            ..Default::default()
                        },
                    ] {
                        record(
                            &mut editor,
                            Input {
                                mouse: point,
                                wheel: [8., 25.],
                                modifiers,
                                ..Default::default()
                            },
                        );
                    }
                    record(&mut editor, press(point, MouseButton::Middle));
                    record(
                        &mut editor,
                        Input {
                            mouse: [point[0] + 50., point[1] - 25.],
                            held: Some(MouseButton::Middle),
                            ..Default::default()
                        },
                    );
                    record(
                        &mut editor,
                        release([point[0] + 50., point[1] - 25.], MouseButton::Middle),
                    );
                    editor.view_preferences.minimap = true;
                    let styles = PaintList::default().styles();
                    let map = editor.minimap(editor.last_canvas, &styles).unwrap();
                    let start = [map.area.min[0] + 12., map.area.min[1] + 12.];
                    record(&mut editor, press(start, MouseButton::Left));
                    record(
                        &mut editor,
                        Input {
                            mouse: center(map.area),
                            held: Some(MouseButton::Left),
                            ..Default::default()
                        },
                    );
                    record(&mut editor, release(center(map.area), MouseButton::Left));
                    editor.view_preferences.minimap = false;
                    editor.view.zoom = zoom;
                    let node = editor
                        .document
                        .nodes
                        .iter()
                        .find(|n| n.id == "number")
                        .unwrap();
                    editor.view.pan = [
                        100. - node.position[0] * zoom,
                        100. - node.position[1] * zoom,
                    ];
                    record(&mut editor, Input::default());
                    let card = editor
                        .cards(editor.last_canvas)
                        .into_iter()
                        .find(|c| c.id == "number")
                        .unwrap();
                    let start = [
                        card.rect.min[0] + NODE_WIDTH * zoom * 0.5,
                        card.rect.min[1] + HEADER * zoom * 0.4,
                    ];
                    let end = [start[0] + 36.25, start[1] + 15.75];
                    let mut input = press(start, MouseButton::Left);
                    input.mouse = end;
                    record(&mut editor, input);
                    record(
                        &mut editor,
                        Input {
                            mouse: end,
                            held: Some(MouseButton::Left),
                            ..Default::default()
                        },
                    );
                    record(&mut editor, release(end, MouseButton::Left));
                    assert!(editor.drag.is_none());
                    assert_eq!(editor.undo.len(), 1);
                    assert!(editor.undo());
                    assert_eq!(editor.document, before.document);
                    assert_eq!(frames.len(), 13);
                    assert_ne!(
                        frames[0], frames[1],
                        "navigation must change actual paint/state"
                    );
                    evidence.insert(
                        format!("{language:?}-{}-{zoom}-{collapsed}", size[0]),
                        frames,
                    );
                }
            }
        }
    }
    assert_eq!(evidence.len(), 24);
    if let Some(path) = std::env::var_os("MOTION_NODE_VIEWPORT_EVIDENCE") {
        std::fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    }
    println!("complete navigation/release comparison frames=312, moving controls=24");
}
