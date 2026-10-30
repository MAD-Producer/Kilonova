use super::*;
use crate::panels::Panels;
use motion_ui::dock::{Node, Panel};
use std::{path::PathBuf, sync::Arc, time::Duration};

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_reset_and_bounds_use_main_and_floating_routes_in_both_languages_at_four_dpis() {
    const CHILD: &str = "MOTION_CAMERA_FIELDS_NATIVE_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let temporary = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_CAMERA_FIELDS_NATIVE_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| temporary.path().to_owned());
        std::fs::create_dir_all(&root).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "panels::camera_controls::tests::native::native_reset_and_bounds_use_main_and_floating_routes_in_both_languages_at_four_dpis"])
            .env(CHILD, &root).env_remove("MOTION_LOG").output().unwrap();
        std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
        std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    };
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let root = PathBuf::from(root);
    let event_loop = winit::event_loop::EventLoop::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    for ratio in [1f32, 1.25, 1.5, 2.] {
        let (engine, object) = fixture(
            &root.join(format!("project-{}", (ratio * 100.) as u32)),
            false,
        );
        edit(
            &engine,
            Command::CameraMode {
                object,
                mode: CameraMode::Orbit,
            },
        );
        edit(
            &engine,
            Command::SetScalar {
                object,
                property: Property::Elevation,
                frame: 0,
                value: 60.,
            },
        );
        for floating in [false, true] {
            let size = if floating { [360., 620.] } else { [500., 620.] };
            let physical = [(size[0] * ratio) as u32, (size[1] * ratio) as u32];
            #[allow(deprecated)]
            let window = Arc::new(
                event_loop
                    .create_window(
                        winit::window::Window::default_attributes()
                            .with_visible(false)
                            .with_inner_size(winit::dpi::PhysicalSize::new(
                                physical[0],
                                physical[1],
                            )),
                    )
                    .unwrap(),
            );
            if floating {
                engine
                    .float_window(1, window, physical[0], physical[1])
                    .unwrap();
            } else {
                engine
                    .attach_async(window, physical[0], physical[1])
                    .unwrap()
                    .recv_timeout(Duration::from_secs(30))
                    .unwrap()
                    .unwrap();
            }
            for (locale, language) in [(Locale::En, "en"), (Locale::Zh, "zh")] {
                let mut panels = Panels::default();
                panels.locale = locale;
                panels.focused = Some(Panel::Properties);
                panels.select_only(Some(object));
                let sheet = PaintList::default().styles();
                let mut atlas = TextAtlas::from_family(
                    sheet.font_family(),
                    sheet.base_font_size(),
                    locale == Locale::Zh,
                )
                .unwrap();
                atlas.set_pixel_ratio(ratio);
                let layout = Node::dock(Panel::Properties.dock());
                let render =
                    |panels: &mut Panels, atlas: &mut TextAtlas, state: &Value, input: &Input| {
                        let mut paint = PaintList::default();
                        let actions = if floating {
                            crate::panels::floating_chrome(
                                &mut paint,
                                atlas,
                                Panel::Properties,
                                size,
                                panels,
                                state,
                                input,
                            )
                        } else {
                            crate::panels::chrome(
                                &mut paint, atlas, size, &layout, panels, state, input,
                            )
                        };
                        (paint, actions)
                    };
                let capture = |paint: PaintList, atlas: &TextAtlas, suffix: &str| {
                    let config = atlas.config();
                    let path = root.join(format!(
                        "{language}-{}-dpi{}-{suffix}.png",
                        if floating { "floating" } else { "main" },
                        (ratio * 100.) as u32
                    ));
                    let report = engine
                        .capture_ui(
                            crate::engine::UiFrame {
                                paint,
                                atlas: Some(crate::engine::AtlasUpload {
                                    pixels: atlas.pixel_data().to_vec(),
                                    width: config.width,
                                    height: config.height,
                                }),
                                logical_size: size,
                                composition: None,
                                inspection: Default::default(),
                                composition_image: None,
                                window: u64::from(floating),
                                node_preview: None,
                            },
                            path.clone(),
                        )
                        .unwrap();
                    assert_eq!(report["width"], physical[0]);
                    assert_eq!(report["height"], physical[1]);
                    assert_eq!(
                        image::open(path).unwrap().into_rgba8().dimensions(),
                        (physical[0], physical[1])
                    );
                };
                let before = engine.state().unwrap();
                let (paint, actions) = render(&mut panels, &mut atlas, &before, &Input::default());
                assert!(actions.is_empty());
                capture(paint, &atlas, "fields");
                let point = center(rect(&panels.camera_controls, Property::Elevation, 0));
                let (paint, actions) = render(
                    &mut panels,
                    &mut atlas,
                    &before,
                    &press(point, MouseButton::Right),
                );
                assert!(actions.is_empty(), "{actions:?}");
                assert!(
                    panels.camera_controls.reset.open,
                    "{language} floating={floating}"
                );
                capture(paint, &atlas, "reset-menu");
                // Escape must close the menu through the same native route.
                let (_, actions) = render(&mut panels, &mut atlas, &before, &key(Key::Escape));
                assert!(actions.is_empty());
                assert!(!panels.camera_controls.editing());
                render(
                    &mut panels,
                    &mut atlas,
                    &before,
                    &press(point, MouseButton::Right),
                );
                let commit = if locale == Locale::En {
                    key(Key::Enter)
                } else {
                    let anchor = panels.camera_controls.reset_target.unwrap().2;
                    press(
                        [
                            point[0],
                            anchor.max[1] + sheet.metric("row.height", 22.) * 0.5,
                        ],
                        MouseButton::Left,
                    )
                };
                let (_, actions) = render(&mut panels, &mut atlas, &before, &commit);
                assert!(
                    matches!(
                        actions.as_slice(),
                        [Action::Edit(Command::SetScalar {
                            property: Property::Elevation,
                            value: 0.,
                            ..
                        })]
                    ),
                    "{actions:?}"
                );
                run(&engine, actions);
                let after = engine.state().unwrap();
                capture(
                    render(&mut panels, &mut atlas, &after, &Input::default()).0,
                    &atlas,
                    "reset-applied",
                );
                assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
                // A rejected pole value keeps focus and emits a localized range
                // readout through Panels, for docked and native floating input.
                render(
                    &mut panels,
                    &mut atlas,
                    &before,
                    &press(point, MouseButton::Left),
                );
                render(
                    &mut panels,
                    &mut atlas,
                    &before,
                    &Input {
                        mouse: point,
                        released: Some((MouseButton::Left, point)),
                        ..Default::default()
                    },
                );
                let mut select_all = key(Key::A);
                select_all.keys[0].1.control = true;
                render(&mut panels, &mut atlas, &before, &select_all);
                render(
                    &mut panels,
                    &mut atlas,
                    &before,
                    &Input {
                        mouse: point,
                        text: "90".into(),
                        ..Default::default()
                    },
                );
                let mut enter = key(Key::Enter);
                enter.mouse = point;
                let (paint, actions) = render(&mut panels, &mut atlas, &before, &enter);
                assert!(actions.is_empty(), "{actions:?}");
                let note = &panels.drag_readout.as_ref().unwrap().1;
                assert!(
                    note.contains(if locale == Locale::Zh {
                        "有限数值"
                    } else {
                        "finite value"
                    }),
                    "{note}"
                );
                capture(paint, &atlas, "invalid-pole");
                let (_, actions) = render(&mut panels, &mut atlas, &before, &key(Key::Escape));
                run(&engine, actions);
                assert!(!panels.camera_controls.editing());
                assert_eq!(engine.state().unwrap()["project"], before["project"]);
            }
        }
    }
}
