use super::super::tests::{frame, press};
use super::*;

fn center(rect: Rect) -> [f32; 2] {
    [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]
}
fn empty() -> NodeEditor {
    let mut document = demo_graph();
    document.nodes.clear();
    document.connections.clear();
    document.public_parameters.clear();
    NodeEditor::new(document)
}

#[test]
fn welcome_requests_existing_example_flow_without_changing_source_or_history() {
    let mut editor = NodeEditor::default();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let before = editor.snapshot();
    let styles = PaintList::default().styles();
    let (examples, _) = editor.onboarding_controls(&styles).unwrap();
    let response = frame(
        &mut editor,
        &mut atlas,
        press(center(examples), MouseButton::Left),
    );
    assert!(!response.changed && !response.semantic_changed);
    assert!(
        matches!(response.actions.as_slice(), [Action::Examples(snapshot)] if snapshot.document == before.document && snapshot.epoch == before.epoch)
    );
    assert_eq!(editor.snapshot().document, before.document);
    assert_eq!(editor.stamp(), before.preview_stamp());
    assert!(editor.undo.is_empty());
    assert!(editor.drag.is_none());
}

#[test]
fn dismiss_preserves_node_screen_positions_source_selection_and_preview() {
    let mut editor = NodeEditor::default();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let before = editor.snapshot();
    let socket = editor.port_position("circle", "distance", true).unwrap();
    let styles = PaintList::default().styles();
    let (_, dismiss) = editor.onboarding_controls(&styles).unwrap();
    frame(
        &mut editor,
        &mut atlas,
        press(center(dismiss.unwrap()), MouseButton::Left),
    );
    assert!(editor.onboarding_controls(&styles).is_none());
    assert_eq!(
        editor.port_position("circle", "distance", true).unwrap(),
        socket
    );
    assert_eq!(editor.snapshot().document, before.document);
    assert_eq!(editor.selected(), Some("circle"));
    assert_eq!(editor.stamp(), before.preview_stamp());
    assert!(editor.undo.is_empty());
}

#[test]
fn empty_root_offers_examples_but_bound_and_nested_graphs_are_untouched() {
    let mut editor = empty();
    let styles = PaintList::default().styles();
    let area = Rect::new(0., 0., 1440., 900.);
    assert!(editor
        .welcome_layout(area, &styles)
        .unwrap()
        .dismiss
        .is_none());
    editor.layer_target = Some(crate::node_layer::Target {
        root: Default::default(),
        composition: "main".into(),
        object: 1,
        effect: 2,
        key: "node".into(),
        label: "Node".into(),
        creating: false,
    });
    assert!(editor.welcome_layout(area, &styles).is_none());
    let mut editor = NodeEditor::default();
    editor.select(Some("circle"), false);
    editor.select(Some("fill"), true);
    editor.group_selected().unwrap();
    let group = editor.selected().unwrap().to_owned();
    assert!(editor.enter_group(&group));
    assert!(editor.welcome_layout(area, &styles).is_none());
}

#[test]
fn opening_or_restoring_a_document_does_not_reintroduce_the_welcome_strip() {
    let mut editor = NodeEditor::default();
    let document = editor.snapshot().document;
    editor.replace_document(document.clone(), Some("saved.msgraph".into()));
    let styles = PaintList::default().styles();
    let area = Rect::new(0., 0., 1440., 900.);
    assert!(editor.welcome_layout(area, &styles).is_none());
    assert_eq!(editor.snapshot().document, document);
    assert!(!editor.dirty());
}

#[test]
fn welcome_fits_both_languages_and_yields_to_popups_and_small_docks() {
    let styles = PaintList::default().styles();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    for language in [Language::Chinese, Language::English] {
        let mut editor = NodeEditor::default();
        let area = Rect::new(0., 0., 900., 600.);
        editor.draw(
            &mut PaintList::default(),
            &mut atlas,
            area,
            &Input::default(),
            language,
        );
        let welcome = editor.welcome_layout(area, &styles).unwrap();
        assert!(editor.last_canvas.min[1] >= welcome.area.max[1]);
        assert!(editor.last_canvas.height() >= styles.metric("row.height", 22.) * 5.);
        editor.context_menu = Some(ContextMenu {
            position: [0., 0.],
            kind: MenuKind::Canvas,
            selected: 0,
        });
        let mut actions = vec![];
        editor.welcome_input(
            area,
            &press(center(welcome.examples), MouseButton::Left),
            &styles,
            &mut actions,
        );
        assert!(actions.is_empty());
        assert!(editor
            .welcome_layout(Rect::new(0., 0., 320., 200.), &styles)
            .is_none());
    }
}

#[test]
fn recommended_example_is_small_editable_and_reversible() {
    let example = motion_graph::builtin_examples()
        .iter()
        .find(|example| example.id == "solid_color")
        .unwrap();
    let mut browser = crate::node_examples::Browser::new(empty().snapshot(), false);
    let point = center(crate::node_examples::Browser::open_button([900., 600.]));
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    let event = browser.draw(
        &mut PaintList::default(),
        &mut atlas,
        [900., 600.],
        &press(point, MouseButton::Left),
        crate::i18n::Locale::En,
    );
    assert!(matches!(
        event,
        Some(crate::node_examples::Event::Open {
            id: "solid_color",
            save_first: false
        })
    ));
    let mut editor = NodeEditor::new(example.load().unwrap());
    assert_eq!(editor.document.nodes.len(), 2);
    assert_eq!(editor.document.connections.len(), 1);
    let before = editor.snapshot().compile().unwrap().source;
    editor.set_constant("color", 0, 0.9).unwrap();
    assert_ne!(editor.snapshot().compile().unwrap().source, before);
    assert!(editor.undo());
    assert_eq!(editor.snapshot().compile().unwrap().source, before);
}

#[test]
fn diagnostic_location_uses_the_canvas_below_welcome() {
    let mut editor = NodeEditor::default();
    editor.disconnect_input(&PortRef::new("output", "color"));
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let styles = PaintList::default().styles();
    let area = Rect::new(0., 0., 1440., 900.);
    let toggle = editor.diagnostic_controls(&styles).0;
    frame(
        &mut editor,
        &mut atlas,
        press(center(toggle), MouseButton::Left),
    );
    let drawer = editor.diagnostic_drawer_layout(area, &styles).unwrap();
    frame(
        &mut editor,
        &mut atlas,
        press(center(drawer.locate), MouseButton::Left),
    );
    let card = editor
        .cards(editor.last_canvas)
        .into_iter()
        .find(|card| card.id == "output")
        .unwrap();
    let expected = (editor.last_canvas.min[1] + drawer.area.min[1]) * 0.5;
    assert!((center(card.rect)[1] - expected).abs() < 0.1);
    assert_eq!(editor.selected(), Some("output"));
}

#[test]
fn diagnostic_drawer_blocks_a_welcome_button_under_its_region() {
    let mut editor = NodeEditor::default();
    editor.disconnect_input(&PortRef::new("output", "color"));
    editor.diagnostic_drawer.open = true;
    let styles = PaintList::default().styles();
    let (area, point) = (300..500)
        .find_map(|height| {
            let area = Rect::new(0., 0., 900., height as f32);
            let welcome = editor.welcome_layout(area, &styles)?;
            let drawer = editor.diagnostic_drawer_layout(area, &styles)?;
            let point = [center(welcome.examples)[0], welcome.examples.max[1] - 1.];
            drawer.area.contains(point).then_some((area, point))
        })
        .expect("exercise the real overlap at a short dock height");
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    let before = editor.snapshot();
    let response = editor.draw(
        &mut PaintList::default(),
        &mut atlas,
        area,
        &press(point, MouseButton::Left),
        Language::English,
    );
    assert!(!response
        .actions
        .iter()
        .any(|action| matches!(action, Action::Examples(_))));
    assert_eq!(editor.snapshot().document, before.document);
}

#[test]
fn adding_the_first_node_to_an_empty_root_preserves_the_drop_position_and_undo() {
    let mut editor = empty();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    let canvas = editor.canvas_bounds();
    let pan = editor.view.pan;
    let id = editor
        .add_node(NodeOp::Float { value: 1. }, [100., 100.])
        .unwrap();
    frame(&mut editor, &mut atlas, Input::default());
    assert_eq!(editor.canvas_bounds(), canvas);
    assert_eq!(editor.view.pan, pan);
    let position = editor
        .cards(editor.last_canvas)
        .into_iter()
        .find(|card| card.id == id)
        .unwrap()
        .rect
        .min;
    assert!(editor.undo());
    frame(&mut editor, &mut atlas, Input::default());
    assert!(editor.document.nodes.is_empty());
    assert!(editor.redo());
    frame(&mut editor, &mut atlas, Input::default());
    assert_eq!(editor.canvas_bounds(), canvas);
    assert_eq!(
        editor
            .cards(editor.last_canvas)
            .into_iter()
            .find(|card| card.id == id)
            .unwrap()
            .rect
            .min,
        position
    );
}
