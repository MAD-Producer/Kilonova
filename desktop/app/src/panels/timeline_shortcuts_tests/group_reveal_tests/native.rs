//! Opt-in evidence: actual worker, native owners and GPU captures.
use super::*;
use crate::engine::{AtlasUpload, Engine as DesktopEngine, UiFrame};
use std::{path::PathBuf, sync::Arc, time::Duration};
use winit::{
    dpi::PhysicalSize, event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows,
    window::Window,
};

fn paint_frame(
    p: &mut Panels,
    state: &Value,
    input: Input,
    window: u64,
    atlas: &mut TextAtlas,
) -> PaintList {
    let mut paint = PaintList::default();
    let actions = if window == 0 {
        super::super::super::chrome(
            &mut paint,
            atlas,
            [1000., 760.],
            &dock::Node::dock(Panel::Timeline.dock()),
            p,
            state,
            &input,
        )
    } else {
        super::super::super::floating_chrome(
            &mut paint,
            atlas,
            Panel::Timeline,
            [1000., 760.],
            p,
            state,
            &input,
        )
    };
    assert!(actions.is_empty());
    paint
}

#[test]
#[ignore = "manual native timeline disclosure captures; retained artifacts required"]
fn native_group_shortcuts_capture_both_owners_and_locales_without_document_edits() {
    const CHILD: &str = "MOTION_TIMELINE_GROUP_NATIVE_CHILD";
    const NAME: &str = "panels::timeline_shortcuts_tests::group_reveal_tests::native::native_group_shortcuts_capture_both_owners_and_locales_without_document_edits";
    let Some(root) = std::env::var_os(CHILD) else {
        let temp = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_TIMELINE_GROUP_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| temp.path().into());
        std::fs::create_dir_all(&root).unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                NAME,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env_remove("MOTION_LOG")
            .env(CHILD, &root)
            .output()
            .unwrap();
        std::fs::write(root.join("stdout.log"), &result.stdout).unwrap();
        std::fs::write(root.join("stderr.log"), &result.stderr).unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.join("logs")).unwrap();
    let (core, _) = fixture();
    let graph = motion_graph::builtin_examples()
        .iter()
        .find(|example| example.id == "soft_sphere_motion")
        .unwrap()
        .load()
        .unwrap();
    let compiled = motion_graph::compile(&graph, &Default::default()).unwrap();
    let source = root.join("fixture.msfx");
    std::fs::write(
        &source,
        motion_graph::compiled_package_bytes(&compiled.manifest, &compiled.source).unwrap(),
    )
    .unwrap();
    let plugins = root.join("plugins");
    motion_effects::Registry::load(&plugins)
        .unwrap()
        .install(&plugins, &source)
        .unwrap();
    let project_root = root.join("project");
    motion_core::storage::save(&project_root, core.project()).unwrap();
    let saved = std::fs::read(project_root.join("project.json")).unwrap();
    let engine = DesktopEngine::start(
        project_root.clone(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let catalogue = engine
        .plugin(serde_json::json!({"op":"catalogue"}).to_string())
        .unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let main = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(1000, 760)),
            )
            .unwrap(),
    );
    #[allow(deprecated)]
    let float = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(1000, 760)),
            )
            .unwrap(),
    );
    let attached = engine.attach_async(main, 1000, 760).unwrap();
    crate::native_test_support::receive(&mut events, &attached, Duration::from_secs(60))
        .unwrap()
        .unwrap();
    engine.float_window(1, float, 1000, 760).unwrap();
    let seek = engine.seek_async(30.).unwrap();
    crate::native_test_support::receive(&mut events, &seek, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let before = engine.state().unwrap();
    let mut atlas = TextAtlas::from_family(
        motion_ui::style::default_stylesheet_arc().font_family(),
        12.,
        true,
    )
    .unwrap();
    assert!(
        atlas.glyph('蒙').uv[2] > atlas.glyph('蒙').uv[0],
        "native captures require Chinese fallback glyphs"
    );
    let mut evidence = vec![];
    for locale in [Locale::En, Locale::Zh] {
        for window in [0, 1] {
            for (key, name) in [(Key::E, "effects"), (Key::M, "masks")] {
                let mut p = panels();
                p.locale = locale;
                p.effect_controls.catalogue = catalogue.clone().into();
                p.timeline.collapsed_transforms.insert(7);
                p.timeline.collapsed_masks.insert(7);
                for phase in ["open", "closed"] {
                    // Route once, then paint the resulting state without another key press.
                    paint_frame(
                        &mut p,
                        &before,
                        Input {
                            keys: vec![(key, Default::default())],
                            ..Default::default()
                        },
                        window,
                        &mut atlas,
                    );
                    let paint = paint_frame(&mut p, &before, Input::default(), window, &mut atlas);
                    let label = format!(
                        "{}-{}-{name}-{phase}",
                        if locale == Locale::Zh { "zh" } else { "en" },
                        if window == 0 { "main" } else { "float" }
                    );
                    let path = root.join(format!("{label}.png"));
                    let config = atlas.config();
                    crate::native_test_support::capture_ui(
                        &mut events,
                        &engine,
                        UiFrame {
                            paint,
                            atlas: Some(AtlasUpload {
                                pixels: atlas.pixel_data().to_vec(),
                                width: config.width,
                                height: config.height,
                            }),
                            logical_size: [1000., 760.],
                            composition: None,
                            composition_image: None,
                            window,
                            node_preview: None,
                        },
                        path.clone(),
                    )
                    .unwrap();
                    let pixels = image::open(&path).unwrap().to_rgba8();
                    assert_eq!(pixels.dimensions(), (1000, 760));
                    assert!(pixels.pixels().any(|pixel| pixel.0[0] > 100));
                    let rows = timeline::rows(&before);
                    let row = rows.iter().find(|row| row.id == 7).unwrap();
                    let layout = timeline_layout::LayerRows::new(row, &p.timeline, 30., locale);
                    if key == Key::E {
                        assert_eq!(layout.effects_open, phase == "open");
                    } else {
                        assert_eq!(layout.mask_rows.len() > 2, phase == "open");
                    }
                    evidence.push(
                        serde_json::json!({"case":label,"effects_open":layout.effects_open,
                        "mask_rows":layout.mask_rows.len(),"actions":0}),
                    );
                }
            }
        }
    }
    assert_eq!(engine.state().unwrap()["project"], before["project"]);
    assert_eq!(
        std::fs::read(project_root.join("project.json")).unwrap(),
        saved
    );
    std::fs::write(
        root.join("results.json"),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
    engine.request_shutdown().unwrap();
}
