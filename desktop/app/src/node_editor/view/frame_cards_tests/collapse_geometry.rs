use super::*;

#[derive(Clone, Copy, Debug)]
enum Fixture {
    Plain,
    Defaults,
    Group,
}

fn prepare(fixture: Fixture, zoom: f32) -> (NodeEditor, String) {
    let mut editor = simple();
    let target = match fixture {
        Fixture::Plain => "math".to_owned(),
        Fixture::Defaults => {
            editor
                .document
                .nodes
                .iter_mut()
                .find(|n| n.id == "math")
                .unwrap()
                .input_defaults
                .insert("a".into(), InputValue::Float(2.));
            // This is fixture construction, not an edit of an existing default.
            editor.document.format_version = editor
                .document
                .format_version
                .max(editor.document.minimum_format_version());
            "math".to_owned()
        }
        Fixture::Group => {
            editor.select(Some("color"), false);
            assert!(editor.group_selected().unwrap());
            editor.primary.clone().unwrap()
        }
    };
    editor
        .document
        .nodes
        .iter_mut()
        .find(|n| n.id == target)
        .unwrap()
        .position = [90., 60.];
    editor.view.zoom = zoom;
    editor.focus_panel();
    editor.select(Some(&target), false);
    (editor, target)
}

fn collapse_point(editor: &NodeEditor, target: &str) -> [f32; 2] {
    let card = editor
        .cards(editor.last_canvas)
        .into_iter()
        .find(|c| c.id == target)
        .unwrap();
    let point = [
        card.rect.max[0] - 15. * editor.view.zoom,
        card.rect.min[1] + 12. * editor.view.zoom,
    ];
    assert!(editor.last_canvas.contains(point));
    point
}

#[test]
fn collapse_clicks_resolve_each_nodes_ports_once_and_preserve_history() {
    let mut counts = vec![];
    for language in [Language::English, Language::Chinese] {
        let mut atlas =
            TextAtlas::from_system_language(12., language == Language::Chinese).unwrap();
        for size in [[1200., 800.], [1600., 1000.]] {
            let area = Rect::new(13., 17., size[0], size[1]);
            for zoom in [0.55, 1.35] {
                for fixture in [Fixture::Plain, Fixture::Defaults, Fixture::Group] {
                    let (mut editor, target) = prepare(fixture, zoom);
                    draw(&mut editor, &mut atlas, area, Input::default(), language);
                    let before = editor.snapshot();
                    let undo = editor.undo.len();
                    let nodes = editor.document.nodes.len();
                    for collapsed in [true, false] {
                        let point = collapse_point(&editor, &target);
                        let (builds, resolutions) = draw_counts(
                            &mut editor,
                            &mut atlas,
                            area,
                            press(point, MouseButton::Left),
                            language,
                        );
                        assert_eq!(
                            editor
                                .document
                                .nodes
                                .iter()
                                .find(|n| n.id == target)
                                .unwrap()
                                .collapsed,
                            collapsed
                        );
                        assert_eq!(editor.semantic_revision, before.semantic_revision);
                        assert_eq!(editor.preview_revision, before.preview_revision);
                        counts.push((
                            format!("{language:?}-{size:?}-{zoom}-{fixture:?}-{collapsed}"),
                            builds,
                            resolutions,
                            nodes,
                        ));
                        draw(
                            &mut editor,
                            &mut atlas,
                            area,
                            release(point, MouseButton::Left),
                            language,
                        );
                    }
                    assert_eq!(editor.revision, before.revision + 2);
                    assert_eq!(editor.undo.len(), undo + 2);
                    assert_eq!(editor.document, before.document);
                    assert!(editor.undo());
                    assert!(
                        editor
                            .document
                            .nodes
                            .iter()
                            .find(|n| n.id == target)
                            .unwrap()
                            .collapsed
                    );
                    assert!(editor.undo());
                    assert_eq!(editor.document, before.document);
                    assert!(editor.redo());
                    assert!(
                        editor
                            .document
                            .nodes
                            .iter()
                            .find(|n| n.id == target)
                            .unwrap()
                            .collapsed
                    );
                    assert!(editor.redo());
                    assert_eq!(editor.document, before.document);
                }
            }
        }
    }
    assert_eq!(counts.len(), 48);
    assert!(
        counts
            .iter()
            .all(|(_, builds, resolutions, nodes)| *builds == 1 && resolutions == nodes),
        "one real port resolution per node after collapse: {counts:?}"
    );
}

#[test]
fn record_complete_collapse_frames_for_original_path_comparison() {
    let mut evidence = std::collections::BTreeMap::new();
    for language in [Language::English, Language::Chinese] {
        let mut atlas =
            TextAtlas::from_system_language(12., language == Language::Chinese).unwrap();
        for size in [[1200., 800.], [1600., 1000.]] {
            let area = Rect::new(13., 17., size[0], size[1]);
            for zoom in [0.55, 1.35] {
                for fixture in [Fixture::Plain, Fixture::Defaults, Fixture::Group] {
                    let (mut editor, target) = prepare(fixture, zoom);
                    draw(&mut editor, &mut atlas, area, Input::default(), language);
                    let before = editor.snapshot();
                    let mut frames = vec![];
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
                                editor.undo.len(),
                                editor.redo.len()
                            )
                        ));
                    };
                    record(&mut editor, Input::default());
                    for _ in 0..2 {
                        let point = collapse_point(&editor, &target);
                        record(&mut editor, press(point, MouseButton::Left));
                        record(&mut editor, release(point, MouseButton::Left));
                    }
                    for _ in 0..2 {
                        assert!(editor.undo());
                        record(&mut editor, Input::default());
                    }
                    assert_eq!(editor.document, before.document);
                    for _ in 0..2 {
                        assert!(editor.redo());
                        record(&mut editor, Input::default());
                    }
                    assert_eq!(editor.document, before.document);
                    assert_eq!(frames.len(), 9);
                    assert_ne!(frames[0], frames[1]);
                    evidence.insert(format!("{language:?}-{size:?}-{zoom}-{fixture:?}"), frames);
                }
            }
        }
    }
    assert_eq!(evidence.len(), 24);
    if let Some(path) = std::env::var_os("MOTION_NODE_COLLAPSE_EVIDENCE") {
        std::fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    }
    println!("complete collapse frames=216; actual collapse/open transitions=48");
}

#[test]
fn presentation_refresh_matches_full_rebuild_for_builtin_empty_and_invalid_port_shapes() {
    let examples = motion_graph::builtin_examples();
    let mut editors: Vec<_> = examples
        .iter()
        .map(|example| NodeEditor::new(example.load().unwrap()))
        .collect();
    let mut invalid = simple();
    invalid
        .document
        .nodes
        .iter_mut()
        .find(|n| n.id == "math")
        .unwrap()
        .operation = NodeOp::Parameter {
        parameter: "missing".into(),
    };
    editors.push(invalid);
    let mut empty = simple();
    empty
        .document
        .nodes
        .iter_mut()
        .find(|n| n.id == "math")
        .unwrap()
        .operation = NodeOp::GroupInput {
        ports: Default::default(),
    };
    editors.push(empty);
    let mut checked = 0;
    for editor in &mut editors {
        for zoom in [0.33, 0.85, 2.75] {
            editor.view.zoom = zoom;
            let canvas = Rect::new(17., 33., 1200., 800.);
            let mut cards = editor.cards(canvas);
            for collapsed in [true, false] {
                for node in &mut editor.document.nodes {
                    node.collapsed = collapsed;
                }
                assert!(editor.refresh_card_positions(&mut cards, canvas));
                let fresh = editor.cards(canvas);
                assert_eq!(geometry(&cards), geometry(&fresh));
                for (card, original) in cards.iter().zip(&fresh) {
                    assert_eq!(card.world_height.to_bits(), original.world_height.to_bits());
                    assert_eq!(card.collapsed, original.collapsed);
                    assert_eq!(card.expanded_rows, original.expanded_rows);
                }
                checked += 1;
            }
            if let Some(last) = cards.last_mut() {
                last.source_node = std::ptr::null();
                let before = geometry(&cards);
                editor.document.nodes[0].collapsed = true;
                assert!(!editor.refresh_card_positions(&mut cards, canvas));
                assert_eq!(
                    geometry(&cards),
                    before,
                    "identity rejection cannot partially update cards"
                );
            }
        }
    }
    assert_eq!(checked, (examples.len() + 2) * 6);
    assert!(checked > 0);
    println!("original rebuild geometry comparisons={checked}");
}

#[test]
fn nonsemantic_group_port_rename_rejects_replaced_identity_before_refresh() {
    let (mut editor, target) = prepare(Fixture::Group, 0.85);
    let canvas = Rect::new(17., 33., 1200., 800.);
    let mut cards = editor.cards(canvas);
    let port = cards
        .iter()
        .find(|c| c.id == target)
        .unwrap()
        .sockets
        .iter()
        .find(|s| s.output)
        .unwrap()
        .port
        .port
        .clone();
    let before = geometry(&cards);
    let revision = editor.revision;
    let preview = editor.preview_revision;
    editor
        .rename_group_port(&target, true, &port, "Renamed output")
        .unwrap();
    assert_eq!(editor.preview_revision, preview);
    assert_eq!(editor.revision, revision + 1);
    assert!(!editor.refresh_card_positions(&mut cards, canvas));
    assert_eq!(geometry(&cards), before);
}
