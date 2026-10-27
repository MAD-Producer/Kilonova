use super::*;
use crate::node_image_fixture as fixture;

fn source() -> NodeEditor {
    let mut editor = NodeEditor::new(fixture::graph(fixture::image()));
    editor.select(Some("image"), false);
    editor.copy_nodes(false).unwrap();
    editor
}
#[test]
fn image_clipboard_transfers_owned_content_deduplicates_and_undoes_atomically() {
    let mut source = source();
    let image = source.document.images.values().next().unwrap().clone();
    let mut target = super::tests::editor();
    let before = target.snapshot().document;
    target.take_clipboard_from(&mut source);
    target.paste_nodes([400., 300.]).unwrap();
    assert_eq!(target.document.images.len(), 1);
    assert_eq!(
        target.document.images[image.id()].png().as_ptr(),
        image.png().as_ptr(),
        "history and clipboard share immutable bytes"
    );
    target.paste_nodes([400., 300.]).unwrap();
    assert_eq!(target.document.images.len(), 1);
    let after = target.snapshot().document;
    assert_eq!(after.referenced_images().len(), 1);
    assert!(target.undo());
    assert_eq!(target.document.images.len(), 1);
    assert!(target.undo());
    assert_eq!(target.snapshot().document, before);
    assert!(target.redo());
    assert!(target.redo());
    assert_eq!(target.snapshot().document, after);
}

#[test]
fn image_clipboard_cut_and_delete_release_unreferenced_content_and_undo_restores_it() {
    let mut editor = source();
    let before = editor.document.clone();
    editor.copy_nodes(true).unwrap();
    assert!(editor.document.images.is_empty());
    assert_eq!(editor.clipboard.as_ref().unwrap().document.images.len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.document, before);
    editor.select(Some("image"), false);
    assert!(editor.delete_selected());
    assert!(editor.document.images.is_empty());
    assert!(editor.undo());
    assert_eq!(editor.document, before);
}

#[test]
fn image_clipboard_refusal_preserves_allocator_history_and_all_resources() {
    let mut editor = super::tests::editor();
    editor.take_clipboard_from(&mut source());
    editor.clipboard.as_mut().unwrap().document.images.clear();
    let before = editor.snapshot().document;
    let allocator = editor.next_id;
    let history = editor.undo.len();
    assert_eq!(
        editor.paste_nodes([400., 300.]).unwrap_err().code,
        ErrorCode::InvalidValue
    );
    assert_eq!(editor.snapshot().document, before);
    assert_eq!(editor.next_id, allocator);
    assert_eq!(editor.undo.len(), history);
    assert_eq!(editor.clipboard.as_ref().unwrap().pastes, 0);
}

#[test]
fn image_clipboard_in_nested_scope_propagates_pool_and_format_and_preserves_outer_references() {
    let mut editor = super::tests::editor();
    assert!(editor.group_selected().unwrap());
    let group = editor.primary.clone().unwrap();
    assert!(editor.enter_group(&group));
    editor.take_clipboard_from(&mut source());
    editor.paste_nodes([400., 300.]).unwrap();
    let image = editor.document.images.keys().next().unwrap().clone();
    let snapshot = editor.snapshot();
    assert_eq!(
        snapshot.document.format_version,
        motion_graph::IMAGE_ASSETS_FORMAT_VERSION
    );
    snapshot.document.validate_images().unwrap();
    assert!(snapshot.document.images.contains_key(&image));
    assert!(editor.leave_group());
    assert!(editor.document.images.contains_key(&image));
    editor.document.nodes.push(Node::new(
        "outer_image",
        NodeOp::ImageAsset {
            image: image.clone(),
        },
    ));
    assert!(editor.enter_group(&group));
    let id = editor
        .document
        .nodes
        .iter()
        .find(|n| matches!(n.operation, NodeOp::ImageAsset { .. }))
        .unwrap()
        .id
        .clone();
    editor.select(Some(&id), false);
    editor.copy_nodes(true).unwrap();
    assert!(
        editor.document.images.contains_key(&image),
        "an outer scope still owns this content"
    );
    assert!(editor.leave_group());
    editor.document.validate_images().unwrap();
    assert!(editor.undo());
    assert!(editor.snapshot().document.images.contains_key(&image));
}
