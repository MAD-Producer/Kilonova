use super::*;
#[test]
fn image_drop_freezes_zoomed_canvas_coordinates_and_commits_one_owned_edit() {
    let mut e = NodeEditor::default();
    e.last_canvas = Rect::new(100., 80., 900., 600.);
    e.view.pan = [54., -23.];
    e.view.zoom = 1.7;
    for language in [Language::Chinese, Language::English] {
        let point = [460., 310.];
        let expected = e.view.screen_to_world(point, e.last_canvas);
        let Action::ImportImage {
            snapshot, position, ..
        } = e.image_drop("sample.png".into(), point, language).unwrap()
        else {
            panic!()
        };
        assert_eq!(position, expected);
        let image = crate::node_image_fixture::image();
        let id = e
            .accept_imported_image(&snapshot, image, position, "  示\n例.png  ")
            .unwrap()
            .unwrap();
        let node = e.document.nodes.iter().find(|n| n.id == id).unwrap();
        assert_eq!(node.position, expected);
        assert_eq!(node.display_name, "示例.png");
        assert_eq!(e.selected(), Some(id.as_str()));
        assert_eq!(e.document.images.len(), 1);
        assert_eq!(e.undo.len(), 1);
        assert!(e.undo());
        assert_eq!(e.document, snapshot.document);
        assert!(e.redo());
        assert!(e.undo());
    }
    assert!(
        e.image_drop("outside.png".into(), [0., 0.], Language::English)
            .is_none()
    );
}
#[test]
fn image_drop_rejects_stale_invalid_and_over_budget_results_without_partial_state() {
    let mut e = NodeEditor::default();
    let snapshot = e.snapshot();
    let image = crate::node_image_fixture::image();
    let before = (e.document.clone(), e.next_id, e.undo.len(), e.redo.len());
    assert!(
        e.accept_imported_image(&snapshot, image.clone(), [f32::NAN, 0.], "x")
            .is_err()
    );
    e.options.max_nodes = e.document.nodes.len();
    assert!(
        e.accept_imported_image(&snapshot, image.clone(), [0., 0.], "x")
            .is_err()
    );
    assert_eq!(
        (e.document.clone(), e.next_id, e.undo.len(), e.redo.len()),
        before
    );
    e.options = CompileOptions::default();
    e.renew_session();
    assert!(
        e.accept_imported_image(&snapshot, image.clone(), [0., 0.], "x")
            .unwrap()
            .is_none()
    );
    let snapshot = e.snapshot();
    e.preview_revision += 1;
    assert!(
        e.accept_imported_image(&snapshot, image, [0., 0.], "x")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        (e.document.clone(), e.next_id, e.undo.len(), e.redo.len()),
        before
    );
}
