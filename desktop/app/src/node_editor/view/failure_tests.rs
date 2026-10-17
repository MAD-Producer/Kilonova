use super::super::tests::{frame, press, simple};
use super::*;
use motion_ui::input::Modifiers;

fn begin_moved_drag(editor: &mut NodeEditor, atlas: &mut TextAtlas) -> GraphDocument {
    frame(editor, atlas, Input::default());
    let before = editor.document.clone();
    let card = editor
        .cards(editor.last_canvas)
        .into_iter()
        .find(|c| c.id == "number")
        .unwrap();
    let point = [card.rect.min[0] + 80., card.rect.min[1] + 10.];
    frame(editor, atlas, press(point, MouseButton::Left));
    frame(
        editor,
        atlas,
        Input {
            mouse: [point[0] + 60., point[1] + 20.],
            held: Some(MouseButton::Left),
            ..Default::default()
        },
    );
    assert!(matches!(editor.drag, Some(Drag::Nodes { .. })));
    assert_ne!(editor.document, before);
    before
}

#[test]
fn canceling_node_movement_publishes_restoration_without_changing_semantics_or_history() {
    for reason in ["escape", "lost_button", "focus"] {
        let mut editor = simple();
        let mut atlas = TextAtlas::from_system(13.).unwrap();
        let before = begin_moved_drag(&mut editor, &mut atlas);
        let (revision, semantic, preview, undo) = (
            editor.revision,
            editor.semantic_revision,
            editor.preview_revision,
            editor.undo.len(),
        );
        let response = match reason {
            "escape" => Some(frame(
                &mut editor,
                &mut atlas,
                Input {
                    keys: vec![(Key::Escape, Modifiers::default())],
                    ..Default::default()
                },
            )),
            "lost_button" => Some(frame(&mut editor, &mut atlas, Input::default())),
            _ => {
                editor.cancel_interaction();
                None
            }
        };
        assert_eq!(editor.document, before, "{reason}");
        assert!(editor.drag.is_none(), "{reason}");
        assert_ne!(editor.revision, revision, "{reason}");
        assert_eq!(editor.semantic_revision, semantic, "{reason}");
        assert_eq!(editor.preview_revision, preview, "{reason}");
        assert_eq!(editor.undo.len(), undo, "{reason}");
        if let Some(response) = response {
            assert!(response.changed && !response.semantic_changed, "{reason}");
        }
    }
}

#[test]
fn shrinking_canvas_cancels_node_drag_and_reports_restoration() {
    let mut editor = simple();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    let before = begin_moved_drag(&mut editor, &mut atlas);
    let mut paint = PaintList::default();
    let response = editor.draw(
        &mut paint,
        &mut atlas,
        Rect::new(0., 0., 200., 300.),
        &Input {
            held: Some(MouseButton::Left),
            ..Default::default()
        },
        Language::English,
    );
    assert_eq!(editor.document, before);
    assert!(!editor.editing());
    assert!(response.changed && !response.semantic_changed);
}

#[test]
fn shrinking_canvas_cancels_a_held_nudge_without_committing_an_undo_step() {
    let mut editor = simple();
    editor.focus_panel();
    let before = editor.document.clone();
    let undo = editor.undo.len();
    editor.keyboard(
        &Input {
            keys: vec![(Key::Right, Modifiers::default())],
            held_keys: vec![Key::Right],
            ..Default::default()
        },
        &mut Vec::new(),
    );
    assert!(editor.nudge_before.is_some());
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    let response = editor.draw(
        &mut PaintList::default(),
        &mut atlas,
        Rect::new(0., 0., 200., 300.),
        &Input {
            held_keys: vec![Key::Right],
            ..Default::default()
        },
        Language::English,
    );
    assert_eq!(editor.document, before);
    assert!(editor.nudge_before.is_none());
    assert_eq!(editor.undo.len(), undo);
    assert!(response.changed && !response.semantic_changed);
}


#[test]
fn file_shortcut_rejection_already_retains_the_invalid_field_and_visible_error() {
    let mut editor = simple();
    editor.focus_panel();
    let before = editor.document.clone();
    editor.field = Some(Field { binding: Binding::Constant("number".into(),0), text:"not a number".into(), cursor:12, replace:false, error:None });
    let mut actions = Vec::new();
    editor.keyboard(&Input {
        keys: vec![(Key::S, Modifiers { control:true,..Default::default() })],
        ..Default::default()
    }, &mut actions);
    assert!(actions.is_empty());
    assert_eq!(editor.document,before);
    let error = editor.field.as_ref().unwrap().error.as_deref().unwrap();
    assert_eq!(editor.notice.as_deref(),Some(error));
    assert_eq!(editor.field.as_ref().unwrap().text,"not a number");
}


#[test]
fn inspector_rejection_is_logged_without_mutating_the_node_or_its_history() {
    const CHILD: &str = "MOTION_INSPECTOR_LOG_TEST_CHILD";
    if let Some(root)=std::env::var_os(CHILD) {
        let _logger=crate::logging::init(std::path::PathBuf::from(root)).unwrap();
        let mut graph=motion_graph::demo_graph();
        graph.nodes.push(Node::new("extra_output",NodeOp::ColorOutput));
        graph.nodes.push(Node::new("toggle",NodeOp::Bool {value:false}));
        let mut editor=NodeEditor::new(graph);
        editor.select(Some("toggle"),false);
        let before=editor.snapshot();
        let history=editor.undo.len();
        let mut atlas=TextAtlas::from_system(13.).unwrap();
        editor.draw_inspector(&mut PaintList::default(),&mut atlas,Rect::new(0.,0.,360.,600.),
            &press([20.,140.],MouseButton::Left),Language::English);
        assert_eq!(editor.document,before.document);
        assert_eq!(editor.revision,before.revision);
        assert_eq!(editor.undo.len(),history);
        return;
    }
    let root=tempfile::tempdir().unwrap();
    let output=std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact","node_editor::view::failure_tests::inspector_rejection_is_logged_without_mutating_the_node_or_its_history"])
        .env(CHILD,root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
    let text=std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    assert!(text.contains("node inspector edit rejected: revision="));
    assert!(text.contains("MultipleOutputs at node toggle"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("node inspector edit rejected:"));
}

#[test]
fn invalid_neighbour_ports_keep_the_cause_when_incompatible_connections_are_removed() {
    const CHILD: &str = "MOTION_NODE_PORT_LOG_TEST_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let _logger = crate::logging::init(std::path::PathBuf::from(root)).unwrap();
        let mut editor = simple();
        editor.document.nodes.push(Node::new(
            "missing_parameter",
            NodeOp::Parameter {
                parameter: "deleted_definition".into(),
            },
        ));
        editor.document.connections.push(motion_graph::Connection {
            from: motion_graph::PortRef {
                node: "missing_parameter".into(),
                port: "value".into(),
            },
            to: motion_graph::PortRef {
                node: "math".into(),
                port: "a".into(),
            },
        });
        let before = editor.document.clone();
        editor
            .set_operation(
                "math",
                NodeOp::Math {
                    operation: MathOp::Multiply,
                    value_type: ValueType::Float,
                },
            )
            .unwrap();
        assert!(!editor
            .document
            .connections
            .iter()
            .any(|c| c.from.node == "missing_parameter"));
        assert!(editor.notice.as_deref().unwrap().contains("incompatible"));
        assert!(editor.undo());
        assert_eq!(editor.document, before);
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "node_editor::view::failure_tests::invalid_neighbour_ports_keep_the_cause_when_incompatible_connections_are_removed"])
        .env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    assert!(text.contains(
        "node ports unavailable during operation change: edited_node=math node=missing_parameter"
    ));
    assert!(text.contains("deleted_definition"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("node ports unavailable"));
}
