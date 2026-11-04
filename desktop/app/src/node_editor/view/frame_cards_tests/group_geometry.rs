use super::*;

#[derive(Clone, Copy, Debug)]
enum Case { Enter, Collapsed, Wheel, ControlWheel, Expired, Displaced, OtherId, FrontBody, FrontSocket, BehindSocket }
const CASES: [Case; 10] = [Case::Enter, Case::Collapsed, Case::Wheel, Case::ControlWheel,
    Case::Expired, Case::Displaced, Case::OtherId, Case::FrontBody, Case::FrontSocket, Case::BehindSocket];
fn enters(case: Case) -> bool { matches!(case, Case::Enter | Case::Collapsed | Case::Wheel | Case::BehindSocket) }

fn prepare(zoom: f32, case: Case) -> (NodeEditor, String) {
    let mut editor = simple();
    editor.inspector_open = false;
    editor.view.zoom = zoom;
    editor.focus_panel();
    editor.select(Some("number"), false);
    editor.select(Some("math"), true);
    assert!(editor.group_selected().unwrap());
    let id = editor.primary.clone().unwrap();
    let node = editor.document.nodes.iter_mut().find(|n| n.id == id).unwrap();
    node.position = [80., 80.];
    node.collapsed = matches!(case, Case::Collapsed);
    for index in 0..16 {
        let mut node = Node::new(format!("far_{index}"), NodeOp::Float { value: 1. });
        node.position = [2000. + index as f32 * 240., 400.];
        editor.document.nodes.push(node);
    }
    editor.undo.clear(); editor.redo.clear();
    (editor, id)
}

fn scenario(editor: &mut NodeEditor, id: &str, case: Case,
    mut step: impl FnMut(&mut NodeEditor, Input, &str)) {
    step(editor, Input::default(), "initial");
    let point = editor.view.world_to_screen([160., 92.], editor.last_canvas);
    assert!(editor.last_canvas.contains(point));
    step(editor, press(point, MouseButton::Left), "first_press");
    assert!(editor.last_group_click.is_some());
    step(editor, release(point, MouseButton::Left), "first_release");
    assert!(editor.drag.is_none());
    // Keep wall-clock scheduling out of the functional fixture. Exercise the
    // existing 400ms boundary explicitly instead of relying on test speed.
    editor.last_group_click.as_mut().unwrap().1 = std::time::Instant::now();
    match case {
        Case::Expired => editor.last_group_click.as_mut().unwrap().1 -= std::time::Duration::from_millis(500),
        Case::Displaced => editor.last_group_click.as_mut().unwrap().2[0] -= 6.,
        Case::OtherId => editor.last_group_click.as_mut().unwrap().0 = "not_the_group".into(),
        Case::FrontBody | Case::FrontSocket | Case::BehindSocket => {
            let mut node = Node::new("occluder", NodeOp::Float { value: 1. });
            node.position = if matches!(case, Case::FrontBody) { [100., 65.] }
                else { [160. - NODE_WIDTH, 92. - HEADER - ROW * 0.5] };
            if matches!(case, Case::BehindSocket) { editor.document.nodes.insert(0, node); }
            else { editor.document.nodes.push(node); }
        }
        _ => {}
    }
    let root = editor.root_document();
    let revision = editor.revision;
    let semantic = editor.semantic_revision;
    let undo_before = history(&editor.undo);
    let redo_before = history(&editor.redo);
    let mut input = press(point, MouseButton::Left);
    if matches!(case, Case::Wheel | Case::ControlWheel) { input.wheel = [4., -7.]; }
    if matches!(case, Case::ControlWheel) { input.modifiers.control = true; }
    step(editor, input, "second_press");
    assert_eq!(editor.parents.len(), usize::from(enters(case)), "{case:?}");
    assert_eq!(editor.revision, revision);
    // Rejected navigation can begin a normal node-drag preview. Its original
    // floating-point movement is cancelled below; successful navigation is read-only.
    if enters(case) { assert_eq!(editor.root_document(), root); }
    assert_eq!(history(&editor.undo), undo_before);
    assert_eq!(history(&editor.redo), redo_before);
    let cancelled_preview_changed = !enters(case) && editor.root_document() != root;
    if enters(case) {
        assert!(editor.drag.is_none() && editor.selected.is_empty() && editor.primary.is_none());
        assert!(editor.last_group_click.is_none());
        assert_eq!(editor.parents[0].group, id);
        step(editor, release(point, MouseButton::Left), "finish");
    } else {
        if matches!(case, Case::FrontSocket) { assert!(matches!(editor.drag, Some(Drag::Wire { .. }))); }
        step(editor, Input { keys: vec![(Key::Escape, Default::default())], ..Default::default() }, "finish");
        assert!(editor.drag.is_none());
    }
    step(editor, Input::default(), "settled");
    let back = if enters(case) { Input { keys: vec![(Key::Tab, Modifiers { control: true, ..Default::default() })], ..Default::default() } }
        else { Input::default() };
    step(editor, back, "back");
    assert!(editor.parents.is_empty());
    step(editor, Input::default(), "final");
    assert_eq!(editor.root_document(), root);
    assert_eq!(editor.revision, revision.wrapping_add(u64::from(cancelled_preview_changed)));
    assert_eq!(editor.semantic_revision, semantic);
    assert_eq!(history(&editor.undo), undo_before);
    assert_eq!(history(&editor.redo), redo_before);
}

fn matrix(mut run: impl FnMut(NodeEditor, String, Language, Case, &mut TextAtlas)) {
    for language in [Language::English, Language::Chinese] {
        let mut atlas = TextAtlas::from_system_language(12., language == Language::Chinese).unwrap();
        for zoom in [0.55, 1.25] { for case in CASES {
            let (editor, id) = prepare(zoom, case);
            run(editor, id, language, case, &mut atlas);
        } }
    }
}

#[test]
fn group_double_clicks_build_only_final_cards_and_preserve_socket_priority() {
    let mut counts = vec![];
    matrix(|mut editor, id, language, case, atlas| {
        let zoom = editor.view.zoom;
        scenario(&mut editor, &id, case, |editor, input, phase| {
            let (builds, ports) = draw_counts(editor, atlas, Rect::new(13., 17., 1200., 800.), input, language);
            counts.push((format!("{language:?}-{zoom}-{case:?}-{phase}"), builds, ports, editor.document.nodes.len()));
        });
    });
    assert_eq!(counts.len(), 320);
    assert!(counts.iter().all(|(_, builds, _, _)| *builds == 1),
        "one final card build for group navigation: {counts:?}");
    assert!(counts.iter().filter(|(name, _, _, _)| name.ends_with("second_press") &&
        ["-Enter-", "-Collapsed-", "-Wheel-", "-BehindSocket-"].iter().any(|case| name.contains(case)))
        .all(|(_, _, ports, nodes)| ports == nodes), "entering a group must not resolve unrelated parent cards");
}

fn parents(editor: &NodeEditor) -> String {
    editor.parents.iter().map(|p| format!("{:?}", (&p.document, &p.group, p.view, &p.selection, &p.primary)))
        .collect::<Vec<_>>().join("\n")
}
fn history(history: &[History]) -> String {
    history.iter().map(|h| format!("{:?}", (&h.document, &h.options, h.semantic, h.view, &h.selection, &h.primary,
        h.parents.iter().map(|p| (&p.document, &p.group, p.view, &p.selection, &p.primary)).collect::<Vec<_>>())))
        .collect::<Vec<_>>().join("\n")
}
fn drag(editor: &NodeEditor) -> String {
    match &editor.drag {
        Some(Drag::Nodes { origin, positions, before }) => format!("nodes {origin:?} {positions:?} {}", history(std::slice::from_ref(before))),
        Some(Drag::Wire { source, old_input }) => format!("wire {source:?} {old_input:?}"),
        None => "none".into(),
        _ => panic!("unexpected drag in group fixture"),
    }
}

#[test]
fn record_group_navigation_frames_for_original_path_comparison() {
    let mut evidence = std::collections::BTreeMap::new();
    matrix(|mut editor, id, language, case, atlas| {
        let zoom = editor.view.zoom;
        scenario(&mut editor, &id, case, |editor, input, phase| {
            let mut paint = PaintList::default();
            let response = editor.draw(&mut paint, atlas, Rect::new(13., 17., 1200., 800.), &input, language);
            let record = format!("{:?}", ((paint.shapes, paint.colors, paint.glyphs, paint.order),
                (response.actions, response.wants_pointer, response.wants_keyboard, response.changed, response.semantic_changed),
                editor.snapshot(), (&editor.document, &editor.selected, &editor.primary, editor.view, &editor.notice, &editor.diagnostic),
                parents(editor), history(&editor.undo), history(&editor.redo), drag(editor),
                editor.last_group_click.as_ref().map(|(id, _, point)| (id, point))));
            assert!(evidence.insert(format!("{language:?}-{zoom}-{case:?}-{phase}"), record).is_none());
        });
    });
    assert_eq!(evidence.len(), 320);
    if let Some(path) = std::env::var_os("MOTION_NODE_GROUP_EVIDENCE") {
        std::fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    }
}
