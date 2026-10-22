//! Real default catalogue, native output and retired-project error boundaries.
#[path = "../../../../crates/motion-effects/tests/support/scene_packages.rs"]
mod scene_fixtures;
use crate::*;

fn capture(shell: &mut Shell, destination: PathBuf) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let (paint, actions) = shell.draw_commands([1280., 800.]);
        assert!(actions.is_empty());
        let ui = shell.ui_frame(paint, [1280., 800.], true);
        if shell
            .engine
            .present_async(ui)
            .unwrap()
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap()
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "catalogue presentation did not complete"
        );
    }
    let (paint, actions) = shell.draw_commands([1280., 800.]);
    assert!(actions.is_empty());
    let ui = shell.ui_frame(paint, [1280., 800.], true);
    shell.engine.capture_ui(ui, destination).unwrap();
}

#[test]
fn retired_effects_leave_catalogue_and_old_project_failures_reach_default_sinks() {
    const CHILD: &str = "MOTION_RETIRED_EFFECT_CHILD";
    const TEST: &str = "shell::retired_effect_tests::retired_effects_leave_catalogue_and_old_project_failures_reach_default_sinks";
    if let Some(root) = std::env::var_os(CHILD) {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let root = PathBuf::from(root);
        let artifacts = PathBuf::from(std::env::var_os("MOTION_RETIRED_EFFECT_ARTIFACTS").unwrap());
        std::fs::create_dir_all(&artifacts).unwrap();
        let _logger = crate::logging::init(artifacts.clone()).unwrap();
        let event_loop = winit::event_loop::EventLoop::builder()
            .with_any_thread(true)
            .build()
            .unwrap();
        // Model the plugin cache created by earlier releases, not just a clean install.
        let cached = root.join("plugins");
        std::fs::create_dir_all(&cached).unwrap();
        let scene = scene_fixtures::scene_package().unwrap();
        let particle = scene_fixtures::particle_package().unwrap();
        for package in [&scene, &particle] {
            std::fs::write(
                cached.join(format!("{}.msfx", package.hash)),
                &package.bytes,
            )
            .unwrap();
        }
        for (tag, locale) in [("zh", Locale::Zh), ("en", Locale::En)] {
            let project_root = root.join(tag);
            let mut project = motion_core::Project::new(128, 128, 30, 120).unwrap();
            project.layers.push(motion_core::Layer::solid(
                1,
                "Independent layer",
                [80.; 2],
                [64., 64., 0.],
                [0.2, 0.5, 0.8, 1.],
            ));
            std::fs::create_dir_all(&project_root).unwrap();
            motion_core::storage::save(&project_root, &project).unwrap();
            let mut shell = Shell::new(project_root, None, Some(locale)).unwrap();
            shell.workspace = workspace::Workspace::default();
            shell.workspace.locale = locale;
            shell.panels.locale = locale;
            shell.panels.select_only(Some(1));
            let catalogue = shell
                .engine
                .plugin(serde_json::json!({"op":"catalogue"}).to_string())
                .unwrap();
            for package in catalogue["packages"].as_array().unwrap() {
                assert!(!motion_effects::builtin::is_retired_plugin(
                    package["manifest"]["id"].as_str().unwrap()
                ));
            }
            std::fs::write(
                artifacts.join(format!("{tag}-catalogue.json")),
                serde_json::to_vec_pretty(&catalogue).unwrap(),
            )
            .unwrap();
            shell.panels.effect_controls.catalogue = catalogue.into();
            #[allow(deprecated)]
            let window = Arc::new(
                event_loop
                    .create_window(
                        winit::window::Window::default_attributes()
                            .with_visible(false)
                            .with_inner_size(winit::dpi::PhysicalSize::new(1280, 800)),
                    )
                    .unwrap(),
            );
            shell.state = shell
                .engine
                .attach_async(window.clone(), 1280, 800)
                .unwrap()
                .recv_timeout(Duration::from_secs(30))
                .unwrap()
                .unwrap();
            shell.window = Some(window);
            shell.attached = true;
            capture(&mut shell, artifacts.join(format!("{tag}-catalogue.png")));
            shell.panels.effect_search = Some(panels::EffectSearch {
                position: [600., 240.],
                query: if tag == "zh" {
                    "镜头光效"
                } else {
                    "particle"
                }
                .into(),
                selected_index: 0,
                owner: None,
                continuous: false,
            });
            capture(
                &mut shell,
                artifacts.join(format!("{tag}-retired-search.png")),
            );
        }
        // Loading and saving never strips an unavailable effect or its animation.
        let project_root = root.join("old-project");
        let mut project = motion_core::Project::new(128, 128, 30, 120).unwrap();
        project.version = motion_model::CURRENT_PROJECT_VERSION;
        let mut layer =
            motion_core::Layer::solid(1, "Saved emitter", [128.; 2], [64., 64., 0.], [0.; 4]);
        layer.effects.push(motion_core::EffectInstance::new(
            1,
            &particle.manifest.id,
            &particle.manifest.version,
            &particle.hash,
            &particle.manifest.effects[0],
            [128.; 2],
        ));
        layer.effects[0].params.get_mut("position").unwrap().track.keys =
            [(0, [0., 0., 0., 0.]), (60, [25., 10., 0., 0.])]
                .into_iter()
                .map(|(frame, value)| motion_core::Keyframe {
                    frame,
                    value,
                    ease: motion_core::Ease::Linear,
                    curve: None,
                    spatial: None,
                })
                .collect();
        project.layers.push(layer);
        project.rebuild_plugin_dependencies();
        motion_core::storage::save(&project_root, &project).unwrap();
        let engine = engine::Engine::start(
            project_root.clone(),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let before = engine.state().unwrap();
        let error = engine
            .plugin(serde_json::json!({"op":"editor_open","object":1,"instance":1}).to_string())
            .unwrap_err();
        assert!(
            error.contains("retired bundled plugin") && error.contains(&particle.hash),
            "{error}"
        );
        let error = engine.plugin(serde_json::json!({"op":"install","path":cached.join(format!("{}.msfx", particle.hash))}).to_string()).unwrap_err();
        assert!(
            error.contains("retired bundled plugin") && error.contains(&particle.manifest.id),
            "{error}"
        );
        assert_eq!(engine.state().unwrap()["project"], before["project"]);
        assert!(!engine.failed());
        engine.save().unwrap();
        assert_eq!(motion_core::storage::load(&project_root).unwrap(), project);
        let remove = motion_core::Command::Effect {
            object: 1,
            action: motion_core::EffectAction::Remove { effect: 1 },
        };
        engine
            .edit(serde_json::to_string(&remove).unwrap())
            .unwrap();
        engine.history(0).unwrap();
        engine.save().unwrap();
        assert_eq!(motion_core::storage::load(&project_root).unwrap(), project);
        for package in [&scene, &particle] {
            assert_eq!(
                std::fs::read(cached.join(format!("{}.msfx", package.hash))).unwrap(),
                package.bytes
            );
        }
        drop(engine);
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let artifacts = std::env::var_os("MOTION_RETIRED_EFFECT_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.path().join("artifacts"));
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST])
        .env(CHILD, root.path())
        .env("MOTION_RETIRED_EFFECT_ARTIFACTS", &artifacts)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    std::fs::create_dir_all(&artifacts).unwrap();
    std::fs::write(artifacts.join("stderr.log"), &output.stderr).unwrap();
    std::fs::write(artifacts.join("stdout.log"), &output.stdout).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(artifacts.join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(file.contains("logging initialized level=INFO"));
    let errors: Vec<_> = file
        .lines()
        .filter(|line| line.contains("retired bundled plugin"))
        .collect();
    assert_eq!(errors.len(), 2, "{file}");
    for line in errors {
        assert!(
            line.contains("ERROR")
                && line.contains("error #")
                && line.contains(" plugin ")
                && line.contains("ms:"),
            "{line}"
        );
        assert!(stderr.contains(line.split("motion_studio::engine: ").nth(1).unwrap()));
    }
    assert_eq!(stderr.matches("retired bundled plugin").count(), 2);
    assert!(!stdout.contains("retired bundled plugin") && !stdout.contains("logging initialized"));
}
