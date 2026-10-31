//! Actual native surface/GPU screenshots. Input remains the production Input
//! driver, rather than claiming these are OS mouse/keyboard injection tests.
use super::super::tests::{context, fixture};
use super::*;
use crate::engine::{AtlasUpload, Command as WorkerCommand, Engine, UiFrame};
use motion_ui::{dock, paint::PaintList, text::TextAtlas};
use std::{path::PathBuf, sync::Arc, time::Duration};
use winit::{
    dpi::PhysicalSize, event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows,
    window::Window,
};

#[test]
#[ignore = "manual native type-menu GPU captures; retains screenshots and original log streams"]
fn native_type_menu_captures_exact_mixed_and_manual_states_in_both_languages() {
    const CHILD: &str = "MOTION_TYPE_MENU_NATIVE_CHILD";
    const NAME: &str = "panels::animation_menu::type_state::native_tests::native_type_menu_captures_exact_mixed_and_manual_states_in_both_languages";
    let Some(root) = std::env::var_os(CHILD) else {
        let temp = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_TYPE_MENU_NATIVE_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| temp.path().into());
        std::fs::create_dir_all(&root).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                NAME,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, &root)
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
        std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.join("logs")).unwrap();
    let (fixture, _) = fixture();
    let before = fixture.snapshot();
    let project_root = root.join("project");
    motion_core::storage::save(&project_root, &before).unwrap();
    let engine = Engine::start(project_root, motion_desktop_media::platform(), 8 << 30).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    let size = [1280., 960.];
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(size[0] as u32, size[1] as u32)),
            )
            .unwrap(),
    );
    let reply = engine
        .attach_async(window.clone(), size[0] as u32, size[1] as u32)
        .unwrap();
    crate::native_test_support::receive(&mut events, &reply, Duration::from_secs(60))
        .unwrap()
        .unwrap();
    let seek = engine.seek_async(22.).unwrap();
    crate::native_test_support::receive(&mut events, &seek, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let mut atlas = TextAtlas::from_family(
        motion_ui::style::default_stylesheet_arc().font_family(),
        12.,
        true,
    )
    .unwrap();
    let mut report = vec![];
    for locale in [Locale::Zh, Locale::En] {
        for phase in ["linear", "automatic", "mixed", "manual"] {
            let state = engine.state().unwrap();
            let mut p = context(&state, locale);
            if phase == "automatic" || phase == "manual" {
                let command = Command::KeyframeAutomatic {
                    selections: vec![KeyframeSelection {
                        target: KeyframeTarget::Property {
                            object: 7,
                            property: Property::Position,
                            axis: None,
                        },
                        frames: vec![22],
                    }],
                    enabled: phase == "automatic",
                };
                engine
                    .call(|reply| WorkerCommand::EditorAction {
                        source: crate::layer_clipboard::KeyframeSource::of(&state),
                        commands: vec![command],
                        reply,
                    })
                    .unwrap();
            }
            let state = engine.state().unwrap();
            if phase == "mixed" {
                p.selected_keys
                    .apply(
                        &state,
                        [12, 22].map(|frame| super::super::super::key_selection::Key {
                            target: KeyframeTarget::Property {
                                object: 7,
                                property: Property::Position,
                                axis: None,
                            },
                            frame,
                        }),
                        super::super::super::key_selection::Mode::Replace,
                    )
                    .unwrap();
            }
            let types = summary(&p, &state);
            let expected = match phase {
                "linear" => Kind::Linear,
                "automatic" => Kind::Automatic,
                "mixed" => Kind::Mixed,
                _ => Kind::Curve,
            };
            assert_eq!(types.kind, Some(expected));
            p.menu = Some(5);
            let mut paint = PaintList::default();
            let actions = super::super::super::chrome(
                &mut paint,
                &mut atlas,
                size,
                &dock::Node::dock(Panel::Timeline.dock()),
                &mut p,
                &state,
                &Input::default(),
            );
            assert!(actions.is_empty(), "painting type menu must not edit");
            let config = atlas.config();
            let name = format!(
                "{}-{phase}.png",
                if locale == Locale::Zh { "zh" } else { "en" }
            );
            crate::native_test_support::capture_ui(
                &mut events,
                &engine,
                UiFrame { views: None,
                    paint,
                    atlas: Some(AtlasUpload {
                        pixels: atlas.pixel_data().to_vec(),
                        width: config.width,
                        height: config.height,
                    }),
                    logical_size: size,
                    composition: None,
                    inspection: Default::default(),
                    composition_image: None,
                    window: 0,
                    node_preview: None,
                },
                root.join(&name),
            )
            .unwrap();
            let image = image::open(root.join(&name)).unwrap().to_rgba8();
            assert_eq!(image.dimensions(), (size[0] as u32, size[1] as u32));
            assert!(
                image.pixels().any(|p| p.0[0] != image.get_pixel(0, 0).0[0]),
                "capture is blank"
            );
            let after = engine.state().unwrap();
            for field in ["project", "revision", "history"] {
                assert_eq!(state[field], after[field], "{phase}: {field}");
            }
            report.push(serde_json::json!({"file":name,"title":types.title(locale),"enable":types.enable,"freeze":types.freeze}));
        }
        engine.history(0).unwrap();
        assert!(engine.history(0).unwrap()["project"] == serde_json::to_value(&before).unwrap());
    }
    std::fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    engine.request_shutdown().unwrap();
}
