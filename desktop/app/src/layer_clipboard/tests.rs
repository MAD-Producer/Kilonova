use super::*;
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};
use worker::{Job, Output, Work};
mod keyframes;

fn project(root: &Path, images: u64) -> motion_core::Project {
    let mut project = motion_core::Project::new(64, 64, 30, 120).unwrap();
    fs::create_dir_all(root.join("assets")).unwrap();
    for id in 1..=images {
        let relative = format!("assets/image-{id}.png");
        image::save_buffer(
            root.join(&relative),
            &vec![id as u8; 64 * 64 * 4],
            64,
            64,
            image::ColorType::Rgba8,
        )
        .unwrap();
        project.assets.push(motion_core::Asset {
            id,
            path: relative,
            width: 64,
            height: 64,
        });
        let mut layer = motion_core::Layer::solid(id, "Image", [64.; 2], [32., 32., 0.], [1.; 4]);
        layer.content = motion_core::Content::Image { asset: id };
        project.layers.push(layer);
    }
    motion_core::storage::save(root, &project).unwrap();
    project
}
fn engine(root: &Path) -> crate::engine::Engine {
    crate::engine::Engine::start(
        root.into(),
        motion_desktop_media::platform(),
        8 * 1024 * 1024 * 1024,
    )
    .unwrap()
}
fn read(engine: &crate::engine::Engine, objects: Vec<u64>) -> Read {
    engine
        .call(|reply| crate::engine::Command::ClipboardRead { objects, reply })
        .unwrap()
}
fn wait(job: &mut Job) -> Result<Output, String> {
    let began = Instant::now();
    loop {
        if let Some(result) = job.poll() {
            return result;
        }
        assert!(
            began.elapsed() < Duration::from_secs(20),
            "clipboard worker did not return: {:?}",
            job.status()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn freeze(engine: &crate::engine::Engine, objects: Vec<u64>, cache: &Path) -> Arc<Frozen> {
    let mut job = Job::start(Work::Capture {
        source: read(engine, objects),
        cache: cache.into(),
    })
    .unwrap();
    let Output::Captured(frozen) = wait(&mut job).unwrap() else {
        panic!("expected capture")
    };
    frozen
}
fn prepare(frozen: Arc<Frozen>, engine: &crate::engine::Engine) -> Result<Prepared, String> {
    let mut job = Job::start(Work::Prepare {
        source: frozen,
        target: read(engine, vec![]),
    })
    .unwrap();
    let Output::Prepared(prepared) = wait(&mut job)? else {
        panic!("expected prepared paste")
    };
    Ok(prepared)
}
fn paste(prepared: Prepared, engine: &crate::engine::Engine) -> Value {
    engine
        .call(|reply| crate::engine::Command::ClipboardCommit {
            edit: Edit::Paste {
                prepared,
                at_frame: Some(12),
                cancel: Arc::new(AtomicBool::new(false)),
            },
            reply,
        })
        .unwrap()
}

#[test]
fn frozen_bytes_survive_removed_source_and_cross_project_paste_reuses_verified_resources() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    project(source.path(), 1);
    project(target.path(), 0);
    let source_engine = engine(source.path());
    let target_engine = engine(target.path());
    let original = fs::read(source.path().join("assets/image-1.png")).unwrap();
    let frozen = freeze(&source_engine, vec![1], cache.path());
    assert_eq!(frozen.objects, [1]);
    fs::remove_file(source.path().join("assets/image-1.png")).unwrap();
    let prepared = prepare(frozen.clone(), &target_engine).unwrap();
    assert_eq!(prepared.files.len(), 1);
    let path = target
        .path()
        .join(&prepared.clipboard.source.assets[0].path);
    let first = paste(prepared, &target_engine);
    assert_eq!(first["clipboard_status"], "applied");
    assert_eq!(first["project"]["layers"][0]["id"], 1);
    assert_eq!(fs::read(&path).unwrap(), original);
    let prepared = prepare(frozen.clone(), &target_engine).unwrap();
    assert!(
        prepared.files.is_empty(),
        "repeated paste compares and reuses the owned source"
    );
    let second = paste(prepared, &target_engine);
    assert_eq!(second["project"]["layers"][1]["id"], 2);
    assert_eq!(second["project"]["assets"].as_array().unwrap().len(), 1);
    drop(frozen);
    assert_eq!(fs::read_dir(cache.path()).unwrap().count(), 0);
    assert_eq!(fs::read(&path).unwrap(), original);
    target_engine.history(0).unwrap();
    assert_eq!(
        target_engine.state().unwrap()["project"]["layers"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    target_engine.history(0).unwrap();
    assert!(target_engine.state().unwrap()["project"]["layers"]
        .as_array()
        .unwrap()
        .is_empty());
    target_engine.history(1).unwrap();
    target_engine.save().unwrap();
    let reopened = motion_core::storage::load(target.path()).unwrap();
    assert_eq!(reopened.layers.len(), 1);
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn staging_failure_removes_only_created_files_and_preserves_foreign_collision() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    project(source.path(), 2);
    let before = project(target.path(), 0);
    let source_engine = engine(source.path());
    let target_engine = engine(target.path());
    let frozen = freeze(&source_engine, vec![1, 2], cache.path());
    let prepared = prepare(frozen.clone(), &target_engine).unwrap();
    let paths: Vec<_> = prepared
        .clipboard
        .source
        .assets
        .iter()
        .map(|a| target.path().join(&a.path))
        .collect();
    drop(prepared);
    assert!(paths.iter().all(|p| !p.exists()));
    fs::write(&paths[1], b"foreign resource").unwrap();
    let error = match prepare(frozen, &target_engine) {
        Err(e) => e,
        Ok(_) => panic!("collision accepted"),
    };
    assert!(error.contains("phase=stage"));
    assert!(!paths[0].exists());
    assert_eq!(fs::read(&paths[1]).unwrap(), b"foreign resource");
    assert_eq!(
        target_engine.state().unwrap()["project"],
        serde_json::to_value(before).unwrap()
    );
}

#[test]
fn stale_target_and_cancelled_commit_discard_uncommitted_owned_sources() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    project(source.path(), 1);
    project(target.path(), 0);
    let source_engine = engine(source.path());
    let target_engine = engine(target.path());
    let frozen = freeze(&source_engine, vec![1], cache.path());
    let mut prepared = prepare(frozen.clone(), &target_engine).unwrap();
    let path = target
        .path()
        .join(&prepared.clipboard.source.assets[0].path);
    prepared.target.generation += 1;
    let state = paste(prepared, &target_engine);
    assert_eq!(state["clipboard_status"], "superseded");
    assert!(!path.exists());
    let prepared = prepare(frozen, &target_engine).unwrap();
    let state = target_engine
        .call(|reply| crate::engine::Command::ClipboardCommit {
            edit: Edit::Paste {
                prepared,
                at_frame: None,
                cancel: Arc::new(AtomicBool::new(true)),
            },
            reply,
        })
        .unwrap();
    assert_eq!(state["clipboard_status"], "superseded");
    assert!(!path.exists());
    assert!(target_engine.state().unwrap()["project"]["layers"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(target_engine.state().unwrap()["revision"], 0);
}

#[test]
fn source_failure_retains_layers_and_completed_cache_allows_one_step_cut() {
    let source = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let before = project(source.path(), 1);
    let engine = engine(source.path());
    let original = fs::read(source.path().join("assets/image-1.png")).unwrap();
    fs::remove_file(source.path().join("assets/image-1.png")).unwrap();
    let mut job = Job::start(Work::Capture {
        source: read(&engine, vec![1]),
        cache: cache.path().into(),
    })
    .unwrap();
    assert!(matches!(wait(&mut job), Err(error) if error.contains("phase=resolve")));
    assert_eq!(
        engine.state().unwrap()["project"],
        serde_json::to_value(&before).unwrap()
    );
    assert_eq!(fs::read_dir(cache.path()).unwrap().count(), 0);
    fs::write(source.path().join("assets/image-1.png"), original).unwrap();
    let frozen = freeze(&engine, vec![1], cache.path());
    let state = engine
        .call(|reply| crate::engine::Command::ClipboardCommit {
            edit: Edit::Cut {
                stamp: frozen.origin.clone(),
                objects: frozen.objects.clone(),
                frame: frozen.frame,
                cancel: Arc::new(AtomicBool::new(false)),
            },
            reply,
        })
        .unwrap();
    assert!(state["project"]["layers"].as_array().unwrap().is_empty());
    engine.history(0).unwrap();
    assert_eq!(
        engine.state().unwrap()["project"],
        serde_json::to_value(before).unwrap()
    );
}

#[test]
fn replacing_the_same_root_and_revision_changes_document_generation() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), 0);
    let mut session = Session::new(
        project.clone(),
        root.path().into(),
        motion_desktop_media::platform(),
    )
    .unwrap();
    let old = Stamp::of(&session);
    session
        .replace_project(
            motion_core::Engine::new(project).unwrap(),
            root.path().into(),
        )
        .unwrap();
    let new = Stamp::of(&session);
    assert_eq!(old.root, new.root);
    assert_eq!(old.revision, new.revision);
    assert_ne!(old.generation, new.generation);
    assert!(!old.same_document(&new));
}

#[test]
fn post_commit_error_and_panic_preserve_referenced_files_and_the_original_failure() {
    let source = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    project(source.path(), 1);
    let source_engine = engine(source.path());
    let frozen = freeze(&source_engine, vec![1], cache.path());
    for panic in [false, true] {
        let target = tempfile::tempdir().unwrap();
        let document = project(target.path(), 0);
        let mut session = Session::new(
            document,
            target.path().into(),
            motion_desktop_media::platform(),
        )
        .unwrap();
        let prepared = storage::prepare(
            &frozen,
            Stamp::of(&session),
            session.engine.project(),
            &AtomicBool::new(false),
        )
        .unwrap();
        let path = target
            .path()
            .join(&prepared.clipboard.source.assets[0].path);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            paste_with(&mut session, prepared, None, |s, commands| {
                s.apply_command_batch(commands)?;
                if panic {
                    panic!("clipboard-post-commit-panic");
                }
                Err("clipboard-post-commit-error".into())
            })
        }));
        if panic {
            let payload = result.unwrap_err();
            assert_eq!(
                payload.downcast_ref::<&str>(),
                Some(&"clipboard-post-commit-panic")
            );
        } else {
            assert_eq!(
                result.unwrap().unwrap_err().to_string(),
                "clipboard-post-commit-error"
            );
        }
        assert_eq!(session.engine.project().layers.len(), 1);
        assert!(path.is_file());
        motion_core::storage::save(target.path(), session.engine.project()).unwrap();
        assert_eq!(
            motion_core::storage::load(target.path()).unwrap(),
            *session.engine.project()
        );
    }
}

#[test]
fn actual_failures_and_worker_panic_reach_default_file_and_error_stderr_without_stdout() {
    const NAME: &str = "layer_clipboard::tests::actual_failures_and_worker_panic_reach_default_file_and_error_stderr_without_stdout";
    const CHILD: &str = "MOTION_LAYER_CLIPBOARD_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let source = root.join("source");
        project(&source, 1);
        let engine = engine(&source);
        assert!(engine
            .call(|reply| crate::engine::Command::ClipboardRead {
                objects: vec![999],
                reply
            })
            .is_err());
        let original = fs::read(source.join("assets/image-1.png")).unwrap();
        fs::remove_file(source.join("assets/image-1.png")).unwrap();
        let (entered, ready) = std::sync::mpsc::channel();
        let (release, proceed) = std::sync::mpsc::channel();
        let mut job = Job::start_with(
            Work::Capture {
                source: read(&engine, vec![1]),
                cache: root.join("cache"),
            },
            move || {
                entered.send(()).unwrap();
                proceed.recv().unwrap();
            },
        )
        .unwrap();
        ready.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(job.status().1, "io");
        crate::logging::flush();
        let blocked = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
        assert!(
            blocked.contains("layer clipboard start request=")
                && blocked.contains("operation=capture phase=queued")
        );
        assert!(!blocked.contains("phase=resolve"));
        release.send(()).unwrap();
        assert!(matches!(wait(&mut job), Err(error) if error.contains("phase=resolve")));
        fs::write(source.join("assets/image-1.png"), original).unwrap();
        let frozen = freeze(&engine, vec![1], &root.join("cache"));
        let prepared = prepare(frozen, &engine).unwrap();
        let path = source.join(&prepared.clipboard.source.assets[0].path);
        let result = engine.call(|reply| crate::engine::Command::ClipboardCommit {
            edit: Edit::Paste {
                prepared,
                at_frame: Some(120),
                cancel: Arc::new(AtomicBool::new(false)),
            },
            reply,
        });
        assert!(result
            .unwrap_err()
            .contains("paste frame outside composition"));
        assert!(!path.exists());
        let mut job = Job::start_with(
            Work::Capture {
                source: read(&engine, vec![1]),
                cache: root.join("cache"),
            },
            || {
                panic!("clipboard-worker-panic-probe");
            },
        )
        .unwrap();
        assert!(
            matches!(wait(&mut job), Err(error) if error.contains("clipboard-worker-panic-probe"))
        );
        crate::logging::flush();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_LAYER_CLIPBOARD_LOG_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(file.contains("logging initialized level=INFO"));
    for marker in [
        "copy layer missing: object=999",
        "phase=resolve",
        "paste frame outside composition",
        "clipboard-worker-panic-probe",
    ] {
        assert!(file.contains(marker), "missing file cause: {marker}");
        assert!(stderr.contains(marker), "missing stderr cause: {marker}");
        assert!(!stdout.contains(marker), "stdout leaked cause: {marker}");
    }
    assert!(
        file.contains("clipboard_read")
            && file.contains("clipboard_commit")
            && file.contains("layer clipboard start request=")
    );
    assert!(!stderr.contains("layer clipboard start request="));
}

#[cfg(any(unix, windows))]
#[test]
fn destination_junction_cannot_create_font_directories_outside_the_project() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let mut document = project(source.path(), 1);
    fs::create_dir_all(source.path().join("assets/fonts")).unwrap();
    fs::write(
        source.path().join("assets/fonts/fixture.ttf"),
        b"ownership boundary fixture",
    )
    .unwrap();
    document.fonts.push(motion_core::FontAsset {
        id: "boundary-font".into(),
        path: "assets/fonts/fixture.ttf".into(),
        name: "Metadata only".into(),
        face_index: 0,
        license: "Original test bytes, no font decoding in this test".into(),
    });
    document.layers[0].content = motion_core::Content::Text {
        text: "Boundary".into(),
        font: "boundary-font".into(),
        raster_asset: 1,
        color: [1.; 4],
    };
    let source_stamp = Stamp {
        generation: 1,
        root: source.path().into(),
        composition: document.composition_id.clone(),
        revision: 0,
    };
    let frozen = storage::capture(
        Read {
            stamp: source_stamp,
            frame: 0,
            clipboard: Some(motion_core::LayerClipboard::capture(&document, vec![1]).unwrap()),
            project: None,
        },
        cache.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let link = target.path().join("assets");
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), &link).unwrap();
    #[cfg(windows)]
    {
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command",
                "New-Item -ItemType Junction -Path $env:MOTION_CLIPBOARD_TEST_LINK -Target $env:MOTION_CLIPBOARD_TEST_TARGET -ErrorAction Stop | Out-Null"])
            .env("MOTION_CLIPBOARD_TEST_LINK", &link).env("MOTION_CLIPBOARD_TEST_TARGET", outside.path()).output().unwrap();
        assert!(
            output.status.success(),
            "junction creation failed: {output:?}"
        );
    }
    // Prove that a recursive mkdir through this real link would mutate the
    // outside directory, then remove that exclusively created control.
    fs::create_dir_all(link.join("control-probe")).unwrap();
    assert!(outside.path().join("control-probe").is_dir());
    fs::remove_dir(outside.path().join("control-probe")).unwrap();
    let target_document = motion_core::Project::new(64, 64, 30, 120).unwrap();
    let stamp = Stamp {
        generation: 2,
        root: target.path().into(),
        composition: target_document.composition_id.clone(),
        revision: 0,
    };
    let error = match storage::prepare(&frozen, stamp, &target_document, &AtomicBool::new(false)) {
        Err(error) => error,
        Ok(_) => panic!("outside destination accepted"),
    };
    assert!(error.contains("phase=target_directory") && error.contains("outside its owner"));
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    #[cfg(windows)]
    fs::remove_dir(link).unwrap();
    #[cfg(unix)]
    fs::remove_file(link).unwrap();
}

#[test]
fn unrelated_edits_preserve_external_parent_but_changed_dependencies_cannot_alias() {
    let root = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    project(root.path(), 2);
    let engine = engine(root.path());
    engine
        .edit(serde_json::json!({"op":"parent","object":2,"parent":1,"frame":0}).to_string())
        .unwrap();
    let frozen = freeze(&engine, vec![2], cache.path());
    // A change to the copied layer itself does not alter its external parent.
    engine
        .edit(serde_json::json!({"op":"rename","object":2,"name":"Changed original"}).to_string())
        .unwrap();
    let prepared = prepare(frozen.clone(), &engine).unwrap();
    let state = paste(prepared, &engine);
    assert_eq!(state["project"]["layers"][2]["parent"]["object"], 1);
    assert_eq!(state["project"]["layers"][2]["name"], "Image");
    engine
        .edit(serde_json::json!({"op":"rename","object":1,"name":"Changed dependency"}).to_string())
        .unwrap();
    let before = engine.state().unwrap();
    let prepared = prepare(frozen, &engine).unwrap();
    let error = engine
        .call(|reply| crate::engine::Command::ClipboardCommit {
            edit: Edit::Paste {
                prepared,
                at_frame: None,
                cancel: Arc::new(AtomicBool::new(false)),
            },
            reply,
        })
        .unwrap_err();
    assert!(error.contains("external layer changed") && error.contains("dependency=1"));
    let after = engine.state().unwrap();
    assert_eq!(after["revision"], before["revision"]);
    assert_eq!(after["project"], before["project"]);
}
