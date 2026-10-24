use super::*;
use crate::panels::tools::tests::native::{capture, draw};
use std::{fs, path::PathBuf, sync::Arc, time::Duration};
use winit::{
    dpi::PhysicalSize, event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows,
    window::Window,
};
#[test]
fn observation_modes_render_on_native_main_and_scaled_float_without_authoring_changes() {
    const CHILD: &str = "MOTION_CAMERA_NATIVE_CHILD";
    const NAME:&str="panels::tools::camera::tests::native::observation_modes_render_on_native_main_and_scaled_float_without_authoring_changes";
    let Some(root) = std::env::var_os(CHILD) else {
        let temp = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_CAMERA_CAPTURE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| temp.path().to_owned());
        fs::create_dir_all(&root).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env(CHILD, &root)
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        fs::write(root.join("native.stdout.log"), &output.stdout).unwrap();
        fs::write(root.join("native.stderr.log"), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.clone()).unwrap();
    let project = tempfile::tempdir().unwrap();
    let e = crate::engine::Engine::start(
        project.path().join("project"),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let mut commands = vec![];
    for (id, position, color) in [
        (101, [620., 450., -200.], [0.25, 0.55, 0.9, 1.]),
        (102, [1100., 650., 150.], [0.9, 0.65, 0.2, 1.]),
        (103, [1400., 350., -100.], [0.7, 0.3, 0.4, 1.]),
    ] {
        let mut layer =
            motion_core::Layer::solid(id, "Observation fixture", [320., 240.], position, color);
        layer.three_d = true;
        commands.push(motion_core::Command::Add { layer });
    }
    let authored = e.edit(serde_json::to_string(&commands).unwrap()).unwrap();
    e.save().unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let main = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(1600, 1000)),
            )
            .unwrap(),
    );
    #[allow(deprecated)]
    let float = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(1350, 900)),
            )
            .unwrap(),
    );
    let attach = e.attach_async(main, 1600, 1000).unwrap();
    crate::native_test_support::receive(&mut events, &attach, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    e.float_window(1, float, 1350, 900).unwrap();
    let mut atlas = motion_ui::text::TextAtlas::from_family(
        motion_ui::style::default_stylesheet_arc().font_family(),
        12.,
        true,
    )
    .unwrap();
    for (window, size, scale, locale, mode, name) in [
        (
            0,
            [1600., 1000.],
            1.,
            crate::i18n::Locale::Zh,
            Active::Camera,
            "orbit-main-zh",
        ),
        (
            0,
            [1600., 1000.],
            1.,
            crate::i18n::Locale::Zh,
            Active::CameraPan,
            "pan-main-zh",
        ),
        (
            1,
            [900., 600.],
            1.5,
            crate::i18n::Locale::En,
            Active::CameraDolly,
            "dolly-float-en",
        ),
    ] {
        let baseline = e.view(1).unwrap()["observationCamera"].clone();
        let mut p = Panels {
            focused: Some(motion_ui::dock::Panel::Composition),
            locale,
            ..Default::default()
        };
        draw(&e, &mut p, &mut atlas, window, size, Input::default());
        let mut out = vec![];
        choose(&mut p, mode, &e.state().unwrap(), &mut out);
        assert!(out.is_empty());
        for i in [
            pointer([400., 300.], 0),
            pointer([480., 245.], 1),
            pointer([480., 245.], 2),
        ] {
            let moving = i.held == Some(MouseButton::Left) && i.pressed.is_none();
            let (_, out) = draw(&e, &mut p, &mut atlas, window, size, i);
            apply(&e, out);
            if moving {
                let (other, other_size) = if window == 0 {
                    (1, [900., 600.])
                } else {
                    (0, [1600., 1000.])
                };
                let (_, background) =
                    draw(&e, &mut p, &mut atlas, other, other_size, Input::default());
                assert!(
                    background.is_empty(),
                    "background paint must not end another window's gesture"
                );
                assert!(p.tools.camera.editing());
            }
        }
        let after = e.state().unwrap();
        assert_ne!(after["observationCamera"], baseline);
        assert_eq!(after["project"], authored["project"]);
        assert_eq!(after["history"], authored["history"]);
        capture(
            &mut events,
            &e,
            &mut p,
            &mut atlas,
            window,
            size,
            scale,
            root.join(format!("{name}.png")),
            Input::default(),
        );
        assert_eq!(
            e.camera_gesture(Op::Undo).unwrap()["observationCamera"],
            baseline
        );
        e.camera_gesture(Op::Redo).unwrap();
        e.camera_gesture(Op::Undo).unwrap();
        e.save().unwrap();
        let loaded = motion_core::storage::load(&project.path().join("project")).unwrap();
        assert_eq!(serde_json::to_value(loaded).unwrap(), authored["project"]);
        let _ = locale;
    }
}
