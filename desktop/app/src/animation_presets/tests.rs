use super::*;
use crate::{i18n::Locale, layer_clipboard::KeyframeSource, Shell};
use std::{path::Path, time::Duration};
#[cfg(target_os = "windows")]
mod native;
fn fixture(root: &Path, locale: Locale) -> Shell {
    std::fs::create_dir_all(root).unwrap();
    let mut p = motion_core::Project::new(320, 180, 30, 180).unwrap();
    for id in [7, 9, 11] {
        let mut l = motion_core::Layer::solid(
            id,
            "Preset study",
            if id == 7 { [200., 100.] } else { [400., 300.] },
            [80., 60., 0.],
            [0.2, 0.4, 0.8, 1.],
        );
        l.locked = id == 11;
        p.layers.push(l);
    }
    p.layers[0]
        .transform
        .position
        .set_animated(10, true)
        .unwrap();
    p.layers[0]
        .transform
        .position
        .set_at(40, [200., 80., 0.])
        .unwrap();
    p.layers[0].transform.anchor.value = [0.25, 0.75];
    motion_core::storage::save(root, &p).unwrap();
    let mut shell = Shell::new(root.into(), None, Some(locale)).unwrap();
    shell.home = None;
    shell.panels.select_only(Some(7));
    shell.animation_presets.initialized = true;
    shell
}
fn recipe(shell: &Shell, path: &Path) {
    let p: motion_core::Project = serde_json::from_value(shell.state["project"].clone()).unwrap();
    let preset = AnimationPreset::capture(
        &p,
        7,
        "像素动画".into(),
        "Motion".into(),
        &[Property::Anchor, Property::Position],
        &[],
    )
    .unwrap();
    motion_core::storage::save_animation_preset(
        Path::new(shell.state["root"].as_str().unwrap()),
        &preset,
        path,
        &AtomicBool::new(false),
    )
    .unwrap();
}
fn wait(controller: &mut Controller, engine: &Engine, allow: bool) -> Result<Completion, String> {
    let began = Instant::now();
    loop {
        assert!(
            began.elapsed() < Duration::from_secs(30),
            "preset hung at {:?}",
            controller.phase()
        );
        if let Some(result) = controller.poll(engine, allow) {
            return result;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn ready(controller: &mut Controller, engine: &Engine) {
    let began = Instant::now();
    while controller.phase() != Some("ready") {
        assert!(
            began.elapsed() < Duration::from_secs(30),
            "preset hung at {:?}",
            controller.phase()
        );
        assert!(controller.poll(engine, false).is_none());
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn worker_save_catalog_apply_undo_redo_and_reopen_use_real_engine_transactions() {
    for locale in [Locale::Zh, Locale::En] {
        let folder = tempfile::tempdir().unwrap();
        let mut shell = fixture(&folder.path().join("project"), locale);
        let catalog = folder.path().join("presets");
        std::fs::create_dir_all(catalog.join("Motion")).unwrap();
        let destination = catalog.join("Motion/移动.msanim");
        shell
            .animation_presets
            .start(
                &shell.engine,
                Intent::Save {
                    object: 7,
                    properties: vec![Property::Position, Property::Anchor],
                    effects: vec![],
                    destination: destination.clone(),
                },
                KeyframeSource::of(&shell.state),
            )
            .unwrap();
        assert!(matches!(
            wait(&mut shell.animation_presets, &shell.engine, true).unwrap(),
            Completion::Saved(_)
        ));
        shell.animation_presets.scan(catalog).unwrap();
        let Completion::Catalog { entries, failures } =
            wait(&mut shell.animation_presets, &shell.engine, true).unwrap()
        else {
            panic!("missing catalog")
        };
        assert_eq!(failures, 0);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].category, "Motion");
        shell.state = shell.engine.seek(30.).unwrap();
        let before = shell.state["project"].clone();
        shell
            .animation_presets
            .start(
                &shell.engine,
                Intent::Apply {
                    objects: vec![9],
                    source: destination,
                },
                KeyframeSource::of(&shell.state),
            )
            .unwrap();
        let Completion::Applied(state) =
            wait(&mut shell.animation_presets, &shell.engine, true).unwrap()
        else {
            panic!("not applied")
        };
        assert_eq!(
            state["project"]["layers"][1]["transform"]["position"]["keys"][0]["frame"],
            30
        );
        assert_eq!(
            state["project"]["layers"][1]["transform"]["anchor"],
            serde_json::json!([0.125, 0.25])
        );
        let after = state["project"].clone();
        assert_eq!(shell.engine.history(0).unwrap()["project"], before);
        assert_eq!(shell.engine.history(1).unwrap()["project"], after);
        shell.engine.save().unwrap();
        assert_eq!(
            serde_json::to_value(
                motion_core::storage::load(&folder.path().join("project")).unwrap()
            )
            .unwrap(),
            after
        );
    }
}
#[test]
fn stale_snapshot_stale_commit_and_explicit_cancel_do_not_write_history() {
    let folder = tempfile::tempdir().unwrap();
    let mut shell = fixture(&folder.path().join("project"), Locale::En);
    let path = folder.path().join("test.msanim");
    recipe(&shell, &path);
    let source = KeyframeSource::of(&shell.state);
    shell.state = shell
        .engine
        .edit(serde_json::json!({"op":"rename","object":9,"name":"Changed"}).to_string())
        .unwrap();
    let mut c = Controller::default();
    c.start(
        &shell.engine,
        Intent::Apply {
            objects: vec![9],
            source: path.clone(),
        },
        source,
    )
    .unwrap();
    assert!(matches!(
        wait(&mut c, &shell.engine, true).unwrap(),
        Completion::Superseded
    ));
    for cancel in [false, true] {
        shell.state = shell.engine.state().unwrap();
        c.start(
            &shell.engine,
            Intent::Apply {
                objects: vec![9],
                source: path.clone(),
            },
            KeyframeSource::of(&shell.state),
        )
        .unwrap();
        ready(&mut c, &shell.engine);
        if cancel {
            c.cancel();
        } else {
            shell.state = shell
                .engine
                .edit(
                    serde_json::json!({"op":"rename","object":9,"name":"Changed again"})
                        .to_string(),
                )
                .unwrap();
        }
        let before = shell.engine.state().unwrap();
        let result = wait(&mut c, &shell.engine, true).unwrap();
        assert!(if cancel {
            matches!(result, Completion::Cancelled)
        } else {
            matches!(result, Completion::Superseded)
        });
        let after = shell.engine.state().unwrap();
        assert_eq!(after["project"], before["project"]);
        assert_eq!(after["revision"], before["revision"]);
    }
}
#[test]
fn actual_missing_file_locked_target_and_schema_failure_reach_default_error_channels() {
    const CHILD: &str = "MOTION_ANIMATION_PRESET_FAILURE_CHILD";
    const NAME: &str = "animation_presets::tests::actual_missing_file_locked_target_and_schema_failure_reach_default_error_channels";
    let Some(root) = std::env::var_os(CHILD) else {
        let folder = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_ANIMATION_PRESET_FAILURE_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| folder.path().into());
        std::fs::create_dir_all(&root).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture"])
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
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stdout.contains("animation preset failed")
                && !stdout.contains("target layer is locked")
        );
        assert!(
            stderr.contains("animation preset failed")
                && stderr.contains("target layer is locked")
                && stderr.contains("controlled preset worker panic")
        );
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.join("logs")).unwrap();
    let cancelled = Job::start(
        Work::Scan(root.join("cancelled-catalog")),
        Arc::new(AtomicBool::new(true)),
    )
    .unwrap();
    assert!(matches!(
        cancelled
            .receive
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap(),
        Output::Complete(Completion::Cancelled)
    ));
    log::logger().flush();
    assert!(
        !std::fs::read_to_string(root.join("logs/motion-studio.log"))
            .unwrap()
            .contains(" ERROR ")
    );
    let (entered, receive) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let job = Job::start_with(
        Work::Scan(root.join("blocked-catalog")),
        Arc::new(AtomicBool::new(false)),
        move || {
            entered.send(()).unwrap();
            gate.recv().unwrap();
        },
    )
    .unwrap();
    receive.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(job.phase(), "io");
    assert!(!root.join("blocked-catalog").exists());
    let start = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    assert!(
        start.contains("animation preset start") && start.contains("operation=scan phase=queued")
    );
    release.send(()).unwrap();
    job.receive
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let panicked = Job::start_with(
        Work::Scan(root.join("panic-catalog")),
        Arc::new(AtomicBool::new(false)),
        || panic!("controlled preset worker panic"),
    )
    .unwrap();
    assert!(panicked
        .receive
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .err()
        .unwrap()
        .contains("controlled preset worker panic"));
    assert_eq!(panicked.phase(), "finished");
    assert!(!root.join("panic-catalog").exists());
    let mut shell = fixture(&root.join("project"), Locale::En);
    let path = root.join("test.msanim");
    recipe(&shell, &path);
    for (objects, source) in [
        (vec![9], root.join("missing.msanim")),
        (vec![9, 11], path.clone()),
    ] {
        let before = shell.engine.state().unwrap();
        shell
            .animation_presets
            .start(
                &shell.engine,
                Intent::Apply { objects, source },
                KeyframeSource::of(&before),
            )
            .unwrap();
        assert!(wait(&mut shell.animation_presets, &shell.engine, true).is_err());
        let after = shell.engine.state().unwrap();
        assert_eq!(after["project"], before["project"]);
        assert_eq!(after["revision"], before["revision"]);
    }
    let package = motion_effects::builtin::package().unwrap();
    let effect = motion_core::EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        package
            .manifest
            .effects
            .iter()
            .find(|d| d.id == "brightness_contrast")
            .unwrap(),
        [200., 100.],
    );
    let mut p: motion_core::Project =
        serde_json::from_value(shell.state["project"].clone()).unwrap();
    p.layers[0].effects.push(effect);
    p.rebuild_plugin_dependencies();
    let mut preset =
        AnimationPreset::capture(&p, 7, "参数".into(), "测试".into(), &[], &[1]).unwrap();
    preset.effects[0]
        .params
        .values_mut()
        .next()
        .unwrap()
        .implemented ^= true;
    let bad = root.join("schema.msanim");
    motion_core::storage::save_animation_preset(
        &root.join("project"),
        &preset,
        &bad,
        &AtomicBool::new(false),
    )
    .unwrap();
    shell
        .animation_presets
        .start(
            &shell.engine,
            Intent::Apply {
                objects: vec![9],
                source: bad,
            },
            KeyframeSource::of(&shell.state),
        )
        .unwrap();
    assert!(wait(&mut shell.animation_presets, &shell.engine, true)
        .err()
        .unwrap()
        .contains("schema mismatch"));
    let after = shell.engine.state().unwrap();
    assert_eq!(after["revision"], shell.state["revision"]);
    preset.effects[0]
        .params
        .values_mut()
        .next()
        .unwrap()
        .implemented ^= true;
    preset.effects[0].plugin = "org.motion.missing-preset-test".into();
    preset.dependencies = vec![preset.effects[0].dependency()];
    let missing = root.join("dependency.msanim");
    motion_core::storage::save_animation_preset(
        &root.join("project"),
        &preset,
        &missing,
        &AtomicBool::new(false),
    )
    .unwrap();
    shell
        .animation_presets
        .start(
            &shell.engine,
            Intent::Apply {
                objects: vec![9],
                source: missing,
            },
            KeyframeSource::of(&shell.state),
        )
        .unwrap();
    assert!(wait(&mut shell.animation_presets, &shell.engine, true)
        .err()
        .unwrap()
        .contains("missing plugin"));
    assert_eq!(shell.engine.state().unwrap()["revision"], after["revision"]);

    let mut p: motion_core::Project =
        serde_json::from_value(shell.state["project"].clone()).unwrap();
    let mut effect = motion_core::EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        package
            .manifest
            .effects
            .iter()
            .find(|e| e.id == "displacement_map")
            .unwrap(),
        [200., 100.],
    );
    effect.image_input = Some(motion_core::EffectImageInput::Asset { asset: 77 });
    p.layers[0].effects.push(effect);
    p.assets.push(motion_core::Asset {
        id: 77,
        path: "assets/broken.png".into(),
        width: 4,
        height: 4,
    });
    p.rebuild_plugin_dependencies();
    std::fs::create_dir_all(root.join("project/assets")).unwrap();
    std::fs::write(
        root.join("project/assets/broken.png"),
        b"invalid independent image fixture",
    )
    .unwrap();
    let image_preset =
        AnimationPreset::capture(&p, 7, "资源".into(), "测试".into(), &[], &[1]).unwrap();
    let image_package = root.join("image.msanim");
    motion_core::storage::save_animation_preset(
        &root.join("project"),
        &image_preset,
        &image_package,
        &AtomicBool::new(false),
    )
    .unwrap();
    shell
        .animation_presets
        .start(
            &shell.engine,
            Intent::Apply {
                objects: vec![9],
                source: image_package.clone(),
            },
            KeyframeSource::of(&shell.state),
        )
        .unwrap();
    assert!(wait(&mut shell.animation_presets, &shell.engine, true)
        .err()
        .unwrap()
        .contains("preset image decode"));
    assert_eq!(shell.engine.state().unwrap()["revision"], after["revision"]);
    assert_eq!(
        std::fs::read_dir(root.join("project/assets"))
            .unwrap()
            .count(),
        1
    );

    image::RgbaImage::from_pixel(4, 4, image::Rgba([100, 120, 150, 255]))
        .save(root.join("project/assets/broken.png"))
        .unwrap();
    motion_core::storage::save_animation_preset(
        &root.join("project"),
        &image_preset,
        &image_package,
        &AtomicBool::new(false),
    )
    .unwrap();
    shell
        .animation_presets
        .start(
            &shell.engine,
            Intent::Apply {
                objects: vec![9],
                source: image_package,
            },
            KeyframeSource::of(&shell.state),
        )
        .unwrap();
    assert!(matches!(
        wait(&mut shell.animation_presets, &shell.engine, true).unwrap(),
        Completion::Applied(_)
    ));
    assert_eq!(
        std::fs::read_dir(root.join("project/assets"))
            .unwrap()
            .count(),
        2
    );
    log::logger().flush();
    let log = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    for needle in [
        "animation preset start",
        "operation=apply",
        "phase=queued",
        "missing.msanim",
        "target layer is locked",
        "schema mismatch",
        "missing plugin",
        "preset image decode",
        "controlled preset worker panic",
    ] {
        assert!(log.contains(needle), "missing {needle}");
    }
}
#[test]
fn late_cancel_preserves_published_save_and_does_not_mask_an_independent_io_failure() {
    let folder = tempfile::tempdir().unwrap();
    let mut shell = fixture(&folder.path().join("project"), Locale::En);
    let saved = folder.path().join("published.msanim");
    shell
        .animation_presets
        .start(
            &shell.engine,
            Intent::Save {
                object: 7,
                properties: vec![Property::Position],
                effects: vec![],
                destination: saved.clone(),
            },
            KeyframeSource::of(&shell.state),
        )
        .unwrap();
    let began = Instant::now();
    while !shell
        .animation_presets
        .pending
        .as_ref()
        .is_some_and(|p| matches!(p.stage, Stage::Working(_)))
    {
        assert!(began.elapsed() < Duration::from_secs(30));
        assert!(shell.animation_presets.poll(&shell.engine, false).is_none());
        std::thread::sleep(Duration::from_millis(1));
    }
    while shell.animation_presets.phase() != Some("finished") {
        assert!(began.elapsed() < Duration::from_secs(30));
        std::thread::sleep(Duration::from_millis(1));
    }
    shell.animation_presets.cancel();
    assert!(
        matches!(wait(&mut shell.animation_presets, &shell.engine, true).unwrap(), Completion::Saved(path) if path == saved)
    );
    assert!(motion_core::storage::read_animation_preset(&saved).is_ok());

    let not_directory = folder.path().join("not-directory");
    std::fs::write(&not_directory, b"independent catalog failure").unwrap();
    shell.animation_presets.scan(not_directory).unwrap();
    let began = Instant::now();
    while shell.animation_presets.phase() != Some("finished") {
        assert!(began.elapsed() < Duration::from_secs(30));
        std::thread::sleep(Duration::from_millis(1));
    }
    shell.animation_presets.cancel();
    assert!(wait(&mut shell.animation_presets, &shell.engine, true).is_err());
    assert!(!shell.animation_presets.busy());
}
#[test]
fn queued_job_publishes_in_flight_state_before_controlled_blocking_io() {
    let folder = tempfile::tempdir().unwrap();
    let (entered, receive) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let job = Job::start_with(
        Work::Scan(folder.path().join("presets")),
        cancel.clone(),
        move || {
            entered.send(()).unwrap();
            wait.recv().unwrap();
        },
    )
    .unwrap();
    receive.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(job.phase(), "io");
    assert!(matches!(
        job.receive.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    cancel.store(true, Ordering::Release);
    release.send(()).unwrap();
    assert!(matches!(
        job.receive
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap(),
        Output::Complete(Completion::Cancelled)
    ));
    assert!(!folder.path().join("presets").exists());
}
