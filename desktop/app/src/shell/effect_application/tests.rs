use super::*;
use motion_ui::{dock::Panel, input::Key, paint::PaintList};
use serde_json::json;
mod recent_menu;

fn fixture(locale: Locale) -> (tempfile::TempDir, Shell, Value) {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("project");
    std::fs::create_dir_all(&root).unwrap();
    let mut project = motion_core::Project::new(96, 64, 30, 60).unwrap();
    for (id, name) in [(1, "One"), (2, "Locked"), (3, "Three")] {
        let mut layer = Layer::solid(id, name, [32.; 2], [48., 32., 0.], [1.; 4]);
        layer.locked = id == 2;
        project.layers.push(layer);
    }
    motion_core::storage::save(&root, &project).unwrap();
    let mut shell = Shell::new(root, None, Some(locale)).unwrap();
    shell.panels.select_only(Some(1));
    let entry = panels::effect_controls::entries(&shell.panels.effect_controls.catalogue, locale)
        .into_iter()
        .find(|item| item.id == "brightness_contrast")
        .unwrap();
    let request = entry.command(1);
    (folder, shell, request)
}

fn search_frame(
    shell: &mut Shell,
    owner: Option<Panel>,
    input: &Input,
) -> (PaintList, Vec<panels::Action>) {
    let mut paint = PaintList::default();
    let actions = if let Some(panel) = owner {
        panels::floating_chrome(
            &mut paint,
            &mut shell.atlas,
            panel,
            [400., 300.],
            &mut shell.panels,
            &shell.state,
            input,
        )
    } else {
        panels::chrome(
            &mut paint,
            &mut shell.atlas,
            [900., 600.],
            &shell.workspace.root,
            &mut shell.panels,
            &shell.state,
            input,
        )
    };
    (paint, actions)
}

#[test]
fn effect_search_applies_atomically_reveals_and_roundtrips_in_both_windows_and_languages() {
    for locale in [Locale::En, Locale::Zh] {
        for owner in [None, Some(Panel::Info)] {
            let (_folder, mut shell, request) = fixture(locale);
            // A long existing stack exercises actual reveal scrolling, not just
            // the first effect in an otherwise empty panel.
            for _ in 0..6 {
                shell.state = shell.engine.plugin(request.to_string()).unwrap();
            }
            shell.panels.add_selection(2);
            shell.panels.add_selection(3);
            shell.panels.selected = Some(1);
            let before = shell.state["project"].clone();
            shell.panels.effect_search = Some(panels::EffectSearch {
                position: [200., 140.],
                query: "brightness_contrast".into(),
                owner,
                continuous: true,
                ..Default::default()
            });
            let (_, actions) = search_frame(
                &mut shell,
                owner,
                &Input {
                    keys: vec![(Key::Enter, Default::default())],
                    ..Default::default()
                },
            );
            assert!(matches!(actions.as_slice(), [panels::Action::Plugin(_)]));
            for action in actions {
                shell.action(action, None).unwrap();
            }
            let applied = shell.state["project"].clone();
            assert_eq!(applied["layers"][0]["effects"].as_array().unwrap().len(), 7);
            assert_eq!(applied["layers"][1]["effects"].as_array().unwrap().len(), 0);
            assert_eq!(applied["layers"][2]["effects"].as_array().unwrap().len(), 1);
            assert_eq!(shell.panels.selection_ids(), vec![1, 2, 3]);
            let search = shell.panels.effect_search.as_ref().unwrap();
            assert_eq!(search.owner, owner);
            assert_eq!(search.query, "brightness_contrast");
            assert!(
                search.continuous && shell.actions.is_empty(),
                "continuous input must not queue a window focus change"
            );
            let message = shell.panels.message.as_deref().unwrap();
            assert!(
                message.contains("One") && message.contains('2'),
                "{message}"
            );
            assert!(message.starts_with(if locale == Locale::Zh {
                "已添加"
            } else {
                "Added"
            }));
            for input in [
                Input::default(),
                Input {
                    keys: vec![(Key::Enter, Default::default())],
                    key_repeats: vec![true],
                    ..Default::default()
                },
            ] {
                assert!(search_frame(&mut shell, owner, &input).1.is_empty());
                assert_eq!(shell.engine.state().unwrap()["project"], applied);
            }
            // Render the actual destination panel. The newest parameters must
            // land inside its viewport after consuming the reveal request.
            let mut paint = PaintList::default();
            assert!(panels::chrome(
                &mut paint,
                &mut shell.atlas,
                [360., 560.],
                &dock::Node::dock(Panel::EffectControls.dock()),
                &mut shell.panels,
                &shell.state,
                &Input::default()
            )
            .is_empty());
            assert!(shell.panels.auxiliary.scroll[&(Panel::EffectControls as u32)] > 0.);
            assert!(shell.panels.effect_controls.test_rects.iter().any(
                |((object, effect, _, _), rect)| *object == 1
                    && *effect == 7
                    && rect.min[1] >= 80.
                    && rect.max[1] < 540.
            ));
            shell.engine.save().unwrap();
            let loaded = motion_core::storage::load(std::path::Path::new(
                shell.state["root"].as_str().unwrap(),
            ))
            .unwrap();
            assert_eq!(serde_json::to_value(loaded).unwrap(), applied);
            shell.state = shell.engine.history(0).unwrap();
            assert_eq!(
                shell.state["project"], before,
                "one undo removes the entire multi-layer addition"
            );
            shell.state = shell.engine.history(1).unwrap();
            assert_eq!(shell.state["project"], applied);
            let snapshot = shell.state["project"].clone();
            assert!(search_frame(
                &mut shell,
                owner,
                &Input {
                    keys: vec![(Key::Escape, Default::default())],
                    ..Default::default()
                }
            )
            .1
            .is_empty());
            assert!(shell.panels.effect_search.is_none());
            assert_eq!(shell.engine.state().unwrap()["project"], snapshot);
        }
    }
}

#[test]
fn effect_add_failure_keeps_project_and_original_error_then_recovers() {
    for locale in [Locale::En, Locale::Zh] {
        let (_folder, mut shell, request) = fixture(locale);
        let before = shell.state["project"].clone();
        for invalid in [
            json!({"object":2}),
            json!({"effect":"missing-effect"}),
            json!({"object":999}),
        ] {
            let mut command = request.clone();
            for (key, value) in invalid.as_object().unwrap() {
                command[key] = value.clone();
            }
            let error = shell
                .action(panels::Action::Plugin(command), None)
                .unwrap_err();
            assert!(shell.panels.message.as_deref().unwrap().contains(&error));
            assert_eq!(shell.state["project"], before);
            assert_eq!(shell.engine.state().unwrap()["project"], before);
            assert!(shell.actions.is_empty());
        }
        shell.action(panels::Action::Plugin(request), None).unwrap();
        assert!(shell
            .panels
            .message
            .as_deref()
            .unwrap()
            .starts_with(if locale == Locale::Zh {
                "已添加"
            } else {
                "Added"
            }));
        assert!(
            matches!(shell.actions.pop_front(), Some(panels::Action::ShowPanel(id))
            if id == Panel::EffectControls.dock())
        );
    }
}

#[test]
fn effect_application_does_not_report_unrelated_additions_from_a_stale_ui_snapshot() {
    let (_folder, mut shell, request) = fixture(Locale::En);
    let mut external = request.clone();
    external["object"] = 3.into();
    shell.engine.plugin(external.to_string()).unwrap();
    // Leave shell.state stale, as when another client edited through MCP.
    assert!(shell.state["project"]["layers"][2]["effects"]
        .as_array()
        .unwrap()
        .is_empty());
    shell.action(panels::Action::Plugin(request), None).unwrap();
    assert_eq!(
        shell.panels.message.as_deref(),
        Some("Added Brightness & Contrast to One")
    );
    assert_eq!(
        shell.state["project"]["layers"][2]["effects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    shell.state = shell.engine.history(0).unwrap();
    assert_eq!(
        shell.state["project"]["layers"][0]["effects"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        shell.state["project"]["layers"][2]["effects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[cfg(target_os = "windows")]
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn effect_search_native_pixels_and_default_failure_sinks() {
    const CHILD: &str = "MOTION_EFFECT_SEARCH_NATIVE_CHILD";
    const TEST: &str =
        "shell::effect_application::tests::effect_search_native_pixels_and_default_failure_sinks";
    if std::env::var_os(CHILD).is_some() {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let artifacts = PathBuf::from(std::env::var_os("MOTION_EFFECT_SEARCH_ARTIFACTS").unwrap());
        std::fs::create_dir_all(&artifacts).unwrap();
        let _logger = crate::logging::init(artifacts.clone()).unwrap();
        let event_loop = winit::event_loop::EventLoop::builder()
            .with_any_thread(true)
            .build()
            .unwrap();
        #[allow(deprecated)]
        let main = Arc::new(
            event_loop
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(900, 600)),
                )
                .unwrap(),
        );
        #[allow(deprecated)]
        let floating = Arc::new(
            event_loop
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(400, 300)),
                )
                .unwrap(),
        );
        let mut receipt = Vec::new();
        for (tag, locale) in [("en", Locale::En), ("zh", Locale::Zh)] {
            let (_folder, mut shell, request) = fixture(locale);
            shell.state = shell
                .engine
                .attach_async(main.clone(), 900, 600)
                .unwrap()
                .recv_timeout(Duration::from_secs(30))
                .unwrap()
                .unwrap();
            shell
                .engine
                .float_window(1, floating.clone(), 400, 300)
                .unwrap();
            for (name, owner, size, key) in [
                ("main", None, [900., 600.], 0),
                ("float", Some(Panel::Info), [400., 300.], 1),
            ] {
                let before = shell.state["project"].clone();
                let chord = crate::shortcuts::EFFECT_SEARCH_ALT;
                let (paint, actions) = search_frame(&mut shell, owner, &Input {
                    mouse: [200., 140.],
                    keys: vec![(chord.key, chord.modifiers)],
                    ..Default::default()
                });
                assert!(actions.is_empty());
                assert_eq!(shell.panels.effect_search.as_ref().unwrap().owner, owner);
                let path = artifacts.join(format!("{tag}-alternate-{name}.png"));
                let mut ui = shell.ui_frame(paint, size, true);
                ui.window = key;
                ui.composition = None;
                ui.composition_image = None;
                ui.node_preview = None;
                shell.engine.capture_ui(ui, path.clone()).unwrap();
                let pixels = image::open(&path).unwrap().to_rgba8();
                assert_eq!([pixels.width(), pixels.height()], [size[0] as u32, size[1] as u32]);
                assert!(pixels.pixels().any(|p| p[0] > 80 && p[1] > 80 && p[2] > 80));
                receipt.push(json!({"path":path,"logicalSize":size,"renderScale":1.,
                    "nativeWindow":key,"osScale":main.scale_factor(),"alternate":true,"continuous":false}));
                let (_, actions) = search_frame(&mut shell, owner, &Input {
                    keys: vec![(chord.key, chord.modifiers), (Key::Delete, Default::default())],
                    ..Default::default()
                });
                assert!(shell.panels.effect_search.is_none());
                assert!(actions.is_empty(), "alternate closing frame escaped into document commands");
                assert_eq!(shell.state["project"], before);
            }
            shell.panels.effect_search = Some(panels::EffectSearch {
                position: [200., 140.],
                query: "brightness".into(),
                ..Default::default()
            });
            assert!(search_frame(
                &mut shell,
                None,
                &Input {
                    keys: vec![(Key::Tab, Default::default())],
                    ..Default::default()
                }
            )
            .1
            .is_empty());
            let (_, actions) = search_frame(
                &mut shell,
                None,
                &Input {
                    keys: vec![(Key::Enter, Default::default())],
                    ..Default::default()
                },
            );
            assert_eq!(actions.len(), 1);
            for action in actions {
                shell.action(action, None).unwrap();
            }
            for (name, owner, size) in [
                ("minimum", None, [900., 600.]),
                ("default", None, [1600., 1000.]),
                ("floating", Some(Panel::Info), [400., 300.]),
                ("narrow", Some(Panel::Info), [220., 180.]),
            ] {
                for scale in [1., 1.5] {
                    let key = if owner.is_some() { 1 } else { 0 };
                    let physical = [(size[0] * scale) as u32, (size[1] * scale) as u32];
                    shell
                        .engine
                        .resize_async(key, physical[0], physical[1])
                        .unwrap()
                        .recv_timeout(Duration::from_secs(20))
                        .unwrap()
                        .unwrap();
                    shell.panels.effect_search.as_mut().unwrap().owner = owner;
                    let mut paint = PaintList::default();
                    let actions = if let Some(panel) = owner {
                        panels::floating_chrome(
                            &mut paint,
                            &mut shell.atlas,
                            panel,
                            size,
                            &mut shell.panels,
                            &shell.state,
                            &Input::default(),
                        )
                    } else {
                        panels::chrome(
                            &mut paint,
                            &mut shell.atlas,
                            size,
                            &shell.workspace.root,
                            &mut shell.panels,
                            &shell.state,
                            &Input::default(),
                        )
                    };
                    assert!(actions.is_empty());
                    let path = artifacts.join(format!("{tag}-{name}-{scale}.png"));
                    let mut ui = shell.ui_frame(paint, size, true);
                    ui.window = key;
                    ui.composition = None;
                    ui.composition_image = None;
                    ui.node_preview = None;
                    shell.engine.capture_ui(ui, path.clone()).unwrap();
                    let pixels = image::open(&path).unwrap().to_rgba8();
                    assert_eq!([pixels.width(), pixels.height()], physical);
                    assert!(
                        pixels.pixels().any(|p| p[0] > 80 && p[1] > 80 && p[2] > 80),
                        "text must produce visible pixels"
                    );
                    receipt.push(json!({"path":path,"logicalSize":size,"renderScale":scale,
                        "nativeWindow":key,"osScale":main.scale_factor(),"continuous":true}));
                }
            }
            let before = shell.state["project"].clone();
            let mut invalid = request;
            invalid["effect"] = "missing-effect".into();
            assert!(shell
                .action(panels::Action::Plugin(invalid), None)
                .unwrap_err()
                .contains("unknown effect"));
            assert_eq!(shell.engine.state().unwrap()["project"], before);
        }
        std::fs::write(
            artifacts.join("receipt.json"),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let artifacts = std::env::var_os("MOTION_EFFECT_SEARCH_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().join("artifacts"));
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST])
        .env(CHILD, "1")
        .env("MOTION_EFFECT_SEARCH_ARTIFACTS", &artifacts)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    std::fs::create_dir_all(&artifacts).unwrap();
    std::fs::write(artifacts.join("stdout.log"), &output.stdout).unwrap();
    std::fs::write(artifacts.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let log = std::fs::read_to_string(artifacts.join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(log.contains("logging initialized level=INFO"));
    assert_eq!(stderr.matches("unknown effect").count(), 2);
    assert_eq!(
        log.lines()
            .filter(|line| line.contains("ERROR") && line.contains("unknown effect"))
            .count(),
        2
    );
    assert!(log.contains("start #") && log.contains(" plugin "));
    assert!(!stdout.contains("unknown effect") && !stdout.contains("logging initialized"));
}
