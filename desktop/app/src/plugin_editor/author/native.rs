use super::*;
use crate::engine::{AtlasUpload, Engine, UiFrame};
use motion_ui::input::Event;
use std::{sync::Arc, time::Duration};
use winit::{dpi::LogicalSize, event_loop::EventLoop, window::Window};

#[test]
#[ignore = "explicit native plugin author acceptance; Windows GPU required"]
fn author_drag_source_history_save_and_native_rendering() {
    const CHILD: &str = "MOTION_PLUGIN_AUTHOR_NATIVE_CHILD";
    const TEST: &str =
        "plugin_editor::author::native::author_drag_source_history_save_and_native_rendering";
    if let Some(root) = std::env::var_os(CHILD) {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let root = PathBuf::from(root);
        std::fs::create_dir_all(&root).unwrap();
        let _logging = crate::logging::init(root.join("logs")).unwrap();
        let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
        #[allow(deprecated)]
        let window = Arc::new(
            events
                .create_window(
                    Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(LogicalSize::new(1440., 900.)),
                )
                .unwrap(),
        );
        let engine = Engine::start(
            root.join("sample"),
            motion_desktop_media::platform(),
            crate::total_memory(),
        )
        .unwrap();
        let reply = engine.attach_async(window.clone(), 1440, 900).unwrap();
        crate::native_test_support::receive(&mut events, &reply, Duration::from_secs(60))
            .unwrap()
            .unwrap();
        let project = super::tests::example();
        let path = root.join("authored.msplugin");
        project.save(&path).unwrap();
        let source = project.clone();
        let mut ws = Workspace::new(project, path.clone(), true).unwrap();
        let styles = motion_ui::style::default_stylesheet_arc();
        let mut atlas =
            TextAtlas::from_family(styles.font_family(), styles.base_font_size(), true).unwrap();
        let bounds = Rect::new(0., 0., 1440., 900.);
        let area = bounds.shrink(styles.metric("padding.panel", 8.));
        let mut render = |ws: &mut Workspace, input: &Input, name: Option<&str>| {
            let mut chrome = PaintList {
                stylesheet: Some(styles.clone()),
                ..Default::default()
            };
            let slot = ws.draw(&mut chrome, &mut atlas, area, input).unwrap();
            if let Some(name) = name {
                let mut paint = PaintList {
                    stylesheet: Some(styles.clone()),
                    ..Default::default()
                };
                super::super::window::paint_outside(
                    &mut paint,
                    bounds,
                    slot,
                    styles.color("panel"),
                );
                paint.append(chrome);
                let cfg = atlas.config();
                let frame = UiFrame { views: None,
                    paint,
                    atlas: Some(AtlasUpload {
                        pixels: atlas.pixel_data().to_vec(),
                        width: cfg.width,
                        height: cfg.height,
                    }),
                    logical_size: [1440., 900.],
                    composition: None,
                    composition_image: None,
                    inspection: Default::default(),
                    window: 0,
                    node_preview: slot.map(|r| crate::node_preview::Viewport {
                        rect: [r.min[0], r.min[1], r.width(), r.height()],
                        seconds: 0.,
                    }),
                };
                crate::native_test_support::capture_ui(
                    &mut events,
                    &engine,
                    frame,
                    root.join(name),
                )
                .unwrap();
            }
        };
        fn ready(ws: &mut Workspace) -> (u64, Ready) {
            let began = Instant::now();
            loop {
                ws.poll().unwrap();
                if let Some(r) = ws.take_ready() {
                    return r;
                }
                assert!(
                    began.elapsed() < Duration::from_secs(45),
                    "author compile did not finish: {:?}",
                    ws.message
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        let (revision, ready) = ready(&mut ws);
        let stamp = (2, revision);
        engine.node_expect(stamp).unwrap();
        let defs = ready.prepared.definitions().to_vec();
        assert!(engine
            .plugin_install(stamp, ready.prepared)
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap());
        ws.install_preview(revision, defs).unwrap();
        render(&mut ws, &neutral(), Some("interface-initial.png"));
        let click = |x, y| Input {
            mouse: [x, y],
            pressed: Some((MouseButton::Left, [x, y])),
            ..neutral()
        };
        render(&mut ws, &click(70., 115.), None); // component library
        render(&mut ws, &neutral(), Some("component-library.png"));
        let component = ws.designer.component_rects[2];
        let component_origin = [
            (component.min[0] + component.max[0]) * 0.5,
            (component.min[1] + component.max[1]) * 0.5,
        ];
        let mut hover = neutral();
        hover.mouse = component_origin;
        render(&mut ws, &hover, Some("component-hover.png"));
        let mut drag = |ws: &mut Workspace, from: [f32; 2], to: [f32; 2]| {
            let mut input = neutral();
            ui::accumulate(
                &mut input,
                Event::MousePressed {
                    position: from,
                    button: MouseButton::Left,
                },
            );
            render(ws, &input, None);
            input.begin_frame();
            ui::accumulate(&mut input, Event::MouseMoved { position: to });
            render(ws, &input, None);
            input.begin_frame();
            ui::accumulate(
                &mut input,
                Event::MouseReleased {
                    position: to,
                    button: MouseButton::Left,
                },
            );
            render(ws, &input, None);
        };
        drag(&mut ws, component_origin, [430., 145.]); // Group -> native canvas
        let group = ws
            .designer
            .selected
            .clone()
            .expect("drag must select inserted component");
        assert!(matches!(
            model::find(&ws.draft.project.interface.root, &group)
                .unwrap()
                .content,
            motion_effects::native_layout::LayoutContent::Group { .. }
        ));
        drop(drag);
        render(&mut ws, &click(40., 115.), None); // structure tree
        let first = source.interface.root.children().next().unwrap().id.clone();
        let mut input = neutral();
        for event in [
            Event::MousePressed {
                position: [60., 159.],
                button: MouseButton::Left,
            },
            Event::MouseMoved {
                position: [60., 291.],
            },
            Event::MouseReleased {
                position: [60., 291.],
                button: MouseButton::Left,
            },
        ] {
            ui::accumulate(&mut input, event);
            render(&mut ws, &input, None);
            input.begin_frame();
        }
        assert_eq!(
            model::find(&ws.draft.project.interface.root, &group)
                .unwrap()
                .children()
                .next()
                .unwrap()
                .id,
            first
        );
        // Drag a public parameter from its library into the actual placed group.
        let group_rect = ws.designer.rects[&group];
        render(&mut ws, &click(117., 115.), None);
        let mut input = neutral();
        for event in [
            Event::MousePressed {
                position: [60., 181.],
                button: MouseButton::Left,
            },
            Event::MouseMoved {
                position: [group_rect.min[0] + 30., group_rect.min[1] + 10.],
            },
            Event::MouseReleased {
                position: [group_rect.min[0] + 30., group_rect.min[1] + 10.],
                button: MouseButton::Left,
            },
        ] {
            ui::accumulate(&mut input, event);
            render(&mut ws, &input, None);
            input.begin_frame();
        }
        let parameter = ws.designer.selected.clone().unwrap();
        assert!(
            matches!(&model::find(&ws.draft.project.interface.root,&parameter).unwrap().content,motion_effects::native_layout::LayoutContent::Parameter{parameter,..} if parameter=="amount")
        );
        // Insert a split, then drag its real divider instead of mutating its ratio.
        render(&mut ws, &click(70., 115.), None);
        let component = ws.designer.component_rects[4];
        let split_origin = [
            (component.min[0] + component.max[0]) * 0.5,
            (component.min[1] + component.max[1]) * 0.5,
        ];
        let mut input = neutral();
        for event in [
            Event::MousePressed {
                position: split_origin,
                button: MouseButton::Left,
            },
            Event::MouseMoved {
                position: [430., 145.],
            },
            Event::MouseReleased {
                position: [430., 145.],
                button: MouseButton::Left,
            },
        ] {
            ui::accumulate(&mut input, event);
            render(&mut ws, &input, None);
            input.begin_frame();
        }
        let split = ws.designer.selected.clone().unwrap();
        render(&mut ws, &neutral(), None);
        let rect = ws.designer.rects[&split];
        let from = [
            rect.min[0] + rect.width() * 0.5,
            rect.min[1] + rect.height() * 0.5,
        ];
        let to = [rect.min[0] + rect.width() * 0.65, from[1]];
        let mut input = neutral();
        for event in [
            Event::MousePressed {
                position: from,
                button: MouseButton::Left,
            },
            Event::MouseMoved { position: to },
            Event::MouseReleased {
                position: to,
                button: MouseButton::Left,
            },
        ] {
            ui::accumulate(&mut input, event);
            render(&mut ws, &input, None);
            input.begin_frame();
        }
        assert!(
            matches!(&model::find(&ws.draft.project.interface.root,&split).unwrap().content,motion_effects::native_layout::LayoutContent::Split{ratio,..} if (*ratio-0.65).abs()<0.01)
        );
        let before_cancel = ws.draft.project.clone();
        let rect = ws.designer.rects[&split];
        let from = [
            rect.min[0] + rect.width() * 0.65,
            rect.min[1] + rect.height() * 0.5,
        ];
        let mut input = neutral();
        for event in [
            Event::MousePressed {
                position: from,
                button: MouseButton::Left,
            },
            Event::MouseMoved {
                position: [from[0] - 50., from[1]],
            },
        ] {
            ui::accumulate(&mut input, event);
            render(&mut ws, &input, None);
            input.begin_frame();
        }
        assert_ne!(ws.draft.project, before_cancel);
        ws.cancel();
        assert_eq!(ws.draft.project, before_cancel);
        render(&mut ws, &neutral(), Some("interface-split.png"));
        let after = ws.draft.project.clone();
        render(&mut ws, &neutral(), Some("interface-moved.png"));
        render(&mut ws, &click(388., 20.), None);
        assert_ne!(ws.draft.project, after);
        render(&mut ws, &click(473., 20.), None);
        assert_eq!(ws.draft.project, after);
        render(&mut ws, &click(133., 20.), None);
        let began = Instant::now();
        while ws.draft.dirty() {
            ws.poll().unwrap();
            assert!(began.elapsed() < Duration::from_secs(30));
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(PluginProject::load(&path).unwrap(), after);
        assert_eq!(
            source.logic, after.logic,
            "layout work must not rewrite shader/defaults"
        );
        // An invalid WGSL draft remains saveable, locates its authored row, and
        // never replaces the previous GPU pipeline.
        ws.page = 2;
        render(&mut ws, &neutral(), Some("preview.png"));
        ws.page = 0;
        render(&mut ws, &neutral(), Some("wgsl-editor.png"));
        if std::env::var_os("MOTION_WGSL_ANALYZER").is_some() {
            let began = Instant::now();
            while !ws
                .source
                .test_language_snapshot()
                .is_some_and(|s| s.status == lsp::Status::Ready && s.flight.is_none())
            {
                ws.source.poll_language().unwrap();
                render(&mut ws, &neutral(), None);
                assert!(began.elapsed() < Duration::from_secs(30));
                std::thread::sleep(Duration::from_millis(10));
            }
            let text = match &ws.draft.project.logic {
                PluginLogic::Wgsl { document } => document.source.clone(),
                _ => unreachable!(),
            };
            let column = text.lines().nth(3).unwrap().find("param_amount").unwrap()
                + "param_amount".len()
                + 1;
            ws.source.locate(4, column);
            let mut complete = neutral();
            complete.keys.push((
                Key::Space,
                motion_ui::input::Modifiers {
                    control: true,
                    ..Default::default()
                },
            ));
            render(&mut ws, &complete, None);
            let began = Instant::now();
            while !ws.source.test_language_snapshot().is_some_and(|s| {
                s.reply
                    .as_ref()
                    .is_some_and(|r| r.request.kind == lsp::Kind::Completion)
            }) {
                ws.source.poll_language().unwrap();
                render(&mut ws, &neutral(), None);
                assert!(began.elapsed() < Duration::from_secs(30));
                std::thread::sleep(Duration::from_millis(10));
            }
            render(&mut ws, &neutral(), Some("wgsl-completion.png"));
            let labels = ws.source.test_completion_labels();
            let index = labels
                .iter()
                .position(|label| label.starts_with("param_amount"))
                .expect("real host-parameter suggestion");
            for _ in 0..index {
                let mut down = neutral();
                down.keys.push((Key::Down, Default::default()));
                render(&mut ws, &down, None);
            }
            let mut enter = neutral();
            enter.keys.push((Key::Enter, Default::default()));
            render(&mut ws, &enter, None);
            assert_eq!(
                ws.source.test_text(),
                text,
                "real completion must replace just the identifier and preserve native source"
            );
        }
        // In-product help is a separate owner and never edits the draft.
        let before_guide = ws.draft.project.clone();
        let mut help = neutral();
        help.keys.push((Key::F1, Default::default()));
        render(&mut ws, &help, Some("guide-quickstart.png"));
        assert_eq!(ws.page, 3);
        assert_eq!(ws.draft.project, before_guide);
        ws.page = 0;
        // Coalesced native text input and Ctrl+S must save the latest edit.
        ws.source.locate(1, 1);
        let mut input = neutral();
        input.text = "// native saved edit\n".into();
        input.keys.push((
            Key::S,
            motion_ui::input::Modifiers {
                control: true,
                ..Default::default()
            },
        ));
        render(&mut ws, &input, None);
        let began = Instant::now();
        while ws.busy {
            ws.poll().unwrap();
            assert!(began.elapsed() < Duration::from_secs(30));
            std::thread::sleep(Duration::from_millis(5));
        }
        let saved = PluginProject::load(&path).unwrap();
        assert_eq!(saved, ws.draft.project);
        assert!(
            matches!(&saved.logic,PluginLogic::Wgsl{document} if document.source.starts_with("// native saved edit\n"))
        );
        ws.draft
            .edit(|p| {
                if let PluginLogic::Wgsl { document } = &mut p.logic {
                    document.source =
                        "fn plugin_fx(p: vec2<f32>) -> vec4<f32> {\nlet broken = ;\n}".into();
                }
                Ok(())
            })
            .unwrap();
        ws.source = source::Source::new(&ws.draft.project);
        render(&mut ws, &neutral(), None);
        let before = engine
            .node_capture(0., root.join("shader-before-error.png"))
            .unwrap();
        let deadline = Instant::now();
        while ws.message.is_none() {
            ws.poll().unwrap();
            assert!(deadline.elapsed() < Duration::from_secs(30));
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(ws.message.as_ref().unwrap().location.map(|p| p.0), Some(2));
        assert!(ws.ready.is_none());
        render(&mut ws, &neutral(), Some("wgsl-diagnostic.png"));
        let after_error = engine
            .node_capture(0., root.join("shader-after-error.png"))
            .unwrap();
        assert_eq!(before["fingerprint"].as_str().unwrap().len(), 16);
        assert_eq!(before["fingerprint"], after_error["fingerprint"]);
        ws.save(false).unwrap();
        let began = Instant::now();
        while ws.draft.dirty() {
            ws.poll().unwrap();
            assert!(began.elapsed() < Duration::from_secs(30));
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(PluginProject::load(&path).unwrap(), ws.draft.project);
        ws.draft
            .edit(|p| {
                p.package.version = "0.2.0".into();
                Ok(())
            })
            .unwrap();
        ws.request_close();
        assert!(ws.close_prompt);
        render(&mut ws, &neutral(), Some("unsaved-close.png"));
        assert!(!ws.close_ready);
        ws.checked = ws.draft.revision;
        let pad = styles.metric("padding.panel", 8.);
        let row = styles.metric("panel.header.height", 24.);
        let action = styles.metric("plugin.author.action.width", 80.);
        let width = (action + pad) * 3. + pad;
        let modal_button = |index: usize| [
            area.min[0] + (area.width() - width) * 0.5 + pad
                + index as f32 * (action + pad) + action * 0.5,
            area.min[1] + area.height() * 0.5 + row * 1.5 + pad,
        ];
        let p = modal_button(0);
        render(&mut ws, &click(p[0], p[1]), None);
        assert!(ws.busy && ws.close_after_save);
        render(&mut ws, &neutral(), Some("save-close-pending.png"));
        let p = modal_button(2);
        render(&mut ws, &click(p[0], p[1]), None);
        assert!(!ws.close_prompt && !ws.close_after_save && !ws.close_ready);
        let began = Instant::now();
        while ws.busy {
            ws.poll().unwrap();
            assert!(began.elapsed() < Duration::from_secs(30));
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!ws.close_ready);
        assert_eq!(PluginProject::load(&path).unwrap(), ws.draft.project);
        render(&mut ws, &neutral(), Some("save-close-cancelled.png"));
        // Existing node editor, with graph controls and root graph snapshots.
        let graph = motion_graph::plugin_project::PluginProject::from_graph(
            crate::node_editor::NodeEditor::default()
                .snapshot()
                .document,
            source.package,
        )
        .unwrap();
        let mut nodes = Workspace::new(graph, root.join("nodes.msplugin"), true).unwrap();
        nodes.page = 0;
        let node_original = nodes.draft.project.clone();
        nodes
            .nodes
            .as_mut()
            .unwrap()
            .add_node(motion_graph::NodeOp::Float { value: 1. }, [160., 160.])
            .unwrap();
        render(&mut nodes, &neutral(), Some("node-editor.png"));
        assert_ne!(nodes.draft.project, node_original);
        let node_only = nodes.draft.project.clone();
        let id = nodes.draft.project.interface.root.id.clone();
        nodes
            .draft
            .edit(|p| {
                model::insert(p, &id, 0, model::Component::Group)?;
                Ok(())
            })
            .unwrap();
        nodes.page = 1;
        render(&mut nodes, &click(388., 20.), None);
        assert_eq!(nodes.draft.project, node_only);
        render(&mut nodes, &click(388., 20.), None);
        assert_eq!(nodes.draft.project, node_original);

        std::fs::write(root.join("report.json"),serde_json::to_vec_pretty(&serde_json::json!({"native_drag_insert":true,"cross_group_move":true,"parameter_library_drag":true,"split_drag_and_cancel":true,"coalesced_text_save":true,"global_undo_redo":true,"node_layout_history":true,"saved_reopened_exactly":true,"invalid_source_saved":true,"error_row":2,"previous_gpu_retained":true})).unwrap()).unwrap();
        crate::logging::flush();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_PLUGIN_AUTHOR_ACCEPTANCE_DIR")
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
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("plugin.author error") && stderr.contains("location=Some((2,"));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("plugin.author") && !stdout.contains("motion_studio::"));
}
