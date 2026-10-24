use super::*;
use crate::{workspace, Shell};
use std::{path::PathBuf, sync::Arc, time::Duration};

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

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires native windows and GPU")]
fn native_shy_switches_persist_and_keep_output_with_default_error_evidence() {
    const CHILD: &str = "MOTION_SHY_NATIVE_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let directory = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_SHY_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| directory.path().join("native"));
        std::fs::create_dir_all(&root).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "panels::shy_layers::tests::native::native_shy_switches_persist_and_keep_output_with_default_error_evidence", "--include-ignored"])
            .env(CHILD, &root).env_remove("MOTION_LOG").output().unwrap();
        std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
        std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(file.contains("logging initialized level=INFO"));
        assert!(file.contains("999999") && stderr.contains("999999"));
        assert!(!stdout.contains("999999") && !stdout.contains("logging initialized"));
        return;
    };
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.clone()).unwrap();
    let event_loop = winit::event_loop::EventLoop::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    for (language, locale) in [("zh", Locale::Zh), ("en", Locale::En)] {
        let project_root = root.join(format!("project-{language}"));
        motion_core::storage::save(&project_root, fixture().project()).unwrap();
        let mut shell = Shell::new(project_root.clone(), None, Some(locale)).unwrap();
        shell.home = None;
        shell.workspace = workspace::Workspace::default();
        shell.layout_path = root.join(format!("workspace-{language}.json"));
        shell.panels.focused = Some(Panel::Timeline);
        let size = [1600., 1000.];
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
        let initial = shell.state["project"].clone();
        for floating in [false, true] {
            let size = if floating { [900., 600.] } else { size };
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
                shell.atlas.set_pixel_ratio(1.5);
            }
            shell.panels.timeline.hide_shy = false;
            let bounds = if floating {
                Rect::new(0., 30., 900., 548.)
            } else {
                crate::panels::frames(&shell.workspace.root, size)
                    .into_iter()
                    .find(|f| f.panel == Panel::Timeline)
                    .unwrap()
                    .content
            };
            let mut paint = PaintList::default();
            let draw = |paint: &mut PaintList, shell: &mut Shell, input: &Input| {
                if floating {
                    crate::panels::floating_chrome(
                        paint,
                        &mut shell.atlas,
                        Panel::Timeline,
                        size,
                        &mut shell.panels,
                        &shell.state,
                        input,
                    )
                } else {
                    crate::panels::chrome(
                        paint,
                        &mut shell.atlas,
                        size,
                        &shell.workspace.root,
                        &mut shell.panels,
                        &shell.state,
                        input,
                    )
                }
            };
            assert!(draw(&mut paint, &mut shell, &Input::default()).is_empty());
            let gutter = 640f32.min(bounds.width() * 0.52);
            let columns = ColumnRects::with_sampling(
                &paint.styles(),
                bounds.min[0],
                gutter,
                shell.panels.timeline.column_visibility,
                shell.panels.timeline.sampling_visibility,
                super::super::super::layer_sampling::available(&shell.state),
            );
            let slot = columns.shy.unwrap();
            let point = [
                slot.x + slot.width * 0.5,
                bounds.min[1] + crate::panels::timeline::header_height(&paint.styles()) + paint.styles().metric("row.height", 22.) * 0.5,
            ];
            let actions = draw(&mut PaintList::default(), &mut shell, &press(point));
            assert_eq!(actions.len(), 1, "{language} float={floating}");
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
            assert_eq!(shell.state["project"]["layers"][4]["shy"], true);
            capture(
                &mut shell,
                root.join(format!(
                    "{language}-{}-marked.png",
                    if floating { "float" } else { "main" }
                )),
                floating,
                size,
            );
            let point = [
                bounds.min[0]
                    + gutter
                    + paint.styles().metric("padding", 5.)
                    + paint.styles().metric("icon.slot", 20.) * 0.5,
                bounds.min[1]
                    + paint.styles().metric("border", 1.) * 2.
                    + paint.styles().metric("field.height", 18.) * 0.5,
            ];
            let actions = draw(&mut PaintList::default(), &mut shell, &press(point));
            assert!(
                matches!(actions.as_slice(), [Action::HideShyLayers(true)]),
                "{actions:?}"
            );
            shell.panels.select_only(Some(5));
            for action in actions {
                shell.action(action, None).unwrap();
            }
            capture(
                &mut shell,
                root.join(format!(
                    "{language}-{}-hidden.png",
                    if floating { "float" } else { "main" }
                )),
                floating,
                size,
            );
            assert_eq!(shell.panels.selected, None);
            assert!(workspace::Workspace::load(&shell.layout_path).timeline_hide_shy);
            assert_eq!(
                shell.state["project"]["layers"].as_array().unwrap().len(),
                5
            );
            shell.state = shell.engine.history(0).unwrap();
            assert_eq!(shell.state["project"], initial);
            shell.panels.timeline.hide_shy = false;
        }
        shell.state = shell
            .engine
            .edit(
                serde_json::to_string(&Command::SetLayerShy {
                    object: 3,
                    shy: true,
                })
                .unwrap(),
            )
            .unwrap();
        shell.engine.save().unwrap();
        assert!(motion_core::storage::load(&project_root).unwrap().layers[2].shy);
        let before = shell.engine.state().unwrap();
        let error = shell
            .engine
            .edit(
                serde_json::to_string(&Command::SetLayerShy {
                    object: 999999,
                    shy: true,
                })
                .unwrap(),
            )
            .unwrap_err();
        assert!(error.contains("999999"));
        let after = shell.engine.state().unwrap();
        assert_eq!(before["project"], after["project"]);
        assert_eq!(before["revision"], after["revision"]);
    }
}
