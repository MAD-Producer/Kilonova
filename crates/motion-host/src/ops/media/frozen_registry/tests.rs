use super::*;
use std::{sync::mpsc, thread, time::Duration};
fn context() -> Context {
    Context {
        operation: "gated_work",
        object: 17,
        sequence: 9,
    }
}

#[test]
fn held_reader_does_not_block_other_handles_and_released_work_keeps_its_permit() {
    let registry = Arc::new(Registry::new(Kind::Video, 2));
    let first = registry.create(|| Ok(1u32)).unwrap();
    let second = registry.create(|| Ok(2u32)).unwrap();
    let (entered, enter) = mpsc::channel();
    let (resume, gate) = mpsc::channel();
    let worker = {
        let registry = registry.clone();
        thread::spawn(move || {
            registry.with(first, context(), |value| {
                entered.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(8)).unwrap();
                *value += 1;
                Ok(*value)
            })
        })
    };
    enter.recv_timeout(Duration::from_secs(3)).unwrap();
    let info = registry.info().unwrap();
    let active = info.readers.iter().find(|r| r.handle == first).unwrap();
    assert_eq!(
        (active.operation, active.object, active.sequence),
        (Some("gated_work"), Some(17), Some(9))
    );
    let (send, receive) = mpsc::channel();
    let other = {
        let registry = registry.clone();
        thread::spawn(move || {
            send.send(registry.with(second, context(), |value| Ok(*value)))
                .unwrap()
        })
    };
    assert_eq!(
        receive
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap(),
        2
    );
    other.join().unwrap();
    assert!(registry.release(first).unwrap());
    assert!(matches!(
        registry.with(first, context(), |_| Ok(())),
        Err(Error::Closed { .. })
    ));
    assert!(matches!(registry.create(|| Ok(3)), Err(Error::Limit(_))));
    assert_eq!(registry.info().unwrap().occupied, 2);
    resume.send(()).unwrap();
    assert_eq!(worker.join().unwrap().unwrap(), 2);
    assert_eq!(registry.info().unwrap().occupied, 1);
    let third = registry.create(|| Ok(3)).unwrap();
    assert!(third > second);
    registry.release(second).unwrap();
    registry.release(third).unwrap();
    assert_eq!(registry.info().unwrap().occupied, 0);
}

#[test]
fn pending_constructor_reserves_capacity_without_holding_the_map() {
    let registry = Arc::new(Registry::new(Kind::Audio, 2));
    let (entered, enter) = mpsc::channel();
    let (resume, gate) = mpsc::channel();
    let worker = {
        let registry = registry.clone();
        thread::spawn(move || {
            registry.create(|| {
                entered.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(8)).unwrap();
                Err(Error::Media(
                    std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "constructor failure",
                    )
                    .into(),
                ))
            })
        })
    };
    enter.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(registry.info().unwrap().occupied, 1);
    let other = registry.create(|| Ok(8u8)).unwrap();
    let called = AtomicUsize::new(0);
    assert!(matches!(
        registry.create(|| {
            called.fetch_add(1, Ordering::Relaxed);
            Ok(9)
        }),
        Err(Error::Limit(_))
    ));
    assert_eq!(called.load(Ordering::Relaxed), 0);
    resume.send(()).unwrap();
    assert!(matches!(
        worker.join().unwrap(),
        Err(Error::Media(motion_media::Error::Io(_)))
    ));
    assert_eq!(registry.info().unwrap().occupied, 1);
    registry.release(other).unwrap();
}

#[test]
fn constructor_and_reader_panics_preserve_capacity_and_isolate_poison() {
    let registry = Registry::new(Kind::Video, 2);
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        registry.create(|| -> Result<u8> { panic!("constructor panic") })
    }));
    assert!(failure.is_err());
    assert_eq!(registry.info().unwrap().occupied, 0);
    let bad = registry.create(|| Ok(1)).unwrap();
    let good = registry.create(|| Ok(2)).unwrap();
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        registry.with(bad, context(), |_| -> Result<()> { panic!("reader panic") })
    }));
    assert!(failure.is_err());
    assert!(registry
        .info()
        .unwrap()
        .readers
        .iter()
        .all(|r| r.operation.is_none() && r.waiting == 0));
    assert!(
        matches!(registry.with(bad, context(), |_| Ok(())), Err(Error::ReaderPoisoned { handle, .. }) if handle == bad)
    );
    assert_eq!(
        registry.with(good, context(), |value| Ok(*value)).unwrap(),
        2
    );
    registry.release(bad).unwrap();
    registry.release(good).unwrap();
    assert_eq!(registry.info().unwrap().occupied, 0);
}

struct BlockingDrop(Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>);
impl Drop for BlockingDrop {
    fn drop(&mut self) {
        if let Some((entered, gate)) = self.0.take() {
            entered.send(()).unwrap();
            let _ = gate.recv_timeout(Duration::from_secs(8));
        }
    }
}
#[test]
fn native_resource_drop_holds_its_capacity_but_not_the_registry_lock() {
    let registry = Arc::new(Registry::new(Kind::Audio, 1));
    let (entered, enter) = mpsc::channel();
    let (resume, gate) = mpsc::channel();
    let handle = registry
        .create(|| Ok(BlockingDrop(Some((entered, gate)))))
        .unwrap();
    let worker = {
        let registry = registry.clone();
        thread::spawn(move || registry.release(handle))
    };
    enter.recv_timeout(Duration::from_secs(3)).unwrap();
    let (send, receive) = mpsc::channel();
    let observer = {
        let registry = registry.clone();
        thread::spawn(move || {
            assert_eq!(registry.info().unwrap().occupied, 1);
            assert!(matches!(
                registry.create(|| Ok(BlockingDrop(None))),
                Err(Error::Limit(_))
            ));
            send.send(()).unwrap();
        })
    };
    receive.recv_timeout(Duration::from_secs(2)).unwrap();
    resume.send(()).unwrap();
    observer.join().unwrap();
    assert!(worker.join().unwrap().unwrap());
    assert_eq!(registry.info().unwrap().occupied, 0);
    assert!(!registry.release(handle).unwrap());
}

#[test]
fn same_handle_waiters_serialize_and_publish_the_active_request_after_handoff() {
    let registry = Arc::new(Registry::new(Kind::Video, 1));
    let handle = registry.create(|| Ok(0u8)).unwrap();
    let (entered, enter) = mpsc::channel();
    let (resume, gate) = mpsc::channel();
    let first = {
        let registry = registry.clone();
        thread::spawn(move || {
            registry.with(handle, context(), |value| {
                entered.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(8)).unwrap();
                *value += 1;
                Ok(())
            })
        })
    };
    enter.recv_timeout(Duration::from_secs(3)).unwrap();
    let (entered, enter) = mpsc::channel();
    let (resume_second, gate) = mpsc::channel();
    let second = {
        let registry = registry.clone();
        thread::spawn(move || {
            registry.with(
                handle,
                Context {
                    sequence: 10,
                    ..context()
                },
                |value| {
                    assert_eq!(*value, 1);
                    entered.send(()).unwrap();
                    gate.recv_timeout(Duration::from_secs(8)).unwrap();
                    Ok(())
                },
            )
        })
    };
    let began = Instant::now();
    while registry.info().unwrap().readers[0].waiting != 1 {
        assert!(began.elapsed() < Duration::from_secs(3));
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(registry.info().unwrap().readers[0].sequence, Some(9));
    resume.send(()).unwrap();
    enter.recv_timeout(Duration::from_secs(3)).unwrap();
    first.join().unwrap().unwrap();
    assert_eq!(registry.info().unwrap().readers[0].sequence, Some(10));
    resume_second.send(()).unwrap();
    second.join().unwrap().unwrap();
    assert_eq!(registry.info().unwrap().readers[0].operation, None);
    registry.release(handle).unwrap();
}
