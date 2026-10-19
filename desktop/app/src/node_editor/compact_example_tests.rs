use super::*;

#[test]
fn compact_examples_keep_nested_edits_parameters_source_and_history_editable() {
    for (id, gradient) in [
        ("soft_sphere", "shadow_gradient"),
        ("soft_sphere_motion", "shadow_gradient"),
        ("floating_lights", "light_1_gradient"),
    ] {
        let original = motion_graph::builtin_examples()
            .iter()
            .find(|e| e.id == id)
            .unwrap()
            .load()
            .unwrap();
        let mut editor = NodeEditor::new(original.clone());
        assert!(editor.enter_group("effect"));
        assert_eq!(
            editor.snapshot().document,
            original,
            "navigation mutated {id}"
        );
        editor
            .set_input_default(gradient, "falloff", motion_graph::InputValue::Float(2.))
            .unwrap();
        let edited = editor.snapshot();
        assert_ne!(edited.document, original);
        assert_eq!(
            edited.document.public_parameters,
            original.public_parameters
        );
        edited.compile().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("nested-edit.msgraph");
        edited.save(&path).unwrap();
        assert_eq!(
            motion_graph::GraphDocument::load(&path).unwrap(),
            edited.document
        );
        assert!(editor.leave_group());
        assert_eq!(editor.snapshot().document, edited.document);
        assert!(editor.undo());
        assert_eq!(editor.snapshot().document, original);
        assert!(editor.redo());
        assert_eq!(editor.snapshot().document, edited.document);
        assert!(editor.enter_group("effect"));
        assert_eq!(editor.snapshot().document, edited.document);
        assert!(editor.leave_group());
    }
}
