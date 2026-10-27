use super::*;
use crate::node_image_fixture as fixture;

fn editor() -> NodeEditor {
    NodeEditor::new(fixture::graph(fixture::image()))
}
fn rejected(e: &mut NodeEditor, code: ErrorCode) {
    let before = e.snapshot();
    let selection = e.selected.clone();
    let allocator = e.next_id;
    let history = (e.undo.len(), e.redo.len());
    assert_eq!(
        e.add_texture_mapping(Language::English).unwrap_err().code,
        code
    );
    assert_eq!(e.document, before.document);
    assert_eq!(
        (e.epoch, e.revision, e.semantic_revision, e.preview_revision),
        (
            before.epoch,
            before.revision,
            before.semantic_revision,
            before.preview_revision
        )
    );
    assert_eq!(e.selected, selection);
    assert_eq!(e.next_id, allocator);
    assert_eq!((e.undo.len(), e.redo.len()), history);
    assert_eq!(e.preview_target, before.preview_target);
}
#[test]
fn texture_mapping_preserves_image_output_and_existing_uv_in_one_undoable_edit() {
    for language in [Language::Chinese, Language::English] {
        for target in ["image", "sample"] {
            let mut e = editor();
            e.select(Some(target), false);
            let before = e.snapshot();
            let mapping = e.add_texture_mapping(language).unwrap();
            assert_eq!(e.document.nodes.len(), before.document.nodes.len() + 1);
            assert!(e
                .document
                .connections
                .contains(&Connection::new("image", "image", "sample", "image")));
            assert!(e
                .document
                .connections
                .contains(&Connection::new("sample", "color", "output", "color")));
            assert!(e.document.connections.contains(&Connection::new(
                "uv",
                "value",
                &mapping,
                "coordinate"
            )));
            assert!(e.document.connections.contains(&Connection::new(
                &mapping,
                "value",
                "sample",
                "coordinate"
            )));
            assert_eq!(e.revision, before.revision + 1);
            let compiled = e.snapshot().compile_runtime().unwrap();
            let after = e.document.clone();
            assert!(e.undo());
            assert_eq!(e.document, before.document);
            assert!(!e.undo());
            assert!(e.redo());
            assert_eq!(e.document, after);
            e.set_input_default(&mapping, "rotation", InputValue::Float(30.))
                .unwrap();
            let numeric = e.snapshot().compile_runtime().unwrap();
            assert_eq!(compiled.shader.wgsl, numeric.shader.wgsl);
            assert_ne!(
                compiled.manifest.effects[0].shader_data,
                numeric.manifest.effects[0].shader_data
            );
            e.select(Some("sample"), false);
            rejected(&mut e, ErrorCode::InvalidValue);
        }
    }
}
#[test]
fn texture_mapping_keeps_authored_constants_and_only_creates_missing_coordinates() {
    for literal in [None, Some(InputValue::Vec2([0.2, 0.7]))] {
        let mut e = editor();
        e.document
            .connections
            .retain(|edge| edge.to != PortRef::new("sample", "coordinate"));
        if let Some(value) = &literal {
            e.document
                .nodes
                .iter_mut()
                .find(|n| n.id == "sample")
                .unwrap()
                .input_defaults
                .insert("coordinate".into(), value.clone());
        }
        e.select(Some("sample"), false);
        let before = e.document.nodes.len();
        let mapping = e.add_texture_mapping(Language::English).unwrap();
        let node = e.document.nodes.iter().find(|n| n.id == mapping).unwrap();
        assert_eq!(node.input_defaults.get("coordinate"), literal.as_ref());
        assert_eq!(
            e.document.nodes.len(),
            before + if literal.is_some() { 1 } else { 2 }
        );
        e.snapshot().compile().unwrap();
    }
}
#[test]
fn texture_mapping_uses_typed_group_image_output_without_replacing_canonical_color() {
    let mut e = editor();
    let group = motion_graph::NodeGroup {
        name: "图像来源".into(),
        english_name: "Image source".into(),
        input_node: "source_in".into(),
        output_node: "source_out".into(),
        nodes: vec![
            Node::new(
                "source_in",
                NodeOp::GroupInput {
                    ports: BTreeMap::new(),
                },
            ),
            Node::new(
                "host_source",
                NodeOp::Image {
                    source: ImageSource::Input,
                },
            ),
            Node::new(
                "source_out",
                NodeOp::GroupOutput {
                    ports: BTreeMap::from([(
                        "picture".into(),
                        motion_graph::GroupPort {
                            name: "图片".into(),
                            english_name: "Picture".into(),
                            value_type: ValueType::Image,
                            semantic: motion_graph::PortSemantic::Image,
                        },
                    )]),
                },
            ),
        ],
        connections: vec![Connection::new(
            "host_source",
            "image",
            "source_out",
            "picture",
        )],
    };
    e.document.nodes.push(Node::new(
        "group",
        NodeOp::Group {
            graph: Box::new(group),
        },
    ));
    e.select(Some("group"), false);
    let old_edges = e.document.connections.clone();
    let old_nodes = e.document.nodes.len();
    let before = e.document.clone();
    e.add_texture_mapping(Language::English).unwrap();
    assert_eq!(e.document.nodes.len(), old_nodes + 3);
    for edge in old_edges {
        assert!(e.document.connections.contains(&edge));
    }
    let target = e.preview_target.clone().unwrap();
    let sample = e
        .document
        .nodes
        .iter()
        .find(|n| n.id == target.node)
        .unwrap();
    assert!(e
        .document
        .connections
        .contains(&Connection::new("group", "picture", &sample.id, "image")));
    e.snapshot().compile_preview().unwrap();
    e.snapshot().compile().unwrap();
    assert!(e.undo());
    assert_eq!(e.document, before);
    assert!(e.preview_target.is_none());
}
#[test]
fn texture_mapping_rejects_ambiguous_pixel_invalid_and_exhausted_targets_without_partial_edits() {
    let mut e = editor();
    e.select(Some("image"), false);
    e.document.nodes.push(Node::new(
        "another_sample",
        NodeOp::SampleImage {
            coordinates: SampleCoordinates::Uv,
        },
    ));
    e.document
        .connections
        .push(Connection::new("image", "image", "another_sample", "image"));
    rejected(&mut e, ErrorCode::InvalidValue);
    e.select(Some("sample"), false);
    e.add_texture_mapping(Language::English).unwrap();
    let mut e = editor();
    e.document
        .nodes
        .iter_mut()
        .find(|n| n.id == "sample")
        .unwrap()
        .operation = NodeOp::SampleImage {
        coordinates: SampleCoordinates::Pixels,
    };
    e.document
        .nodes
        .iter_mut()
        .find(|n| n.id == "uv")
        .unwrap()
        .operation = NodeOp::Position;
    e.select(Some("sample"), false);
    rejected(&mut e, ErrorCode::InvalidValue);
    let mut e = editor();
    e.select(Some("output"), false);
    rejected(&mut e, ErrorCode::InvalidValue);
    e.select(Some("sample"), false);
    e.options.max_nodes = e.document.nodes.len();
    rejected(&mut e, ErrorCode::Budget);
    e.options = CompileOptions::default();
    e.next_id = u64::MAX;
    rejected(&mut e, ErrorCode::Budget);
    e.next_id = 1;
    e.document
        .connections
        .push(Connection::new("image", "image", "sample", "coordinate"));
    rejected(&mut e, ErrorCode::TypeMismatch);
}

#[test]
fn texture_mapping_shortcut_respects_focus_ime_modifiers_and_repeat_in_both_languages() {
    use motion_ui::input::{Key, Modifiers};
    let area = Rect::new(0., 0., 1200., 700.);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for language in [Language::Chinese, Language::English] {
        for case in [
            "active",
            "unfocused",
            "composing",
            "consumed",
            "lost",
            "repeat",
            "alt",
            "search",
        ] {
            let mut e = editor();
            e.draw(
                &mut PaintList::default(),
                &mut atlas,
                area,
                &Input::default(),
                language,
            );
            e.select(Some("sample"), false);
            if case != "unfocused" {
                e.focus_panel();
            }
            e.search_focus = case == "search";
            let before = e.snapshot();
            let input = Input {
                keys: vec![(
                    Key::T,
                    Modifiers {
                        control: true,
                        alt: case == "alt",
                        ..Default::default()
                    },
                )],
                key_repeats: vec![case == "repeat"],
                composing: case == "composing",
                ime_consumed: case == "consumed",
                focus_lost: case == "lost",
                ..Default::default()
            };
            e.draw(
                &mut PaintList::default(),
                &mut atlas,
                area,
                &input,
                language,
            );
            if case == "active" {
                assert_eq!(e.revision, before.revision + 1);
                assert_eq!(e.document.nodes.len(), before.document.nodes.len() + 1);
                assert!(e.undo());
            }
            assert_eq!(e.document, before.document, "{language:?}: {case}");
        }
    }
}
