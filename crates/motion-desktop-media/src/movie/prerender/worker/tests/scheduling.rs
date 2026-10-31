use super::*;

#[test]
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
fn actual_optional_owner_has_background_priority_before_source_preparation() {
    let root = tempfile::tempdir().unwrap();
    let session = session(root.path());
    let (entered, observed) = mpsc::sync_channel(1);
    let (release, wait) = mpsc::sync_channel(1);
    let wait = Mutex::new(wait);
    let mut worker = Worker::start_with_phase_hook(3072, move |_, phase| {
        if phase == "freeze" {
            super::super::scheduling::assert_background();
            entered.send(()).unwrap();
            wait.lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(30))
                .unwrap();
        }
    })
    .unwrap();
    worker.prepare(plan(&session)).unwrap().unwrap();
    let entered = observed.recv_timeout(Duration::from_secs(30));
    let flight = worker.snapshot().unwrap().in_flight;
    // Release before asserting so a failed assertion cannot strand the worker.
    let _ = release.send(());
    entered.expect("actual worker did not reach source preparation at background priority");
    assert_eq!(flight.unwrap().phase, "freeze");
    assert!(matches!(poll(&mut worker).outcome, Outcome::Prepared(_)));
    stop(&mut worker);
}
