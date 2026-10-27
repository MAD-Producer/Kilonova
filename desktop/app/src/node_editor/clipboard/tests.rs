use super::*;
use motion_ui::input::{Key, Modifiers};
pub(super) fn editor() -> NodeEditor {
    let mut e = super::super::tests::simple();
    e.last_canvas = Rect::new(0., 0., 1200., 700.);
    e.document
        .connections
        .push(Connection::new("number", "value", "math", "a"));
    e.selected = ["number".into(), "math".into()].into();
    e.primary = Some("number".into());
    e.focus_panel();
    e
}
fn command(e: &mut NodeEditor, key: Key, language: Language) {
    let input = Input {
        keys: vec![(
            key,
            Modifiers {
                control: true,
                ..Default::default()
            },
        )],
        mouse: [600., 300.],
        ..Default::default()
    };
    let mut list = PaintList::default();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    e.draw(
        &mut list,
        &mut atlas,
        Rect::new(0., 0., 1200., 700.),
        &input,
        language,
    );
}
#[test]
fn copy_paste_keeps_internal_wires_detaches_external_and_undoes_once() {
    let mut e = editor();
    e.document.nodes.push(Node::new(
        "outside",
        NodeOp::Math {
            operation: MathOp::Add,
            value_type: ValueType::Float,
        },
    ));
    e.document
        .connections
        .push(Connection::new("math", "value", "outside", "a"));
    let before = e.document.clone();
    let revision = e.revision;
    command(&mut e, Key::C, Language::Chinese);
    assert_eq!(e.document, before);
    assert_eq!(e.revision, revision);
    assert_eq!(e.clipboard.as_ref().unwrap().document.connections.len(), 1);
    command(&mut e, Key::V, Language::Chinese);
    assert_eq!(e.document.nodes.len(), before.nodes.len() + 2);
    assert_eq!(e.document.connections.len(), before.connections.len() + 1);
    assert_eq!(e.selected.len(), 2);
    assert!(e
        .selected
        .iter()
        .all(|id| !before.nodes.iter().any(|n| &n.id == id)));
    let copy = e.document.clone();
    assert!(e.undo());
    assert_eq!(e.document, before);
    assert!(e.redo());
    assert_eq!(e.document, copy);
    let first = e.selected.clone();
    command(&mut e, Key::V, Language::English);
    assert!(first.is_disjoint(&e.selected));
    assert!(e.notice.as_ref().unwrap().contains("Nodes pasted"));
}
#[test]
fn cut_caches_before_delete_and_clipboard_survives_undo_and_opening_other_graph() {
    let mut e = editor();
    let before = e.document.clone();
    command(&mut e, Key::X, Language::English);
    assert_eq!(e.document.nodes.len(), 2);
    assert_eq!(e.clipboard.as_ref().unwrap().document.nodes.len(), 2);
    assert!(e.undo());
    assert_eq!(e.document, before);
    let destination = super::super::tests::simple().document;
    e.replace_document(destination.clone(), None);
    assert!(e.clipboard.is_some());
    e.paste_nodes([600., 300.]).unwrap();
    assert_eq!(e.document.nodes.len(), destination.nodes.len() + 2);
    assert!(e.undo());
    assert_eq!(e.document, destination);
}
#[test]
fn nested_groups_remap_interfaces_and_public_parameters_without_aliases() {
    let mut e = editor();
    let parameter = PublicParameter {
        id: "amount".into(),
        name: "数值".into(),
        english_name: "Amount".into(),
        value_type: ValueType::Float,
        default: [0.7, 0., 0., 0.],
        min: 0.,
        max: 1.,
        step: 0.01,
        units: "px".into(),
        animatable: true,
    };
    e.document.public_parameters.push(parameter.clone());
    e.document.nodes.push(Node::new(
        "parameter",
        NodeOp::Parameter {
            parameter: "amount".into(),
        },
    ));
    e.selected = ["parameter".into(), "math".into()].into();
    e.primary = Some("parameter".into());
    assert!(e.group_selected().unwrap());
    let inner = e.primary.clone().unwrap();
    assert!(e.group_selected().unwrap());
    let group = e.primary.clone().unwrap();
    let original = e.document.clone();
    e.copy_nodes(false).unwrap();
    e.paste_nodes([500., 300.]).unwrap();
    assert_eq!(e.document.public_parameters.len(), 2);
    let pasted = e.primary.clone().unwrap();
    assert_ne!(pasted, group);
    let NodeOp::Group { graph } = &e
        .document
        .nodes
        .iter()
        .find(|n| n.id == pasted)
        .unwrap()
        .operation
    else {
        panic!("group missing")
    };
    assert!(graph.nodes.iter().any(|n| n.id == graph.input_node));
    assert!(graph.nodes.iter().any(|n| n.id == graph.output_node));
    assert!(graph.nodes.iter().all(|n| n.id != inner));
    let mut refs = BTreeSet::new();
    references(&graph.nodes, &mut refs);
    assert_eq!(refs.len(), 1);
    assert!(!refs.contains("amount"));
    let copied = e
        .document
        .public_parameters
        .iter()
        .find(|p| p.id != "amount")
        .unwrap();
    let mut expected = parameter;
    expected.id = copied.id.clone();
    assert_eq!(*copied, expected);
    motion_graph::validate(&e.document, &e.options).unwrap();
    assert!(e.undo());
    assert_eq!(e.document, original);
    e.select(Some(&group), false);
    assert!(e.enter_group(&group));
    e.select(
        Some(
            &e.document
                .nodes
                .iter()
                .find(|n| grouping::boundary(&n.operation))
                .unwrap()
                .id
                .clone(),
        ),
        false,
    );
    let previous = e.clipboard.as_ref().unwrap().document.clone();
    assert!(e.copy_nodes(true).is_err());
    assert_eq!(e.clipboard.as_ref().unwrap().document, previous);
}
#[test]
fn invalid_pastes_preserve_document_selection_allocator_history_and_clipboard() {
    let mut e = editor();
    e.copy_nodes(false).unwrap();
    for fault in ["budget", "type", "cycle", "colour"] {
        let original_clip = e.clipboard.clone();
        let options = e.options.clone();
        match fault {
            "budget" => e.options.max_nodes = e.document.nodes.len(),
            "type" => {
                e.clipboard
                    .as_mut()
                    .unwrap()
                    .document
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == "number")
                    .unwrap()
                    .operation = NodeOp::Color {
                    value: [0.1, 0.2, 0.3, 1.],
                };
            }
            "cycle" => e
                .clipboard
                .as_mut()
                .unwrap()
                .document
                .connections
                .push(Connection::new("math", "value", "math", "b")),
            "colour" => {
                e.clipboard.as_mut().unwrap().document.alpha_mode =
                    motion_effects::AlphaMode::Straight;
                e.document.alpha_mode = motion_effects::AlphaMode::Premultiplied;
            }
            _ => unreachable!(),
        }
        let before = e.document.clone();
        let selected = e.selected.clone();
        let counter = e.next_id;
        let undo = e.undo.len();
        let revision = e.revision;
        assert!(e.paste_nodes([300.; 2]).is_err(), "{fault}");
        assert_eq!(e.document, before);
        assert_eq!(e.selected, selected);
        assert_eq!(e.next_id, counter);
        assert_eq!(e.undo.len(), undo);
        assert_eq!(e.revision, revision);
        assert_eq!(e.clipboard.as_ref().unwrap().pastes, 0);
        e.clipboard = original_clip;
        e.options = options;
    }
}
#[test]
fn output_can_move_to_an_empty_graph_but_never_creates_a_second_output() {
    let mut e = editor();
    e.selected = ["color".into(), "output".into()].into();
    e.copy_nodes(false).unwrap();
    let original = e.document.clone();
    assert_eq!(
        e.paste_nodes([0.; 2]).unwrap_err().code,
        ErrorCode::MultipleOutputs
    );
    assert_eq!(e.document, original);
    let mut empty = original;
    empty.nodes.clear();
    empty.connections.clear();
    e.replace_document(empty, None);
    e.paste_nodes([100.; 2]).unwrap();
    e.snapshot().compile().unwrap();
}
#[test]
fn text_search_rename_and_unfocused_inputs_do_not_cut_or_paste_nodes() {
    let mut e = editor();
    e.copy_nodes(false).unwrap();
    let before = e.document.clone();
    e.focus = false;
    command(&mut e, Key::X, Language::English);
    assert_eq!(e.document, before);
    e.focus_panel();
    e.search_focus = true;
    command(&mut e, Key::V, Language::English);
    assert_eq!(e.document, before);
    e.search_focus = false;
    e.field = Some(Field {
        binding: Binding::Constant("number".into(), 0),
        text: "1".into(),
        cursor: 1,
        replace: false,
        editor: Default::default(),
        error: None,
    });
    command(&mut e, Key::X, Language::Chinese);
    assert_eq!(e.document, before);
    assert_eq!(
        crate::shortcuts::resolve(
            crate::shortcuts::Scope::Nodes,
            Key::C,
            Modifiers {
                control: true,
                ..Default::default()
            },
            true
        ),
        None
    );
    assert_eq!(
        crate::shortcuts::resolve(
            crate::shortcuts::Scope::Application,
            Key::V,
            Modifiers {
                control: true,
                ..Default::default()
            },
            false
        ),
        None
    );
}
#[test]
fn pasted_subgraph_save_reopen_compile_and_export_are_real() {
    let mut e = editor();
    e.selected = ["color".into()].into();
    e.copy_nodes(false).unwrap();
    e.paste_nodes([550., 100.]).unwrap();
    let id = e.primary.clone().unwrap();
    e.connect_ports(PortRef::new(id, "value"), PortRef::new("output", "color"))
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("copied.msgraph");
    e.snapshot().save(&path).unwrap();
    let restored = GraphDocument::load(&path).unwrap();
    assert_eq!(restored, e.document);
    let other = NodeEditor::new(restored);
    other.snapshot().compile().unwrap();
    other
        .snapshot()
        .export_msfx(&temp.path().join("copied.msfx"))
        .unwrap();
    assert!(
        std::fs::metadata(temp.path().join("copied.msfx"))
            .unwrap()
            .len()
            > 0
    );
}
#[test]
fn clipboard_failures_reach_default_file_stderr_and_never_stdout() {
    const CHILD: &str = "MOTION_NODE_CLIPBOARD_LOG_CHILD";
    if let Some(dir) = std::env::var_os(CHILD) {
        let _guard = crate::logging::init(dir.into()).unwrap();
        let mut e = editor();
        e.copy_nodes(false).unwrap();
        e.options.max_nodes = e.document.nodes.len();
        let before = e.document.clone();
        e.clipboard_command(Command::PasteNodes, Language::Chinese, [300.; 2]);
        assert_eq!(e.document, before);
        assert!(e.notice.as_ref().unwrap().contains("图未改变"));
        crate::logging::flush();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","node_editor::clipboard::tests::clipboard_failures_reach_default_file_stderr_and_never_stdout","--nocapture"]).env_remove("MOTION_LOG").env(CHILD,root.path()).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    for marker in [
        "node clipboard rejected operation=PasteNodes",
        "cause=Budget",
        "epoch=",
        "revision=",
    ] {
        assert!(file.contains(marker), "{file}");
        assert!(String::from_utf8_lossy(&output.stderr).contains(marker));
        assert!(!String::from_utf8_lossy(&output.stdout).contains(marker));
    }
}

#[cfg(all(target_os = "windows", feature = "gpu-tests"))]
#[path = "tests/native.rs"]
mod native;

#[test]
fn nested_depth_and_missing_copy_targets_preserve_the_previous_clipboard() {
    let mut e = editor();
    e.copy_nodes(false).unwrap();
    let previous = e.clipboard.as_ref().unwrap().document.clone();
    e.selected = ["missing-node".into()].into();
    assert!(e.copy_nodes(true).is_err());
    assert_eq!(e.clipboard.as_ref().unwrap().document, previous);
    let mut node = Node::new("leaf", NodeOp::Float { value: 1. });
    for i in 0..17 {
        node = Node::new(
            format!("group_{i}"),
            NodeOp::Group {
                graph: Box::new(motion_graph::NodeGroup {
                    name: "层级".into(),
                    english_name: "Nested".into(),
                    nodes: vec![
                        Node::new(
                            "in",
                            NodeOp::GroupInput {
                                ports: Default::default(),
                            },
                        ),
                        Node::new(
                            "out",
                            NodeOp::GroupOutput {
                                ports: Default::default(),
                            },
                        ),
                        node,
                    ],
                    connections: vec![],
                    input_node: "in".into(),
                    output_node: "out".into(),
                }),
            },
        );
    }
    let id = node.id.clone();
    e.document.nodes.push(node);
    e.selected = [id].into();
    assert_eq!(e.copy_nodes(false).unwrap_err().code, ErrorCode::Budget);
    assert_eq!(e.clipboard.as_ref().unwrap().document, previous);
    let mut next = editor();
    next.take_clipboard_from(&mut e);
    assert!(e.clipboard.is_none());
    assert_eq!(next.clipboard.as_ref().unwrap().document, previous);
    next.paste_nodes([300.; 2]).unwrap();
}

#[test]
fn main_and_floating_chrome_route_clipboard_once_and_preserve_ime_ownership() {
    use motion_ui::dock::Panel;
    let temp = tempfile::tempdir().unwrap();
    let host = crate::engine::Engine::start(
        temp.path().into(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let state = host.state().unwrap();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        for locale in [crate::i18n::Locale::Zh, crate::i18n::Locale::En] {
            let mut panels = crate::panels::Panels {
                focused: Some(Panel::Nodes),
                locale,
                ..Default::default()
            };
            panels.node_editor = editor();
            let before = panels.node_editor.document.clone();
            let mut render = |panels: &mut crate::panels::Panels, input: &Input| {
                let mut list = PaintList::default();
                if floating {
                    crate::panels::floating_chrome(
                        &mut list,
                        &mut atlas,
                        Panel::Nodes,
                        [1280., 800.],
                        panels,
                        &state,
                        input,
                    )
                } else {
                    crate::panels::chrome(
                        &mut list,
                        &mut atlas,
                        [1280., 800.],
                        &motion_ui::dock::Node::dock(Panel::Nodes.dock()),
                        panels,
                        &state,
                        input,
                    )
                }
            };
            for key in [Key::C, Key::V] {
                let actions = render(
                    &mut panels,
                    &Input {
                        keys: vec![(
                            key,
                            Modifiers {
                                control: true,
                                ..Default::default()
                            },
                        )],
                        ..Default::default()
                    },
                );
                assert!(!actions.iter().any(|a| matches!(
                    a,
                    crate::panels::Action::Edit(_)
                        | crate::panels::Action::EditBatch(_)
                        | crate::panels::Action::History(_)
                )));
            }
            assert_eq!(
                panels.node_editor.document.nodes.len(),
                before.nodes.len() + 2,
                "floating={floating} locale={locale:?}"
            );
            assert_eq!(panels.node_editor.revision, 1);
            for (composing, ime_consumed, focus_lost) in [
                (true, false, false),
                (false, true, false),
                (false, false, true),
            ] {
                let original = panels.node_editor.document.clone();
                render(
                    &mut panels,
                    &Input {
                        keys: vec![(
                            Key::X,
                            Modifiers {
                                control: true,
                                ..Default::default()
                            },
                        )],
                        composing,
                        ime_consumed,
                        focus_lost,
                        ..Default::default()
                    },
                );
                assert_eq!(panels.node_editor.document, original);
            }
            assert!(panels.node_editor.undo());
            assert_eq!(panels.node_editor.document, before);
        }
    }
}
