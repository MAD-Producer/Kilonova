use super::*;
use std::sync::{mpsc, Barrier};

#[test]
fn aliases_share_one_reservation_and_failed_admission_is_atomic() {
    let budget = AllocationBudget::new(8).unwrap();
    let first = budget.reserve(4).unwrap();
    let alias = first.clone();
    let error = budget.reserve(8).err().unwrap();
    assert_eq!(
        error.resource_rejection(),
        Some(&crate::ResourceRejection::Budget {
            scope: ResourceBudgetScope::RenderAllocations,
            required_bytes: 12,
            budget_bytes: 8,
        })
    );
    assert_eq!(budget.info().unwrap().admitted_bytes, 4);
    drop(first);
    assert_eq!(budget.info().unwrap().live_allocations, 1);
    drop(alias);
    assert_eq!(budget.info().unwrap().admitted_bytes, 0);
    assert_eq!(budget.info().unwrap().peak_admitted_bytes, 4);
    let full = budget.reserve(8).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, 8);
    drop(full);
    assert_eq!(budget.info().unwrap().live_allocations, 0);
}

#[test]
fn lowered_limit_does_not_erase_live_capacity_and_invalid_updates_are_atomic() {
    let budget = AllocationBudget::new(8).unwrap();
    let lease = budget.reserve(8).unwrap();
    budget.set_limit(4).unwrap();
    assert!(budget.check().is_err());
    assert!(budget.reserve(1).is_err());
    assert!(budget.set_limit(0).is_err());
    assert!(budget.set_limit(MAX_RENDER_ALLOCATION_BUDGET + 1).is_err());
    let info = budget.info().unwrap();
    assert_eq!((info.budget_bytes, info.admitted_bytes), (4, 8));
    drop(lease);
    budget.check().unwrap();
    let next = budget.reserve(4).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, 4);
    drop(next);
}

#[test]
fn concurrent_renderers_cannot_admit_the_same_remaining_capacity_twice() {
    let budget = AllocationBudget::new(64).unwrap();
    let start = Arc::new(Barrier::new(9));
    let release = Arc::new(Barrier::new(9));
    let (send, recv) = mpsc::channel();
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let budget = budget.clone();
            let start = start.clone();
            let release = release.clone();
            let send = send.clone();
            std::thread::spawn(move || {
                start.wait();
                let reservation = budget.reserve(64);
                send.send(reservation.is_ok()).unwrap();
                release.wait();
                drop(reservation);
            })
        })
        .collect();
    start.wait();
    let winners = (0..8)
        .map(|_| {
            recv.recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
        })
        .filter(|accepted| *accepted)
        .count();
    // Release every worker before asserting so a failing test cannot strand it.
    let info = budget.info().unwrap();
    release.wait();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(winners, 1);
    assert_eq!(info.admitted_bytes, 64);
    assert_eq!(info.peak_admitted_bytes, 64);
    assert_eq!(budget.info().unwrap().admitted_bytes, 0);
}

struct ReleaseCapture {
    state: Arc<Mutex<State>>,
    events: Mutex<Vec<(log::Level, String)>>,
}
impl log::Log for ReleaseCapture {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record<'_>) {
        let text = record.args().to_string();
        if text.starts_with("render allocation release ") {
            assert!(!matches!(
                self.state.try_lock(),
                Err(std::sync::TryLockError::WouldBlock)
            ));
            self.events.lock().unwrap().push((record.level(), text));
        }
    }
    fn flush(&self) {}
}

#[test]
fn release_faults_fail_closed_and_emit_once_at_info_after_unlock() {
    const TEST: &str =
        "allocation_budget::tests::release_faults_fail_closed_and_emit_once_at_info_after_unlock";
    const CHILD: &str = "MOTION_TEST_ALLOCATION_RELEASE_FAULT";
    let Ok(mode) = std::env::var(CHILD) else {
        for mode in ["poison", "missing", "underflow"] {
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
            assert!(!String::from_utf8_lossy(&output.stdout).contains("render allocation release "));
        }
        return;
    };
    let budget = AllocationBudget::new(8).unwrap();
    let first = budget.reserve(4).unwrap();
    let alias = first.clone();
    let second = budget.reserve(4).unwrap();
    let capture = Box::leak(Box::new(ReleaseCapture {
        state: budget.0.clone(),
        events: Mutex::new(Vec::new()),
    }));
    log::set_logger(capture).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    if mode == "poison" {
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _state = budget.0.lock().unwrap();
            panic!("controlled allocation state poison");
        }))
        .is_err());
    } else {
        let mut state = budget.0.lock().unwrap();
        if mode == "missing" {
            state.allocations.remove(&first.0.id);
        } else {
            state.used = 0;
        }
    }
    drop(first);
    assert!(capture.events.lock().unwrap().is_empty());
    drop(alias);
    drop(second);
    assert!(budget.check().is_err());
    assert!(budget.info().is_err());
    assert!(budget.reserve(1).is_err());
    assert!(budget.set_limit(8).is_err());
    let events = capture.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, log::Level::Error);
    assert!(events[0].1.contains("id=1 bytes=4"));
    assert!(events[0].1.contains(if mode == "poison" {
        "poisoned=true inconsistent=false"
    } else {
        "poisoned=false inconsistent=true"
    }));
}
