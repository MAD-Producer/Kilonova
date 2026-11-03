use super::*;

#[test]
fn steady_node_drags_build_cards_once_and_keep_fresh_geometry_release_and_undo() {
    for language in [Language::English, Language::Chinese] {
        let mut atlas =
            TextAtlas::from_system_language(12., language == Language::Chinese).unwrap();
        for size in [[900., 600.], [1600., 1000.]] {
            for zoom in [0.33, 0.85, 2.75] {
                for collapsed in [false, true] {
                    let mut editor = simple();
                    editor.view_preferences.minimap = false;
                    let area = Rect::new(0., 0., size[0], size[1]);
                    draw(&mut editor, &mut atlas, area, Input::default(), language);
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
                    draw(&mut editor, &mut atlas, area, Input::default(), language);
                    let before = editor.snapshot();
                    let cards = editor.cards(editor.last_canvas);
                    let card = cards.iter().find(|card| card.id == "number").unwrap();
                    let start = [
                        card.rect.min[0] + NODE_WIDTH * zoom * 0.5,
                        card.rect.min[1] + HEADER * zoom * 0.4,
                    ];
                    draw(
                        &mut editor,
                        &mut atlas,
                        area,
                        press(start, MouseButton::Left),
                        language,
                    );
                    assert!(matches!(editor.drag, Some(Drag::Nodes { .. })));
                    for step in 1..=12 {
                        let mouse = [start[0] + step as f32 * 2.5, start[1] + step as f32];
                        let builds = draw(
                            &mut editor,
                            &mut atlas,
                            area,
                            Input {
                                mouse,
                                held: Some(MouseButton::Left),
                                ..Default::default()
                            },
                            language,
                        );
                        assert_eq!(
                            builds, 1,
                            "held position-only frame must resolve card metadata once"
                        );
                        assert_eq!(editor.revision, before.revision);
                        assert!(editor.undo.is_empty());
                    }
                    let end = [start[0] + 30., start[1] + 12.];
                    assert_ne!(editor.document, before.document);
                    draw(
                        &mut editor,
                        &mut atlas,
                        area,
                        release(end, MouseButton::Left),
                        language,
                    );
                    assert!(editor.drag.is_none());
                    assert_eq!(editor.undo.len(), 1);
                    assert!(editor.undo());
                    assert_eq!(editor.document, before.document);
                }
            }
        }
    }
}

#[test]
fn position_refresh_preserves_all_metadata_and_declines_changed_card_identity_without_partial_updates(
) {
    let mut editor = simple();
    let canvas = Rect::new(17., 33., 900., 600.);
    for zoom in [0.33, 0.85, 2.75] {
        editor.view.zoom = zoom;
        let mut cards = editor.cards(canvas);
        for (i, node) in editor.document.nodes.iter_mut().enumerate() {
            node.position = [i as f32 * 37.25 - 100., i as f32 * -19.75 + 20.];
        }
        assert!(editor.refresh_card_positions(&mut cards, canvas));
        assert_eq!(geometry(&cards), geometry(&editor.cards(canvas)));
        let original = geometry(&cards);
        cards.last_mut().unwrap().source_node = std::ptr::null();
        editor.document.nodes[0].position[0] += 50.;
        assert!(!editor.refresh_card_positions(&mut cards, canvas));
        assert_eq!(
            geometry(&cards),
            original,
            "identity checks precede every mutation"
        );
        assert!(!editor.refresh_card_positions(&mut [], canvas));
        cards.pop();
        assert!(!editor.refresh_card_positions(&mut cards, canvas));
        let mut fresh = editor.cards(canvas);
        fresh.swap(0, 1);
        assert!(!editor.refresh_card_positions(&mut fresh, canvas));
    }
}
