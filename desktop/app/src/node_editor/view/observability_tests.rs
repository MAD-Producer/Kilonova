use super::super::inline::Popup;
use super::super::tests::simple;
use super::*;

#[test]
fn real_popup_and_inspector_degradations_are_bounded_and_keep_their_context() {
    const CHILD: &str = "MOTION_NODE_DEGRADATION_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let mut editor = simple();
        let original = editor.snapshot();
        let mut atlas = TextAtlas::from_system(13.).unwrap();
        editor.inline.popup = Some(Popup::Color {
            position: [0.; 2],
            node: "number".into(),
            bindings: std::array::from_fn(|i| Binding::Constant("missing_color".into(), i)),
            before: editor.state(true),
            hue: 0.,
            scroll: 0.,
            drag: None,
        });
        for _ in 0..120 {
            editor.paint_control_popup(
                &mut PaintList::default(),
                &mut atlas,
                Rect::new(0., 0., 640., 480.),
                &Input::default(),
                Language::English,
            );
        }
        assert_eq!(editor.document, original.document);
        assert_eq!(editor.revision, original.revision);
        let text = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        let events: Vec<_> = text
            .lines()
            .filter(|line| line.contains("site=popup.color_binding"))
            .collect();
        assert_eq!(events.len(), 2, "{text}");
        assert!(events[0].contains("WARN") && events[0].contains("count=1"));
        assert!(events[1].contains("ERROR") && events[1].contains("count=60"));
        assert!(events
            .iter()
            .all(|line| line.contains("missing_color") && line.contains("fallback=0")));
        editor.cancel_inline();
        editor
            .document
            .nodes
            .iter_mut()
            .find(|n| n.id == "number")
            .unwrap()
            .operation = NodeOp::Unsupported {
            feature: "test-unsupported-node".into(),
        };
        editor.select(Some("number"), false);
        for _ in 0..120 {
            editor.draw_inspector(
                &mut PaintList::default(),
                &mut atlas,
                Rect::new(0., 0., 360., 600.),
                &Input::default(),
                Language::English,
            );
        }
        let text = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        let events: Vec<_> = text
            .lines()
            .filter(|line| line.contains("site=inspector.unsupported"))
            .collect();
        assert_eq!(events.len(), 2);
        assert!(events
            .iter()
            .all(|line| line.contains("number") && line.contains("test-unsupported-node")));
        let before = editor.state(true);
        editor.options.author = "changed options".into();
        editor.finish_inline_history(before, false);
        // Keep the existing panic policy; verify evidence precedes unwinding.
        for binding in [
            Binding::ParamName("deleted".into(), false),
            Binding::ParamMin("deleted".into()),
            Binding::ParamMax("deleted".into()),
            Binding::ParamStep("deleted".into()),
            Binding::ParamUnits("deleted".into()),
        ] {
            assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || editor.apply_field(binding, "1")
            ))
            .is_err());
        }
        let text = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        assert!(text.contains("site=inline.history_options"));
        assert_eq!(
            text.lines()
                .filter(|line| line.contains("node field binding missing before apply"))
                .count(),
            5
        );
        assert!(text.contains("parameter=\"deleted\""));
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "node_editor::view::observability_tests::real_popup_and_inspector_degradations_are_bounded_and_keep_their_context"])
        .env_remove("MOTION_LOG").env(CHILD, temp.path()).output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("site=popup.color_binding")
            && stderr.contains("node field binding missing before apply")
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("node degradation"));
}

#[test]
fn malformed_ports_and_rejected_requests_log_once_per_site_without_editing() {
    const CHILD: &str = "MOTION_NODE_PORT_FALLBACK_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let mut editor = simple();
        editor.document.nodes.iter_mut().find(|n| n.id == "number").unwrap().operation = NodeOp::Unsupported { feature: "controlled-missing-op".into() };
        editor.select(Some("number"),false);
        let original = editor.document.clone();
        let revision = editor.revision;
        let mut atlas = TextAtlas::from_system(13.).unwrap();
        let area = Rect::new(0.,0.,900.,600.);
        for _ in 0..120 {
            editor.arrange_without_history();
            let cards = editor.cards(area);
            assert!(cards.iter().find(|c| c.id == "number").unwrap().sockets.is_empty());
            editor.paint_canvas(&mut PaintList::default(),&mut atlas,&cards,area,&Input::default(),Language::English);
            editor.draw_inspector(&mut PaintList::default(),&mut atlas,Rect::new(0.,0.,360.,600.),&Input::default(),Language::English);
            editor.context_menu = Some(ContextMenu { kind: MenuKind::Node("number".into()), position: [10.;2], selected:0 });
            editor.paint_context_menu(&mut PaintList::default(),&mut atlas,area,&Input::default(),Language::English);
        }
        editor.field = Some(Field { binding: Binding::Constant("number".into(),0), text:"invalid".into(),cursor:7,replace:false,error:None });
        for _ in 0..120 { assert!(editor.request(Request::Save).is_err()); }
        assert_eq!(editor.document,original);
        assert_eq!(editor.revision,revision);
        assert!(editor.field.as_ref().unwrap().error.is_some() && editor.notice.is_some());
        let text = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        for site in ["canvas.sockets","canvas.labels","context.ports","inspector.ports","inspector.help_ports","request.field","layout.arrange"] {
            let records = text.lines().filter(|l| l.contains(&format!("site={site} "))).collect::<Vec<_>>();
            assert_eq!(records.len(),2,"{site}: {text}");
            assert!(records[0].contains("WARN") && records[1].contains("ERROR"));
        }
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact", "node_editor::view::observability_tests::malformed_ports_and_rejected_requests_log_once_per_site_without_editing"]).env_remove("MOTION_LOG").env(CHILD,temp.path()).output().unwrap();
    assert!(output.status.success(),"{}\n{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stderr).contains("site=canvas.sockets"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("node degradation"));
}
