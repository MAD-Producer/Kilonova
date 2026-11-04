use super::*;

#[derive(Clone, Copy, Debug)]
enum Case {
    Constant, Input, Default, NameZh, NameEn, Min, Max, Step, Units,
    GraphZh, GraphEn, Plugin, Version, Author, License, InvalidNumber, InvalidRange,
}
const CASES: [Case; 17] = [Case::Constant, Case::Input, Case::Default, Case::NameZh,
    Case::NameEn, Case::Min, Case::Max, Case::Step, Case::Units, Case::GraphZh,
    Case::GraphEn, Case::Plugin, Case::Version, Case::Author, Case::License,
    Case::InvalidNumber, Case::InvalidRange];

fn prepare(zoom: f32) -> (NodeEditor, String) {
    let mut editor = simple();
    editor.inspector_open = false;
    editor.view.zoom = zoom;
    editor.focus_panel();
    editor.document.nodes.iter_mut().find(|n| n.id == "math").unwrap()
        .input_defaults.insert("a".into(), InputValue::Float(1.));
    editor.add_public_parameter(ValueType::Float, [80., 60.]).unwrap();
    let parameter = editor.document.public_parameters.last_mut().unwrap();
    parameter.min = -2.; parameter.max = 10.; parameter.default[0] = 0.;
    parameter.step = 0.25; parameter.units.clear();
    let parameter = parameter.id.clone();
    editor.undo.clear(); editor.redo.clear();
    editor.select(Some("number"), false);
    (editor, parameter)
}

fn field(case: Case, parameter: &str) -> Field {
    let (binding, text) = match case {
        Case::Constant => (Binding::Constant("number".into(), 0), "2"),
        Case::Input => (Binding::InputValue("math".into(), "a".into()), "2"),
        Case::Default => (Binding::ParamDefault(parameter.into(), 0), "2"),
        Case::NameZh => (Binding::ParamName(parameter.into(), true), "测试参数"),
        Case::NameEn => (Binding::ParamName(parameter.into(), false), "Field parameter"),
        Case::Min => (Binding::ParamMin(parameter.into()), "-3"),
        Case::Max => (Binding::ParamMax(parameter.into()), "12"),
        Case::Step => (Binding::ParamStep(parameter.into()), "0.5"),
        Case::Units => (Binding::ParamUnits(parameter.into()), "px"),
        Case::GraphZh => (Binding::GraphName(true), "测试图"),
        Case::GraphEn => (Binding::GraphName(false), "Field graph"),
        Case::Plugin => (Binding::PluginId, "dev.test.field"),
        Case::Version => (Binding::Version, "1.2.3"),
        Case::Author => (Binding::Author, "Field author"),
        Case::License => (Binding::License, "Apache-2.0"),
        Case::InvalidNumber => (Binding::Constant("number".into(), 0), "not-a-number"),
        Case::InvalidRange => (Binding::ParamMin(parameter.into()), "20"),
    };
    Field {binding, text:text.into(), cursor:text.len(), replace:false,
        editor:Default::default(), error:None}
}

fn history_state(history: &[History]) -> String {
    history.iter().map(|h| format!("{:?}", (&h.document, &h.options, h.semantic,
        h.view, &h.selection, &h.primary))).collect::<Vec<_>>().join("\n")
}
fn drag_state(editor: &NodeEditor) -> String {
    match &editor.drag {
        Some(Drag::Marquee {origin,current,base,primary,extend,toggle}) =>
            format!("{origin:?} {current:?} {base:?} {primary:?} {extend} {toggle}"),
        None => "none".into(),
        _ => panic!("unexpected field fixture drag"),
    }
}
fn scenario(editor: &mut NodeEditor, parameter: &str, case: Case, wheel: usize,
    mut step: impl FnMut(&mut NodeEditor, Input, &str)) {
    step(editor, Input::default(), "initial");
    let before = editor.snapshot();
    let before_options = format!("{:?}", editor.options);
    let before_view = editor.view;
    editor.field = Some(field(case, parameter));
    let point = [editor.last_canvas.max[0] - 31., editor.last_canvas.min[1] + 7.];
    let mut input = press(point, MouseButton::Left);
    if wheel != 0 { input.wheel = [3., -11.]; }
    if wheel == 2 { input.modifiers.control = true; }
    step(editor, input, "commit");
    if wheel != 0 {
        assert_ne!((editor.view.pan, editor.view.zoom), (before_view.pan, before_view.zoom));
    }
    let invalid = matches!(case, Case::InvalidNumber | Case::InvalidRange);
    if invalid {
        assert_eq!(editor.document, before.document);
        assert_eq!(format!("{:?}",editor.options), before_options);
        assert_eq!(editor.revision, before.revision);
        assert!(editor.undo.is_empty());
        assert!(editor.field.as_ref().unwrap().error.is_some() && editor.notice.is_some());
        assert!(editor.drag.is_none());
    } else {
        assert!(editor.field.is_none());
        assert_eq!(editor.revision, before.revision.wrapping_add(1));
        assert_eq!(editor.undo.len(), 1);
        assert_eq!(editor.undo[0].document, before.document);
        assert_eq!(format!("{:?}", editor.undo[0].options), before_options);
        assert_eq!((editor.undo[0].view.pan, editor.undo[0].view.zoom),
            (editor.view.pan, editor.view.zoom), "field history includes the same-event wheel");
        assert!(matches!(editor.drag, Some(Drag::Marquee {..})));
    }
    step(editor, release(point, MouseButton::Left), "release");
    step(editor, Input::default(), "settled");
    if !invalid {
        let after = editor.snapshot();
        let options = format!("{:?}",editor.options);
        assert!(editor.undo());
        assert_eq!(editor.document, before.document);
        assert_eq!(format!("{:?}",editor.options), before_options);
        assert!(editor.redo());
        assert_eq!(editor.document, after.document);
        assert_eq!(format!("{:?}",editor.options), options);
    }
}

#[test]
fn canvas_field_commits_build_final_geometry_once_and_preserve_wheel_history() {
    let mut counts = vec![];
    for language in [Language::English, Language::Chinese] {
        let mut atlas = TextAtlas::from_system_language(12., language == Language::Chinese).unwrap();
        for zoom in [0.5, 1.25] { for case in CASES { for wheel in 0..3 {
            let (mut editor, parameter) = prepare(zoom);
            scenario(&mut editor, &parameter, case, wheel, |editor,input,phase| {
                let (builds,ports) = draw_counts(editor,&mut atlas,Rect::new(13.,17.,1200.,800.),input,language);
                counts.push((format!("{language:?}-{zoom}-{case:?}-{wheel}-{phase}"),builds,ports,editor.document.nodes.len()));
            });
        } } }
    }
    assert_eq!(counts.len(),816);
    assert!(counts.iter().all(|(_,builds,ports,nodes)| *builds==1 && ports==nodes),
        "one final card build for field commits: {counts:?}");
}

#[test]
fn record_complete_canvas_field_frames_for_original_path_comparison() {
    let mut evidence = std::collections::BTreeMap::new();
    for language in [Language::English, Language::Chinese] {
        let mut atlas = TextAtlas::from_system_language(12., language == Language::Chinese).unwrap();
        for zoom in [0.5, 1.25] { for case in CASES { for wheel in 0..3 {
            let (mut editor, parameter) = prepare(zoom);
            scenario(&mut editor, &parameter, case, wheel, |editor,input,phase| {
                let mut paint = PaintList::default();
                let response = editor.draw(&mut paint,&mut atlas,Rect::new(13.,17.,1200.,800.),&input,language);
                let record = format!("{:?}",((paint.shapes,paint.colors,paint.glyphs,paint.order),
                    (response.actions,response.wants_pointer,response.wants_keyboard,response.changed,response.semantic_changed),
                    editor.snapshot(),(&editor.selected,&editor.primary,editor.view,&editor.notice,&editor.diagnostic),
                    editor.field.as_ref().map(|f| (&f.binding,&f.text,f.cursor,f.replace,&f.error)),
                    drag_state(editor),history_state(&editor.undo),history_state(&editor.redo)));
                assert!(evidence.insert(format!("{language:?}-{zoom}-{case:?}-{wheel}-{phase}"),record).is_none());
            });
        } } }
    }
    assert_eq!(evidence.len(),816);
    if let Some(path) = std::env::var_os("MOTION_NODE_FIELD_EVIDENCE") {
        std::fs::write(path,serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    }
}
