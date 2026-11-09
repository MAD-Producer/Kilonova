//! A rejected stable-key drag must preserve state and reach both default sinks.
use super::*;
use crate::engine::Engine;
use motion_core::{Ease, Layer, Project, Track};
use std::{path::PathBuf, time::Duration};

#[test]
fn missing_drag_key_reports_original_cause_without_mutation_and_allows_recovery() {
    const NAME:&str="engine::viewer_input::free_drag_tests::missing_drag_key_reports_original_cause_without_mutation_and_allows_recovery";
    const CHILD: &str = "MOTION_FREE_DRAG_FAULT_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let temporary = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_FREE_DRAG_FAULT_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| temporary.path().into());
        std::fs::create_dir_all(&root).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
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
        let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(file.contains("logging initialized level=INFO"));
        for operation in ["viewer_drag", "drag"] {
            assert!(file
                .lines()
                .any(|l| l.contains("start #") && l.ends_with(operation)));
        }
        for log in [&*file, &*stderr] {
            assert_eq!(
                log.lines()
                    .filter(|l| l.contains("ERROR")
                        && l.contains("key=999: spatial key no longer exists"))
                    .count(),
                2,
                "{log}"
            );
        }
        assert!(
            !stdout.contains("motion_studio::") && !stdout.contains("spatial key no longer exists")
        );
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.clone()).unwrap();
    let mut p = Project::new(64, 64, 30, 60).unwrap();
    let mut l = Layer::solid(7, "drag fault", [24.; 2], [32., 32., 0.], [1.; 4]);
    let mut track = Track::constant([32., 32., 0.]);
    for (t, x) in [(0, 20.), (4, 30.), (10, 45.), (20, 50.)] {
        track.upsert(t, [x, 32., 0.], Ease::Linear).unwrap();
    }
    track.enable_roving(&[4]).unwrap();
    l.transform.position = track;
    p.layers.push(l);
    motion_core::storage::save(&root.join("project"), &p).unwrap();
    let engine = Engine::start(
        root.join("project"),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    engine.seek(4.).unwrap();

    let before = engine.state().unwrap();
    let request = |s: &Value, key| Drag {
        window: 0,
        source: KeyframeSource::of(s),
        frame: s["frame"].as_f64().unwrap(),
        camera: Camera::Composition,
        object: 7,
        spatial_key: Some(key),
        delta: [3., -2.],
        size: [64, 64],
    };
    let error = engine
        .viewer_drag_async(request(&before, 999))
        .unwrap()
        .recv_timeout(Duration::from_secs(30))
        .unwrap()
        .unwrap_err();
    assert!(error.to_string().contains("object=7 key=999: spatial key no longer exists"));
    let error = engine
        .drag_at_key_async(7, [3., -2.], [64, 64], Some(999))
        .unwrap()
        .recv_timeout(Duration::from_secs(30))
        .unwrap()
        .unwrap_err();
    assert!(error.to_string().contains("object=7 key=999: spatial key no longer exists"));
    let after = engine.state().unwrap();
    for field in [
        "project",
        "revision",
        "history",
        "canUndo",
        "canRedo",
        "frame",
        "document_generation",
    ] {
        assert_eq!(after[field], before[field], "{field}");
    }
    engine.history(2).unwrap();
    let recovered = engine
        .viewer_drag_async(request(&after, 2))
        .unwrap()
        .recv_timeout(Duration::from_secs(30))
        .unwrap()
        .unwrap();
    assert_ne!(recovered["project"], before["project"]);
    assert_eq!(
        recovered["project"]["layers"][0]["transform"]["position"]["spatial_animation"]["keys"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(engine.history(4).unwrap()["project"], before["project"]);
    // A superseded source is a normal cancellation before key validation.
    engine.seek(5.).unwrap();
    let cancelled = engine
        .viewer_drag_async(request(&before, 999))
        .unwrap()
        .recv_timeout(Duration::from_secs(30))
        .unwrap()
        .unwrap();
    assert_eq!(cancelled["viewerDrag"]["cancelled"], true);
    engine.request_shutdown().unwrap();
    crate::logging::flush();
}
