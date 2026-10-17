use super::*;
#[test]
fn application_fallbacks_report_real_failures_recover_and_keep_previous_styles() {
    const CHILD: &str = "MOTION_APPLICATION_FALLBACK_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let file = root.join("motion-studio.log");
        let layout = root.join("layout.json");
        assert!(crate::workspace::Workspace::load(&layout).valid());
        assert!(std::fs::read_to_string(&file).unwrap().is_empty());
        std::fs::write(&layout, b"{").unwrap();
        for _ in 0..120 {
            assert!(crate::workspace::Workspace::load(&layout).valid());
        }
        let workspace = crate::workspace::Workspace::default();
        workspace.save(&layout).unwrap();
        assert!(crate::workspace::Workspace::load(&layout).valid());
        std::fs::write(&layout, vec![b' '; 65537]).unwrap();
        for _ in 0..60 {
            assert!(crate::workspace::Workspace::load(&layout).valid());
        }
        assert_eq!(std::fs::metadata(&layout).unwrap().len(), 65537);
        // A real directory read failure is observable and retains the directory.
        let directory = root.join("not-a-file");
        std::fs::create_dir(&directory).unwrap();
        workspace.save(&layout).unwrap();
        crate::workspace::Workspace::load(&layout);
        for _ in 0..60 {
            assert!(crate::workspace::Workspace::load(&directory).valid());
        }
        assert!(directory.is_dir());
        workspace.save(&layout).unwrap();
        crate::workspace::Workspace::load(&layout);
        let mut invalid = serde_json::to_value(&workspace).unwrap();
        invalid["hidden"] = serde_json::json!([1]);
        std::fs::write(&layout, serde_json::to_vec(&invalid).unwrap()).unwrap();
        for _ in 0..60 {
            assert!(crate::workspace::Workspace::load(&layout).valid());
        }
        let mut shell =
            crate::Shell::new(root.join("project"), None, Some(crate::i18n::Locale::En)).unwrap();
        shell.layout_path = root.join("shell-layout.json");
        let styles = root.join("late-style.json");
        for _ in 0..120 {
            assert!(shell.load_style_source(styles.clone()).is_err());
        }
        std::fs::write(&styles, br#"{}"#).unwrap();
        shell.load_style_source(styles.clone()).unwrap();
        let previous = shell.panels.styles.clone().unwrap();
        std::fs::write(&styles, b"{").unwrap();
        for _ in 0..120 {
            assert!(shell.reload_styles().is_err());
        }
        assert!(std::sync::Arc::ptr_eq(
            &previous,
            shell.panels.styles.as_ref().unwrap()
        ));
        assert!(shell.panels.message.is_some());
        std::fs::write(&styles, br#"{}"#).unwrap();
        shell.reload_styles().unwrap();
        assert!(shell.panels.message.is_none());
        let text = std::fs::read_to_string(&file).unwrap();
        for phase in ["json", "size", "read", "validation"] {
            assert!(text.contains(&format!("phase={phase} ")), "{text}");
        }
        for (site, episodes) in [
            ("workspace.load", 4),
            ("styles.stamp", 1),
            ("styles.reload", 2),
        ] {
            let records = text
                .lines()
                .filter(|l| l.contains(&format!("site={site} ")))
                .collect::<Vec<_>>();
            assert_eq!(records.len(), episodes * 2, "{site}: {text}");
            assert_eq!(
                records
                    .iter()
                    .filter(|l| l.contains("ERROR") && l.contains("count=60 "))
                    .count(),
                episodes
            );
        }
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "fallback_observation::tests::application_fallbacks_report_real_failures_recover_and_keep_previous_styles"])
        .env_remove("MOTION_LOG").env(CHILD, dir.path()).output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("persistent desktop degradation"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("desktop degradation"));
}

#[test]
fn poisoned_business_lock_is_released_before_logging_even_during_unwind() {
    const CHILD: &str = "MOTION_RECOVERED_LOCK_CHILD";
    if std::env::var_os(CHILD).is_some() {
        struct Probe(std::sync::Arc<Mutex<usize>>, AtomicUsize);
        impl log::Log for Probe {
            fn enabled(&self, _: &log::Metadata<'_>) -> bool {
                true
            }
            fn log(&self, _: &log::Record<'_>) {
                // Poison is retained; PoisonError contains the acquired guard.
                assert!(!matches!(
                    self.0.try_lock(),
                    Err(std::sync::TryLockError::WouldBlock)
                ));
                self.1.fetch_add(1, Ordering::Relaxed);
            }
            fn flush(&self) {}
        }
        let mutex = std::sync::Arc::new(Mutex::new(0));
        let poisoned = mutex.clone();
        assert!(std::thread::spawn(move || {
            let _guard = poisoned.lock().unwrap();
            panic!("controlled poison");
        })
        .join()
        .is_err());
        let probe = Box::leak(Box::new(Probe(mutex.clone(), AtomicUsize::new(0))));
        log::set_logger(probe).unwrap();
        log::set_max_level(log::LevelFilter::Info);
        let counter = Counter::new();
        for _ in 0..120 {
            recovered(&mutex, &counter, "test.lock", |value| *value += 1);
        }
        assert_eq!(*mutex.lock().unwrap_err().into_inner(), 120);
        assert_eq!(probe.1.load(Ordering::Relaxed), 2);
        let counter = Counter::new();
        assert!(std::panic::catch_unwind(|| recovered(
            &mutex,
            &counter,
            "test.unwind",
            |_| panic!("controlled closure panic")
        ))
        .is_err());
        assert_eq!(probe.1.load(Ordering::Relaxed), 3);
        assert!(
            bounded(format_args!("{}", "长\n".repeat(2000)))
                .chars()
                .count()
                <= 768
        );
        assert!(bounded(format_args!(
            "path={} reason=known_cause",
            short("长".repeat(2000))
        ))
        .contains("reason=known_cause"));
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact", "fallback_observation::tests::poisoned_business_lock_is_released_before_logging_even_during_unwind"]).env(CHILD,"1").output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
