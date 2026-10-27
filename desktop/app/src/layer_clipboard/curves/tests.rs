use super::*;

fn wait(job: &Job) -> Result<Output, String> {
    let began = Instant::now();
    loop {
        if let Some(result) = job.poll() {
            return result;
        }
        assert!(
            began.elapsed() < Duration::from_secs(20),
            "clipboard blocked at {:?}",
            job.status()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn curve_clipboard_publishes_blocked_phase_and_cancels_without_opening_system_clipboard() {
    let mut service = Service::default();
    let (entered, ready) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let job = service
        .submit_with(
            Work::Paste,
            cancel.clone(),
            Some(Box::new(move || {
                entered.send(()).unwrap();
                gate.recv().unwrap();
            })),
        )
        .unwrap();
    ready.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(job.status().0 > 0);
    assert_eq!(job.status().1, "io");
    assert!(job.poll().is_none());
    cancel.store(true, Ordering::Release);
    release.send(()).unwrap();
    assert!(matches!(wait(&job).unwrap(), Output::Cancelled));
    assert_eq!(job.status().1, "complete");
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "manual system clipboard integration; requires an existing text clipboard to restore"]
fn curve_clipboard_real_system_roundtrip_between_processes_and_default_error_output() {
    const STAGE: &str = "MOTION_CURVE_SYSTEM_CLIPBOARD_STAGE";
    const ROOT: &str = "MOTION_CURVE_SYSTEM_CLIPBOARD_ARTIFACTS";
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os(ROOT)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temp.path().into());
    std::fs::create_dir_all(&root).unwrap();
    if let Ok(stage) = std::env::var(STAGE) {
        let _log = crate::logging::init(root.join(&stage).join("logs")).unwrap();
        let mut service = Service::default();
        let cancel = Arc::new(AtomicBool::new(false));
        match stage.as_str() {
            "write" => {
                let easing = motion_core::Easing {
                    ease: motion_core::Ease::Out,
                    curve: None,
                };
                let job = service
                    .submit(Work::Copy(CurveClipboard::new(easing).unwrap()), cancel)
                    .unwrap();
                assert!(matches!(wait(&job).unwrap(), Output::Copied));
            }
            "read" => {
                let job = service.submit(Work::Paste, cancel).unwrap();
                let Output::Paste(payload) = wait(&job).unwrap() else {
                    panic!("paste returned no payload");
                };
                assert_eq!(payload.easing.ease, motion_core::Ease::Out);
                let mut project = motion_core::Project::new(64, 64, 30, 120).unwrap();
                let mut layer =
                    motion_core::Layer::solid(7, "Destination", [32.; 2], [0.; 3], [1.; 4]);
                layer
                    .transform
                    .opacity
                    .upsert(0, 0.2, motion_core::Ease::Linear)
                    .unwrap();
                layer
                    .transform
                    .opacity
                    .upsert(30, 0.8, motion_core::Ease::Linear)
                    .unwrap();
                project.layers.push(layer);
                let mut engine = motion_core::Engine::new(project.clone()).unwrap();
                engine
                    .apply(motion_core::Command::PasteEasing {
                        clipboard: Box::new(payload),
                        selections: vec![KeyframeSelection {
                            target: motion_core::KeyframeTarget::Property {
                                object: 7,
                                property: motion_core::Property::Opacity,
                                axis: None,
                            },
                            frames: vec![0],
                        }],
                    })
                    .unwrap();
                let after = engine.snapshot();
                assert_eq!(
                    after.layers[0].transform.opacity.keys[0].ease,
                    motion_core::Ease::Out
                );
                engine.undo().unwrap();
                assert_eq!(engine.project(), &project);
                assert!(!engine.can_undo());
                engine.redo().unwrap();
                motion_core::storage::save(&root.join("read/project"), engine.project()).unwrap();
                assert_eq!(
                    motion_core::storage::load(&root.join("read/project")).unwrap(),
                    after
                );
            }
            "invalid" => {
                arboard::Clipboard::new()
                    .unwrap()
                    .set_text("{\"do_not_log_this_user_text\":true}")
                    .unwrap();
                let job = service.submit(Work::Paste, cancel).unwrap();
                let Err(cause) = wait(&job) else {
                    panic!("invalid system clipboard accepted");
                };
                assert!(cause.contains("invalid easing clipboard JSON"));
                assert!(!cause.contains("do_not_log_this_user_text"));
            }
            "panic" => {
                let job = service
                    .submit_with(
                        Work::Paste,
                        cancel,
                        Some(Box::new(|| panic!("controlled curve clipboard panic"))),
                    )
                    .unwrap();
                assert!(
                    matches!(wait(&job), Err(cause) if cause.contains("controlled curve clipboard panic"))
                );
            }
            "blocked" => {
                let (entered, ready) = mpsc::channel();
                let (release, gate) = mpsc::channel();
                let job = service
                    .submit_with(
                        Work::Paste,
                        cancel.clone(),
                        Some(Box::new(move || {
                            entered.send(()).unwrap();
                            gate.recv().unwrap();
                        })),
                    )
                    .unwrap();
                ready.recv_timeout(Duration::from_secs(10)).unwrap();
                assert_eq!(job.status().1, "io");
                assert!(job.poll().is_none());
                let file =
                    std::fs::read_to_string(root.join("blocked/logs/motion-studio.log")).unwrap();
                assert!(file.contains("curve clipboard start") && file.contains("phase=queued"));
                cancel.store(true, Ordering::Release);
                release.send(()).unwrap();
                assert!(matches!(wait(&job).unwrap(), Output::Cancelled));
            }
            _ => panic!("unknown clipboard acceptance phase"),
        }
        return;
    }
    let _log = crate::logging::init(root.join("parent/logs")).unwrap();
    // Preserve existing user text. Do not discard non-text formats merely to
    // obtain screenshots; this explicit acceptance requires a text clipboard.
    struct Restore(String);
    impl Drop for Restore {
        fn drop(&mut self) {
            if let Err(cause) =
                arboard::Clipboard::new().and_then(|mut c| c.set_text(self.0.clone()))
            {
                log::error!("curve clipboard acceptance restore failed reason={cause}");
            }
        }
    }
    let _restore = Restore(
        arboard::Clipboard::new()
            .unwrap()
            .get_text()
            .expect("acceptance requires restorable text clipboard"),
    );
    for stage in ["write", "read", "invalid", "panic", "blocked"] {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "layer_clipboard::curves::tests::curve_clipboard_real_system_roundtrip_between_processes_and_default_error_output", "--ignored", "--nocapture", "--test-threads=1"])
            .env(STAGE, stage).env(ROOT, &root).env_remove("MOTION_LOG").output().unwrap();
        let folder = root.join(stage);
        std::fs::write(folder.join("stdout.log"), &result.stdout).unwrap();
        std::fs::write(folder.join("stderr.log"), &result.stderr).unwrap();
        assert!(
            result.status.success(),
            "{stage} failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let stdout = String::from_utf8_lossy(&result.stdout);
        assert!(
            !stdout.contains("curve clipboard start") && !stdout.contains("curve clipboard failed")
        );
        let file = std::fs::read_to_string(folder.join("logs/motion-studio.log")).unwrap();
        assert!(file.contains("curve clipboard start") && file.contains("phase=queued"));
        if matches!(stage, "invalid" | "panic") {
            assert!(file.contains("curve clipboard failed"));
            assert!(String::from_utf8_lossy(&result.stderr).contains("curve clipboard failed"));
        } else {
            assert!(!String::from_utf8_lossy(&result.stderr).contains("curve clipboard failed"));
        }
        assert!(!file.contains("do_not_log_this_user_text"));
        assert!(!stdout.contains("do_not_log_this_user_text"));
        assert!(!String::from_utf8_lossy(&result.stderr).contains("do_not_log_this_user_text"));
    }
}
