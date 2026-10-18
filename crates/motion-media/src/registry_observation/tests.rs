use std::{
    collections::HashMap,
    sync::{Mutex, TryLockError},
};

pub(crate) fn child(test: &str) -> bool {
    if std::env::var("MOTION_TEST_REGISTRY_OBSERVATION").as_deref() == Ok(test) {
        return true;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env("MOTION_TEST_REGISTRY_OBSERVATION", test)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("media registry recovery"));
    false
}

struct Capture {
    site: &'static str,
    unlocked: Box<dyn Fn() + Send + Sync>,
    events: Mutex<Vec<(log::Level, String)>>,
}
impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        let text = record.args().to_string();
        if text.contains(self.site) {
            (self.unlocked)();
            self.events.lock().unwrap().push((record.level(), text));
        }
    }
    fn flush(&self) {}
}

pub(crate) fn check<T: Send + 'static>(
    registry: &'static Mutex<HashMap<String, T>>,
    site: &'static str,
    contains: impl Fn(&str) -> bool,
    healthy_contains: impl Fn(&str) -> bool,
) {
    let capture = Box::leak(Box::new(Capture {
        site,
        unlocked: Box::new(move || {
            assert!(
                !matches!(registry.try_lock(), Err(TryLockError::WouldBlock)),
                "host logger was called while the task registry was locked"
            );
        }),
        events: Mutex::new(Vec::new()),
    }));
    log::set_logger(capture).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    for _ in 0..120 {
        assert!(contains("present"));
        assert!(!contains("missing"));
    }
    assert!(capture.events.lock().unwrap().is_empty());
    let poison = || {
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = registry.lock().unwrap();
            panic!("controlled registry poison");
        }))
        .is_err());
    };
    poison();
    let long_request = "故".repeat(4096);
    assert!(!contains(&long_request));
    for count in 2..=120 {
        let present = count % 2 == 0;
        assert_eq!(
            contains(if present { "present" } else { "missing" }),
            present
        );
        assert!(!healthy_contains("missing"));
    }
    assert!(
        registry.is_poisoned(),
        "observation must not clear poisoning"
    );
    // Even an explicit caller-side repair and a later poisoned episode must
    // respect the documented process-lifetime limit for this fixed site.
    registry.clear_poison();
    assert!(contains("present"));
    poison();
    assert!(contains("present"));
    let events = capture.events.lock().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].0, log::Level::Warn);
    assert!(events[0].1.contains("lifetime_count=1 "));
    assert!(events[0].1.contains(&"故".repeat(128)));
    assert!(!events[0].1.contains(&"故".repeat(129)));
    assert!(events[0].1.contains("contains=false"));
    assert_eq!(events[1].0, log::Level::Error);
    assert!(events[1].1.contains("lifetime_count=60 "));
    assert!(events[1].1.contains("request=\"present\""));
    assert!(events[1].1.contains("contains=true"));
    for (_, text) in events.iter() {
        assert!(text.contains("entries=1 "));
        assert!(text.contains("reason=poisoned_registry recovery=into_inner"));
    }
}
