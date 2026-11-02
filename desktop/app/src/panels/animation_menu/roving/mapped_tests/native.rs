//! Capture the real main menu and floating timeline on native GPU surfaces.
use super::*;
use crate::engine::{AtlasUpload, Engine as Worker, UiFrame};
use motion_ui::{dock, paint::PaintList, text::TextAtlas};
use std::{path::PathBuf, sync::Arc, time::Duration};
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows};

#[test]
#[ignore = "manual signed-local Roving native menu and floating-window captures"]
fn signed_local_native_menus_and_floating_views_use_real_worker_conversion() {
    const NAME: &str = "panels::animation_menu::roving::mapped_tests::native::signed_local_native_menus_and_floating_views_use_real_worker_conversion";
    const CHILD: &str = "MOTION_LOCAL_ROVING_NATIVE_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let temp = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_LOCAL_ROVING_NATIVE_ARTIFACTS")
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
        assert_eq!(
            std::fs::read_dir(&root)
                .unwrap()
                .map(Result::unwrap)
                .filter(|e| e.path().extension().is_some_and(|x| x == "png"))
                .count(),
            24
        );
        return;
    };
    let root = PathBuf::from(root);
    let _log = crate::logging::init(root.join("logs")).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let windows: [_; 2] = std::array::from_fn(|_| {
        Arc::new(
            events
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(1000, 800)),
                )
                .unwrap(),
        )
    });
    let mut atlas = TextAtlas::from_family(
        motion_ui::style::default_stylesheet_arc().font_family(),
        12.,
        true,
    )
    .unwrap();
    let mut report = vec![];
    for reverse in [false, true] {
        let tag = if reverse { "reverse" } else { "forward" };
        let initial = fixture(reverse).snapshot();
        let path = root.join(tag);
        motion_core::storage::save(&path, &initial).unwrap();
        let engine = Worker::start(path, motion_desktop_media::platform(), 8 << 30).unwrap();
        crate::native_test_support::receive(
            &mut events,
            &engine.attach_async(windows[0].clone(), 1000, 800).unwrap(),
            Duration::from_secs(60),
        )
        .unwrap()
        .unwrap();
        engine
            .float_window(1, windows[1].clone(), 1000, 800)
            .unwrap();
        for locale in [Locale::Zh, Locale::En] {
            for floating in [false, true] {
                let mut state = engine.state().unwrap();
                let mut p = panels(&state, locale, Panel::Timeline, &[-9]);
                p.timeline.expanded.insert(7);
                p.timeline.zoom = 2.;
                for phase in ["ordinary", "roving", "fixed"] {
                    if phase != "ordinary" {
                        let mut actions = vec![];
                        super::super::super::apply(
                            &mut p,
                            &state,
                            Operation::Roving(phase == "roving"),
                            &mut actions,
                        );
                        let [Action::EditorAction { source, commands }] = actions.as_slice() else {
                            panic!("one guarded worker action: {actions:?}")
                        };
                        let reply = engine
                            .request(|reply| crate::engine::Command::EditorAction {
                                source: source.clone(),
                                commands: commands.clone(),
                                reply,
                            })
                            .unwrap();
                        state = crate::native_test_support::receive(
                            &mut events,
                            &reply,
                            Duration::from_secs(30),
                        )
                        .unwrap()
                        .unwrap();
                        p.normalize_selection(&state);
                        assert_eq!(p.selected_keys.spatial_selections(), [(target(7), vec![2])]);
                    }
                    let expected = match phase {
                        "ordinary" => (true, false),
                        "roving" => (false, true),
                        _ => (true, false),
                    };
                    assert_eq!(
                        (
                            super::super::super::enabled(&p, &state, Operation::Roving(true)),
                            super::super::super::enabled(&p, &state, Operation::Roving(false))
                        ),
                        expected
                    );
                    // A changed selection normally dismisses an open menu.
                    // Capture a newly opened menu for this authoritative state.
                    p.animation_menu_scope = None;
                    p.menu = (!floating).then_some(5);
                    let mut paint = PaintList::default();
                    let actions = if floating {
                        crate::panels::floating_chrome(
                            &mut paint,
                            &mut atlas,
                            Panel::Timeline,
                            [1000., 800.],
                            &mut p,
                            &state,
                            &Input::default(),
                        )
                    } else {
                        crate::panels::chrome(
                            &mut paint,
                            &mut atlas,
                            [1000., 800.],
                            &dock::Node::dock(Panel::Timeline.dock()),
                            &mut p,
                            &state,
                            &Input::default(),
                        )
                    };
                    assert!(actions.is_empty(), "capture must not edit: {actions:?}");
                    assert_eq!(p.menu, (!floating).then_some(5));
                    let cfg = atlas.config();
                    let file = format!(
                        "{tag}-{}-{}-{phase}.png",
                        if locale == Locale::Zh { "zh" } else { "en" },
                        if floating { "float" } else { "main-menu" }
                    );
                    crate::native_test_support::capture_ui(
                        &mut events,
                        &engine,
                        UiFrame {
                            views: None,
                            paint,
                            atlas: Some(AtlasUpload {
                                pixels: atlas.pixel_data().to_vec(),
                                width: cfg.width,
                                height: cfg.height,
                            }),
                            logical_size: [1000., 800.],
                            composition: None,
                            inspection: Default::default(),
                            composition_image: None,
                            window: u64::from(floating),
                            node_preview: None,
                        },
                        root.join(&file),
                    )
                    .unwrap();
                    let image = image::open(root.join(&file)).unwrap().to_rgba8();
                    assert_eq!(image.dimensions(), (1000, 800));
                    assert!(image.pixels().any(|pixel| pixel != image.get_pixel(0, 0)));
                    let after = engine.state().unwrap();
                    for field in ["project", "revision", "history"] {
                        assert_eq!(after[field], state[field]);
                    }
                    report.push(serde_json::json!({"file": file, "enabled": expected.0, "freeze": expected.1, "state": state}));
                }
                engine.history(0).unwrap();
                assert_eq!(
                    engine.history(0).unwrap()["project"],
                    serde_json::to_value(&initial).unwrap()
                );
            }
        }
        engine.request_shutdown().unwrap();
    }
    std::fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}
