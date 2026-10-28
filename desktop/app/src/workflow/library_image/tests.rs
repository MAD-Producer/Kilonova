use super::*;
use crate::{engine, layer_clipboard::KeyframeSource};
use std::{sync::mpsc::channel, time::Duration};

fn selected(root: &std::path::Path) -> PathBuf {
    let path = root.join("selected.png");
    image::RgbaImage::from_pixel(12, 8, image::Rgba([25, 70, 155, 255]))
        .save(&path)
        .unwrap();
    path
}
fn ready(task: &Task) -> Result<Option<Prepared>, String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(result) = task.poll()? {
            return result;
        }
        assert!(Instant::now() < deadline, "image prepare did not complete");
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn background_image_prepare_publishes_pending_before_blocking_and_cancelled_workers_keep_their_permits(
) {
    let temp = tempfile::tempdir().unwrap();
    let path = selected(temp.path());
    let pool = Pool::default();
    let mut releases = Vec::new();
    let mut tasks = Vec::new();
    for _ in 0..2 {
        let (entered, waiting) = channel();
        let (release, gate) = channel();
        let task = Task::start_with_before(
            &pool,
            temp.path().join("project"),
            path.clone(),
            move || {
                entered.send(()).unwrap();
                gate.recv().unwrap();
            },
        )
        .unwrap();
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(task.phase(), "queued");
        assert!(pool.0.lock().unwrap().contains_key(&task.request()));
        releases.push(release);
        tasks.push(task);
    }
    let requests = tasks.iter().map(Task::request).collect::<Vec<_>>();
    drop(tasks);
    assert_eq!(
        pool.0.lock().unwrap().len(),
        2,
        "cancellation freed a blocked worker's permit"
    );
    let Err(error) = Task::start(&pool, temp.path().join("project"), path.clone()) else {
        panic!("unbounded background image tasks")
    };
    for request in requests {
        assert!(error.contains(&format!("request={request}")), "{error}");
    }
    for release in releases {
        release.send(()).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while !pool.0.lock().unwrap().is_empty() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        !temp.path().join("project/assets").exists(),
        "cancelled job opened the source"
    );
    let project = temp.path().join("project");
    let prepared = ready(&Task::start(&pool, project.clone(), path).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!((prepared.width, prepared.height), (12, 8));
    assert!(project.join(&prepared.relative).is_file());
    // Discard is the actual cancellation/receiver-drop ownership path.
    let owned = project.join(&prepared.relative);
    drop(prepared);
    assert!(!owned.exists());
}

#[test]
fn prepared_image_stale_publication_discards_owned_bytes_without_mutating_another_revision() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let engine =
        engine::Engine::start(project.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
    let state = engine.state().unwrap();
    let source = KeyframeSource::of(&state);
    let prepared =
        ready(&Task::start(&Pool::default(), project.clone(), selected(temp.path())).unwrap())
            .unwrap()
            .unwrap();
    let owned = project.join(&prepared.relative);
    let changed = engine
        .edit(
            serde_json::to_string(&motion_core::Command::ProjectLibrary {
                action: motion_core::LibraryAction::CreateFolder {
                    id: 7,
                    parent: None,
                    name: "new".into(),
                },
            })
            .unwrap(),
        )
        .unwrap();
    let result = engine
        .request(|reply| engine::Command::ProjectAsset {
            source,
            operation: super::super::library_media::Request::PreparedImage {
                prepared,
                folder: None,
            },
            reply,
        })
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    assert_eq!(result["assetOperation"]["cancelled"], true);
    assert_eq!(engine.state().unwrap()["project"], changed["project"]);
    assert!(!owned.exists());
}

#[test]
fn background_image_default_errors_and_panics_preserve_causes_stderr_and_stdout_isolation() {
    const CHILD: &str = "MOTION_LIBRARY_IMAGE_ERROR_CHILD";
    const NAME:&str="workflow::library_image::tests::background_image_default_errors_and_panics_preserve_causes_stderr_and_stdout_isolation";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let pool = Pool::default();
        let project = root.join("project");
        let engine =
            engine::Engine::start(project.clone(), motion_desktop_media::platform(), 8 << 30)
                .unwrap();
        let before = engine.state().unwrap();
        let missing =
            ready(&Task::start(&pool, project.clone(), root.join("missing.png")).unwrap())
                .unwrap_err();
        let selected = selected(&root);
        let prepared = ready(&Task::start(&pool, project.clone(), selected.clone()).unwrap())
            .unwrap()
            .unwrap();
        let owned = project.join(&prepared.relative);
        let source = KeyframeSource::of(&before);
        let rejected = engine
            .request(|reply| engine::Command::ProjectAsset {
                source,
                operation: super::super::library_media::Request::PreparedImage {
                    prepared,
                    folder: Some(999),
                },
                reply,
            })
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap_err();
        assert!(!owned.exists());
        let panic = Task::start_with_before(&pool, project.clone(), selected, || {
            panic!("controlled library image preparation panic")
        })
        .unwrap();
        let disconnected = ready(&panic).unwrap_err();
        assert!(disconnected.contains("disconnected"));
        assert_eq!(engine.state().unwrap()["project"], before["project"]);
        std::fs::write(
            root.join("causes.json"),
            serde_json::to_vec(
                &serde_json::json!({"missing":missing,"publish":rejected,"panic":disconnected}),
            )
            .unwrap(),
        )
        .unwrap();
        crate::logging::flush();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_LIBRARY_IMAGE_ERROR_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().to_owned());
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
    let causes: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("causes.json")).unwrap()).unwrap();
    for cause in [
        causes["missing"].as_str().unwrap(),
        causes["publish"].as_str().unwrap(),
        "controlled library image preparation panic",
    ] {
        assert!(file.contains(cause), "missing file cause: {cause}");
        assert!(stderr.contains(cause), "missing stderr cause: {cause}");
    }
    assert!(
        file.contains("project image start request=")
            && file.contains("phase=queued")
            && file.contains("phase=decode")
    );
    assert!(file.contains("project image exited unexpectedly") && file.contains("reason=unwind"));
    assert!(file.contains("Backtrace") || file.contains("backtrace"));
    assert!(
        !stdout.contains("INFO ")
            && !stdout.contains("ERROR ")
            && !stdout.contains("project image start")
    );
}
