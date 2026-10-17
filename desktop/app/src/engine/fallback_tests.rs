use super::*;
#[test]
fn poisoned_engine_caches_keep_drop_policy_and_terminal_messages() {
    const CHILD: &str = "MOTION_ENGINE_CACHE_POISON_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let engine = Engine::start(
            root.join("project"),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let pool = engine.paint_pool.clone();
        assert!(std::thread::spawn(move || {
            let _guard = pool.lock().unwrap();
            panic!("controlled paint pool poison");
        })
        .join()
        .is_err());
        for _ in 0..120 {
            let paint = engine.take_paint();
            assert!(paint.order.is_empty());
            engine.recycle_paint(paint);
            recycle_paint_pool(
                &engine.paint_pool,
                motion_ui::paint::PaintList::default(),
                true,
            );
        }
        assert!(engine
            .paint_pool
            .lock()
            .err()
            .expect("poison retained")
            .into_inner()
            .is_empty());
        let failure = engine.failure.clone();
        assert!(std::thread::spawn(move || {
            let _guard = failure.lock().unwrap();
            panic!("controlled failure lock poison");
        })
        .join()
        .is_err());
        for _ in 0..120 {
            assert!(engine.check_running().is_ok());
            assert!(engine.stopped_error().contains("reopen the project"));
        }
        *engine.failure.lock().unwrap_err().into_inner() = Some("original terminal reason".into());
        assert_eq!(
            engine.check_running().unwrap_err(),
            "original terminal reason"
        );
        assert_eq!(engine.stopped_error(), "original terminal reason");
        // This fixture injected a terminal value into a live worker. Restore
        // its lifecycle invariant before Drop sends the ordinary Shutdown.
        *engine.failure.lock().unwrap_err().into_inner() = None;
        let text = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
        for site in [
            "engine.paint.take",
            "engine.paint.recycle",
            "engine.paint.worker_recycle",
            "engine.failure.check",
            "engine.failure.stopped",
        ] {
            let records = text
                .lines()
                .filter(|l| l.contains(&format!("site={site} ")))
                .collect::<Vec<_>>();
            assert_eq!(records.len(), 2, "{site}: {text}");
            assert!(records[0].contains("WARN") && records[1].contains("ERROR"));
        }
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "engine::fallback_tests::poisoned_engine_caches_keep_drop_policy_and_terminal_messages",
        ])
        .env_remove("MOTION_LOG")
        .env(CHILD, temp.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("site=engine.paint.take"));
}
