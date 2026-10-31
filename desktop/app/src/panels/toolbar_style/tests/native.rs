use super::*;
use crate::{
    engine::{AtlasUpload, Engine, UiFrame},
    panels,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows};

fn dispatch(engine: &Engine, actions: Vec<Action>) {
    for action in actions {
        match action {
            Action::History(op) => {
                engine.history(op).unwrap();
            }
            Action::EditBatch(commands) => {
                engine
                    .edit(serde_json::to_string(&commands).unwrap())
                    .unwrap();
            }
            action => panic!("unexpected native toolbar action {action:?}"),
        }
    }
}
fn draw(
    engine: &Engine,
    p: &mut panels::Panels,
    atlas: &mut TextAtlas,
    size: [f32; 2],
    input: &Input,
) -> PaintList {
    let mut list = PaintList::default();
    let layout = motion_ui::dock::Node::dock(motion_ui::dock::Panel::Composition.dock());
    let actions = panels::chrome(
        &mut list,
        atlas,
        size,
        &layout,
        p,
        &engine.state().unwrap(),
        input,
    );
    dispatch(engine, actions);
    list
}
fn capture(
    events: &mut EventLoop<()>,
    engine: &Engine,
    p: &mut panels::Panels,
    atlas: &mut TextAtlas,
    size: [f32; 2],
    root: &Path,
    name: &str,
) -> PathBuf {
    let paint = draw(engine, p, atlas, size, &Input::default());
    let config = atlas.config();
    let layout = motion_ui::dock::Node::dock(motion_ui::dock::Panel::Composition.dock());
    let view = panels::composition_view(&layout, size);
    let ratio = atlas.pixel_ratio();
    let viewport = [view.min[0], view.min[1], view.width(), view.height()].map(|v| v * ratio);
    // CaptureUi consumes the retained scene. Drive the actual Present path
    // first, while pumping messages on the native window owner thread.
    let front = engine
        .present_async(UiFrame { views: None,
            paint: PaintList::default(),
            atlas: None,
            logical_size: size,
            composition: Some(viewport),
            inspection: Default::default(),
            composition_image: None,
            window: 0,
            node_preview: None,
        })
        .unwrap();
    assert!(
        crate::native_test_support::receive(events, &front, Duration::from_secs(30))
            .unwrap()
            .unwrap()
    );
    let ui = UiFrame { views: None,
        paint,
        atlas: Some(AtlasUpload {
            pixels: atlas.pixel_data().to_vec(),
            width: config.width,
            height: config.height,
        }),
        logical_size: size,
        composition: Some(viewport),
        inspection: Default::default(),
        composition_image: None,
        window: 0,
        node_preview: None,
    };
    let path = root.join(format!("{name}.png"));
    crate::native_test_support::capture_ui(events, engine, ui, path.clone()).unwrap();
    let image = image::open(&path).unwrap().to_rgba8();
    assert!(image.width() >= size[0] as u32 && image.height() >= size[1] as u32);
    assert!(
        image.pixels().any(|p| p.0[0] > 80 && p.0[2] > 80),
        "native capture contains real paint"
    );
    path
}
#[test]
fn native_toolbar_controls_capture_real_preview_and_cancel_at_float_boundary() {
    const NAME:&str="panels::toolbar_style::tests::native::native_toolbar_controls_capture_real_preview_and_cancel_at_float_boundary";
    const CHILD: &str = "MOTION_TOOLBAR_NATIVE_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let temp = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_TOOLBAR_CAPTURE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| temp.path().to_owned());
        std::fs::create_dir_all(&root).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env(CHILD, &root)
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        std::fs::write(root.join("native.stdout.log"), &output.stdout).unwrap();
        std::fs::write(root.join("native.stderr.log"), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!String::from_utf8_lossy(&output.stdout).contains("ERROR"));
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.clone()).unwrap();
    let project = root.join("project");
    motion_core::storage::save(&project, fixture().project()).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                winit::window::Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(1600, 1000)),
            )
            .unwrap(),
    );
    let engine = Engine::start(project, motion_desktop_media::platform(), 8 << 30).unwrap();
    let attached = engine.attach_async(window.clone(), 1600, 1000).unwrap();
    crate::native_test_support::receive(&mut events, &attached, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    engine.seek(0.).unwrap();
    let before = engine.state().unwrap()["project"].clone();
    let mut panels = panels::Panels {
        locale: Locale::Zh,
        ..Default::default()
    };
    panels.select_only(Some(7));
    let mut atlas = TextAtlas::from_family(
        motion_ui::style::default_stylesheet_arc().font_family(),
        18.,
        true,
    )
    .unwrap();
    let size = [1600., 1000.];
    let mut screens = vec![];
    screens.push(capture(
        &mut events,
        &engine,
        &mut panels,
        &mut atlas,
        size,
        &root,
        "toolbar-zh",
    ));
    let p = center(panels.toolbar_style.hits["fill"]);
    draw(
        &engine,
        &mut panels,
        &mut atlas,
        size,
        &pointer(p, true, false),
    );
    screens.push(capture(
        &mut events,
        &engine,
        &mut panels,
        &mut atlas,
        size,
        &root,
        "fill-popup",
    ));
    let p = center(panels.toolbar_style.hits["mode"]);
    draw(
        &engine,
        &mut panels,
        &mut atlas,
        size,
        &pointer(p, true, false),
    );
    screens.push(capture(
        &mut events,
        &engine,
        &mut panels,
        &mut atlas,
        size,
        &root,
        "fill-mode-menu",
    ));
    // Close the mode menu using its own arrow, then scrub a real channel.
    draw(
        &engine,
        &mut panels,
        &mut atlas,
        size,
        &pointer(p, true, false),
    );
    let p = center(panels.toolbar_style.hits["r"]);
    draw(
        &engine,
        &mut panels,
        &mut atlas,
        size,
        &pointer(p, true, false),
    );
    draw(
        &engine,
        &mut panels,
        &mut atlas,
        size,
        &pointer([p[0] + 80., p[1]], false, true),
    );
    assert_ne!(engine.state().unwrap()["project"], before);
    screens.push(capture(
        &mut events,
        &engine,
        &mut panels,
        &mut atlas,
        size,
        &root,
        "fill-live-preview",
    ));
    let float_input = pointer([120., 80.], true, false);
    #[allow(deprecated)]
    let float = Arc::new(
        events
            .create_window(
                winit::window::Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(380, 600)),
            )
            .unwrap(),
    );
    engine.float_window(1, float, 380, 600).unwrap();
    let mut float_paint = PaintList::default();
    let actions = panels::floating_chrome(
        &mut float_paint,
        &mut atlas,
        motion_ui::dock::Panel::Properties,
        [380., 600.],
        &mut panels,
        &engine.state().unwrap(),
        &float_input,
    );
    assert!(matches!(actions.as_slice(), [Action::History(4)]));
    dispatch(&engine, actions);
    assert_eq!(engine.state().unwrap()["project"], before);
    let config = atlas.config();
    let float_path = root.join("floating-properties.png");
    crate::native_test_support::capture_ui(
        &mut events,
        &engine,
        UiFrame { views: None,
            paint: float_paint,
            atlas: Some(AtlasUpload {
                pixels: atlas.pixel_data().to_vec(),
                width: config.width,
                height: config.height,
            }),
            logical_size: [380., 600.],
            composition: None,
            inspection: Default::default(),
            composition_image: None,
            window: 1,
            node_preview: None,
        },
        float_path.clone(),
    )
    .unwrap();
    screens.push(float_path);
    engine.close_window(1).unwrap();
    screens.push(capture(
        &mut events,
        &engine,
        &mut panels,
        &mut atlas,
        size,
        &root,
        "preview-cancelled",
    ));
    let pixels = |name: &str| {
        image::open(root.join(format!("{name}.png")))
            .unwrap()
            .crop_imm(100, 340, 1400, 580)
            .to_rgba8()
    };
    assert_ne!(
        pixels("toolbar-zh"),
        pixels("fill-live-preview"),
        "real composition pixels must change"
    );
    assert_eq!(
        pixels("toolbar-zh"),
        pixels("preview-cancelled"),
        "cancelling restores real composition pixels"
    );
    let p = center(panels.toolbar_style.hits["stroke"]);
    draw(
        &engine,
        &mut panels,
        &mut atlas,
        size,
        &pointer(p, true, false),
    );
    screens.push(capture(
        &mut events,
        &engine,
        &mut panels,
        &mut atlas,
        size,
        &root,
        "stroke-popup",
    ));
    dispatch(&engine, panels::cancel_interaction(&mut panels));
    panels.selected_layers = vec![7, 8];
    screens.push(capture(
        &mut events,
        &engine,
        &mut panels,
        &mut atlas,
        size,
        &root,
        "mixed-selection",
    ));
    panels.path_controls.tool = crate::panels::path_controls::Tool::Mask;
    screens.push(capture(
        &mut events,
        &engine,
        &mut panels,
        &mut atlas,
        size,
        &root,
        "mask-disabled",
    ));
    panels.path_controls.tool = crate::panels::path_controls::Tool::Select;
    panels.select_only(Some(7));
    panels.locale = Locale::En;
    screens.push(capture(
        &mut events,
        &engine,
        &mut panels,
        &mut atlas,
        size,
        &root,
        "toolbar-en",
    ));
    let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(900, 600));
    let resized = engine.resize_async(0, 900, 600).unwrap();
    crate::native_test_support::receive(&mut events, &resized, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    screens.push(capture(
        &mut events,
        &engine,
        &mut panels,
        &mut atlas,
        [900., 600.],
        &root,
        "toolbar-minimum",
    ));
    for ratio in [1.5, 2.] {
        let physical = [(900. * ratio) as u32, (600. * ratio) as u32];
        let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(physical[0], physical[1]));
        let resized = engine.resize_async(0, physical[0], physical[1]).unwrap();
        crate::native_test_support::receive(&mut events, &resized, Duration::from_secs(30))
            .unwrap()
            .unwrap();
        atlas.set_pixel_ratio(ratio);
        panels.locale = Locale::Zh;
        draw(
            &engine,
            &mut panels,
            &mut atlas,
            [900., 600.],
            &Input::default(),
        );
        let p = center(panels.toolbar_style.hits["fill"]);
        draw(
            &engine,
            &mut panels,
            &mut atlas,
            [900., 600.],
            &pointer(p, true, false),
        );
        screens.push(capture(
            &mut events,
            &engine,
            &mut panels,
            &mut atlas,
            [900., 600.],
            &root,
            &format!("fill-dpi-{ratio}"),
        ));
        dispatch(&engine, panels::cancel_interaction(&mut panels));
    }
    assert_eq!(engine.state().unwrap()["project"], before);
    std::fs::write(
        root.join("screens.json"),
        serde_json::to_vec_pretty(&screens).unwrap(),
    )
    .unwrap();
}
