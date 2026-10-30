use super::*;

// Explicit native acceptance, rather than a costly default CI gate.
#[test]
#[ignore = "requires a real Windows GPU and native window owner"]
fn native_layout_controls_render_and_update_shader_without_recompilation() {
    const CHILD: &str = "MOTION_PLUGIN_NATIVE_CHILD";
    const TEST:&str="plugin_editor::window::acceptance::native_layout_controls_render_and_update_shader_without_recompilation";
    if let Some(root) = std::env::var_os(CHILD) {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
        #[allow(deprecated)]
        let window = Arc::new(
            events
                .create_window(
                    Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(LogicalSize::new(1000., 680.)),
                )
                .unwrap(),
        );
        let engine = Engine::start(
            root.join("sample-session"),
            motion_desktop_media::platform(),
            crate::total_memory(),
        )
        .unwrap();
        let attached = engine.attach_async(window.clone(), 1000, 680).unwrap();
        crate::native_test_support::receive(&mut events, &attached, Duration::from_secs(60))
            .unwrap()
            .unwrap();
        let project = motion_graph::plugin_project::PluginProject::from_json(include_str!(
            "../../../../crates/motion-graph/examples/parameter-workbench.msplugin"
        ))
        .unwrap();
        let source = project.to_json().unwrap();
        let definitions = project.parameters().unwrap();
        let prepared =
            crate::node_preview::PreparedPlugin::new(project.compile_logic().unwrap()).unwrap();
        engine.node_expect(STAMP).unwrap();
        let installed = engine.plugin_install(STAMP, prepared).unwrap();
        assert!(crate::native_test_support::receive(
            &mut events,
            &installed,
            Duration::from_secs(60)
        )
        .unwrap()
        .unwrap());
        let styles = motion_ui::style::default_stylesheet_arc();
        let mut app = App {
            engine,
            atlas: TextAtlas::from_family(styles.font_family(), styles.base_font_size(), true)
                .unwrap(),
            styles,
            window: Some(window),
            input: neutral(),
            pointer: Default::default(),
            preview: Some(Preview::new(project.interface.clone(), definitions, true).unwrap()),
            author: None,
            author_path: None,
            author_load: None,
            author_installing: None,
            stamp: STAMP,
            load: None,
            prepared: None,
            attach: None,
            install: None,
            values: None,
            present: None,
            resize: None,
            desired_size: None,
            latest: None,
            revision: 0,
            installed: true,
            dirty: false,
            scale: 1.,
            error: None,
            zh: true,
            force_atlas: true,
            preparation: Arc::new(super::super::preparation::Preparation::new(Path::new(
                "native-sample",
            ))),
            frame_started: false,
            frame_visible: false,
            startup_warned: false,
            embedded: false,
            finished: false,
            _scratch: Scratch::new().unwrap(),
        };
        fn capture(app: &mut App, events: &mut EventLoop<()>, path: PathBuf) -> serde_json::Value {
            let mut frame = app.draw().unwrap();
            let config = app.atlas.config();
            frame.atlas = Some(AtlasUpload {
                pixels: app.atlas.pixel_data().to_vec(),
                width: config.width,
                height: config.height,
            });
            crate::native_test_support::capture_ui(events, &app.engine, frame, path).unwrap()
        }
        capture(&mut app, &mut events, root.join("wide-zh.png"));
        let before = app
            .engine
            .node_capture(0., root.join("shader-before.png"))
            .unwrap();
        let info = app.engine.state().unwrap();
        // Pointer interaction on the ordinary native number editor, not a model shortcut.
        let p = app.preview.as_mut().unwrap();
        let area = Rect::new(0., 0., 360., 480.);
        let bounds = Rect::new(0., 0., 1000., 680.);
        let metric = Metrics::new(&app.styles);
        let x = metric.pad + (352. - metric.pad * 2.) * 0.5 + 10.;
        let y = metric.header + metric.row * 1.5;
        let mut input = neutral();
        let mut draw = |p: &mut Preview, input: &Input| {
            let mut list = PaintList {
                stylesheet: Some(app.styles.clone()),
                ..Default::default()
            };
            p.draw(&mut list, &mut app.atlas, area, bounds, input)
                .unwrap();
        };
        ui::accumulate(
            &mut input,
            Event::MousePressed {
                position: [x, y],
                button: MouseButton::Left,
            },
        );
        draw(p, &input);
        input.begin_frame();
        ui::accumulate(
            &mut input,
            Event::MouseMoved {
                position: [x + 30., y],
            },
        );
        draw(p, &input);
        input.begin_frame();
        ui::accumulate(
            &mut input,
            Event::MouseReleased {
                position: [x + 30., y],
                button: MouseButton::Left,
            },
        );
        draw(p, &input);
        assert!(
            p.session.values()[2][0] > 0.5,
            "native drag must change the actual sample parameter"
        );
        let values = p.session.values().to_vec();
        let updated = app.engine.plugin_values(STAMP, values).unwrap();
        assert!(crate::native_test_support::receive(
            &mut events,
            &updated,
            Duration::from_secs(60)
        )
        .unwrap()
        .unwrap());
        let after = app
            .engine
            .node_capture(0., root.join("shader-after.png"))
            .unwrap();
        assert_ne!(before["fingerprint"], after["fingerprint"]);
        assert_eq!(
            info["desktop"]["nodePreview"]["candidatePipelineBuilds"],
            app.engine.state().unwrap()["desktop"]["nodePreview"]["candidatePipelineBuilds"]
        );
        p_undo(&mut app);
        let updated = app
            .engine
            .plugin_values(
                STAMP,
                app.preview.as_ref().unwrap().session.values().to_vec(),
            )
            .unwrap();
        assert!(crate::native_test_support::receive(
            &mut events,
            &updated,
            Duration::from_secs(60)
        )
        .unwrap()
        .unwrap());
        let restored = app
            .engine
            .node_capture(0., root.join("shader-undo.png"))
            .unwrap();
        assert_eq!(before["fingerprint"], restored["fingerprint"]);
        // Failure must preserve the installed effect and use the default error channels.
        let invalid = app
            .engine
            .plugin_values(STAMP, vec![[f32::NAN; 4]])
            .unwrap();
        assert!(crate::native_test_support::receive(
            &mut events,
            &invalid,
            Duration::from_secs(60)
        )
        .unwrap()
        .is_err());
        let retained = app
            .engine
            .node_capture(0., root.join("shader-retained.png"))
            .unwrap();
        assert_eq!(restored["fingerprint"], retained["fingerprint"]);
        // Stale requests are normal supersession, not faults.
        let stale = app.engine.plugin_values((0, 0), vec![]).unwrap();
        assert!(
            !crate::native_test_support::receive(&mut events, &stale, Duration::from_secs(60))
                .unwrap()
                .unwrap()
        );
        app.preview.as_mut().unwrap().session.begin();
        app.preview.as_mut().unwrap().color = Some(("tint-field".into(), 1));
        capture(&mut app, &mut events, root.join("color-popup.png"));
        app.preview.as_mut().unwrap().cancel();
        app.preview
            .as_mut()
            .unwrap()
            .fields
            .entry("blend-field".into())
            .or_default()
            .dropdown
            .open = true;
        capture(&mut app, &mut events, root.join("enum-popup.png"));
        app.preview.as_mut().unwrap().cancel();
        // An authored viewport participates in the layout; a popup may cover
        // it, and native chrome must still compose above the shader content.
        let original = project.interface.clone();
        let preview_node = LayoutNode {
            id: "authored-preview".into(),
            width: LayoutExtent::Auto,
            height: LayoutExtent::Auto,
            content: LayoutContent::Preview,
        };
        app.preview.as_mut().unwrap().layout.root = LayoutNode {
            id: "workspace".into(),
            width: LayoutExtent::Auto,
            height: LayoutExtent::Auto,
            content: LayoutContent::Column {
                gap: LayoutSpacing::Normal,
                align: LayoutAlign::Stretch,
                children: vec![original.root.clone(), preview_node.clone()],
            },
        };
        capture(
            &mut app,
            &mut events,
            root.join("authored-preview-slot.png"),
        );
        app.preview.as_mut().unwrap().session.begin();
        app.preview.as_mut().unwrap().color = Some(("tint-field".into(), 1));
        capture(&mut app, &mut events, root.join("popup-over-preview.png"));
        let clear = image::open(root.join("authored-preview-slot.png"))
            .unwrap()
            .into_rgba8();
        let covered = image::open(root.join("popup-over-preview.png"))
            .unwrap()
            .into_rgba8();
        let pixel = covered.get_pixel(500, 360).0;
        assert!(
            pixel[0] < 60 && pixel[1] < 60 && pixel[2] < 60,
            "the popup must cover shader pixels: {pixel:?}"
        );
        assert_ne!(clear.get_pixel(500, 360), covered.get_pixel(500, 360));
        app.preview.as_mut().unwrap().cancel();
        app.preview.as_mut().unwrap().layout.root = LayoutNode {
            id: "workspace-tabs".into(),
            width: LayoutExtent::Auto,
            height: LayoutExtent::Auto,
            content: LayoutContent::Tabs {
                tabs: vec![
                    LayoutTab {
                        id: "parameters-tab".into(),
                        title: "参数".into(),
                        child: original.root.clone(),
                    },
                    LayoutTab {
                        id: "preview-tab".into(),
                        title: "预览".into(),
                        child: preview_node,
                    },
                ],
            },
        };
        assert!(
            app.draw().unwrap().node_preview.is_none(),
            "an inactive viewport tab must not fall back over the parameter controls"
        );
        app.preview.as_mut().unwrap().layout = original;
        app.zh = false;
        app.preview.as_mut().unwrap().zh = false;
        capture(&mut app, &mut events, root.join("wide-en.png"));
        // Same native renderer at a narrow logical width and at 200% density.
        let _resize_request = app
            .window
            .as_ref()
            .unwrap()
            .request_inner_size(winit::dpi::PhysicalSize::new(1000, 900));
        let resize = app.engine.resize_async(0, 1000, 900).unwrap();
        crate::native_test_support::receive(&mut events, &resize, Duration::from_secs(60))
            .unwrap()
            .unwrap();
        app.scale = 2.;
        app.atlas.set_pixel_ratio(2.);
        capture(&mut app, &mut events, root.join("narrow-2x.png"));
        assert_eq!(source, project.to_json().unwrap());
        // The graph authoring path uses the identical layout renderer and GPU
        // install protocol, including public uniforms in the baked graph API.
        let graph = motion_graph::plugin_project::PluginProject::from_graph(
            crate::node_editor::NodeEditor::default()
                .snapshot()
                .document,
            motion_graph::plugin_project::PackageIdentity {
                id: "studio.motion.examples.graph_preview".into(),
                version: "0.1.0".into(),
                author: "Motion Studio contributors".into(),
                license: "MIT".into(),
            },
        )
        .unwrap();
        let graph_path = root.join("graph.msplugin");
        graph.save(&graph_path).unwrap();
        let (_, compiled) = crate::plugin_check::load_compiled(&graph_path).unwrap();
        let prepared = crate::node_preview::PreparedPlugin::new(compiled).unwrap();
        let mut graph_values: Vec<_> = prepared.definitions().iter().map(|p| p.default).collect();
        let installed = app.engine.plugin_install(STAMP, prepared).unwrap();
        assert!(crate::native_test_support::receive(
            &mut events,
            &installed,
            Duration::from_secs(60)
        )
        .unwrap()
        .unwrap());
        let graph_before = app
            .engine
            .node_capture(0., root.join("graph-before.png"))
            .unwrap();
        graph_values[0] = [0.9, 0.1, 0.2, 1.];
        let changed = app.engine.plugin_values(STAMP, graph_values).unwrap();
        assert!(crate::native_test_support::receive(
            &mut events,
            &changed,
            Duration::from_secs(60)
        )
        .unwrap()
        .unwrap());
        let graph_after = app
            .engine
            .node_capture(0., root.join("graph-after.png"))
            .unwrap();
        assert_ne!(graph_before["fingerprint"], graph_after["fingerprint"]);
        let report = serde_json::json!({"drag_changes_shader":true,"undo_restores_pixels":true,"invalid_values_preserve_pixels":true,"pipeline_recompiled_for_parameter_edits":false,"source_unchanged":true,"wide":[1000,680],"narrow_logical":[500,450],"scale":2});
        std::fs::write(
            root.join("report.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        crate::logging::flush();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_PLUGIN_ACCEPTANCE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().to_owned());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--ignored", "--nocapture"])
        .env(CHILD, &root)
        .env("MOTION_LOG", "info")
        .output()
        .unwrap();
    std::fs::write(root.join("child.stdout.txt"), &output.stdout).unwrap();
    std::fs::write(root.join("child.stderr.txt"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // libtest owns stdout; product logging must not add records there.
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("plugin.preview") && !stdout.contains("motion_studio::"));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("plugin preview values violate installed parameter contract"));
    let file = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    assert!(file.contains("plugin_preview_install") && file.contains("plugin_preview_values"));
    assert!(file.contains("plugin preview values violate installed parameter contract"));
}
fn p_undo(app: &mut App) {
    app.preview.as_mut().unwrap().session.undo();
    assert!(!app.preview.as_ref().unwrap().session.can_undo());
}
