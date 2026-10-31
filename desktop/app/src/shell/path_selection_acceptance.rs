//! Real main/floating panel routes, attached native surfaces and GPU captures.
use crate::*;
use motion_core::vector::{PathNode, VectorPath};
use motion_ui::{
    dock::Panel,
    input::{Key, Modifiers},
};

fn size() -> [f32; 2] {
    match std::env::var("MOTION_POINT_SELECTION_NATIVE_SIZE").as_deref() {
        Ok("minimum") => [900., 600.],
        Ok("default") => [1600., 1000.],
        Err(std::env::VarError::NotPresent) => [1280., 800.],
        other => panic!("invalid native acceptance size: {other:?}"),
    }
}
fn pixel_ratio() -> f32 {
    match std::env::var("MOTION_POINT_SELECTION_NATIVE_SCALE").as_deref() {
        Ok("1.5") => 1.5,
        Ok("2") => 2.,
        Err(std::env::VarError::NotPresent) | Ok("1") => 1.,
        other => panic!("invalid native acceptance scale: {other:?}"),
    }
}
fn physical_size() -> (u32, u32) {
    ((size()[0] * pixel_ratio()) as u32,
     (size()[1] * pixel_ratio()) as u32)
}
fn render(shell: &mut Shell, floating: bool, input: &Input) -> (PaintList, Vec<panels::Action>) {
    let mut paint = PaintList::default();
    paint.stylesheet = shell.panels.styles.clone();
    let actions = if floating {
        panels::floating_chrome(
            &mut paint,
            &mut shell.atlas,
            Panel::Composition,
            size(),
            &mut shell.panels,
            &shell.state,
            input,
        )
    } else {
        panels::chrome(
            &mut paint,
            &mut shell.atlas,
            size(),
            shell.workspace.display_root(),
            &mut shell.panels,
            &shell.state,
            input,
        )
    };
    (paint, actions)
}
fn apply(shell: &mut Shell, actions: Vec<panels::Action>) -> Vec<i32> {
    let mut history = vec![];
    for action in actions {
        match action {
            panels::Action::Edit(command) => {
                shell.state = shell
                    .engine
                    .edit(serde_json::to_string(&command).unwrap())
                    .unwrap();
            }
            panels::Action::EditBatch(commands) => {
                shell.state = shell
                    .engine
                    .edit(serde_json::to_string(&commands).unwrap())
                    .unwrap();
            }
            panels::Action::History(h) => {
                shell.state = shell.engine.history(h).unwrap();
                history.push(h);
            }
            other => panic!("path input escaped into another domain: {other:?}"),
        }
    }
    history
}
fn input(shell: &mut Shell, floating: bool, event: Input) -> Vec<i32> {
    let (_, actions) = render(shell, floating, &event);
    apply(shell, actions)
}
fn view(shell: &Shell, floating: bool) -> motion_ui::Rect {
    let styles = shell
        .panels
        .styles
        .clone()
        .unwrap_or_else(motion_ui::style::default_stylesheet_arc);
    if floating {
        panels::floating_view_styled(Panel::Composition, size(), &styles).unwrap()
    } else {
        panels::composition_view_styled(shell.workspace.display_root(), size(), &styles)
    }
}
fn capture(
    event_loop: &mut winit::event_loop::EventLoop<()>,
    shell: &mut Shell,
    floating: bool,
    paint: PaintList,
    path: PathBuf,
) {
    let view = view(shell, floating);
    let mapping = shell
        .panels
        .composition
        .navigation
        .mapping(view, [640., 480.])
        .unwrap();
    let mut ui = shell.ui_frame(paint, size(), true);
    ui.window = u64::from(floating);
    ui.node_preview = None;
    ui.composition = Some([view.min[0], view.min[1], view.width(), view.height()].map(|v| v * pixel_ratio()));
    ui.composition_image = Some([
        mapping.image.min[0],
        mapping.image.min[1],
        mapping.image.width(),
        mapping.image.height(),
    ].map(|v| v * pixel_ratio()));
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let reply = shell
            .engine
            .present_async(crate::engine::UiFrame { views: None,
                paint: PaintList::default(),
                atlas: None,
                logical_size: size(),
                composition: ui.composition,
                inspection: ui.inspection,
                composition_image: ui.composition_image,
                window: ui.window,
                node_preview: None,
            })
            .unwrap();
        let ready = crate::native_test_support::receive(
            event_loop, &reply, Duration::from_secs(10),
        ).unwrap().unwrap();
        if ready {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "path capture presentation unavailable"
        );
    }
    crate::native_test_support::capture_ui(event_loop, &shell.engine, ui, path.clone()).unwrap();
    let pixels = image::open(path).unwrap().into_rgba8();
    assert_eq!(pixels.dimensions(), physical_size());
    let first = pixels.get_pixel(0, 0);
    assert!(pixels.pixels().any(|p| p != first));
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn point_selection_main_and_native_floating_routes_capture_and_restore() {
    const CHILD: &str = "MOTION_POINT_SELECTION_NATIVE_CHILD";
    const TEST:&str="shell::path_selection_acceptance::point_selection_main_and_native_floating_routes_capture_and_restore";
    if let Some(root) = std::env::var_os(CHILD) {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let root = PathBuf::from(root);
        let artifacts =
            PathBuf::from(std::env::var_os("MOTION_POINT_SELECTION_ARTIFACTS").unwrap());
        std::fs::create_dir_all(&artifacts).unwrap();
        let _logger = crate::logging::init(artifacts.clone()).unwrap();
        let mut event_loop = winit::event_loop::EventLoop::builder()
            .with_any_thread(true)
            .build()
            .unwrap();
        for (tag, locale) in [("zh", Locale::Zh), ("en", Locale::En)] {
            for floating in [false, true] {
                let label = format!("{tag}-{}", if floating { "floating" } else { "main" });
                let project_root = root.join(&label);
                let mut project = motion_core::Project::new(640, 480, 30, 120).unwrap();
                let path = VectorPath {
                    tangents: Default::default(), id: 3,
                    closed: true,
                    nodes: [
                        (11, [-120., -80., -25., 0., 25., 0.]),
                        (42, [120., -80., -25., 0., 25., 0.]),
                        (99, [120., 80., 0., 0., 0., 0.]),
                        (123, [-120., 80., 0., 0., 0., 0.]),
                    ]
                    .into_iter()
                    .map(|(id, g)| PathNode {
                        id,
                        geometry: motion_core::Track::constant(g),
                    })
                    .collect(),
                };
                let mut layer =
                    Layer::solid(7, "Editable path", [240., 160.], [320., 240., 0.], [1.; 4]);
                layer.content = motion_core::Content::Vector {
                    vector: panels::path_controls::new_vector(path),
                };
                project.layers.push(layer);
                motion_core::storage::save(&project_root, &project).unwrap();
                let mut shell = Shell::new(project_root.clone(), None, Some(locale)).unwrap();
                shell.scale = pixel_ratio();
                shell.atlas.set_pixel_ratio(shell.scale);
                shell.workspace = workspace::Workspace::default();
                shell.workspace.locale = locale;
                shell.panels.locale = locale;
                shell.panels.focused = Some(Panel::Composition);
                shell.panels.select_only(Some(7));
                shell.panels.path_controls.tool = panels::path_controls::Tool::Pen;
                #[allow(deprecated)]
                let main = Arc::new(
                    event_loop
                        .create_window(
                            winit::window::Window::default_attributes()
                                .with_visible(false)
                                .with_inner_size(winit::dpi::PhysicalSize::new(physical_size().0, physical_size().1)),
                        )
                        .unwrap(),
                );
                let reply = shell
                    .engine
                    .attach_async(main.clone(), physical_size().0, physical_size().1)
                    .unwrap();
                shell.state = crate::native_test_support::receive(
                    &mut event_loop, &reply, Duration::from_secs(30),
                ).unwrap().unwrap();
                shell.window = Some(main);
                shell.attached = true;
                let before = shell.state["project"].clone();
                #[allow(deprecated)]
                let floated = if floating {
                    let window = Arc::new(
                        event_loop
                            .create_window(
                                winit::window::Window::default_attributes()
                                    .with_visible(false)
                                    .with_inner_size(winit::dpi::PhysicalSize::new(physical_size().0, physical_size().1)),
                            )
                            .unwrap(),
                    );
                    shell.state = shell
                        .engine
                        .float_window(1, window.clone(), physical_size().0, physical_size().1)
                        .unwrap();
                    Some(window)
                } else {
                    None
                };
                input(&mut shell, floating, Input::default());
                let mapping = shell
                    .panels
                    .composition
                    .navigation
                    .mapping(view(&shell, floating), [640., 480.])
                    .unwrap();
                let projection =
                    panels::path_controls::layer_projection(&shell.state, 7, mapping.image)
                        .unwrap();
                let a = projection.screen([-150., -110.]).unwrap();
                let b = projection.screen([150., -50.]).unwrap();
                input(
                    &mut shell,
                    floating,
                    Input {
                        mouse: a,
                        pressed: Some((MouseButton::Left, a)),
                        held: Some(MouseButton::Left),
                        modifiers: Modifiers {
                            control: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                );
                let (paint, actions) = render(
                    &mut shell,
                    floating,
                    &Input {
                        mouse: b,
                        held: Some(MouseButton::Left),
                        ..Default::default()
                    },
                );
                assert!(actions.is_empty());
                capture(
                    &mut event_loop,
                    &mut shell,
                    floating,
                    paint,
                    artifacts.join(format!("{label}-marquee.png")),
                );
                assert!(input(
                    &mut shell,
                    floating,
                    Input {
                        mouse: b,
                        released: Some((MouseButton::Left, b)),
                        ..Default::default()
                    }
                )
                .is_empty());
                let start = projection.screen([-120., -80.]).unwrap();
                let end = projection.screen([-85., -55.]).unwrap();
                input(
                    &mut shell,
                    floating,
                    Input {
                        mouse: start,
                        pressed: Some((MouseButton::Left, start)),
                        held: Some(MouseButton::Left),
                        ..Default::default()
                    },
                );
                assert_eq!(
                    input(
                        &mut shell,
                        floating,
                        Input {
                            mouse: end,
                            held: Some(MouseButton::Left),
                            ..Default::default()
                        }
                    ),
                    vec![2]
                );
                assert_eq!(
                    input(
                        &mut shell,
                        floating,
                        Input {
                            focus_lost: true,
                            ..Default::default()
                        }
                    ),
                    vec![4]
                );
                assert_eq!(shell.state["project"], before);
                input(
                    &mut shell,
                    floating,
                    Input {
                        mouse: start,
                        pressed: Some((MouseButton::Left, start)),
                        held: Some(MouseButton::Left),
                        ..Default::default()
                    },
                );
                assert_eq!(
                    input(
                        &mut shell,
                        floating,
                        Input {
                            mouse: [1200., 780.],
                            released: Some((MouseButton::Left, end)),
                            ..Default::default()
                        }
                    ),
                    vec![2, 3]
                );
                let changed: motion_core::Project =
                    serde_json::from_value(shell.state["project"].clone()).unwrap();
                let motion_core::Content::Vector { vector } = &changed.layers[0].content else {
                    panic!()
                };
                let motion_core::vector::VectorSource::Paths { paths } = &vector.source else {
                    panic!()
                };
                for (i, expected) in [[-85., -55.], [155., -55.], [120., 80.], [-120., 80.]]
                    .iter()
                    .enumerate()
                {
                    for axis in 0..2 {
                        assert!(
                            (paths[0].nodes[i].geometry.value[axis] - expected[axis]).abs() < 0.01
                        );
                    }
                }
                let (paint, actions) = render(&mut shell, floating, &Input::default());
                assert!(actions.is_empty());
                capture(
                    &mut event_loop,
                    &mut shell,
                    floating,
                    paint,
                    artifacts.join(format!("{label}-moved.png")),
                );
                let saved = root.join(format!("{label}-saved"));
                motion_core::storage::save(&saved, &changed).unwrap();
                assert_eq!(motion_core::storage::load(&saved).unwrap(), changed);
                input(
                    &mut shell,
                    floating,
                    Input {
                        keys: vec![(
                            Key::Z,
                            Modifiers {
                                control: true,
                                ..Default::default()
                            },
                        )],
                        ..Default::default()
                    },
                );
                assert_eq!(shell.state["project"], before, "path undo must own Ctrl-Z");
                input(
                    &mut shell,
                    floating,
                    Input {
                        keys: vec![(Key::Delete, Modifiers::default())],
                        ..Default::default()
                    },
                );
                assert_eq!(
                    shell.state["project"]["layers"].as_array().unwrap().len(),
                    1,
                    "Delete must not delete the layer"
                );
                let value = &shell.state["project"]["layers"][0]["content"]["vector"]["source"]
                    ["paths"][0]["nodes"];
                assert_eq!(value.as_array().unwrap().len(), 2);
                shell.state = shell.engine.history(0).unwrap();
                assert_eq!(shell.state["project"], before);
                exercise_pen_modes(&mut event_loop,&mut shell,floating,&label,&artifacts);
                if floating {
                    shell.engine.close_window(1).unwrap();
                }
                drop(floated);
            }
        }
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let artifacts = std::env::var_os("MOTION_POINT_SELECTION_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.path().join("artifacts"));
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST])
        .env(CHILD, root.path())
        .env("MOTION_POINT_SELECTION_ARTIFACTS", &artifacts)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    std::fs::create_dir_all(&artifacts).unwrap();
    std::fs::write(artifacts.join("stdout.log"), &output.stdout).unwrap();
    std::fs::write(artifacts.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("motion-engine"));
}

fn exercise_pen_modes(
    event_loop: &mut winit::event_loop::EventLoop<()>,
    shell: &mut Shell,
    floating: bool,
    label: &str,
    artifacts: &std::path::Path,
) {
    let styles = shell
        .panels
        .styles
        .clone()
        .unwrap_or_else(motion_ui::style::default_stylesheet_arc);
    let v = view(shell, floating);
    let pad = styles.metric("padding.panel", 8.);
    let menu = [
        v.min[0] + pad + styles.metric("path.tools.width", 176.) * 0.5,
        v.min[1] + pad + styles.metric("field.height", 18.) * 0.5,
    ];
    let before = shell.state["project"].clone();
    let mut current = 0;
    for mode in 1..=4 {
        input(
            shell,
            floating,
            Input {
                mouse: menu,
                pressed: Some((MouseButton::Left, menu)),
                held: Some(MouseButton::Left),
                ..Default::default()
            },
        );
        if mode == 1 {
            let (paint, actions) = render(shell, floating, &Input::default());
            assert!(actions.is_empty());
            capture(
                event_loop,
                shell,
                floating,
                paint,
                artifacts.join(format!("{label}-pen-menu.png")),
            );
        }
        for _ in current..mode {
            input(
                shell,
                floating,
                Input {
                    keys: vec![(Key::Down, Modifiers::default())],
                    ..Default::default()
                },
            );
        }
        input(
            shell,
            floating,
            Input {
                keys: vec![(Key::Enter, Modifiers::default())],
                ..Default::default()
            },
        );
        current = mode;
        let mapping = shell
            .panels
            .composition
            .navigation
            .mapping(v, [640., 480.])
            .unwrap();
        let p = panels::path_controls::layer_projection(&shell.state, 7, mapping.image).unwrap();
        let anchor = match mode {
            1 => [0., -80.],
            3 => [120., 80.],
            _ => [-120., -80.],
        };
        let mouse = p.screen(anchor).unwrap();
        input(
            shell,
            floating,
            Input {
                mouse,
                pressed: Some((MouseButton::Left, mouse)),
                held: Some(MouseButton::Left),
                ..Default::default()
            },
        );
        assert_eq!(
            shell.state["project"], before,
            "press must not commit topology"
        );
        input(
            shell,
            floating,
            Input {
                focus_lost: true,
                ..Default::default()
            },
        );
        input(
            shell,
            floating,
            Input {
                mouse,
                released: Some((MouseButton::Left, mouse)),
                ..Default::default()
            },
        );
        assert_eq!(
            shell.state["project"], before,
            "blur must cancel topology click"
        );
        input(
            shell,
            floating,
            Input {
                mouse,
                pressed: Some((MouseButton::Left, mouse)),
                held: Some(MouseButton::Left),
                ..Default::default()
            },
        );
        input(
            shell,
            floating,
            Input {
                mouse: [1., 1.],
                released: Some((MouseButton::Left, mouse)),
                ..Default::default()
            },
        );
        assert_ne!(
            shell.state["project"], before,
            "mode={mode} must apply through the real panel route"
        );
        let changed: motion_core::Project =
            serde_json::from_value(shell.state["project"].clone()).unwrap();
        let saved = artifacts.join(format!("{label}-pen-{mode}-saved"));
        motion_core::storage::save(&saved, &changed).unwrap();
        assert_eq!(motion_core::storage::load(&saved).unwrap(), changed);
        let (paint, actions) = render(shell, floating, &Input::default());
        assert!(actions.is_empty());
        capture(
            event_loop,
            shell,
            floating,
            paint,
            artifacts.join(format!("{label}-pen-{mode}.png")),
        );
        input(
            shell,
            floating,
            Input {
                keys: vec![(
                    Key::Z,
                    Modifiers {
                        control: true,
                        ..Default::default()
                    },
                )],
                ..Default::default()
            },
        );
        assert_eq!(
            shell.state["project"], before,
            "one undo restores the entire path mode={mode}"
        );
    }
}
