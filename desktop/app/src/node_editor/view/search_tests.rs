use super::super::tests::{frame, press, simple};
use super::*;
use crate::node_editor::search as catalogue;
use motion_ui::input::Modifiers;

fn key(key: Key) -> Input {
    Input {
        keys: vec![(key, Default::default())],
        ..Default::default()
    }
}
fn open(editor: &mut NodeEditor, atlas: &mut TextAtlas) {
    editor.focus_panel();
    frame(editor, atlas, Input::default());
    frame(
        editor,
        atlas,
        Input {
            mouse: [600., 420.],
            keys: vec![(
                Key::A,
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
            )],
            ..Default::default()
        },
    );
    assert!(editor.add_menu.is_some());
}

#[test]
fn menu_search_crosses_the_clicked_category_and_preserves_ime_and_placement() {
    let mut editor = simple();
    let mut atlas = TextAtlas::from_system_language(13., true).unwrap();
    open(&mut editor, &mut atlas);
    let styles = PaintList::default().styles();
    let menu = editor.add_menu.as_ref().unwrap();
    let world = menu.world;
    let area = search::menu_area(Rect::new(0., 0., 1440., 900.), menu.position, &styles).unwrap();
    let top = area.min[1]
        + styles.metric("padding.panel", 8.) * 2.
        + styles.metric("viewer.field.height", 26.);
    frame(
        &mut editor,
        &mut atlas,
        press(
            [
                area.min[0] + 30.,
                top + styles.metric("row.height", 22.) * 4.5,
            ],
            MouseButton::Left,
        ),
    );
    assert_eq!(
        editor.add_menu.as_ref().unwrap().category,
        Some(catalog::Category::Math)
    );
    frame(
        &mut editor,
        &mut atlas,
        Input {
            text: "yuanxing".into(),
            ..Default::default()
        },
    );
    assert!(matches!(
        catalogue::find(
            &editor.add_menu.as_ref().unwrap().query,
            Some(catalog::Category::Math)
        )[0]
        .item
        .operation,
        NodeOp::SdfCircle
    ));
    let before = editor.document.clone();
    let mut ime = key(Key::Enter);
    ime.ime_consumed = true;
    frame(&mut editor, &mut atlas, ime);
    assert!(editor.add_menu.is_some());
    assert_eq!(editor.document, before);
    let response = frame(&mut editor, &mut atlas, key(Key::Enter));
    assert!(editor.add_menu.is_none());
    let node = editor.document.nodes.last().unwrap();
    assert!(matches!(node.operation, NodeOp::SdfCircle));
    assert_eq!(node.position, world);
    assert!(matches!(
        editor.recent_nodes.items()[0].item.operation,
        NodeOp::SdfCircle
    ));
    assert!(response
        .actions
        .iter()
        .any(|a| matches!(a, Action::RecentNodesChanged(_))));
    editor.undo();
    assert_eq!(editor.document, before);
    assert_eq!(
        editor.recent_nodes.items().len(),
        1,
        "UI recents must survive graph undo"
    );
}

#[test]
fn menu_empty_results_cancel_and_failed_additions_do_not_record_recent_nodes() {
    let mut editor = simple();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    open(&mut editor, &mut atlas);
    let before = editor.document.clone();
    frame(
        &mut editor,
        &mut atlas,
        Input {
            text: "no_such_node".into(),
            ..Default::default()
        },
    );
    frame(&mut editor, &mut atlas, key(Key::Enter));
    assert!(editor.add_menu.is_some());
    assert_eq!(editor.document, before);
    assert!(editor.recent_nodes.items().is_empty());
    frame(&mut editor, &mut atlas, key(Key::Escape));
    assert!(editor.add_menu.is_none());
    open(&mut editor, &mut atlas);
    frame(
        &mut editor,
        &mut atlas,
        Input {
            text: "sqrt".into(),
            ..Default::default()
        },
    );
    editor.options.max_nodes = editor.document.nodes.len();
    frame(&mut editor, &mut atlas, key(Key::Enter));
    assert!(editor.add_menu.is_some());
    assert_eq!(editor.document, before);
    assert!(editor.recent_nodes.items().is_empty());
    assert!(editor.notice.is_some());
    editor.options.max_nodes += 1;
    frame(&mut editor, &mut atlas, key(Key::Enter));
    assert!(editor.add_menu.is_none());
    assert_eq!(editor.recent_nodes.items().len(), 1);
    assert!(matches!(
        editor.document.nodes.last().unwrap().operation,
        NodeOp::Math {
            operation: MathOp::Sqrt,
            ..
        }
    ));
}

#[test]
fn library_keyboard_uses_shared_aliases_and_recent_choices() {
    let mut editor = simple();
    editor.library_open = true;
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    editor.focus_panel();
    frame(&mut editor, &mut atlas, Input::default());
    let area = editor
        .layout(Rect::new(0., 0., 1440., 900.))
        .library
        .unwrap();
    frame(
        &mut editor,
        &mut atlas,
        press([area.min[0] + 30., area.min[1] + 49.], MouseButton::Left),
    );
    frame(
        &mut editor,
        &mut atlas,
        Input {
            text: "parameter".into(),
            ..Default::default()
        },
    );
    let candidates = catalogue::find("parameter", None);
    assert!(candidates.len() > 1);
    frame(&mut editor, &mut atlas, key(Key::Down));
    let expected = candidates[1].key.clone();
    let count = editor.document.nodes.len();
    let mut ime = key(Key::Enter);
    ime.composing = true;
    frame(&mut editor, &mut atlas, ime);
    assert_eq!(editor.document.nodes.len(), count);
    let response = frame(&mut editor, &mut atlas, key(Key::Enter));
    assert_eq!(editor.document.nodes.len(), count + 1);
    assert!(!editor.search_focus);
    assert_eq!(editor.recent_nodes.items()[0].key, expected);
    assert!(response
        .actions
        .iter()
        .any(|a| matches!(a, Action::RecentNodesChanged(_))));
    let recent = editor.recent_nodes.clone();
    editor.replace_document(demo_graph(), None);
    assert_eq!(editor.recent_nodes, recent);
}

#[test]
fn recent_nodes_survive_workspace_save_reopen_and_legacy_json() {
    let mut editor = simple();
    let item = catalogue::find("sqrt", None)[0];
    editor.add_catalogue_entry(item, [10., 20.]).unwrap();
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("workspace.json");
    let mut workspace = crate::workspace::Workspace::default();
    workspace.node_recents = editor.recent_nodes.clone();
    workspace.save(&path).unwrap();
    let reopened = crate::workspace::Workspace::load(&path);
    assert_eq!(reopened.node_recents, editor.recent_nodes);
    let mut legacy: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    legacy.as_object_mut().unwrap().remove("node_recents");
    std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    assert!(crate::workspace::Workspace::load(&path)
        .node_recents
        .items()
        .is_empty());
}

#[test]
fn popup_geometry_remains_inside_small_and_offset_bounds() {
    let styles = PaintList::default().styles();
    for bounds in [
        Rect::new(0., 0., 900., 600.),
        Rect::new(400., 200., 320., 260.),
    ] {
        for position in [[-100., -100.], [5000., 5000.]] {
            let popup = search::menu_area(bounds, position, &styles).unwrap();
            assert!(popup.min[0] >= bounds.min[0] && popup.min[1] >= bounds.min[1]);
            assert!(popup.max[0] <= bounds.max[0] && popup.max[1] <= bounds.max[1]);
        }
    }
    assert!(search::menu_area(Rect::new(0., 0., 0., 0.), [0., 0.], &styles).is_none());
}

#[test]
fn result_description_remains_visible_when_workspace_rows_are_taller() {
    let entry = catalogue::find("circle", None)[0];
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    let normal = PaintList::default().styles();
    let taller = normal
        .with_overrides(br#"{"tokens":{"sizes":{"row.height":32}}}"#)
        .unwrap();
    let mut baseline = PaintList::with_styles(normal);
    let mut customized = PaintList::with_styles(std::sync::Arc::new(taller));
    for paint in [&mut baseline, &mut customized] {
        search::result_row(
            paint,
            &mut atlas,
            Rect::new(0., 0., 340., 44.),
            entry,
            Language::English,
            false,
            false,
        );
    }
    assert!(baseline.glyphs.len() > entry.title(Language::English).chars().count());
    assert_eq!(
        customized
            .glyphs
            .iter()
            .map(|g| g.size[1])
            .collect::<Vec<_>>(),
        baseline
            .glyphs
            .iter()
            .map(|g| g.size[1])
            .collect::<Vec<_>>(),
        "taller property rows must not crop letters in a catalogue result description"
    );
}

#[test]
fn short_popup_keeps_categories_reachable_and_category_click_clears_the_query() {
    let mut editor = simple();
    let before = editor.document.clone();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    let bounds = Rect::new(0., 0., 600., 182.);
    let mut paint = PaintList::default();
    let styles = paint.styles();
    editor.add_menu = Some(AddMenu {
        position: [8., 8.],
        world: [10., 20.],
        query: SearchQuery::default(),
        recent: false,
        category: None,
        category_scroll: 0.,
        selected: 0,
        scroll: 0.,
    });
    let area = search::menu_area(bounds, [8., 8.], &styles).unwrap();
    let pad = styles.metric("padding.panel", 8.);
    let top = area.min[1] + pad * 2. + styles.metric("viewer.field.height", 26.);
    let pointer = [area.min[0] + pad * 2., top + 5.];
    editor.paint_add_menu(
        &mut paint,
        &mut atlas,
        bounds,
        &Input {
            mouse: pointer,
            wheel: [0., 1000.],
            ..Default::default()
        },
        Language::English,
    );
    let menu = editor.add_menu.as_ref().unwrap();
    assert!(menu.category_scroll > 0.);
    assert_eq!(
        menu.category, None,
        "scrolling must not switch the category"
    );
    let output_point = [
        pointer[0],
        top + styles.metric("row.height", 22.) * 7.5 - menu.category_scroll,
    ];
    editor.paint_add_menu(
        &mut PaintList::default(),
        &mut atlas,
        bounds,
        &press(output_point, MouseButton::Left),
        Language::English,
    );
    assert_eq!(
        editor.add_menu.as_ref().unwrap().category,
        Some(catalog::Category::Output)
    );
    editor.paint_add_menu(
        &mut PaintList::default(),
        &mut atlas,
        bounds,
        &Input {
            text: "circle".into(),
            ..Default::default()
        },
        Language::English,
    );
    assert_eq!(editor.add_menu.as_ref().unwrap().query, "circle");
    editor.paint_add_menu(
        &mut PaintList::default(),
        &mut atlas,
        bounds,
        &press(output_point, MouseButton::Left),
        Language::English,
    );
    let menu = editor.add_menu.as_ref().unwrap();
    assert!(menu.query.is_empty());
    assert_eq!(menu.category, Some(catalog::Category::Output));
    assert_eq!(editor.document, before);
    assert!(editor.recent_nodes.items().is_empty());
}
