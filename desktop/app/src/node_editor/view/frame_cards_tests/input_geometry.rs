use super::*;

#[derive(Clone, Copy, Debug)]
enum Edit {
    Library,
    Delete,
    Undo,
    Parent,
}

fn toolbar_button(editor: &NodeEditor, area: Rect, target: usize) -> Rect {
    let mut x = area.min[0] + 6.;
    let mut y = area.min[1] + 5.;
    for (id, width, _, _) in editor.toolbar_items() {
        if x + width > area.max[0] - 6. {
            x = area.min[0] + 6.;
            y += 34.;
        }
        if id == target {
            return Rect::new(x, y, width, 27.);
        }
        x += width + 4.;
    }
    panic!("missing toolbar item {target}");
}

fn prepare(
    edit: Edit,
    atlas: &mut TextAtlas,
    area: Rect,
    language: Language,
) -> (NodeEditor, Input) {
    let mut editor = simple();
    editor.focus_panel();
    match edit {
        Edit::Undo => {
            editor
                .add_node(NodeOp::Float { value: 7. }, [600., 350.])
                .unwrap();
        }
        Edit::Delete => editor.select(Some("number"), false),
        Edit::Parent => {
            editor.select(Some("number"), false);
            editor.select(Some("math"), true);
            assert!(editor.group_selected().unwrap());
            let group = editor.primary.clone().unwrap();
            assert!(editor.enter_group(&group));
        }
        Edit::Library => editor.library_open = true,
    }
    draw(&mut editor, atlas, area, Input::default(), language);
    let layout = editor.guided_layout(area, PaintList::default().styles().as_ref());
    let input = match edit {
        Edit::Undo => press(
            center(toolbar_button(&editor, layout.toolbar, 9)),
            MouseButton::Left,
        ),
        Edit::Parent => press(
            center(Rect::new(
                layout.toolbar.min[0] + 6.,
                layout.toolbar.max[1] - 27.,
                85.,
                25.,
            )),
            MouseButton::Left,
        ),
        Edit::Delete => {
            let inspector = layout.inspector.unwrap();
            let styles = PaintList::default().styles();
            let pad = styles.metric("padding.panel", 8.);
            let row = styles.metric("node.dropdown.row.height", 28.);
            let content = inspector.width() - pad * 2.;
            let width = styles.metric("inspector.action.width", 112.).min(content);
            press(
                [
                    inspector.max[0] - pad - width * 0.5,
                    inspector.min[1] + styles.metric("panel.header.height", 24.) + pad + row * 0.5,
                ],
                MouseButton::Left,
            )
        }
        Edit::Library => {
            let library = layout.library.unwrap();
            let styles = PaintList::default().styles();
            let point = [
                library.min[0] + styles.metric("padding.panel", 8.) + 15.,
                library.min[1]
                    + styles.metric("padding.panel", 8.)
                    + styles.metric("row.height", 22.)
                    + styles.metric("padding", 5.)
                    + 10.,
            ];
            draw(
                &mut editor,
                atlas,
                area,
                press(point, MouseButton::Left),
                language,
            );
            assert!(editor.search_focus);
            draw(
                &mut editor,
                atlas,
                area,
                Input {
                    text: "parameter".into(),
                    ..Default::default()
                },
                language,
            );
            assert_eq!(&*editor.search, "parameter");
            Input {
                keys: vec![(Key::Enter, Default::default())],
                ..Default::default()
            }
        }
    };
    (editor, input)
}

fn verify_and_restore(
    edit: Edit,
    editor: &mut NodeEditor,
    before: &super::super::super::EditorSnapshot,
    undo: usize,
) {
    match edit {
        Edit::Library => {
            assert_eq!(editor.document.nodes.len(), before.document.nodes.len() + 1);
            assert_eq!(editor.undo.len(), undo + 1);
            assert!(editor.undo());
        }
        Edit::Delete => {
            assert!(!editor.document.nodes.iter().any(|n| n.id == "number"));
            assert_eq!(editor.document.nodes.len() + 1, before.document.nodes.len());
            assert_eq!(editor.undo.len(), undo + 1);
            assert!(editor.undo());
        }
        Edit::Undo => {
            assert_eq!(editor.undo.len() + 1, undo);
            assert_eq!(editor.redo.len(), 1);
            assert_ne!(editor.document, before.document);
            assert!(editor.redo());
        }
        Edit::Parent => {
            assert!(editor.parents.is_empty());
            assert_eq!(editor.undo.len(), undo);
            assert_eq!(editor.revision, before.revision);
            assert_ne!(editor.preview_revision, before.preview_revision);
        }
    }
    assert_eq!(editor.snapshot().document, before.document);
}

#[test]
fn edits_outside_the_canvas_build_only_the_final_document_geometry() {
    let mut counts = vec![];
    for language in [Language::English, Language::Chinese] {
        let mut atlas =
            TextAtlas::from_system_language(12., language == Language::Chinese).unwrap();
        for size in [[1200., 800.], [1600., 1000.]] {
            let area = Rect::new(13., 17., size[0], size[1]);
            for edit in [Edit::Library, Edit::Delete, Edit::Undo, Edit::Parent] {
                let (mut editor, input) = prepare(edit, &mut atlas, area, language);
                let before = editor.snapshot();
                let undo = editor.undo.len();
                let count = draw(&mut editor, &mut atlas, area, input, language);
                verify_and_restore(edit, &mut editor, &before, undo);
                counts.push((format!("{language:?}-{size:?}-{edit:?}"), count));
            }
        }
    }
    assert_eq!(counts.len(), 16);
    assert!(
        counts.iter().all(|(_, count)| *count == 1),
        "build only the final geometry: {counts:?}"
    );
}

#[test]
fn record_complete_outside_canvas_edits_for_original_path_comparison() {
    let mut evidence = std::collections::BTreeMap::new();
    for language in [Language::English, Language::Chinese] {
        let mut atlas =
            TextAtlas::from_system_language(12., language == Language::Chinese).unwrap();
        for size in [[1200., 800.], [1600., 1000.]] {
            let area = Rect::new(13., 17., size[0], size[1]);
            for edit in [Edit::Library, Edit::Delete, Edit::Undo, Edit::Parent] {
                let (mut editor, input) = prepare(edit, &mut atlas, area, language);
                let before = editor.snapshot();
                let undo = editor.undo.len();
                let point = input.mouse;
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
                record(&mut editor, input);
                record(&mut editor, release(point, MouseButton::Left));
                verify_and_restore(edit, &mut editor, &before, undo);
                record(&mut editor, Input::default());
                assert_eq!(frames.len(), 4);
                assert_ne!(frames[0], frames[1]);
                evidence.insert(format!("{language:?}-{size:?}-{edit:?}"), frames);
            }
        }
    }
    assert_eq!(evidence.len(), 16);
    if let Some(path) = std::env::var_os("MOTION_NODE_INPUT_EVIDENCE") {
        std::fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    }
    println!("outside canvas complete paint/state frames=64, changed controls=16");
}
