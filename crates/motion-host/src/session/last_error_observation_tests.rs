use super::*;
use crate::{Platform, SurfaceTarget, VideoDecoder, VideoQuery};
use motion_media::Result as MediaResult;
use std::{path::Path, sync::Arc};
pub(super) struct Stub;
impl Platform for Stub {
    fn name(&self) -> &'static str {
        "memory-test"
    }
    fn probe_video(
        &self,
        _: &Path,
        _: Option<u32>,
        _: Option<u32>,
        _: &dyn Fn() -> MediaResult<()>,
    ) -> MediaResult<motion_media::VideoProbe> {
        Err("unused".into())
    }
    fn open_decoder(
        &self,
        _: &Path,
        _: motion_core::VideoAsset,
        _: Vec<u64>,
    ) -> Result<Box<dyn VideoDecoder>> {
        Err("unused".into())
    }
    fn decode_audio(
        &self,
        _: &Path,
        _: &Path,
        _: Option<u32>,
        _: u64,
        _: &mut dyn FnMut(f64) -> MediaResult<()>,
    ) -> MediaResult<motion_core::AudioAsset> {
        Err("unused".into())
    }
    fn media_capabilities(&self, _: Option<&VideoQuery>) -> Result<serde_json::Value> {
        Ok(serde_json::json!({}))
    }
    fn attach_surface(
        &self,
        _: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        _: u32,
        _: u32,
    ) -> Result<SurfaceTarget> {
        Err("unused".into())
    }
}

struct Capture(Mutex<Vec<(log::Level, String)>>);
impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        let text = record.args().to_string();
        if text.starts_with("session retained render error ") {
            assert!(
                !matches!(
                    sessions().try_lock(),
                    Err(std::sync::TryLockError::WouldBlock)
                ),
                "registry held during logging"
            );
            self.0.lock().unwrap().push((record.level(), text));
        }
    }
    fn flush(&self) {}
}
#[test]
fn successful_edits_with_sample_errors_are_observed_after_unlock_without_rollback() {
    const TEST: &str = "session::last_error_observation_tests::successful_edits_with_sample_errors_are_observed_after_unlock_without_rollback";
    const CHILD: &str = "MOTION_LATCHED_SESSION_ERROR_TEST";
    let Ok(mode) = std::env::var(CHILD) else {
        for mode in ["episodes", "primary", "owner"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", TEST, "--nocapture"])
                .env(CHILD, mode)
                .env_remove("MOTION_LOG")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{mode}\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                !String::from_utf8_lossy(&output.stdout).contains("site=host.session.last_error")
            );
        }
        return;
    };
    let capture = Box::leak(Box::new(Capture(Mutex::new(Vec::new()))));
    log::set_logger(capture).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    project.layers.push(motion_core::Layer::solid(
        1,
        "before",
        [64.; 2],
        [32., 32., 0.],
        [1.; 4],
    ));
    let id = open(root.clone(), project.clone(), Arc::new(Stub)).unwrap();
    for _ in 0..120 {
        assert!(with_session(id, |s| Ok(s.snapshot())).unwrap()["renderError"].is_null());
    }
    assert!(capture.0.lock().unwrap().is_empty());
    if mode == "owner" {
        let result = std::thread::spawn(move || with_session(id, |_| Ok(())))
            .join()
            .unwrap();
        assert_eq!(
            result.unwrap_err(),
            "native session must run on its owning worker thread"
        );
        assert!(capture.0.lock().unwrap().is_empty());
    }
    let edit = || {
        with_session(id, |s| {
            s.frame = 1000.; // An actual invalid sample, after a real edit has committed.
            let snapshot = s.apply_command_batch(vec![motion_core::Command::Rename {
                object: 1,
                name: "committed".into(),
            }])?;
            assert_eq!(s.engine.project().layers[0].name, "committed");
            assert!(snapshot["renderError"]
                .as_str()
                .unwrap()
                .contains("invalid sample time"));
            if mode == "primary" {
                return Err("primary operation failure".into());
            }
            Ok(snapshot)
        })
    };
    if mode == "primary" {
        assert_eq!(edit().unwrap_err(), "primary operation failure");
        let records = capture.0.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, log::Level::Warn);
        assert!(records[0].1.contains("invalid sample time"));
        return;
    }
    for _ in 0..120 {
        edit().unwrap();
    }
    {
        let records = capture.0.lock().unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].0, log::Level::Warn);
        assert_eq!(records[1].0, log::Level::Error);
        assert!(records[0].1.contains("calls_with_latched_error=1 "));
        assert!(records[1].1.contains("calls_with_latched_error=60 "));
        assert!(records
            .iter()
            .all(|(_, s)| s.contains(&format!("session={id} "))
                && s.contains("frame=1000")
                && s.contains("scope_elapsed_ms=")
                && s.contains("invalid sample time")));
    }
    // Reset through a real project replacement, preserving existing latch rules.
    with_session(id, |s| {
        s.replace_project(
            Engine::new(project.clone()).map_err(|e| e.to_string())?,
            root.clone(),
        )
    })
    .unwrap();
    for _ in 0..120 {
        assert!(with_session(id, |s| Ok(s.snapshot())).unwrap()["renderError"].is_null());
    }
    for _ in 0..59 {
        edit().unwrap();
    }
    assert_eq!(
        capture
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|(l, _)| *l == log::Level::Error)
            .count(),
        1
    );
    edit().unwrap();
    {
        let records = capture.0.lock().unwrap();
        assert_eq!(records.len(), 4);
        assert_eq!(records[2].0, log::Level::Warn);
        assert_eq!(records[3].0, log::Level::Error);
    }
    with_session(id, |s| {
        s.replace_project(Engine::new(project).map_err(|e| e.to_string())?, root)
    })
    .unwrap();
    let original = "private-long-error-敏".repeat(4000);
    with_session(id, |s| {
        s.last_error = Some(original.clone());
        Ok(())
    })
    .unwrap();
    let records = capture.0.lock().unwrap();
    let last = &records.last().unwrap().1;
    assert!(last.chars().count() < 850);
    assert!(!last.contains(&original));
    assert!(last.contains("reason=private-long-error"));
    drop(records);
    with_session(id, |s| {
        assert_eq!(s.last_error.as_deref(), Some(original.as_str()));
        Ok(())
    })
    .unwrap();
}
