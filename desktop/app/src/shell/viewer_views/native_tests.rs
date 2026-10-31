//! Real shell chrome and native GPU captures; explicit UI inputs, not OS injection.
use super::*;
use crate::{
    native_test_support,
    viewer_views::{Layout, Mode},
};
use motion_ui::{
    dock::Panel,
    input::{Input, MouseButton},
};
use winit::{
    dpi::PhysicalSize, event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows,
    window::Window,
};

fn draw(
    shell: &mut Shell,
    size: [f32; 2],
    floating: bool,
    input: Input,
) -> (PaintList, Vec<panels::Action>) {
    if !floating {
        shell.input = input;
        shell.draw_commands(size)
    } else {
        shell.panels.viewer_views.prepare(1);
        let mut paint = PaintList::default();
        paint.stylesheet = shell.panels.styles.clone();
        let actions = panels::floating_chrome(
            &mut paint,
            &mut shell.atlas,
            Panel::Composition,
            size,
            &mut shell.panels,
            &shell.state,
            &input,
        );
        (paint, actions)
    }
}
fn execute(shell: &mut Shell, actions: Vec<panels::Action>) {
    for action in actions {
        shell.action(action, None).unwrap();
    }
}
fn projections(shell: &mut Shell) {
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        shell.poll_viewer_views();
        if shell
            .panels
            .viewer_views
            .cache
            .as_ref()
            .is_some_and(|(key, _)| *key == shell.projection_key())
        {
            return;
        }
        assert!(
            shell.panels.viewer_views.failed.is_none(),
            "projection failed: {:?}",
            shell.panels.message
        );
        assert!(Instant::now() < end, "projection did not complete");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn frame(shell: &mut Shell, size: [f32; 2], floating: bool) -> engine::UiFrame {
    let (paint, actions) = draw(shell, size, floating, Input::default());
    execute(shell, actions);
    let frame = if !floating {
        shell.ui_frame(paint, size, true)
    } else {
        let view = panels::floating_view_styled(Panel::Composition, size, &paint.styles()).unwrap();
        let views = shell
            .viewer_plan(view, shell.scale, &paint.styles(), true)
            .unwrap();
        let config = shell.atlas.config();
        engine::UiFrame {
            views,
            paint,
            atlas: Some(engine::AtlasUpload {
                pixels: shell.atlas.pixel_data().to_vec(),
                width: config.width,
                height: config.height,
            }),
            logical_size: size,
            window: 1,
            node_preview: None,
            composition_image: None,
            inspection: Default::default(),
            composition: Some([
                view.min[0] * shell.scale,
                view.min[1] * shell.scale,
                view.width() * shell.scale,
                view.height() * shell.scale,
            ]),
        }
    };
    if !floating
        && shell.panels.viewer_views.settings.layout() == Layout::Single
        && shell.panels.viewer_views.settings.view(0).unwrap().mode == Mode::Composition
    {
        assert!(
            frame.views.is_none(),
            "ordinary composition retains its original presentation path"
        );
        assert!(frame.composition_image.is_some());
    } else {
        assert_eq!(
            frame.views.as_ref().unwrap().cells.len(),
            shell.panels.viewer_views.settings.layout().count()
        );
    }
    frame
}
fn capture(
    events: &mut EventLoop<()>,
    shell: &mut Shell,
    size: [f32; 2],
    floating: bool,
    path: PathBuf,
) {
    let ui = frame(shell, size, floating);
    let pending = shell.engine.present_async(ui).unwrap();
    assert!(
        native_test_support::receive(events, &pending, Duration::from_secs(30))
            .unwrap()
            .unwrap(),
        "test scene must present a complete group"
    );
    let ui = frame(shell, size, floating);
    native_test_support::capture_ui(events, &shell.engine, ui, path.clone()).unwrap();
    assert_eq!(
        image::open(path).unwrap().width(),
        (size[0] * shell.scale) as u32
    );
}

#[test]
#[ignore = "opens real native main/floating surfaces and writes visual acceptance images"]
fn native_view_layout_navigation_projection_and_reopen() {
    let root = std::env::var_os("MOTION_MULTIVIEW_UI_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("motion-multiview-ui-{}", std::process::id()))
        });
    std::fs::create_dir_all(&root).unwrap();
    let _logger = crate::logging::init(root.join("logs")).unwrap();
    let mut project = motion_core::Project::new(480, 270, 30, 120).unwrap();
    for (id, size, pos, color, rotation) in [
        (
            7,
            [210., 120.],
            [210., 130., -30.],
            [0.2, 0.65, 1., 1.],
            [25., 40., 0.],
        ),
        (
            8,
            [100., 100.],
            [300., 150., 45.],
            [1., 0.45, 0.15, 1.],
            [55., -35., 15.],
        ),
    ] {
        let mut layer = motion_core::Layer::solid(id, "View study", size, pos, color);
        layer.three_d = true;
        layer.transform.rotation.value = rotation;
        project.layers.push(layer);
    }
    motion_core::storage::save(&root.join("project"), &project).unwrap();
    let mut shell = Shell::new(root.join("project"), None, Some(Locale::Zh)).unwrap();
    shell.home = None;
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(1600, 1000)),
            )
            .unwrap(),
    );
    shell.state = native_test_support::receive(
        &mut events,
        &shell
            .engine
            .attach_async(window.clone(), 1600, 1000)
            .unwrap(),
        Duration::from_secs(30),
    )
    .unwrap()
    .unwrap();
    shell.window = Some(window.clone());
    shell.scale = 1.;
    shell.panels.select_only(Some(7));
    let baseline = shell.engine.state().unwrap();
    for (floating, size, scale, locale, name) in [
        (false, [1600., 1000.], 1., Locale::Zh, "main-zh"),
        (true, [900., 600.], 1.5, Locale::En, "float-en-150"),
    ] {
        shell.workspace.locale = locale;
        shell.panels.locale = locale;
        shell.scale = scale;
        shell.atlas.set_pixel_ratio(scale);
        if floating {
            #[allow(deprecated)]
            let float = Arc::new(
                events
                    .create_window(
                        Window::default_attributes()
                            .with_visible(false)
                            .with_inner_size(PhysicalSize::new(
                                (size[0] * scale) as u32,
                                (size[1] * scale) as u32,
                            )),
                    )
                    .unwrap(),
            );
            shell
                .engine
                .float_window(
                    1,
                    float.clone(),
                    (size[0] * scale) as u32,
                    (size[1] * scale) as u32,
                )
                .unwrap();
            shell.floating.insert(
                float.id(),
                FloatWindow {
                    window: float,
                    key: 1,
                    panel: Panel::Composition,
                    input: Default::default(),
                    paint_input: Default::default(),
                    pointer_position: Default::default(),
                    scale,
                },
            );
        }
        for (layout, suffix) in [
            (Layout::Single, "single"),
            (Layout::Two, "two"),
            (Layout::Four, "four"),
        ] {
            let mut settings = shell.panels.viewer_views.settings.clone();
            settings.set_layout(layout);
            settings.activate(0);
            shell.save_view_settings(settings).unwrap();
            projections(&mut shell);
            capture(
                &mut events,
                &mut shell,
                size,
                floating,
                root.join(format!("{name}-{suffix}.png")),
            );
        }
        let styles = PaintList::default().styles();
        let viewport = if floating {
            panels::floating_view_styled(Panel::Composition, size, &styles).unwrap()
        } else {
            panels::composition_view_styled(shell.workspace.display_root(), size, &styles)
        };
        let tiles = Layout::Four
            .tiles(viewport, styles.metric("viewer.multiview.gap", 2.))
            .unwrap();
        let before = shell.panels.viewer_views.settings.clone();
        let point = [
            (tiles[1].min[0] + tiles[1].max[0]) * 0.5,
            (tiles[1].min[1] + tiles[1].max[1]) * 0.5,
        ];
        let (_, actions) = draw(
            &mut shell,
            size,
            floating,
            Input {
                mouse: point,
                wheel: [0., 1.],
                ..Default::default()
            },
        );
        execute(&mut shell, actions);
        assert_eq!(shell.panels.viewer_views.settings.active(), 1);
        assert_ne!(
            shell
                .panels
                .viewer_views
                .settings
                .view(1)
                .unwrap()
                .navigation,
            before.view(1).unwrap().navigation
        );
        for i in [0, 2, 3] {
            assert_eq!(shell.panels.viewer_views.settings.view(i), before.view(i));
        }
        // A local camera transaction changes only cell 3 and supports rollback,
        // undo/redo. A subsequent frame invalidates old transaction messages.
        shell.panels.viewer_views.settings.activate(3);
        projections(&mut shell);
        let (_, actions) = draw(&mut shell, size, floating, Input::default());
        execute(&mut shell, actions);
        let at = [
            (tiles[3].min[0] + tiles[3].max[0]) * 0.5,
            (tiles[3].min[1] + tiles[3].max[1]) * 0.5,
        ];
        let ui_before = shell.panels.viewer_views.settings.view(3).unwrap().clone();
        for input in [
            Input {
                mouse: at,
                pressed: Some((MouseButton::Right, at)),
                held: Some(MouseButton::Right),
                ..Default::default()
            },
            Input {
                mouse: [at[0] + 35., at[1] + 12.],
                held: Some(MouseButton::Right),
                ..Default::default()
            },
            Input {
                mouse: [at[0] + 35., at[1] + 12.],
                released: Some((MouseButton::Right, [at[0] + 35., at[1] + 12.])),
                ..Default::default()
            },
        ] {
            let (_, actions) = draw(&mut shell, size, floating, input);
            assert!(
                actions
                    .iter()
                    .any(|action| matches!(action, panels::Action::ViewerCamera { cell: 3, .. })),
                "camera input lost its view: {actions:?}"
            );
            execute(&mut shell, actions);
        }
        assert_ne!(
            shell.panels.viewer_views.settings.view(3).unwrap(),
            &ui_before
        );
        let original = shell.panels.viewer_views.settings.view(3).unwrap().clone();
        use motion_host::ops::preview::{CameraGesture as Op, CameraMode};
        let key = shell.projection_key();
        shell
            .camera_view(key.clone(), 3, Op::Begin { id: 42 })
            .unwrap();
        shell
            .camera_view(
                key.clone(),
                3,
                Op::Update {
                    id: 42,
                    mode: CameraMode::Orbit,
                    delta: [80., -35.],
                    viewport: [480, 270],
                },
            )
            .unwrap();
        let changed = shell.panels.viewer_views.settings.view(3).unwrap().clone();
        assert_ne!(changed, original);
        shell
            .camera_view(
                key.clone(),
                3,
                Op::End {
                    id: 42,
                    commit: true,
                },
            )
            .unwrap();
        shell
            .camera_view(shell.projection_key(), 3, Op::Undo)
            .unwrap();
        assert_eq!(
            shell.panels.viewer_views.settings.view(3).unwrap(),
            &original
        );
        shell
            .camera_view(shell.projection_key(), 3, Op::Redo)
            .unwrap();
        assert_eq!(
            shell.panels.viewer_views.settings.view(3).unwrap(),
            &changed
        );
        assert_eq!(shell.panels.viewer_views.settings.view(0), before.view(0));
        projections(&mut shell);
        capture(
            &mut events,
            &mut shell,
            size,
            floating,
            root.join(format!("{name}-independent.png")),
        );
        shell.workspace.viewer_views = shell.panels.viewer_views.settings.clone();
        shell.workspace.save(&shell.layout_path).unwrap();
        std::fs::copy(
            &shell.layout_path,
            root.join(format!("{name}-workspace.json")),
        )
        .unwrap();
        let saved = workspace::Workspace::load(&shell.layout_path);
        assert_eq!(saved.viewer_views, shell.panels.viewer_views.settings);
        let host = shell.engine.state().unwrap();
        for key in [
            "project",
            "frame",
            "revision",
            "canUndo",
            "canRedo",
            "observing",
            "observationCamera",
        ] {
            assert_eq!(host[key], baseline[key], "host changed: {key}");
        }
        // Keep a real open layout dropdown capture for visual ownership review.
        let area = if floating {
            motion_ui::Rect::new(
                0.,
                styles.metric("viewer.floating.top", 30.),
                size[0],
                size[1]
                    - styles.metric("viewer.floating.top", 30.)
                    - styles.metric("viewer.floating.bottom", 22.),
            )
        } else {
            panels::frames(shell.workspace.display_root(), size)
                .into_iter()
                .find(|f| f.panel == Panel::Composition)
                .unwrap()
                .content
        };
        let p = [
            area.max[0]
                - styles.metric("padding.panel", 8.)
                - styles.metric("viewer.count.width", 74.) * 0.5,
            area.max[1] - styles.metric("viewer.controls.height", 28.) * 0.5,
        ];
        let (_, actions) = draw(
            &mut shell,
            size,
            floating,
            Input {
                mouse: p,
                pressed: Some((MouseButton::Left, p)),
                ..Default::default()
            },
        );
        execute(&mut shell, actions);
        assert!(
            shell.panels.viewer_views.popup(),
            "layout popup did not open at {p:?}"
        );
        capture(
            &mut events,
            &mut shell,
            size,
            floating,
            root.join(format!("{name}-layout-menu.png")),
        );
        let (_, actions) = draw(
            &mut shell,
            size,
            floating,
            Input {
                keys: vec![(motion_ui::input::Key::Escape, Default::default())],
                ..Default::default()
            },
        );
        execute(&mut shell, actions);
        assert!(!shell.panels.viewer_views.popup());
    }
    let before = shell.engine.state().unwrap();
    let point = |state: &Value, id: u64| {
        state["project"]["layers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|layer| layer["id"] == id)
            .unwrap()["transform"]["position"]["value"]
            .clone()
    };
    let position = point(&before, 7);
    shell
        .drag_view(shell.projection_key(), 3, 7, [20., 0.], [480, 270])
        .unwrap();
    let (_, receive) = shell
        .editing
        .take()
        .expect("explicit view drag must be dispatched");
    let after = native_test_support::receive(&mut events, &receive, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    assert_ne!(point(&after, 7), position);
    assert_ne!(
        point(&after, 7)[2],
        position[2],
        "observer-plane drag must use its own depth direction"
    );
    assert_eq!(point(&after, 8), point(&before, 8));
    shell.state = after;
    shell.state = shell.engine.history(0).unwrap();
    assert_eq!(shell.state["project"], before["project"]);
    assert_eq!(
        shell.panels.viewer_views.settings.view(3).unwrap().mode,
        Mode::Free
    );
    // The old scratch target cannot supply a non-composition cell. Pick a
    // visibly colored interior pixel whose same normalized position in cell 0
    // differs; this fails if the worker silently samples the first view.
    let catalogue = shell.engine.plugin(r#"{"op":"catalogue"}"#.into()).unwrap();
    let package = catalogue["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| {
            p["manifest"]["effects"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["id"] == "gradient_ramp")
        })
        .unwrap();
    shell
        .engine
        .plugin(
            serde_json::json!({"op":"add","object":7,
        "plugin":package["manifest"]["id"],"version":package["manifest"]["version"],
        "hash":package["hash"],"effect":"gradient_ramp"})
            .to_string(),
        )
        .unwrap();
    shell.state = shell.engine.state().unwrap();
    let effect = shell.state["project"]["layers"][0]["effects"][0]["id"]
        .as_u64()
        .unwrap();
    shell.state = shell
        .engine
        .edit(
            serde_json::to_string(&motion_core::Command::Effect {
                object: 7,
                action: motion_core::EffectAction::Enable {
                    effect,
                    enabled: false,
                },
            })
            .unwrap(),
        )
        .unwrap();
    // Select cell zero first; the armed one-shot pick must still identify cell 3.
    shell.panels.viewer_views.settings.activate(0);
    shell.panels.viewer_views.loaded = None;
    projections(&mut shell);
    let image_path = root.join("float-en-150-color-source.png");
    capture(
        &mut events,
        &mut shell,
        [900., 600.],
        true,
        image_path.clone(),
    );
    let pixels = image::open(image_path).unwrap().to_rgba8();
    let plan = frame(&mut shell, [900., 600.], true).views.unwrap();
    let reference = plan.cells[0].placement.image.unwrap();
    let image = plan.cells[3].placement.image.unwrap();
    let clip = plan.cells[3].placement.viewport;
    let mut candidate = None;
    'scan: for y in 30..95 {
        for x in 5..95 {
            let norm = [x as f64 / 100., y as f64 / 100.];
            let p = [
                image[0] + image[2] * norm[0] as f32,
                image[1] + image[3] * norm[1] as f32,
            ];
            if p[0] < clip[0] + 3.
                || p[0] >= clip[0] + clip[2] - 3.
                || p[1] < clip[1] + 30.
                || p[1] >= clip[1] + clip[3] - 3.
            {
                continue;
            }
            let c = pixels.get_pixel(p[0] as u32, p[1] as u32).0;
            if c[2] < 210 || c[1] < 100 || c[0] > 100 {
                continue;
            }
            if !(-2..=2).all(|dy| {
                (-2..=2).all(|dx| {
                    pixels
                        .get_pixel((p[0] as i32 + dx) as u32, (p[1] as i32 + dy) as u32)
                        .0
                        == c
                })
            }) {
                continue;
            }
            let other = [
                reference[0] + reference[2] * norm[0] as f32,
                reference[1] + reference[3] * norm[1] as f32,
            ];
            if other[0] < 0.
                || other[1] < 0.
                || other[0] >= pixels.width() as f32
                || other[1] >= pixels.height() as f32
            {
                continue;
            }
            let b = pixels.get_pixel(other[0] as u32, other[1] as u32).0;
            if (0..3)
                .map(|i| (c[i] as i32 - b[i] as i32).abs())
                .sum::<i32>()
                < 80
            {
                continue;
            }
            candidate = Some((p, c));
            break 'scan;
        }
    }
    let (physical, expected) = candidate
        .expect("explicit free view must contain a stable blue pixel distinct from composition");
    let point = physical.map(|v| v / shell.scale);
    let before = shell.state.clone();
    shell.panels.effect_pick.ticket = Some(panels::effect_pick::Ticket::new(
        &before,
        7,
        effect,
        "start_color",
        panels::effect_pick::Kind::Color,
    ));
    let (_, actions) = draw(
        &mut shell,
        [900., 600.],
        true,
        Input {
            mouse: point,
            pressed: Some((MouseButton::Left, point)),
            ..Default::default()
        },
    );
    let [panels::Action::EffectPick { ticket, point }] = actions.as_slice() else {
        panic!("pick must own its cell: {actions:?}")
    };
    assert_eq!(
        ticket
            .view
            .as_ref()
            .map(|(window, index, _)| (*window, *index)),
        Some((1, 3))
    );
    let mut closed = ticket.clone();
    closed.view.as_mut().unwrap().0 = 999;
    let pending = shell
        .engine
        .request(|reply| engine::Command::EffectPick {
            ticket: closed,
            point: *point,
            reply,
        })
        .unwrap();
    let cancelled = native_test_support::receive(&mut events, &pending, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    assert_eq!(cancelled["editorAction"]["reason"], "context_changed");
    assert_eq!(cancelled["project"], before["project"]);
    let pending = shell
        .engine
        .request(|reply| engine::Command::EffectPick {
            ticket: ticket.clone(),
            point: *point,
            reply,
        })
        .unwrap();
    let changed = native_test_support::receive(&mut events, &pending, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let color =
        &changed["project"]["layers"][0]["effects"][0]["params"]["start_color"]["track"]["value"];
    assert!(changed["editorAction"]["cancelled"] != true, "{changed}");
    for axis in 0..3 {
        assert!(
            (color[axis].as_f64().unwrap() - expected[axis] as f64 / 255.).abs() < 0.02,
            "wrong cell color: {color}, expected {expected:?}"
        );
    }
    shell.state = shell.engine.history(0).unwrap();
    assert_eq!(shell.state["project"], before["project"]);
}

#[test]
fn view_camera_and_preferences_failures_keep_default_file_stderr_and_stdout_contracts() {
    const NAME:&str="shell::viewer_views::native_tests::view_camera_and_preferences_failures_keep_default_file_stderr_and_stdout_contracts";
    const CHILD: &str = "MOTION_VIEWER_UI_FAILURE_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env(CHILD, root.path())
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let file = std::fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
        assert!(file.contains("logging initialized level=INFO"));
        for marker in [
            "viewer camera failed operation=viewer_camera phase=gesture",
            "viewer preferences failed operation=save_view_settings phase=write",
        ] {
            assert_eq!(file.matches(marker).count(), 1, "{marker}");
            assert_eq!(
                String::from_utf8_lossy(&output.stderr)
                    .matches(marker)
                    .count(),
                1,
                "{marker}"
            );
            assert!(!String::from_utf8_lossy(&output.stdout).contains(marker));
        }
        assert!(file.contains("reason=invalid view camera gesture coordinates"));
        assert!(file
            .contains("viewer camera unavailable operation=viewer_reconcile phase=source_change"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("viewer camera unavailable"));
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.join("logs")).unwrap();
    let mut shell = Shell::new(root.join("project"), None, Some(Locale::En)).unwrap();
    shell.panels.viewer_views.settings.set_layout(Layout::Four);
    shell.panels.viewer_views.settings.activate(3);
    let before = shell.panels.viewer_views.settings.clone();
    use motion_host::ops::preview::{CameraGesture as Op, CameraMode};
    shell
        .camera_view(shell.projection_key(), 3, Op::Undo)
        .unwrap();
    shell
        .camera_view(shell.projection_key(), 3, Op::Begin { id: 7 })
        .unwrap();
    assert_eq!(
        shell
            .camera_view(
                shell.projection_key(),
                3,
                Op::Update {
                    id: 7,
                    mode: CameraMode::Pan,
                    delta: [10., 10.],
                    viewport: [0, 0]
                }
            )
            .unwrap_err(),
        "invalid view camera gesture coordinates"
    );
    shell
        .camera_view(
            shell.projection_key(),
            3,
            Op::End {
                id: 7,
                commit: false,
            },
        )
        .unwrap();
    assert_eq!(shell.panels.viewer_views.settings, before);
    let blocker = root.join("not-a-directory");
    std::fs::write(&blocker, b"blocker").unwrap();
    shell.layout_path = blocker.join("workspace.json");
    assert!(shell.persist_view_settings().is_err());
    assert_eq!(shell.panels.viewer_views.settings, before);
    shell.panels.viewer_views.settings.view_mut(2).unwrap().mode = Mode::Camera(999);
    shell.reconcile_view_cameras();
    assert_eq!(
        shell.panels.viewer_views.settings.view(2).unwrap().mode,
        Mode::Composition
    );
    for i in [0, 1, 3] {
        assert_eq!(shell.panels.viewer_views.settings.view(i), before.view(i));
    }
}
