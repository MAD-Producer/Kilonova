//! Real GPU/native surfaces; inputs enter the production normalized boundary.
//! This does not inject foreground OS mouse or change the system DPI setting.
use super::*;
use crate::{native_test_support, Shell};
use std::{path::PathBuf, sync::Arc, time::Duration};
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows, window::Window};

fn draw(
    shell: &mut Shell,
    floating: bool,
    size: [f32; 2],
    input: &Input,
) -> (PaintList, Vec<Action>) {
    let mut paint = PaintList::default();
    paint.stylesheet = shell.panels.styles.clone();
    let actions = if floating {
        crate::panels::floating_chrome(
            &mut paint,
            &mut shell.atlas,
            Panel::Properties,
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
    };
    (paint, actions)
}
fn capture(
    shell: &mut Shell,
    events: &mut EventLoop<()>,
    paint: PaintList,
    size: [f32; 2],
    path: PathBuf,
) {
    let mut frame = shell.ui_frame(paint, size, true);
    frame.composition = None;
    frame.composition_image = None;
    frame.node_preview = None;
    native_test_support::capture_ui(events, &shell.engine, frame, path.clone()).unwrap();
    assert_eq!(
        image::open(path).unwrap().to_rgba8().dimensions(),
        (
            (size[0] * shell.scale) as u32,
            (size[1] * shell.scale) as u32
        )
    );
}

#[test]
#[ignore = "manual Windows GPU masks; normalized input, not physical OS input"]
fn native_mask_feather_link_main_and_float_pixels_and_transactions() {
    let temporary = tempfile::tempdir().unwrap();
    let output = std::env::var_os("MOTION_MASK_BOUNDARY_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().join("evidence"));
    std::fs::create_dir_all(&output).unwrap();
    let _logging = crate::logging::init(output.join("logs")).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    for (locale, language) in [(Locale::Zh, "zh"), (Locale::En, "en")] {
        for floating in [false, true] {
            for scale in [1., 1.5, 2.] {
                let project = temporary
                    .path()
                    .join(format!("{language}-{floating}-{scale}"));
                motion_core::storage::save(&project, fixture().project()).unwrap();
                let mut shell = Shell::new(project.clone(), None, Some(locale)).unwrap();
                shell.home = None;
                shell.scale = scale;
                shell.panels.select_only(Some(7));
                shell.panels.focused = Some(Panel::Properties);
                shell.panels.properties.transform_collapsed = true;
                shell.panels.path_controls.masks_open = true;
                shell.workspace.root = motion_ui::dock::Node::dock(Panel::Properties.dock());
                let size = if floating { [320., 420.] } else { [900., 600.] };
                let pixels = [(size[0] * scale) as u32, (size[1] * scale) as u32];
                #[allow(deprecated)]
                let window = Arc::new(
                    events
                        .create_window(
                            Window::default_attributes()
                                .with_visible(false)
                                .with_inner_size(winit::dpi::PhysicalSize::new(
                                    pixels[0], pixels[1],
                                )),
                        )
                        .unwrap(),
                );
                let reply = shell
                    .engine
                    .attach_async(window.clone(), pixels[0], pixels[1])
                    .unwrap();
                shell.state =
                    native_test_support::receive(&mut events, &reply, Duration::from_secs(60))
                        .unwrap()
                        .unwrap();
                shell.window = Some(window);
                shell.attached = true;
                shell.atlas.set_pixel_ratio(scale);
                shell.state = shell.engine.seek(33.).unwrap();
                let before = shell.state["project"].clone();
                let prefix = format!(
                    "mask-feather-{language}-{}-{scale}x",
                    if floating { "floating-route" } else { "main" }
                );
                let (paint, actions) = draw(&mut shell, floating, size, &Input::default());
                assert!(actions.is_empty());
                let rect = shell.panels.path_controls.feather_link_rects[&(7, 1)];
                let linked = output.join(format!("{prefix}-linked.png"));
                capture(&mut shell, &mut events, paint, size, linked.clone());
                let point = [
                    rect.min[0] + rect.width() * 0.5,
                    rect.min[1] + rect.height() * 0.5,
                ];
                let (_, actions) = draw(
                    &mut shell,
                    floating,
                    size,
                    &Input {
                        mouse: point,
                        pressed: Some((MouseButton::Left, point)),
                        ..Default::default()
                    },
                );
                assert_eq!(actions.len(), 1);
                for action in actions {
                    shell.action(action, None).unwrap();
                    if let Some((_, reply)) = shell.editing.take() {
                        shell.accept_edit_state(
                            native_test_support::receive(
                                &mut events,
                                &reply,
                                Duration::from_secs(30),
                            )
                            .unwrap()
                            .unwrap(),
                        );
                    }
                }
                let mut expected = before.clone();
                expected["layers"][0]["masks"][0]["feather_linked"] = serde_json::json!(false);
                assert_eq!(shell.state["project"], expected);
                let (paint, actions) = draw(&mut shell, floating, size, &Input::default());
                assert!(actions.is_empty());
                let unlinked = output.join(format!("{prefix}-unlinked.png"));
                capture(&mut shell, &mut events, paint, size, unlinked.clone());
                let crop = |path| {
                    let pixels = image::open(path).unwrap().to_rgba8();
                    image::imageops::crop_imm(
                        &pixels,
                        (rect.min[0] * scale) as u32,
                        (rect.min[1] * scale) as u32,
                        (rect.width() * scale) as u32,
                        (rect.height() * scale) as u32,
                    )
                    .to_image()
                };
                assert_ne!(
                    crop(linked),
                    crop(unlinked),
                    "actual checkbox pixels must reflect the model state"
                );
                shell.engine.save().unwrap();
                assert_eq!(
                    serde_json::to_value(motion_core::storage::load(&project).unwrap()).unwrap(),
                    expected
                );
                shell.state = shell.engine.history(0).unwrap();
                assert_eq!(shell.state["project"], before);
            }
        }
    }
}
