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
