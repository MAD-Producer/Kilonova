use super::*;
use crate::{workspace, Shell};
use std::{path::PathBuf, sync::Arc, time::Duration};

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires native windows and GPU")]
fn native_work_area_drag_save_undo_main_float_bilingual() {
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let root = std::env::var_os("MOTION_WORK_AREA_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("motion-work-area-native"));
    std::fs::create_dir_all(&root).unwrap();
    let event_loop = winit::event_loop::EventLoop::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    for (language, locale) in [("zh", Locale::Zh), ("en", Locale::En)] {
        let project_root = root.join(format!("project-{language}"));
        let mut project: Project = serde_json::from_value(fixture()["project"].clone()).unwrap();
        project.layers.push(motion_core::Layer::solid(
            1,
            "Range study",
            [160., 100.],
            [320., 180., 0.],
            [0.2, 0.5, 0.8, 1.],
        ));
        motion_core::storage::save(&project_root, &project).unwrap();
        let mut shell = Shell::new(project_root.clone(), None, Some(locale)).unwrap();
        shell.home = None;
        shell.workspace = workspace::Workspace::default();
        shell.layout_path = root.join(format!("workspace-{language}.json"));
        shell.panels.focused = Some(Panel::Timeline);
        #[allow(deprecated)]
        let window = Arc::new(
            event_loop
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(1600, 1000)),
                )
                .unwrap(),
        );
        shell.state = shell
            .engine
            .attach_async(window.clone(), 1600, 1000)
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap();
        shell.window = Some(window);
        shell.attached = true;
        for floating in [false, true] {
            let size = if floating {
                [900., 600.]
            } else {
                [1600., 1000.]
            };
            if floating {
                #[allow(deprecated)]
                let window = Arc::new(
                    event_loop
                        .create_window(
                            winit::window::Window::default_attributes()
                                .with_visible(false)
                                .with_inner_size(winit::dpi::PhysicalSize::new(1350, 900)),
                        )
                        .unwrap(),
                );
                shell.engine.float_window(1, window, 1350, 900).unwrap();
            }
            shell.atlas.set_pixel_ratio(if floating { 1.5 } else { 1. });
            let draw = |shell: &mut Shell, input: &Input| {
                let mut paint = PaintList::default();
                if floating {
                    crate::panels::floating_chrome(
                        &mut paint,
                        &mut shell.atlas,
                        Panel::Timeline,
                        size,
                        &mut shell.panels,
                        &shell.state,
                        input,
                    )
                } else {
                    crate::panels::chrome(
                        &mut paint,
                        &mut shell.atlas,
                        size,
                        &shell.workspace.root,
                        &mut shell.panels,
                        &shell.state,
                        input,
                    )
                }
            };
            assert!(draw(&mut shell, &Input::default()).is_empty());
            let band = shell.panels.timeline.work_area_band.unwrap();
            let ppf = band.width() / 120. * shell.panels.timeline.zoom.max(1.);
            let point = [
                band.min[0] + (20. - shell.panels.timeline.offset) * ppf,
                (band.min[1] + band.max[1]) / 2.,
            ];
            let mut down = pointer(point, false);
            down.pressed = Some((MouseButton::Left, point));
            assert!(draw(&mut shell, &down).is_empty());
            let moved = [point[0] + 10. * ppf, point[1]];
            assert!(draw(&mut shell, &pointer(moved, false)).is_empty());
            assert_eq!(shell.state["project"]["work_area"]["in_frame"], 20);
            let actions = draw(&mut shell, &pointer(moved, true));
            assert!(
                matches!(
                    actions.as_slice(),
                    [Action::EditChecked {
                        command: Command::SetWorkArea {
                            in_frame: 30,
                            out_frame: 80
                        },
                        ..
                    }]
                ),
                "{actions:?}"
            );
            for action in actions {
                shell.action(action, None).unwrap();
            }
            let (_, reply) = shell.editing.take().unwrap();
            shell.accept_edit_state(
                reply
                    .recv_timeout(Duration::from_secs(10))
                    .unwrap()
                    .unwrap(),
            );
            capture(
                &mut shell,
                root.join(format!(
                    "{language}-{}-range.png",
                    if floating { "float" } else { "main" }
                )),
                floating,
                size,
            );
            shell.engine.save().unwrap();
            assert_eq!(
                motion_core::storage::load(&project_root).unwrap().work_area,
                WorkArea {
                    in_frame: 30,
                    out_frame: 80
                }
            );
            shell.state = shell.engine.history(0).unwrap();
            assert_eq!(shell.state["project"]["work_area"]["in_frame"], 20);
            shell.state = shell.engine.history(1).unwrap();
            assert_eq!(shell.state["project"]["work_area"]["in_frame"], 30);
            shell.state = shell.engine.history(0).unwrap();
        }
        #[allow(deprecated)]
        let window = Arc::new(
            event_loop
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(540, 780)),
                )
                .unwrap(),
        );
        shell.engine.float_window(2, window, 540, 780).unwrap();
        shell
            .action(
                Action::PreviewPlayback(crate::preview_playback::Options {
                    scope: motion_core::FrameScope::WorkArea,
                    start: crate::preview_playback::Start::RangeStart,
                    skip: crate::preview_playback::Skip::One,
                    looping: true,
                }),
                None,
            )
            .unwrap();
        capture_panel(
            &mut shell,
            root.join(format!("{language}-preview-options.png")),
            Panel::Preview,
        );
        let style = PaintList::default().styles();
        let point = [
            200.,
            style.metric("title.height", 22.)
                + style.metric("padding.panel", 8.)
                + style.metric("auxiliary.control.height", 26.)
                + style.metric("auxiliary.gap", 6.) * 2.
                + style.metric("auxiliary.control.height", 26.) / 2.,
        ];
        let actions = crate::panels::floating_chrome(
            &mut PaintList::default(),
            &mut shell.atlas,
            Panel::Preview,
            [360., 520.],
            &mut shell.panels,
            &shell.state,
            &Input {
                mouse: point,
                pressed: Some((MouseButton::Left, point)),
                ..Default::default()
            },
        );
        assert!(actions.is_empty());
        assert_eq!(shell.panels.auxiliary.owner(), Some(Panel::Preview));
        capture_panel(
            &mut shell,
            root.join(format!("{language}-preview-range-menu.png")),
            Panel::Preview,
        );
        assert!(crate::panels::cancel_interaction(&mut shell.panels).is_empty());
        shell
            .action(
                Action::MovieExportScope(motion_core::FrameScope::WorkArea),
                None,
            )
            .unwrap();
        capture_panel(
            &mut shell,
            root.join(format!("{language}-export-scope.png")),
            Panel::RenderQueue,
        );
    }
}

fn capture_panel(shell: &mut Shell, path: PathBuf, panel: Panel) {
    let size = [360., 520.];
    let mut paint = PaintList::default();
    assert!(crate::panels::floating_chrome(
        &mut paint,
        &mut shell.atlas,
        panel,
        size,
        &mut shell.panels,
        &shell.state,
        &Input::default()
    )
    .is_empty());
    let mut scene = shell.ui_frame(paint, size, true);
    scene.window = 2;
    scene.composition = None;
    scene.composition_image = None;
    scene.node_preview = None;
    let report = shell.engine.capture_ui(scene, path.clone()).unwrap();
    assert_eq!(report["width"], 540);
    assert!(std::fs::metadata(path).unwrap().len() > 1000);
}

fn capture(shell: &mut Shell, path: PathBuf, floating: bool, size: [f32; 2]) {
    let (paint, actions) = if floating {
        let mut paint = PaintList::default();
        let actions = crate::panels::floating_chrome(
            &mut paint,
            &mut shell.atlas,
            Panel::Timeline,
            size,
            &mut shell.panels,
            &shell.state,
            &Input::default(),
        );
        (paint, actions)
    } else {
        shell.input = Input::default();
        shell.draw_commands(size)
    };
    assert!(actions.is_empty());
    let mut scene = shell.ui_frame(PaintList::default(), size, true);
    scene.window = u64::from(floating);
    if floating {
        scene.composition = None;
        scene.composition_image = None;
        scene.node_preview = None;
    }
    let presented = shell
        .engine
        .present_async(scene)
        .unwrap()
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    assert!(presented);
    let mut ui = shell.ui_frame(paint, size, true);
    ui.window = u64::from(floating);
    if floating {
        ui.composition = None;
        ui.composition_image = None;
        ui.node_preview = None;
    }
    let report = shell.engine.capture_ui(ui, path.clone()).unwrap();
    assert_eq!(
        report["width"].as_u64().unwrap(),
        if floating { 1350 } else { 1600 }
    );
    assert!(std::fs::metadata(path).unwrap().len() > 1000);
}
