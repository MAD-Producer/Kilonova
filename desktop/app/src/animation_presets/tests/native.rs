//! Real Windows surfaces and production chrome. Explicit Input exercises routing;
//! this does not claim physical keyboard injection or native file-dialog automation.
use super::*;
use crate::{
    engine::{AtlasUpload, UiFrame},
    native_test_support,
    panels::{self, Action},
};
use motion_ui::{
    dock::{self, Panel},
    input::{Input, Key, MouseButton, Rect},
    PaintList,
};
use winit::{
    dpi::PhysicalSize, event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows,
    window::Window,
};
fn click(rect: Rect, button: MouseButton) -> Input {
    let point = [
        (rect.min[0] + rect.max[0]) / 2.,
        (rect.min[1] + rect.max[1]) / 2.,
    ];
    Input {
        mouse: point,
        pressed: Some((button, point)),
        ..Default::default()
    }
}
fn frame(
    shell: &mut Shell,
    window: u64,
    size: [f32; 2],
    panel: Panel,
    input: &Input,
) -> (PaintList, Vec<Action>) {
    let mut paint = PaintList::default();
    paint.stylesheet = shell.panels.styles.clone();
    let actions = if window == 0 {
        panels::chrome(
            &mut paint,
            &mut shell.atlas,
            size,
            &dock::Node::dock(panel.dock()),
            &mut shell.panels,
            &shell.state,
            input,
        )
    } else {
        panels::floating_chrome(
            &mut paint,
            &mut shell.atlas,
            panel,
            size,
            &mut shell.panels,
            &shell.state,
            input,
        )
    };
    (paint, actions)
}
fn capture(
    events: &mut EventLoop<()>,
    shell: &mut Shell,
    window: u64,
    size: [f32; 2],
    scale: f32,
    panel: Panel,
    path: PathBuf,
) {
    let (paint, actions) = frame(shell, window, size, panel, &Input::default());
    assert!(actions.is_empty(), "{actions:?}");
    let config = shell.atlas.config();
    let ui = UiFrame { views: None,
        paint,
        atlas: Some(AtlasUpload {
            pixels: shell.atlas.pixel_data().to_vec(),
            width: config.width,
            height: config.height,
        }),
        logical_size: size,
        composition: None,
        inspection: Default::default(),
        composition_image: None,
        window,
        node_preview: None,
    };
    let front = shell
        .engine
        .present_async(UiFrame { views: None,
            paint: PaintList::default(),
            atlas: None,
            logical_size: size,
            composition: None,
            inspection: Default::default(),
            composition_image: None,
            window,
            node_preview: None,
        })
        .unwrap();
    assert!(
        native_test_support::receive(events, &front, Duration::from_secs(30))
            .unwrap()
            .unwrap()
    );
    native_test_support::capture_ui(events, &shell.engine, ui, path.clone()).unwrap();
    assert_eq!(
        image::open(path).unwrap().to_rgba8().dimensions(),
        ((size[0] * scale) as u32, (size[1] * scale) as u32)
    );
}
fn menu_row(shell: &Shell, index: usize) -> Rect {
    shell
        .panels
        .browser
        .library_row(&format!("__context:{index}"))
        .unwrap()
}
#[test]
#[ignore = "manual native screenshots; run explicitly with MOTION_ANIMATION_PRESET_UI_ARTIFACTS"]
fn native_preset_and_effect_context_menus_use_bilingual_main_and_floating_windows() {
    const CHILD: &str = "MOTION_ANIMATION_PRESET_NATIVE_CHILD";
    const NAME: &str = "animation_presets::tests::native::native_preset_and_effect_context_menus_use_bilingual_main_and_floating_windows";
    let Some(root) = std::env::var_os(CHILD) else {
        let temporary = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_ANIMATION_PRESET_UI_ARTIFACTS")
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
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.join("logs")).unwrap();
    let mut shell = fixture(&root.join("project"), Locale::Zh);
    shell.layout_path = root.join("workspace.json");
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let main = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(900, 600)),
            )
            .unwrap(),
    );
    #[allow(deprecated)]
    let float = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(600, 750)),
            )
            .unwrap(),
    );
    let attach = shell.engine.attach_async(main.clone(), 900, 600).unwrap();
    shell.state = native_test_support::receive(&mut events, &attach, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    shell.window = Some(main);
    shell
        .engine
        .float_window(1, float.clone(), 600, 750)
        .unwrap();
    for (window, size, scale, locale, name) in [
        (0, [900., 600.], 1., Locale::Zh, "zh-main"),
        (1, [400., 500.], 1.5, Locale::En, "en-float"),
    ] {
        shell.workspace.locale = locale;
        shell.panels.locale = locale;
        shell.scale = scale;
        shell.atlas.set_pixel_ratio(scale);
        shell.panels.select_only(Some(7));
        shell.panels.add_selection(9);
        shell.panels.selected = Some(7);
        let entry =
            panels::effect_controls::entries(&shell.panels.effect_controls.catalogue, locale)
                .into_iter()
                .find(|e| e.id == "brightness_contrast")
                .unwrap();
        shell
            .panels
            .browser
            .locate_library_effect(entry.key.clone());
        let (_, actions) = frame(&mut shell, window, size, Panel::Effects, &Input::default());
        assert!(actions.is_empty());
        let rect = shell.panels.browser.library_row(&entry.key).unwrap();
        let (_, actions) = frame(
            &mut shell,
            window,
            size,
            Panel::Effects,
            &click(rect, MouseButton::Right),
        );
        assert!(actions.is_empty());
        capture(
            &mut events,
            &mut shell,
            window,
            size,
            scale,
            Panel::Effects,
            root.join(format!("{name}-effect-library-menu.png")),
        );
        let (foreign_window, foreign_size) = if window == 0 {
            (1, [400., 500.])
        } else {
            (0, [900., 600.])
        };
        assert!(frame(
            &mut shell,
            foreign_window,
            foreign_size,
            Panel::Effects,
            &Input::default()
        )
        .1
        .is_empty());
        assert!(
            shell.panels.browser.editing(),
            "passive foreign repaint must retain the owning popup"
        );
        assert!(
            frame(
                &mut shell,
                foreign_window,
                foreign_size,
                Panel::Effects,
                &Input {
                    keys: vec![(Key::Space, Default::default())],
                    ..Default::default()
                }
            )
            .1
            .is_empty(),
            "foreign dismissal must not start playback"
        );
        assert!(!shell.panels.browser.editing());
        frame(
            &mut shell,
            window,
            size,
            Panel::Effects,
            &click(rect, MouseButton::Right),
        );
        let favorite = menu_row(&shell, 2);
        let favorite_before = shell.workspace.favorites.contains(&entry.key);
        let (_, actions) = frame(
            &mut shell,
            window,
            size,
            Panel::Effects,
            &click(favorite, MouseButton::Left),
        );
        assert!(matches!(actions.as_slice(), [Action::Favorites(_)]));
        for action in actions {
            shell.action(action, None).unwrap();
        }
        assert_eq!(
            shell.workspace.favorites.contains(&entry.key),
            !favorite_before
        );
        frame(
            &mut shell,
            window,
            size,
            Panel::Effects,
            &click(rect, MouseButton::Right),
        );
        let apply = menu_row(&shell, 1);
        let (_, actions) = frame(
            &mut shell,
            window,
            size,
            Panel::Effects,
            &click(apply, MouseButton::Left),
        );
        assert!(
            matches!(actions.as_slice(), [Action::Plugin(request)] if request["objects"] == serde_json::json!([7,9])),
            "actions={actions:?} selection={:?}",
            shell.panels.selection_ids()
        );
        for action in actions {
            shell.action(action, None).unwrap();
        }
        shell.state = shell.engine.state().unwrap();
        assert!(!shell.state["project"]["layers"][0]["effects"]
            .as_array()
            .unwrap()
            .is_empty());
        shell.panels.select_only(Some(7));
        frame(
            &mut shell,
            window,
            size,
            Panel::EffectControls,
            &Input::default(),
        );
        let id = shell.state["project"]["layers"][0]["effects"][0]["id"]
            .as_u64()
            .unwrap();
        let more = shell.panels.effect_controls.test_rects[&(7, id, "__actions".into(), 0)];
        frame(
            &mut shell,
            window,
            size,
            Panel::EffectControls,
            &click(more, MouseButton::Left),
        );
        capture(
            &mut events,
            &mut shell,
            window,
            size,
            scale,
            Panel::EffectControls,
            root.join(format!("{name}-effect-instance-menu.png")),
        );
        assert!(frame(
            &mut shell,
            foreign_window,
            foreign_size,
            Panel::Effects,
            &Input::default()
        )
        .1
        .is_empty());
        assert!(shell.panels.effect_controls.editing());
        assert!(frame(
            &mut shell,
            foreign_window,
            foreign_size,
            Panel::Effects,
            &Input {
                keys: vec![(Key::Space, Default::default())],
                ..Default::default()
            }
        )
        .1
        .is_empty());
        assert!(!shell.panels.effect_controls.editing());
        frame(
            &mut shell,
            window,
            size,
            Panel::EffectControls,
            &click(more, MouseButton::Left),
        );
        let save = shell.panels.effect_controls.test_rects[&(7, id, "__menu".into(), 6)];
        let (_, actions) = frame(
            &mut shell,
            window,
            size,
            Panel::EffectControls,
            &click(save, MouseButton::Left),
        );
        assert!(
            matches!(actions.as_slice(), [Action::SaveAnimationPreset { object: 7, effects: Some(ids) }] if ids == &[id])
        );
        // File-dialog selection is supplied by the user in production. The same
        // worker path is separately verified by the non-native transaction test.
        let path = root.join(format!("{name}.msanim"));
        recipe(&shell, &path);
        shell.panels.browser.presets = vec![Entry {
            path: path.clone(),
            name: "像素动画 / Pixel motion".into(),
            category: "Motion".into(),
        }];
        frame(&mut shell, window, size, Panel::Effects, &Input::default());
        let preset_tab = shell.panels.browser.library_row("__tab:3").unwrap();
        frame(
            &mut shell,
            window,
            size,
            Panel::Effects,
            &click(preset_tab, MouseButton::Left),
        );
        shell.panels.select_only(Some(9));
        capture(
            &mut events,
            &mut shell,
            window,
            size,
            scale,
            Panel::Effects,
            root.join(format!("{name}-preset-list.png")),
        );
        let row = shell
            .panels
            .browser
            .library_row(&path.to_string_lossy())
            .unwrap();
        frame(
            &mut shell,
            window,
            size,
            Panel::Effects,
            &click(row, MouseButton::Right),
        );
        capture(
            &mut events,
            &mut shell,
            window,
            size,
            scale,
            Panel::Effects,
            root.join(format!("{name}-preset-menu.png")),
        );
        let apply = menu_row(&shell, 1);
        let mut composing = click(apply, MouseButton::Left);
        composing.composing = true;
        assert!(frame(&mut shell, window, size, Panel::Effects, &composing)
            .1
            .is_empty());
        assert!(shell.panels.browser.editing());
        assert!(frame(
            &mut shell,
            window,
            size,
            Panel::Effects,
            &Input {
                keys: vec![(Key::Space, Default::default())],
                ..Default::default()
            }
        )
        .1
        .is_empty());
        assert!(frame(
            &mut shell,
            window,
            size,
            Panel::Effects,
            &Input {
                focus_lost: true,
                ..Default::default()
            }
        )
        .1
        .is_empty());
        assert!(!shell.panels.browser.editing());
        frame(
            &mut shell,
            window,
            size,
            Panel::Effects,
            &click(row, MouseButton::Right),
        );
        let (_, actions) = frame(
            &mut shell,
            window,
            size,
            Panel::Effects,
            &click(apply, MouseButton::Left),
        );
        assert!(
            matches!(actions.as_slice(), [Action::ApplyAnimationPreset { objects, .. }] if objects == &[9])
        );
        for action in actions {
            shell.action(action, None).unwrap();
        }
        let began = Instant::now();
        while shell.animation_presets.busy() {
            assert!(
                began.elapsed() < Duration::from_secs(30),
                "phase={:?} queued_actions={:?}",
                shell.animation_presets.phase(),
                shell.actions
            );
            // Production dispatches queued ShowPanel feedback before committing
            // a prepared edit. This fixture renders chrome directly, so service
            // those real actions instead of bypassing the commit guard.
            if !shell.animation_presets.committing() {
                while let Some(action) = shell.actions.pop_front() {
                    shell.action(action, None).unwrap();
                }
            }
            shell.poll_animation_presets();
            let (_hold, empty) = mpsc::channel::<()>();
            let _ = native_test_support::receive(&mut events, &empty, Duration::from_millis(1));
        }
        assert!(shell
            .panels
            .message
            .as_ref()
            .unwrap()
            .contains(if locale == Locale::Zh {
                "已应用"
            } else {
                "applied"
            }));
        assert_eq!(
            shell.state["project"]["layers"][1]["transform"]["anchor"],
            serde_json::json!([0.125, 0.25])
        );
        frame(
            &mut shell,
            window,
            size,
            Panel::Effects,
            &Input {
                keys: vec![(Key::Escape, Default::default())],
                ..Default::default()
            },
        );
    }
    shell.engine.close_window(1).unwrap();
    drop(float);
}
