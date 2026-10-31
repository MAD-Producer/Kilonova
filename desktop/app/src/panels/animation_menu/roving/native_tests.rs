//! Native GPU menu captures and real worker conversion, without OS focus theft.
use super::super::tests::context;
use super::*;
use crate::engine::{AtlasUpload, Command as WorkerCommand, Engine, UiFrame};
use motion_ui::{dock, paint::PaintList, text::TextAtlas};
use std::{path::PathBuf, time::Duration};
use winit::{
    dpi::PhysicalSize, event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows,
    window::Window,
};

#[test]
#[ignore = "manual native Roving menu GPU captures with real worker transactions"]
fn native_roving_menu_captures_conversion_fixed_and_mixed_states_in_both_languages() {
    const CHILD: &str = "MOTION_ROVING_MENU_NATIVE_CHILD";
    const NAME:&str="panels::animation_menu::roving::native_tests::native_roving_menu_captures_conversion_fixed_and_mixed_states_in_both_languages";
    let Some(root) = std::env::var_os(CHILD) else {
        let temporary = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_ROVING_MENU_NATIVE_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| temporary.path().into());
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
                .filter(|f| f.path().extension().is_some_and(|e| e == "png"))
                .count(),
            8
        );
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.join("logs")).unwrap();
    let initial = super::tests::fixture().snapshot();
    let project = root.join("project");
    motion_core::storage::save(&project, &initial).unwrap();
    let engine = Engine::start(project, motion_desktop_media::platform(), 8 << 30).unwrap();
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
    let reply = engine.seek_async(22.).unwrap();
    crate::native_test_support::receive(&mut events, &reply, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let mut atlas = TextAtlas::from_family(
        motion_ui::style::default_stylesheet_arc().font_family(),
        12.,
        true,
    )
    .unwrap();
    let target = KeyframeTarget::Property {
        object: 7,
        property: Property::Position,
        axis: None,
    };
    let mut report = vec![];
    for locale in [Locale::Zh, Locale::En] {
        for phase in ["ordinary", "roving", "fixed", "mixed"] {
            let mut state = engine.state().unwrap();
            let mut p = context(&state, locale);
            let mut changes = 0;
            let operations = match phase {
                "ordinary" => vec![],
                "fixed" => vec![true, false],
                _ => vec![true],
            };
            for desired in operations {
                let mut actions = vec![];
                super::super::apply(&mut p, &state, Operation::Roving(desired), &mut actions);
                let Action::EditorAction { source, commands } = actions.pop().unwrap() else {
                    panic!("one typed editor action");
                };
                assert!(actions.is_empty());
                let receive = engine
                    .request(|reply| WorkerCommand::EditorAction {
                        source,
                        commands,
                        reply,
                    })
                    .unwrap();
                state = crate::native_test_support::receive(
                    &mut events,
                    &receive,
                    Duration::from_secs(30),
                )
                .unwrap()
                .unwrap();
                p.normalize_selection(&state);
                changes += 1;
                assert_eq!(state["history"]["undoEntries"], changes);
                assert_eq!(
                    p.selected_keys.spatial_selections(),
                    [(target.clone(), vec![2])]
                );
            }
            if phase == "mixed" {
                p.selected_keys
                    .apply_addressed(
                        &state,
                        [1, 2].map(|id| {
                            SpatialKey {
                                target: target.clone(),
                                id,
                            }
                            .into()
                        }),
                        key_selection::Mode::Replace,
                    )
                    .unwrap();
            }
            let prepared = plans(&p, &state);
            let enabled = super::super::enabled(&p, &state, Operation::Roving(true));
            let freeze = super::super::enabled(&p, &state, Operation::Roving(false));
            assert_eq!(
                (enabled, freeze),
                match phase {
                    "ordinary" | "fixed" => (true, false),
                    _ => (false, true),
                }
            );
            assert_eq!(prepared.all_roving(), phase == "roving");
            assert_eq!(prepared.partial(), phase == "mixed");
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
            assert!(actions.is_empty());
            let cfg = atlas.config();
            let name = format!(
                "{}-{phase}.png",
                if locale == Locale::Zh { "zh" } else { "en" }
            );
            crate::native_test_support::capture_ui(
                &mut events,
                &engine,
                UiFrame {
                    paint,
                    atlas: Some(AtlasUpload {
                        pixels: atlas.pixel_data().to_vec(),
                        width: cfg.width,
                        height: cfg.height,
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
            assert_eq!(image.dimensions(), (1280, 960));
            assert!(image.pixels().any(|p| p != image.get_pixel(0, 0)));
            let after = engine.state().unwrap();
            for field in ["project", "revision", "history"] {
                assert_eq!(after[field], state[field]);
            }
            report.push(serde_json::json!({"file":name,"title":prepared.title(locale),"enable":enabled,"freeze":freeze,"checked":prepared.all_roving(),"undoEntries":changes}));
            for _ in 0..changes {
                engine.history(0).unwrap();
            }
            assert_eq!(
                engine.state().unwrap()["project"],
                serde_json::to_value(&initial).unwrap()
            );
        }
    }
    std::fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    engine.request_shutdown().unwrap();
}
