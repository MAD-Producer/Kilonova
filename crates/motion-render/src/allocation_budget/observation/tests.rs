use super::*;
use crate::allocation_budget::{AllocationBudget, State};
use std::sync::{Arc, Mutex, TryLockError};

struct Capture {
    business: Mutex<()>,
    budgets: Vec<Arc<Mutex<State>>>,
    events: Mutex<Vec<(log::Level, String)>>,
}
impl log::Log for Capture {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record<'_>) {
        let text = record.args().to_string();
        if !text.starts_with("render allocation release ") {
            return;
        }
        assert!(!matches!(
            self.business.try_lock(),
            Err(TryLockError::WouldBlock)
        ));
        for budget in &self.budgets {
            assert!(!matches!(budget.try_lock(), Err(TryLockError::WouldBlock)));
        }
        PENDING.with(|pending| assert!(pending.try_borrow_mut().is_ok()));
        assert_eq!(SESSION.get(), None);
        self.events.lock().unwrap().push((record.level(), text));
    }
    fn flush(&self) {}
}

#[test]
fn release_faults_defer_past_nested_business_guards_and_unwind() {
    const NAME: &str = "allocation_budget::observation::tests::release_faults_defer_past_nested_business_guards_and_unwind";
    const CHILD: &str = "MOTION_ALLOCATION_RELEASE_SCOPE_CHILD";
    let Ok(mode) = std::env::var(CHILD) else {
        for mode in ["return", "panic", "saturate"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
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
            assert!(!String::from_utf8_lossy(&output.stdout).contains("render allocation release "));
        }
        return;
    };

    let first_budget = AllocationBudget::new(8).unwrap();
    let second_budget = AllocationBudget::new(4).unwrap();
    let third_budget = AllocationBudget::new(4).unwrap();
    let first = first_budget.reserve(4).unwrap();
    let alias = first.clone();
    let suppressed = first_budget.reserve(4).unwrap();
    let second = second_budget.reserve(4).unwrap();
    let third = third_budget.reserve(4).unwrap();
    let capture = Box::leak(Box::new(Capture {
        business: Mutex::new(()),
        budgets: [&first_budget, &second_budget, &third_budget]
            .map(|budget| budget.0.clone())
            .to_vec(),
        events: Mutex::new(Vec::new()),
    }));
    log::set_logger(capture).unwrap();
    log::set_max_level(log::LevelFilter::Info);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> Result<(), &'static str> {
            let _outer = AllocationLogScope::enter(41);
            let _business = capture.business.lock().unwrap();
            {
                let _nested = AllocationLogScope::enter(42);
                // A real Lease destructor detects category drift. The combined
                // known capacity stays intact, and all further admission fails.
                first_budget.0.lock().unwrap().categories
                    [AllocationCategory::OtherBuffers as usize] = 0;
                drop(first);
                PENDING.with(|pending| assert!(pending.borrow().first.is_none()));
                drop(alias);
                drop(suppressed);
                assert!(capture.events.lock().unwrap().is_empty());
            }
            assert_eq!(SESSION.get(), Some(41));
            if mode == "saturate" {
                PENDING.with(|pending| pending.borrow_mut().omitted = usize::MAX);
            }
            second_budget
                .0
                .lock()
                .unwrap()
                .allocations
                .remove(&second.0.id);
            drop(second);
            assert!(capture.events.lock().unwrap().is_empty());
            if mode == "panic" {
                panic!("primary controlled release caller panic");
            }
            Err("primary controlled release caller error")
        },
    ));
    if mode == "panic" {
        let primary = result.unwrap_err();
        assert_eq!(
            primary.downcast_ref::<&str>(),
            Some(&"primary controlled release caller panic")
        );
    } else {
        assert_eq!(
            result.unwrap().unwrap_err(),
            "primary controlled release caller error"
        );
    }
    for budget in [&first_budget, &second_budget] {
        assert!(budget.check().is_err());
        assert!(budget.info().is_err());
        assert!(budget.reserve(1).is_err());
    }
    assert_eq!(first_budget.0.lock().unwrap().used, 8);
    assert_eq!(second_budget.0.lock().unwrap().used, 4);
    {
        let events = capture.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, log::Level::Error);
        for context in [
            "id=1 bytes=4 category=OtherBuffers",
            "poisoned=false inconsistent=true",
            "session=Some(42) phase=retire deferred_ms=",
        ] {
            assert!(
                events[0].1.contains(context),
                "missing {context}: {}",
                events[0].1
            );
        }
        let omitted = if mode == "saturate" { usize::MAX } else { 1 };
        assert!(events[0]
            .1
            .ends_with(&format!("deferred_omitted={omitted}")));
    }
    // A new outer operation starts with empty bounded observation state.
    {
        let _scope = AllocationLogScope::enter(43);
        let _business = capture
            .business
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        third_budget.0.lock().unwrap().used = 0;
        drop(third);
        assert_eq!(capture.events.lock().unwrap().len(), 1);
    }
    let events = capture.events.lock().unwrap();
    assert_eq!(events.len(), 2);
    assert!(events[1].1.contains("session=Some(43)"));
    assert!(events[1].1.ends_with("deferred_omitted=0"));
    PENDING.with(|pending| {
        let pending = pending.borrow();
        assert!(pending.first.is_none());
        assert_eq!(pending.omitted, 0);
    });
}
