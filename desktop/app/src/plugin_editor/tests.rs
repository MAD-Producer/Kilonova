use super::*;
fn source() -> motion_graph::plugin_project::PluginProject {
    motion_graph::plugin_project::PluginProject::from_json(include_str!(
        "../../../../crates/motion-graph/examples/parameter-tint.msplugin"
    ))
    .unwrap()
}
#[test]
fn gestures_are_atomic_cancel_restores_and_preview_never_changes_source() {
    let project = source();
    let original = project.to_json().unwrap();
    let mut session = Session::new(project.parameters().unwrap()).unwrap();
    session.begin();
    for i in 1..=30 {
        session.set(2, [i as f32 / 30., 0., 0., 0.]).unwrap();
    }
    session.commit();
    assert_eq!(session.values()[2][0], 1.);
    session.undo();
    assert_eq!(session.values()[2][0], 0.5);
    assert!(!session.can_undo());
    session.redo();
    assert_eq!(session.values()[2][0], 1.);
    session.begin();
    session.set(2, [0.1, 0., 0., 0.]).unwrap();
    session.cancel();
    assert_eq!(session.values()[2][0], 1.);
    assert!(session.set(2, [f32::NAN, 0., 0., 0.]).is_err());
    assert!(session.set(0, [0.5, 0., 0., 0.]).is_err());
    assert_eq!(session.values()[0][0], 1.);
    assert_eq!(project.to_json().unwrap(), original);
}
#[test]
fn narrow_layout_preserves_minimums_and_hides_folded_descendants() {
    let project = source();
    let styles = motion_ui::style::default_stylesheet();
    let metrics = Metrics::new(styles);
    let mut state = ViewState::default();
    let size = metrics.measure(&project.interface.root, &state);
    assert!(size[0] >= metrics.min_width);
    let mut placed = Vec::new();
    metrics.place(
        &project.interface.root,
        Rect::new(0., 0., size[0], size[1]),
        &state,
        &mut placed,
    );
    let rows: Vec<_> = placed
        .iter()
        .filter(|p| matches!(p.node.content, LayoutContent::Parameter { .. }))
        .collect();
    assert_eq!(rows.len(), 3);
    for pair in rows.windows(2) {
        assert!(pair[0].rect.max[1] <= pair[1].rect.min[1]);
    }
    state.folded.insert(project.interface.root.id.clone());
    placed.clear();
    metrics.place(
        &project.interface.root,
        Rect::new(0., 0., size[0], size[1]),
        &state,
        &mut placed,
    );
    assert_eq!(placed.len(), 1);
}

#[test]
fn native_pointer_cancel_clipping_and_shared_bindings_use_one_value() {
    let mut project = source();
    if let LayoutContent::Group { children, .. } = &mut project.interface.root.content {
        let mut alias = children[2].clone();
        alias.id = "amount-alias".into();
        children.push(alias);
    }
    let styles = motion_ui::style::default_stylesheet_arc();
    let mut atlas =
        TextAtlas::from_family(styles.font_family(), styles.base_font_size(), true).unwrap();
    let definitions = project.parameters().unwrap();
    let mut p = Preview::new(project.interface, definitions, true).unwrap();
    let area = Rect::new(0., 0., 360., 480.);
    let mut draw = |p: &mut Preview, input: &Input| {
        let mut list = PaintList {
            stylesheet: Some(styles.clone()),
            ..Default::default()
        };
        p.draw(&mut list, &mut atlas, area, area, input).unwrap();
    };
    let metrics = Metrics::new(&styles);
    let x = metrics.pad + (352. - metrics.pad * 2.) * 0.5 + 10.;
    let y = metrics.header + metrics.row * 2.5;
    let mut input = neutral();
    ui::accumulate(
        &mut input,
        motion_ui::input::Event::MousePressed {
            position: [x, y],
            button: MouseButton::Left,
        },
    );
    draw(&mut p, &input);
    input.begin_frame();
    ui::accumulate(
        &mut input,
        motion_ui::input::Event::MouseMoved {
            position: [x + 20., y],
        },
    );
    draw(&mut p, &input);
    input.begin_frame();
    assert!(p.session.active());
    assert!(p.session.values()[2][0] > 0.5);
    ui::accumulate(&mut input, motion_ui::input::Event::FocusLost);
    draw(&mut p, &input);
    assert_eq!(p.session.values()[2][0], 0.5);
    assert!(!p.session.can_undo());
    assert!(p.owner().is_none());
    p.session.set(2, [0.75, 0., 0., 0.]).unwrap();
    p.session.commit();
    draw(&mut p, &neutral());
    assert_eq!(
        p.session.values().len(),
        3,
        "alias rows must not allocate additional values"
    );
    p.session.undo();
    assert_eq!(p.session.values()[2][0], 0.5);
    // A press originating beyond the viewport must not activate a clipped row.
    let outside = Input {
        mouse: [x, y],
        pressed: Some((MouseButton::Left, [x, -1.])),
        ..Default::default()
    };
    draw(&mut p, &outside);
    assert!(p.owner().is_none());
}

#[test]
fn split_rows_tabs_and_weighted_extents_keep_children_inside_measured_bounds() {
    let project = source();
    let styles = motion_ui::style::default_stylesheet();
    let metrics = Metrics::new(styles);
    let param = if let LayoutContent::Group { children, .. } = &project.interface.root.content {
        children[2].clone()
    } else {
        panic!("sample group missing")
    };
    let mut second = param.clone();
    second.id = "second".into();
    second.width = LayoutExtent::Weight { value: 2. };
    let row = LayoutNode {
        id: "row".into(),
        width: LayoutExtent::Auto,
        height: LayoutExtent::Auto,
        content: LayoutContent::Row {
            gap: LayoutSpacing::Normal,
            align: LayoutAlign::Stretch,
            children: vec![param, second],
        },
    };
    let root = LayoutNode {
        id: "split".into(),
        width: LayoutExtent::Auto,
        height: LayoutExtent::Auto,
        content: LayoutContent::Split {
            axis: LayoutAxis::Horizontal,
            ratio: 0.1,
            first: Box::new(row),
            second: Box::new(LayoutNode {
                id: "tabs".into(),
                width: LayoutExtent::Auto,
                height: LayoutExtent::Auto,
                content: LayoutContent::Tabs {
                    tabs: vec![LayoutTab {
                        id: "tab".into(),
                        title: "参数".into(),
                        child: LayoutNode {
                            id: "note".into(),
                            width: LayoutExtent::Auto,
                            height: LayoutExtent::Auto,
                            content: LayoutContent::Note {
                                text: "说明".into(),
                            },
                        },
                    }],
                },
            }),
        },
    };
    let layout = NativeLayout {
        format_version: 1,
        root,
    };
    layout
        .validate_bindings(&project.parameters().unwrap())
        .unwrap();
    let state = ViewState::default();
    let minimum = metrics.measure(&layout.root, &state);
    let r = Rect::new(0., 0., minimum[0] + metrics.min_width, minimum[1]);
    let mut placed = Vec::new();
    metrics.place(&layout.root, r, &state, &mut placed);
    for item in placed {
        assert!(
            r.contains_rect(item.rect),
            "{} {:?}",
            item.node.id,
            item.rect
        );
    }
}

#[test]
fn preparation_publishes_before_decode_and_reports_faults_at_default_level() {
    use std::sync::{atomic::Ordering, mpsc, Arc};
    const CHILD: &str = "MOTION_PLUGIN_PREP_CHILD";
    const TEST:&str="plugin_editor::tests::preparation_publishes_before_decode_and_reports_faults_at_default_level";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let path = root.join("source.msplugin");
        source().save(&path).unwrap();
        let flight = Arc::new(preparation::Preparation::new(&path));
        let worker_flight = flight.clone();
        let worker_path = path.clone();
        let (entered, entry) = mpsc::sync_channel(1);
        let (release, released) = mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            worker_flight.prepare_with(&worker_path, |compiled| {
                entered.send(()).unwrap();
                released.recv().unwrap();
                crate::node_preview::PreparedPlugin::new(compiled)
            })
        });
        entry
            .recv_timeout(std::time::Duration::from_secs(30))
            .unwrap();
        assert_eq!(flight.phase.load(Ordering::Acquire), 2);
        let file = root.join("logs/motion-studio.log");
        let log = std::fs::read_to_string(&file).unwrap();
        assert!(log.contains("phase=resource_decode in_flight=true"));
        release.send(()).unwrap();
        assert!(worker.join().unwrap().is_ok());
        assert_eq!(flight.phase.load(Ordering::Acquire), 0);
        let bad = flight.prepare_with(&path, |_| {
            motion_render::image_resources::decode_png_with_budget(
                b"bad png",
                motion_effects::DEFAULT_DECODED_IMAGE_BUDGET,
            )
            .map_err(motion_render::RenderError::from)?;
            unreachable!("bad PNG must be rejected by the actual decoder")
        });
        assert!(bad.is_err());
        assert_eq!(flight.phase.load(Ordering::Acquire), 0);
        let panic = std::panic::catch_unwind(|| {
            flight.prepare_with(&path, |_| {
                panic!("controlled plugin resource decoder panic")
            })
        });
        assert!(panic.is_err());
        assert_eq!(flight.phase.load(Ordering::Acquire), 0);
        crate::logging::flush();
        let log = std::fs::read_to_string(file).unwrap();
        assert!(log.contains("plugin.preview error request=1 phase=2"));
        assert!(log.contains("plugin.preview aborted request=1 phase=2"));
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let evidence = std::env::var_os("MOTION_PLUGIN_PREPARATION_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temp.path().to_owned());
    std::fs::create_dir_all(&evidence).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture"])
        .env(CHILD, &evidence)
        .env("MOTION_LOG", "info")
        .output()
        .unwrap();
    std::fs::write(evidence.join("child.stdout.txt"), &output.stdout).unwrap();
    std::fs::write(evidence.join("child.stderr.txt"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("plugin.preview error request=1 phase=2")
            && stderr.contains("controlled plugin resource decoder panic")
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("plugin.preview") && !stdout.contains("motion_studio::"));
}

#[test]
fn color_and_enum_popups_own_input_and_commit_one_undo_step() {
    let project = motion_graph::plugin_project::PluginProject::from_json(include_str!(
        "../../../../crates/motion-graph/examples/parameter-workbench.msplugin"
    ))
    .unwrap();
    let definitions = project.parameters().unwrap();
    let mut p = Preview::new(project.interface, definitions, true).unwrap();
    let styles = motion_ui::style::default_stylesheet_arc();
    let m = Metrics::new(&styles);
    let mut atlas =
        TextAtlas::from_family(styles.font_family(), styles.base_font_size(), true).unwrap();
    let area = Rect::new(0., 0., 360., 480.);
    let x = m.pad + (352. - m.pad * 2.) * 0.5 + 10.;
    let mut draw = |p: &mut Preview, input: &Input| {
        let mut list = PaintList {
            stylesheet: Some(styles.clone()),
            ..Default::default()
        };
        p.draw(&mut list, &mut atlas, area, area, input).unwrap();
    };
    let click = |y| Input {
        mouse: [x, y],
        pressed: Some((MouseButton::Left, [x, y])),
        ..Default::default()
    };
    draw(&mut p, &click(m.header + m.row * 2.5));
    assert!(p.color.is_some());
    draw(&mut p, &click(m.header + m.row * 0.5));
    assert_eq!(
        p.session.values()[0][0],
        1.,
        "color popup must mask the underlying checkbox"
    );
    draw(
        &mut p,
        &Input {
            keys: vec![(Key::Escape, Default::default())],
            ..neutral()
        },
    );
    assert!(p.color.is_none());
    assert!(!p.session.active());
    draw(&mut p, &click(m.header + m.row * 5.5));
    assert!(p.fields["blend-field"].dropdown.open);
    // Native dropdown popup starts below the field, with token-sized option rows.
    let option_y = m.header
        + m.row * 5.
        + (m.row - styles.metric("field.height", 18.)) * 0.5
        + styles.metric("field.height", 18.)
        + m.row * 1.5;
    draw(&mut p, &click(option_y));
    assert_eq!(p.session.values()[5][0], 1.);
    assert!(!p.fields["blend-field"].dropdown.open);
    p.session.undo();
    assert_eq!(p.session.values()[5][0], 0.);
    assert!(!p.session.can_undo());
}
