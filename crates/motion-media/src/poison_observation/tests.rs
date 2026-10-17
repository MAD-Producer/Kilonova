use super::*;
use std::sync::{mpsc, Arc, Mutex, TryLockError};
use std::time::Duration;

pub(crate) fn case(test: &str, cases: &[&str]) -> Option<String> {
    const CHILD: &str = "MOTION_TEST_POISON_RECOVERY";
    if std::env::var(CHILD).as_deref() == Ok(test) {
        return Some(std::env::var("MOTION_TEST_POISON_CASE").unwrap());
    }
    for case in cases {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test, "--nocapture"])
            .env(CHILD, test)
            .env("MOTION_TEST_POISON_CASE", case)
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "case={case}\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!String::from_utf8_lossy(&output.stdout).contains("media poison recovery"));
    }
    None
}

pub(crate) fn poison<T>(mutex: &Mutex<T>) {
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = mutex.lock().unwrap();
        panic!("controlled media mutex poison");
    }))
    .is_err());
}

pub(crate) fn unlocked<T>(mutex: &Mutex<T>) {
    assert!(
        !matches!(mutex.try_lock(), Err(TryLockError::WouldBlock)),
        "host logger called before business guard was released"
    );
}

pub(crate) fn recovered<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    assert!(mutex.is_poisoned());
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

pub(crate) struct Capture {
    check: Box<dyn Fn(&str) + Send + Sync>,
    events: Mutex<Vec<(log::Level, String)>>,
}
impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        let text = record.args().to_string();
        (self.check)(&text);
        self.events.lock().unwrap().push((record.level(), text));
    }
    fn flush(&self) {}
}
impl Capture {
    pub(crate) fn install(check: impl Fn(&str) + Send + Sync + 'static) -> &'static Self {
        let capture = Box::leak(Box::new(Self {
            check: Box::new(check),
            events: Mutex::new(Vec::new()),
        }));
        log::set_logger(capture).unwrap();
        log::set_max_level(log::LevelFilter::Info);
        capture
    }
    pub(crate) fn matching(&self, marker: &str) -> Vec<(log::Level, String)> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, text)| text.contains(marker))
            .cloned()
            .collect()
    }
    pub(crate) fn first(&self, site: &str, request: &str) {
        wait(|| !self.matching(site).is_empty());
        let events = self.matching(site);
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(events[0].0, log::Level::Warn);
        for marker in [
            "lifetime_count=1 ",
            &format!("request={request:?}"),
            "scope_elapsed_ms=",
            "reason=poisoned_mutex recovery=into_inner",
        ] {
            assert!(events[0].1.contains(marker), "{events:?}");
        }
    }
}

pub(crate) fn wait(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "missing media recovery observation"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub(crate) struct Gate {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
pub(crate) struct Release(mpsc::Sender<()>);
impl Drop for Release {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}
impl Gate {
    pub(crate) fn new() -> (Arc<Self>, mpsc::Receiver<()>, Release) {
        let (entered, waiting) = mpsc::channel();
        let (release, released) = mpsc::channel();
        (
            Arc::new(Self {
                entered,
                release: Mutex::new(released),
            }),
            waiting,
            Release(release),
        )
    }
    pub(crate) fn block(&self) {
        self.entered.send(()).unwrap();
        self.release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(15))
            .unwrap();
    }
    pub(crate) fn fail<T>(&self) -> crate::Result<T> {
        self.block();
        Err("controlled native failure".into())
    }
}

#[test]
fn deferred_recovery_is_bounded_lifetime_counted_and_unlocked() {
    let Some(mode) = case(
        "poison_observation::tests::deferred_recovery_is_bounded_lifetime_counted_and_unlocked",
        &["threshold", "unwind", "overflow"],
    ) else {
        return;
    };
    let outer = Arc::new(Mutex::new(()));
    let inner = Arc::new(Mutex::new(7));
    let a = outer.clone();
    let b = inner.clone();
    let capture = Capture::install(move |_| {
        unlocked(&a);
        unlocked(&b);
    });
    static ONE: RecoverySite = RecoverySite::new("test.one");
    static TWO: RecoverySite = RecoverySite::new("test.two");
    let request = "故".repeat(4096);
    for _ in 0..120 {
        let mut scope = RecoveryScope::new();
        assert_eq!(*scope.recover(inner.lock(), &ONE, &request, "test"), 7);
    }
    assert!(capture.matching("media poison").is_empty());
    poison(&inner);
    if mode == "unwind" {
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut scope = RecoveryScope::new();
            let _outer = outer.lock().unwrap();
            let _inner = scope.recover(inner.lock(), &ONE, &request, "test");
            panic!("controlled unwind after recovery");
        }))
        .is_err());
        capture.first("test.one", &"故".repeat(128));
        assert!(outer.is_poisoned());
        return;
    }
    {
        let mut scope = RecoveryScope::new();
        let _outer = outer.lock().unwrap();
        for _ in 0..120 {
            assert_eq!(*scope.recover(inner.lock(), &ONE, &request, "test"), 7);
            assert_eq!(*scope.recover(inner.lock(), &TWO, "second", "test"), 7);
        }
        assert!(capture.matching("media poison").is_empty());
        if mode == "overflow" {
            static EXTRA: RecoverySite = RecoverySite::new("test.extra");
            drop(scope.recover(inner.lock(), &EXTRA, "extra", "test"));
        }
    }
    inner.clear_poison();
    {
        let mut scope = RecoveryScope::new();
        drop(scope.recover(inner.lock(), &ONE, "healthy", "test"));
    }
    poison(&inner);
    {
        let mut scope = RecoveryScope::new();
        drop(scope.recover(inner.lock(), &ONE, "again", "test"));
    }
    for (site, request) in [
        ("test.one", "故".repeat(128)),
        ("test.two", "second".into()),
    ] {
        let events = capture.matching(site);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].0, log::Level::Warn);
        assert_eq!(events[1].0, log::Level::Error);
        assert!(events[0].1.contains(&format!("request={request:?}")));
        assert!(events[1].1.contains("lifetime_count=60 "));
        assert!(!events[0].1.contains(&"故".repeat(129)));
    }
    assert_eq!(
        capture.matching("buffer exhausted dropped=1").len(),
        usize::from(mode == "overflow")
    );
    assert!(
        inner.is_poisoned(),
        "observation must not repair business state"
    );
}
