use super::*;
use std::{fs, sync::Arc, time::Duration};
use winit::{
    dpi::PhysicalSize, event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows,
    window::Window,
};
fn draw(
    e: &crate::engine::Engine,
    p: &mut Panels,
    a: &mut TextAtlas,
    window: u64,
    size: [f32; 2],
    input: Input,
) -> (PaintList, Vec<Action>) {
    let mut list = PaintList::default();
    let out = if window == 0 {
        crate::panels::chrome(
            &mut list,
            a,
            size,
            &motion_ui::dock::Node::dock(Panel::Composition.dock()),
            p,
            &e.state().unwrap(),
            &input,
        )
    } else {
        crate::panels::floating_chrome(
            &mut list,
            a,
            Panel::Composition,
            size,
            p,
            &e.state().unwrap(),
            &input,
        )
    };
    (list, out)
}
fn capture(
    events: &mut EventLoop<()>,
    e: &crate::engine::Engine,
    p: &mut Panels,
    a: &mut TextAtlas,
    window: u64,
    size: [f32; 2],
    scale: f32,
    path: PathBuf,
    input: Input,
) {
    a.set_pixel_ratio(scale);
    let (paint, actions) = draw(e, p, a, window, size, input);
    assert!(actions.is_empty());
    let styles = paint.styles();
    let view = if window == 0 {
        crate::panels::composition_view_styled(
            &motion_ui::dock::Node::dock(Panel::Composition.dock()),
            size,
            &styles,
        )
    } else {
        crate::panels::floating_view_styled(Panel::Composition, size, &styles).unwrap()
    };
    let composition =
        Some([view.min[0], view.min[1], view.width(), view.height()].map(|v| v * scale));
    let front = e
        .present_async(crate::engine::UiFrame {
            paint: PaintList::default(),
            atlas: None,
            logical_size: size,
            composition,
            composition_image: None,
            window,
            node_preview: None,
        })
        .unwrap();
    assert!(
        crate::native_test_support::receive(events, &front, Duration::from_secs(30))
            .unwrap()
            .unwrap()
    );
    let c = a.config();
    crate::native_test_support::capture_ui(
        events,
        e,
        crate::engine::UiFrame {
            paint,
            atlas: Some(crate::engine::AtlasUpload {
                pixels: a.pixel_data().to_vec(),
                width: c.width,
                height: c.height,
            }),
            logical_size: size,
            composition,
            composition_image: None,
            window,
            node_preview: None,
        },
        path.clone(),
    )
    .unwrap();
    assert_eq!(
        image::open(path).unwrap().to_rgba8().dimensions(),
        ((size[0] * scale) as u32, (size[1] * scale) as u32)
    );
}
#[test]
fn native_tool_activation_drag_and_created_shapes_use_real_surfaces() {
    const CHILD: &str = "MOTION_TOOLS_NATIVE_CHILD";
    const NAME:&str="panels::tools::tests::native::native_tool_activation_drag_and_created_shapes_use_real_surfaces";
    let Some(root) = std::env::var_os(CHILD) else {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_TOOLS_CAPTURE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| tmp.path().to_owned());
        fs::create_dir_all(&root).unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env(CHILD, &root)
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        fs::write(root.join("native.stdout.log"), &result.stdout).unwrap();
        fs::write(root.join("native.stderr.log"), &result.stderr).unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.clone()).unwrap();
    let _project = tempfile::tempdir().unwrap();
    let e = crate::engine::Engine::start(
        _project.path().join("project"),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    assert_eq!(
        e.state().unwrap()["project"]["layers"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
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
    let mut p = Panels {
        focused: Some(Panel::Composition),
        ..Default::default()
    };
    let mut atlas = TextAtlas::from_family(
        motion_ui::style::default_stylesheet_arc().font_family(),
        12.,
        true,
    )
    .unwrap();
    for (window, size, scale, locale, start, end, name, id) in [
        (
            0,
            [1600., 1000.],
            1.,
            Locale::Zh,
            [400., 300.],
            [800., 520.],
            "rectangle-main-zh",
            101,
        ),
        (
            1,
            [900., 600.],
            1.5,
            Locale::En,
            [250., 200.],
            [550., 350.],
            "ellipse-floating-en",
            102,
        ),
    ] {
        p.locale = locale;
        atlas.set_pixel_ratio(scale);
        draw(&e, &mut p, &mut atlas, window, size, Input::default());
        let before = e.state().unwrap()["project"].clone();
        let (_, out) = draw(&e, &mut p, &mut atlas, window, size, key(Key::Q));
        assert!(out.is_empty());
        assert_eq!(e.state().unwrap()["project"], before);
        draw(
            &e,
            &mut p,
            &mut atlas,
            window,
            size,
            Input {
                mouse: start,
                pressed: Some((MouseButton::Left, start)),
                held: Some(MouseButton::Left),
                ..Default::default()
            },
        );
        capture(
            &mut events,
            &e,
            &mut p,
            &mut atlas,
            window,
            size,
            scale,
            root.join(format!("{name}-drag.png")),
            Input {
                mouse: end,
                held: Some(MouseButton::Left),
                ..Default::default()
            },
        );
        let (_, out) = draw(
            &e,
            &mut p,
            &mut atlas,
            window,
            size,
            Input {
                mouse: end,
                released: Some((MouseButton::Left, end)),
                ..Default::default()
            },
        );
        let [Action::PlaceShape {
            shape,
            size: shape_size,
            position,
        }] = out.as_slice()
        else {
            panic!("{out:?}")
        };
        e.edit(
            serde_json::to_string(&motion_core::Command::AddShape {
                id,
                name: format!("Authored shape {id}"),
                shape: *shape,
                size: *shape_size,
                position: *position,
            })
            .unwrap(),
        )
        .unwrap();
        p.select_only(Some(id));
        capture(
            &mut events,
            &e,
            &mut p,
            &mut atlas,
            window,
            size,
            scale,
            root.join(format!("{name}-created.png")),
            Input::default(),
        );
        e.save().unwrap();
        let reopened = motion_core::storage::load(&_project.path().join("project")).unwrap();
        assert_eq!(
            serde_json::to_value(reopened).unwrap(),
            e.state().unwrap()["project"]
        );
        e.history(0).unwrap();
        assert_eq!(e.state().unwrap()["project"], before);
        e.history(1).unwrap();
        e.history(0).unwrap();
    }
}
