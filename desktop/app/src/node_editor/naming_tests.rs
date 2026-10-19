use super::naming::RenameTarget;
use super::tests::{frame, press, simple};
use super::*;
use motion_ui::input::{Key, MouseButton};

fn key(key: Key) -> Input {
    Input {
        keys: vec![(key, Default::default())],
        ..Default::default()
    }
}
fn pack(editor: &mut NodeEditor) -> String {
    editor.select(Some("circle"), false);
    editor.select(Some("fill"), true);
    assert!(editor.group_selected().unwrap());
    editor.primary.clone().unwrap()
}

#[test]
fn aliases_roundtrip_with_one_undo_without_changing_identity_shader_or_preview() {
    let mut editor = simple();
    // The shared interaction fixture intentionally contains an unwired Math node.
    // This test compares valid compiled outputs, so remove that incomplete node.
    editor.document.nodes.retain(|n| n.id != "math");
    let before = editor.snapshot();
    let shader = before.compile().unwrap().source;
    let stamp = editor.stamp();
    editor.rename_node("color", "  球体颜色  ").unwrap();
    assert_eq!(editor.document.nodes[0].display_name, "球体颜色");
    assert_eq!(editor.document.nodes[0].id, "color");
    assert_eq!(editor.document.connections, before.document.connections);
    assert_eq!(editor.stamp(), stamp);
    assert_eq!(editor.semantic_revision, before.semantic_revision);
    assert_eq!(editor.snapshot().compile().unwrap().source, shader);
    let saved = editor.document.to_json().unwrap();
    assert_eq!(GraphDocument::from_json(&saved).unwrap(), editor.document);
    assert_eq!(
        catalog::node_title(&editor.document.nodes[0], &[], Language::English),
        "球体颜色"
    );
    editor.rename_node("color", "球体颜色").unwrap();
    assert_eq!(editor.undo.len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.document, before.document);
    assert!(editor.redo());
    editor.rename_node("color", "").unwrap();
    assert_eq!(
        catalog::node_title(&editor.document.nodes[0], &[], Language::English),
        catalog::operation_title(&editor.document.nodes[0].operation, Language::English)
    );
}

#[test]
fn invalid_names_and_missing_ports_leave_document_and_history_untouched() {
    let mut editor = simple();
    let before = editor.document.clone();
    for name in ["x\ny".to_owned(), "字".repeat(41), "a".repeat(121)] {
        assert!(editor.rename_node("color", &name).is_err());
    }
    assert!(editor.rename_node("missing", "Name").is_err());
    assert!(editor
        .rename_group_port("color", true, "value", "Name")
        .is_err());
    assert_eq!(editor.document, before);
    assert!(editor.undo.is_empty());
    assert_eq!(editor.revision, 0);
}

#[test]
fn group_and_port_names_keep_boundary_keys_types_connections_and_shader() {
    let mut editor = NodeEditor::default();
    let id = pack(&mut editor);
    let before = editor.snapshot();
    let shader = before.compile().unwrap().source;
    let ports = motion_graph::node_ports(
        editor.document.nodes.iter().find(|n| n.id == id).unwrap(),
        &editor.document.public_parameters,
    )
    .unwrap();
    let input = ports.inputs.keys().next().unwrap().clone();
    let output = ports.outputs.keys().next().unwrap().clone();
    let stamp = editor.stamp();
    editor.rename_node(&id, "Soft Sphere").unwrap();
    editor
        .rename_group_port(&id, false, &input, "球体位置")
        .unwrap();
    editor
        .rename_group_port(&id, true, &output, "颜色结果")
        .unwrap();
    let renamed = editor.document.clone();
    let node = renamed.nodes.iter().find(|n| n.id == id).unwrap();
    assert_eq!(
        motion_graph::node_ports(node, &renamed.public_parameters)
            .unwrap()
            .inputs,
        ports.inputs
    );
    assert_eq!(
        motion_graph::node_ports(node, &renamed.public_parameters)
            .unwrap()
            .outputs,
        ports.outputs
    );
    assert_eq!(
        motion_graph::group_port(&node.operation, &input, false)
            .unwrap()
            .name,
        "球体位置"
    );
    assert_eq!(editor.document.connections, before.document.connections);
    assert_eq!(editor.stamp(), stamp);
    assert_eq!(editor.snapshot().compile().unwrap().source, shader);
    assert!(editor.enter_group(&id));
    assert_eq!(editor.document.name, "Soft Sphere");
    let (boundary, key) = editor
        .document
        .nodes
        .iter()
        .find_map(|n| {
            if let NodeOp::GroupInput { ports } = &n.operation {
                Some((n.id.clone(), ports.keys().next().unwrap().clone()))
            } else {
                None
            }
        })
        .unwrap();
    editor
        .rename_group_port(&boundary, true, &key, "Position")
        .unwrap();
    let inner = editor.document.clone();
    assert!(editor
        .rename_group_port(&boundary, false, &key, "Wrong direction")
        .is_err());
    assert!(editor
        .rename_group_port(&boundary, true, "missing", "Missing")
        .is_err());
    assert!(editor
        .rename_group_port(&boundary, true, &key, " ")
        .is_err());
    assert_eq!(editor.document, inner);
    assert!(editor.leave_group());
    let node = editor.document.nodes.iter().find(|n| n.id == id).unwrap();
    assert_eq!(
        motion_graph::group_port(&node.operation, &input, false)
            .unwrap()
            .english_name,
        "Position"
    );
    assert_eq!(editor.snapshot().compile().unwrap().source, shader);
    assert!(editor.undo());
    assert_eq!(editor.root_document(), renamed);
}

#[test]
fn generated_group_and_parameter_labels_are_human_readable_and_unique() {
    let mut editor = NodeEditor::default();
    let id = pack(&mut editor);
    let node = editor.document.nodes.iter().find(|n| n.id == id).unwrap();
    let NodeOp::Group { graph } = &node.operation else {
        panic!()
    };
    assert_eq!(graph.english_name, "Node Group 1");
    assert_eq!(editor.next_group_names().1, "Node Group 2");
    let input = graph.ports(false).unwrap().values().next().unwrap();
    assert!(!input.name.contains("circle"));
    assert!(!input.english_name.contains("circle ·"));
    let first = editor
        .add_public_parameter(ValueType::Float, [0., 0.])
        .unwrap();
    let second = editor
        .add_public_parameter(ValueType::Float, [0., 0.])
        .unwrap();
    for (id, name) in [(&first, "Value 1"), (&second, "Value 2")] {
        let node = editor.document.nodes.iter().find(|n| &n.id == id).unwrap();
        assert_eq!(
            catalog::node_title(node, &editor.document.public_parameters, Language::English),
            name
        );
        let NodeOp::Parameter { parameter } = &node.operation else {
            panic!()
        };
        assert!(parameter.starts_with("parameter_"));
    }
    // Either localized name can reserve a number; never produce a collision.
    let last = editor.document.public_parameters.last_mut().unwrap();
    last.name = "数值 3".into();
    assert_eq!(
        editor.next_parameter_names(ValueType::Float),
        ("数值 4".into(), "Value 4".into())
    );
}

#[test]
fn context_menu_opens_native_rename_and_commit_is_one_transaction() {
    let mut editor = simple();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let point = editor.view.world_to_screen([80., 90.], editor.last_canvas);
    frame(&mut editor, &mut atlas, press(point, MouseButton::Right));
    assert!(editor.context_menu.is_some());
    frame(&mut editor, &mut atlas, key(Key::Enter));
    assert!(editor.rename.is_some());
    assert!(editor.undo.is_empty());
    frame(
        &mut editor,
        &mut atlas,
        Input {
            text: "主色".into(),
            ..Default::default()
        },
    );
    frame(&mut editor, &mut atlas, key(Key::Enter));
    assert!(editor.rename.is_none());
    assert_eq!(editor.document.nodes[0].display_name, "主色");
    assert_eq!(editor.undo.len(), 1);
    assert!(editor.undo());
    assert!(editor.document.nodes[0].display_name.is_empty());
}

#[test]
fn rename_popup_cancellation_ime_and_limits_preserve_uncommitted_state() {
    let mut editor = simple();
    let before = editor.document.clone();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    for cancel in [
        key(Key::Escape),
        Input {
            focus_lost: true,
            ..Default::default()
        },
        press([1400., 800.], MouseButton::Left),
    ] {
        editor.begin_rename(
            RenameTarget::Node("color".into()),
            [100., 100.],
            Language::English,
        );
        frame(&mut editor, &mut atlas, Input::default());
        frame(
            &mut editor,
            &mut atlas,
            Input {
                text: "Uncommitted".into(),
                ..Default::default()
            },
        );
        frame(&mut editor, &mut atlas, cancel);
        assert!(editor.rename.is_none());
        assert_eq!(editor.document, before);
        assert!(editor.undo.is_empty());
    }
    editor.begin_rename(
        RenameTarget::Node("color".into()),
        [100., 100.],
        Language::English,
    );
    frame(&mut editor, &mut atlas, Input::default());
    let mut ime = key(Key::Enter);
    ime.ime_consumed = true;
    ime.text = "中文".into();
    frame(&mut editor, &mut atlas, ime);
    assert!(editor.rename.is_some());
    assert_eq!(editor.document, before);
    frame(&mut editor, &mut atlas, key(Key::Enter));
    assert_eq!(editor.document.nodes[0].display_name, "中文");
    editor.begin_rename(
        RenameTarget::Node("color".into()),
        [100., 100.],
        Language::English,
    );
    frame(&mut editor, &mut atlas, Input::default());
    frame(
        &mut editor,
        &mut atlas,
        Input {
            text: "字".repeat(41),
            ..Default::default()
        },
    );
    frame(&mut editor, &mut atlas, key(Key::Enter));
    assert!(editor.rename.as_ref().unwrap().editor.error.is_some());
    assert_eq!(editor.document.nodes[0].display_name, "中文");
    assert_eq!(
        editor.rename.as_ref().unwrap().editor.text.chars().count(),
        41
    );
}
