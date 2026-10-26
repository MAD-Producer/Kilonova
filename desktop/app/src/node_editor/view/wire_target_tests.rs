use super::super::tests::{frame, press, release, simple};
use super::*;

fn editor() -> NodeEditor {
    let mut editor = simple();
    editor.inspector_open = false;
    editor
}
fn key(key: Key) -> Input {
    Input {
        keys: vec![(key, Default::default())],
        ..Default::default()
    }
}
fn socket(editor: &NodeEditor, node: &str, port: &str, output: bool) -> [f32; 2] {
    editor
        .cards(editor.last_canvas)
        .iter()
        .flat_map(|card| &card.sockets)
        .find(|socket| socket.port == PortRef::new(node, port) && socket.output == output)
        .unwrap()
        .point
}
fn body(editor: &NodeEditor, node: &str) -> [f32; 2] {
    let card = editor
        .cards(editor.last_canvas)
        .into_iter()
        .find(|card| card.id == node)
        .unwrap();
    [
        (card.rect.min[0] + card.rect.max[0]) * 0.5,
        card.rect.min[1] + HEADER * editor.view.zoom * 0.5,
    ]
}
fn drag(editor: &mut NodeEditor, atlas: &mut TextAtlas, from: [f32; 2], to: [f32; 2]) {
    frame(editor, atlas, press(from, MouseButton::Left));
    frame(
        editor,
        atlas,
        Input {
            mouse: to,
            held: Some(MouseButton::Left),
            ..Default::default()
        },
    );
}

#[test]
fn nearest_dense_socket_is_shared_by_feedback_and_release_at_every_zoom() {
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    for zoom in [0.35, 0.5, 0.7, 1., 2.5] {
        let mut editor = editor();
        editor.view.zoom = zoom;
        if zoom > 1. {
            editor.view.pan[1] = -160.; // Keep both dense ports on screen at 250%.
        }
        frame(&mut editor, &mut atlas, Input::default());
        let a = socket(&editor, "math", "a", false);
        let b = socket(&editor, "math", "b", false);
        let point = [b[0], b[1] - (b[1] - a[1]).min(6.) * 0.25];
        let cards = editor.cards(editor.last_canvas);
        assert_eq!(
            wires::socket_at(&cards, editor.last_canvas, point, zoom)
                .unwrap()
                .port,
            PortRef::new("math", "b")
        );
        let from = socket(&editor, "number", "value", true);
        drag(&mut editor, &mut atlas, from, point);
        let cards = editor.cards(editor.last_canvas);
        let (feedback, result) = editor
            .wire_feedback(&cards, editor.last_canvas, point)
            .unwrap();
        assert_eq!(feedback.port, PortRef::new("math", "b"));
        result.unwrap();
        frame(&mut editor, &mut atlas, release(point, MouseButton::Left));
        assert!(editor
            .document
            .connections
            .iter()
            .any(|edge| edge.from == PortRef::new("number", "value")
                && edge.to == PortRef::new("math", "b")));
        assert!(!editor
            .document
            .connections
            .iter()
            .any(|edge| edge.to == PortRef::new("math", "a")));
        assert_eq!(editor.undo.len(), 1);
    }
}

#[test]
fn socket_hit_preserves_hover_area_visible_edges_and_front_card_occlusion() {
    let mut editor = editor();
    let canvas = Rect::new(0., 0., 2000., 1800.);
    for zoom in [0.5, 1., 2.5] {
        editor.view.zoom = zoom;
        let mut cards = editor.cards(canvas);
        let point = cards
            .iter()
            .find(|card| card.id == "math")
            .unwrap()
            .sockets
            .iter()
            .find(|s| s.port.port == "a")
            .unwrap()
            .point;
        let extent = 9f32.max(wires::socket_radius(zoom) + 1.);
        assert!(
            wires::socket_at(&cards, canvas, [point[0] + extent - 0.1, point[1]], zoom).is_some()
        );
        cards.push(Card {
            source_node: std::ptr::null(),
            id: "front".into(),
            rect: Rect::new(point[0] - 20., point[1] - 20., 40., 40.),
            sockets: vec![],
        });
        assert!(wires::socket_at(&cards, canvas, point, zoom).is_none());
    }
    let mut cards = editor.cards(canvas);
    let mut card = cards.remove(0);
    card.sockets.truncate(1);
    card.rect = Rect::new(-100., 100., 100., 100.);
    card.sockets[0].point = [-8., 150.];
    assert!(wires::socket_at(std::slice::from_ref(&card), canvas, [0., 150.], 0.5).is_none());
    card.sockets[0].point = [-2., 150.];
    assert!(wires::socket_at(&[card], canvas, [0., 150.], 0.5).is_some());
    assert!(wires::socket_at(&cards, canvas, [f32::NAN, 100.], 1.).is_none());
}

#[test]
fn unique_body_drop_connects_in_both_directions_as_one_undo() {
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    for reverse in [false, true] {
        let mut editor = editor();
        editor.document.connections.clear();
        frame(&mut editor, &mut atlas, Input::default());
        let before = editor.document.clone();
        let (from, to) = if reverse {
            (
                socket(&editor, "output", "color", false),
                body(&editor, "color"),
            )
        } else {
            (
                socket(&editor, "color", "value", true),
                body(&editor, "output"),
            )
        };
        drag(&mut editor, &mut atlas, from, to);
        let cards = editor.cards(editor.last_canvas);
        let offer = editor.wire_offer(&cards, editor.last_canvas, to).unwrap();
        assert!(!offer.direct);
        assert_eq!(offer.targets.len(), 1);
        assert!(offer.error.is_none());
        frame(&mut editor, &mut atlas, release(to, MouseButton::Left));
        assert_eq!(
            editor.document.connections,
            [Connection::new("color", "value", "output", "color")]
        );
        assert_eq!(editor.undo.len(), 1);
        assert!(!editor.popup_open());
        assert!(editor.undo());
        assert_eq!(editor.document, before);
        assert!(editor.redo());
    }
}

#[test]
fn ambiguous_body_drop_requires_a_choice_and_cancel_preserves_document_and_history() {
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    for cancel in [
        key(Key::Escape),
        Input {
            focus_lost: true,
            ..Default::default()
        },
        press([5., 5.], MouseButton::Left),
    ] {
        let mut editor = editor();
        frame(&mut editor, &mut atlas, Input::default());
        let before = editor.document.clone();
        let from = socket(&editor, "number", "value", true);
        let to = body(&editor, "math");
        drag(&mut editor, &mut atlas, from, to);
        frame(&mut editor, &mut atlas, release(to, MouseButton::Left));
        assert!(
            matches!(&editor.context_menu,Some(ContextMenu {kind:MenuKind::Connect {connections,..},..}) if connections.len()==2)
        );
        frame(&mut editor, &mut atlas, key(Key::Delete));
        assert_eq!(editor.document, before);
        assert!(editor.undo.is_empty());
        frame(&mut editor, &mut atlas, cancel);
        assert!(editor.context_menu.is_none());
        assert_eq!(editor.document, before);
        assert!(editor.undo.is_empty());
    }
    let mut editor = editor();
    frame(&mut editor, &mut atlas, Input::default());
    let from = socket(&editor, "number", "value", true);
    let to = body(&editor, "math");
    drag(&mut editor, &mut atlas, from, to);
    frame(&mut editor, &mut atlas, release(to, MouseButton::Left));
    frame(&mut editor, &mut atlas, key(Key::Down));
    let expected = match &editor.context_menu.as_ref().unwrap().kind {
        MenuKind::Connect { connections, .. } => connections[1].clone(),
        _ => panic!(),
    };
    frame(&mut editor, &mut atlas, key(Key::Enter));
    assert!(editor.document.connections.contains(&expected));
    assert_eq!(editor.undo.len(), 1);
}

#[test]
fn rewire_menu_keeps_both_original_edges_until_commit_and_undo_restores_them() {
    let mut editor = editor();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    let second = editor
        .add_node(NodeOp::Float { value: 2. }, [20., 450.])
        .unwrap();
    editor
        .connect_ports(PortRef::new("number", "value"), PortRef::new("math", "a"))
        .unwrap();
    editor
        .connect_ports(PortRef::new(&second, "value"), PortRef::new("math", "b"))
        .unwrap();
    editor.undo.clear();
    frame(&mut editor, &mut atlas, Input::default());
    let before = editor.document.clone();
    let from = socket(&editor, "math", "a", false);
    let b = socket(&editor, "math", "b", false);
    let to = [body(&editor, "math")[0], b[1]];
    drag(&mut editor, &mut atlas, from, to);
    frame(&mut editor, &mut atlas, release(to, MouseButton::Left));
    assert_eq!(editor.document, before);
    frame(&mut editor, &mut atlas, key(Key::Escape));
    assert_eq!(editor.document, before);
    drag(&mut editor, &mut atlas, from, to);
    frame(&mut editor, &mut atlas, release(to, MouseButton::Left));
    frame(&mut editor, &mut atlas, key(Key::Enter));
    assert!(!editor
        .document
        .connections
        .iter()
        .any(|edge| edge.to == PortRef::new("math", "a")));
    assert!(editor
        .document
        .connections
        .contains(&Connection::new("number", "value", "math", "b")));
    assert_eq!(
        editor.document.connections.len(),
        before.connections.len() - 1
    );
    assert_eq!(editor.undo.len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.document, before);
}

#[test]
fn body_rejections_keep_primary_type_unit_cycle_and_budget_causes() {
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    for (node, port, target, code) in [
        ("number", "value", "output", ErrorCode::TypeMismatch),
        ("math", "value", "math", ErrorCode::Cycle),
        ("number", "value", "math", ErrorCode::Budget),
    ] {
        let mut editor = editor();
        if code == ErrorCode::Budget {
            editor.options.max_connections = editor.document.connections.len();
        }
        frame(&mut editor, &mut atlas, Input::default());
        let before = editor.document.clone();
        let from = socket(&editor, node, port, true);
        let to = body(&editor, target);
        drag(&mut editor, &mut atlas, from, to);
        let cards = editor.cards(editor.last_canvas);
        let offer = editor.wire_offer(&cards, editor.last_canvas, to).unwrap();
        assert_eq!(offer.error.unwrap().code, code);
        frame(&mut editor, &mut atlas, release(to, MouseButton::Left));
        assert_eq!(editor.document, before);
        assert!(editor.undo.is_empty());
        assert!(editor.notice.is_some());
        assert!(!editor.popup_open());
    }
    let mut editor = editor();
    let uv = editor.add_node(NodeOp::Uv, [20., 450.]).unwrap();
    let circle = editor.add_node(NodeOp::SdfCircle, [700., 80.]).unwrap();
    editor.undo.clear();
    frame(&mut editor, &mut atlas, Input::default());
    let before = editor.document.clone();
    let from = socket(&editor, &uv, "value", true);
    let to = body(&editor, &circle);
    drag(&mut editor, &mut atlas, from, to);
    let cards = editor.cards(editor.last_canvas);
    let offer = editor.wire_offer(&cards, editor.last_canvas, to).unwrap();
    assert_eq!(offer.error.as_ref().unwrap().code, ErrorCode::TypeMismatch);
    assert!(offer.error.as_ref().unwrap().message.contains("unit"));
    frame(&mut editor, &mut atlas, release(to, MouseButton::Left));
    assert_eq!(editor.document, before);
}

#[test]
fn reverse_multi_output_and_collapsed_or_empty_drops_give_explicit_results() {
    let mut editor = editor();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    let split = editor.add_node(NodeOp::SplitColor, [700., 80.]).unwrap();
    editor.undo.clear();
    frame(&mut editor, &mut atlas, Input::default());
    let from = socket(&editor, "math", "a", false);
    let to = body(&editor, &split);
    drag(&mut editor, &mut atlas, from, to);
    frame(&mut editor, &mut atlas, release(to, MouseButton::Left));
    assert!(
        matches!(&editor.context_menu,Some(ContextMenu {kind:MenuKind::Connect {connections,..},..}) if connections.len()==4)
    );
    frame(&mut editor, &mut atlas, key(Key::End));
    let expected = match &editor.context_menu.as_ref().unwrap().kind {
        MenuKind::Connect { connections, .. } => connections[3].clone(),
        _ => panic!(),
    };
    frame(&mut editor, &mut atlas, key(Key::Enter));
    assert!(editor.document.connections.contains(&expected));
    assert!(editor.undo());
    editor.toggle_collapsed("math");
    editor.undo.clear();
    frame(&mut editor, &mut atlas, Input::default());
    let before = editor.document.clone();
    let from = socket(&editor, "number", "value", true);
    let to = body(&editor, "math");
    drag(&mut editor, &mut atlas, from, to);
    frame(&mut editor, &mut atlas, release(to, MouseButton::Left));
    assert!(editor.notice.as_ref().unwrap().contains("Expand"));
    assert_eq!(editor.document, before);
    editor.toggle_collapsed("math");
    frame(&mut editor, &mut atlas, Input::default());
    let from = socket(&editor, "math", "b", false);
    let to = [1200., 750.];
    drag(&mut editor, &mut atlas, from, to);
    frame(&mut editor, &mut atlas, release(to, MouseButton::Left));
    assert!(editor.notice.as_ref().unwrap().contains("No connection"));
}

#[test]
fn explicit_wrong_port_does_not_fall_back_to_compatible_body_and_original_wire_survives() {
    let mut editor = editor();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    let circle = editor.add_node(NodeOp::SdfCircle, [700., 80.]).unwrap();
    editor
        .connect_ports(PortRef::new("number", "value"), PortRef::new("math", "a"))
        .unwrap();
    editor.undo.clear();
    frame(&mut editor, &mut atlas, Input::default());
    let before = editor.document.clone();
    let from = socket(&editor, "math", "a", false);
    let to = socket(&editor, &circle, "center", false);
    drag(&mut editor, &mut atlas, from, to);
    let cards = editor.cards(editor.last_canvas);
    let offer = editor.wire_offer(&cards, editor.last_canvas, to).unwrap();
    assert!(offer.direct);
    assert_eq!(offer.error.unwrap().code, ErrorCode::TypeMismatch);
    frame(&mut editor, &mut atlas, release(to, MouseButton::Left));
    assert_eq!(editor.document, before);
    assert!(editor.undo.is_empty());
    assert!(!editor.popup_open());
    drag(&mut editor, &mut atlas, from, [10., 100.]);
    frame(
        &mut editor,
        &mut atlas,
        release([10., 100.], MouseButton::Left),
    );
    assert!(!editor
        .document
        .connections
        .iter()
        .any(|edge| edge.to == PortRef::new("math", "a")));
    assert_eq!(editor.undo.len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.document, before);
}

#[test]
fn large_group_port_chooser_scrolls_reveals_keyboard_selection_and_mouse_chooses_visible_row() {
    use motion_graph::{GroupPort, NodeGroup, PortSemantic};
    let mut editor = editor();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    let ports: BTreeMap<_, _> = (0..64)
        .map(|i| {
            (
                format!("p{i:02}"),
                GroupPort {
                    name: format!("长端口名称 · {i:02}"),
                    english_name: format!("Very long authored output port label {i:02}"),
                    value_type: ValueType::Float,
                    semantic: PortSemantic::Scalar,
                },
            )
        })
        .collect();
    let graph = NodeGroup {
        name: "组".into(),
        english_name: "Group".into(),
        nodes: vec![
            Node::new(
                "in",
                NodeOp::GroupInput {
                    ports: BTreeMap::new(),
                },
            ),
            Node::new("constant", NodeOp::Float { value: 2. }),
            Node::new(
                "out",
                NodeOp::GroupOutput {
                    ports: ports.clone(),
                },
            ),
        ],
        connections: ports
            .keys()
            .map(|id| Connection::new("constant", "value", "out", id))
            .collect(),
        input_node: "in".into(),
        output_node: "out".into(),
    };
    let mut group = Node::new(
        "many",
        NodeOp::Group {
            graph: Box::new(graph),
        },
    );
    group.position = [700., 30.];
    editor.document.nodes.push(group);
    frame(&mut editor, &mut atlas, Input::default());
    let before = editor.document.clone();
    let from = socket(&editor, "math", "a", false);
    let to = body(&editor, "many");
    drag(&mut editor, &mut atlas, from, to);
    frame(&mut editor, &mut atlas, release(to, MouseButton::Left));
    frame(&mut editor, &mut atlas, key(Key::End));
    let menu = editor.context_menu.as_ref().unwrap();
    assert_eq!(menu.selected, 63);
    assert!(
        matches!(&menu.kind, MenuKind::Connect {scroll, connections,..} if *scroll > 0. && connections.len()==64)
    );
    assert_eq!(editor.document, before);
    frame(&mut editor, &mut atlas, key(Key::Home));
    assert!(
        matches!(&editor.context_menu.as_ref().unwrap().kind, MenuKind::Connect {scroll,..} if *scroll == 0.)
    );
    // Resize while the chooser is open: it stays inside the new viewport and reveals End.
    let bounds = Rect::new(0., 0., 360., 300.);
    let mut paint = PaintList::default();
    editor.draw(
        &mut paint,
        &mut atlas,
        bounds,
        &key(Key::End),
        Language::English,
    );
    assert_eq!(editor.context_menu.as_ref().unwrap().selected, 63);
    let menu = editor.context_menu.as_ref().unwrap();
    let styles = paint.styles();
    let padding = styles.metric("padding", 5.);
    let row = styles.metric("context.row.height", 20.);
    let visible = ((bounds.height() - padding * 4. - row) / row).floor() as usize;
    let height = row * (visible + 1) as f32 + padding * 2.;
    let y = menu.position[1]
        .min(bounds.max[1] - height - padding)
        .max(padding);
    // The long menu is right-clamped; this is inside it regardless of text width.
    let point = [bounds.max[0] - padding * 3., y + padding + row * 1.5];
    editor.draw(
        &mut PaintList::default(),
        &mut atlas,
        bounds,
        &Input {
            mouse: point,
            wheel: [0., -100000.],
            ..Default::default()
        },
        Language::English,
    );
    assert!(
        matches!(&editor.context_menu.as_ref().unwrap().kind, MenuKind::Connect {scroll,..} if *scroll == 0.)
    );
    editor.draw(
        &mut PaintList::default(),
        &mut atlas,
        bounds,
        &press(point, MouseButton::Left),
        Language::English,
    );
    assert!(!editor.popup_open());
    assert!(editor
        .document
        .connections
        .contains(&Connection::new("many", "p00", "math", "a")));
    assert_eq!(editor.undo.len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.document, before);
}
