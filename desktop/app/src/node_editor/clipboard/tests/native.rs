use super::*;
use std::{fs, path::PathBuf, sync::Arc, time::Duration};
use winit::{
    dpi::PhysicalSize, event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows,
    window::Window,
};
fn draw(
    e: &mut NodeEditor,
    atlas: &mut TextAtlas,
    size: [f32; 2],
    input: &Input,
    language: Language,
) -> PaintList {
    let mut list = PaintList::default();
    e.draw(
        &mut list,
        atlas,
        Rect::new(0., 0., size[0], size[1]),
        input,
        language,
    );
    list
}
fn capture(
    events: &mut EventLoop<()>,
    engine: &crate::engine::Engine,
    e: &mut NodeEditor,
    atlas: &mut TextAtlas,
    window: u64,
    size: [f32; 2],
    scale: f32,
    path: PathBuf,
    language: Language,
) {
    atlas.set_pixel_ratio(scale);
    let paint = draw(e, atlas, size, &Input::default(), language);
    let config = atlas.config();
    let ui = crate::engine::UiFrame {
        paint,
        atlas: Some(crate::engine::AtlasUpload {
            pixels: atlas.pixel_data().to_vec(),
            width: config.width,
            height: config.height,
        }),
        logical_size: size,
        composition: None,
        composition_image: None,
        window,
        node_preview: None,
    };
    let front = engine
        .present_async(crate::engine::UiFrame {
            paint: PaintList::default(),
            atlas: None,
            logical_size: size,
            composition: None,
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
    crate::native_test_support::capture_ui(events, engine, ui, path.clone()).unwrap();
    assert_eq!(
        image::open(path).unwrap().to_rgba8().dimensions(),
        ((size[0] * scale) as u32, (size[1] * scale) as u32)
    );
}
#[test]
fn native_clipboard_menus_and_pasted_graph_use_real_window_surfaces() {
    const CHILD: &str = "MOTION_NODE_CLIPBOARD_NATIVE_CHILD";
    const NAME:&str="node_editor::clipboard::tests::native::native_clipboard_menus_and_pasted_graph_use_real_window_surfaces";
    let Some(root) = std::env::var_os(CHILD) else {
        let temp = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_NODE_CLIPBOARD_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| temp.path().into());
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
        assert!(!String::from_utf8_lossy(&result.stdout).contains("node clipboard rejected"));
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.clone()).unwrap();
    let project = motion_core::Project::new(320, 180, 30, 90).unwrap();
    motion_core::storage::save(&root.join("project"), &project).unwrap();
    let engine = crate::engine::Engine::start(
        root.join("project"),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let main = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(1200, 700)),
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
    let attach = engine.attach_async(main.clone(), 1200, 700).unwrap();
    crate::native_test_support::receive(&mut events, &attach, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    engine.float_window(1, float.clone(), 1350, 900).unwrap();
    let mut atlas = TextAtlas::from_family(
        motion_ui::style::default_stylesheet_arc().font_family(),
        12.,
        true,
    )
    .unwrap();
    let mut e = editor();
    e.inspector_open = false;
    for node in &mut e.document.nodes {
        if node.id == "color" || node.id == "output" {
            node.position[1] = 20.;
        }
    }
    draw(
        &mut e,
        &mut atlas,
        [1200., 700.],
        &Input::default(),
        Language::Chinese,
    );
    let before = e.document.clone();
    for (window, size, scale, language, suffix) in [
        (0, [1200., 700.], 1., Language::Chinese, "main-zh"),
        (1, [900., 600.], 1.5, Language::English, "floating-en"),
    ] {
        e.context_menu = None;
        e.focus_panel();
        e.select(Some("number"), false);
        e.select(Some("math"), true);
        let input = Input {
            keys: vec![(
                Key::C,
                Modifiers {
                    control: true,
                    ..Default::default()
                },
            )],
            ..Default::default()
        };
        draw(&mut e, &mut atlas, size, &input, language);
        assert_eq!(e.document, before);
        e.context_menu = Some(ContextMenu {
            position: [400., 100.],
            kind: MenuKind::Node("math".into()),
            selected: 0,
        });
        capture(
            &mut events,
            &engine,
            &mut e,
            &mut atlas,
            window,
            size,
            scale,
            root.join(format!("node-menu-{suffix}.png")),
            language,
        );
        e.context_menu = Some(ContextMenu {
            position: [350., 150.],
            kind: MenuKind::Canvas,
            selected: 0,
        });
        capture(
            &mut events,
            &engine,
            &mut e,
            &mut atlas,
            window,
            size,
            scale,
            root.join(format!("paste-menu-{suffix}.png")),
            language,
        );
        e.context_menu = None;
        let input = Input {
            keys: vec![(
                Key::V,
                Modifiers {
                    control: true,
                    ..Default::default()
                },
            )],
            mouse: [450., 500.],
            ..Default::default()
        };
        draw(&mut e, &mut atlas, size, &input, language);
        assert_eq!(e.document.nodes.len(), before.nodes.len() + 2);
        e.fit_all(e.last_canvas);
        capture(
            &mut events,
            &engine,
            &mut e,
            &mut atlas,
            window,
            size,
            scale,
            root.join(format!("pasted-{suffix}.png")),
            language,
        );
        assert!(e.undo());
        assert_eq!(e.document, before);
    }
    engine.close_window(1).unwrap();
    drop(engine);
    drop(float);
    drop(main);
    crate::logging::flush();
}
