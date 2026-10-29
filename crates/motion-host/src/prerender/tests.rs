use super::*;

fn context() -> Context {
    Context {
        document_generation: 1,
        device_generation: 1,
        content: [1; 32],
        output: [2; 32],
    }
}
fn policy() -> Policy {
    Policy {
        enabled: true,
        ..Policy::default()
    }
}
fn demand() -> Demand {
    Demand {
        context: context(),
        frame: 10.25,
        total_frames: 100,
        direction: Direction::Forward,
        playing: true,
        editing: false,
        foreground_pending: false,
        pressure: false,
        now: Duration::ZERO,
    }
}
fn ready() -> Scheduler {
    let mut scheduler = Scheduler::new(policy()).unwrap();
    scheduler.update(demand()).unwrap();
    scheduler
}

#[test]
fn exact_fractional_directional_window_is_bounded() {
    let mut scheduler = ready();
    let mut frames = Vec::new();
    while let Some(ticket) = scheduler.take_next(true).unwrap() {
        frames.push(ticket.key().frame());
        assert_eq!(scheduler.finish(&ticket), Some(ticket.key()));
    }
    assert_eq!(frames, vec![11.25, 12.25, 13.25]);
    let backward = Demand {
        direction: Direction::Backward,
        frame: 2.25,
        now: Duration::from_secs(1),
        ..demand()
    };
    scheduler.update(backward).unwrap();
    let first = scheduler.take_next(true).unwrap().unwrap();
    assert_eq!(first.key().frame(), 1.25);
    scheduler.finish(&first);
    let second = scheduler.take_next(true).unwrap().unwrap();
    assert_eq!(second.key().frame(), 0.25);
    scheduler.finish(&second);
    assert!(scheduler.take_next(true).unwrap().is_none());
    scheduler
        .update(Demand {
            frame: 99.,
            now: Duration::from_secs(2),
            ..demand()
        })
        .unwrap();
    assert_eq!(scheduler.status().pending, 0);
}

#[test]
fn idle_admission_uses_injected_monotonic_time() {
    let mut scheduler = Scheduler::new(policy()).unwrap();
    let stopped = Demand {
        playing: false,
        ..demand()
    };
    scheduler.update(stopped).unwrap();
    assert_eq!(scheduler.status().hold, Hold::IdleDelay);
    scheduler
        .update(Demand {
            now: Duration::from_millis(199),
            ..stopped
        })
        .unwrap();
    assert!(scheduler.take_next(true).unwrap().is_none());
    scheduler
        .update(Demand {
            now: Duration::from_millis(200),
            ..stopped
        })
        .unwrap();
    assert!(scheduler.take_next(true).unwrap().is_some());
}

#[test]
fn healthy_polling_neither_duplicates_nor_cancels_work() {
    let mut scheduler = ready();
    let ticket = scheduler.take_next(true).unwrap().unwrap();
    for ms in 1..100 {
        scheduler
            .update(Demand {
                now: Duration::from_millis(ms),
                ..demand()
            })
            .unwrap();
        assert_eq!(scheduler.status().pending, 2);
        assert!(scheduler.take_next(true).unwrap().is_none());
        assert!(!ticket.canceled());
    }
    scheduler.finish(&ticket);
    scheduler
        .update(Demand {
            now: Duration::from_secs(1),
            ..demand()
        })
        .unwrap();
    assert_eq!(scheduler.status().pending, 2);
    assert_eq!(scheduler.status().counts.started, 1);
}

#[test]
fn foreground_editing_pressure_and_disable_cancel_before_new_admission() {
    for hold in [
        Hold::Foreground,
        Hold::Editing,
        Hold::Pressure,
        Hold::Disabled,
    ] {
        let mut scheduler = ready();
        let ticket = scheduler.take_next(true).unwrap().unwrap();
        let next = Demand {
            foreground_pending: hold == Hold::Foreground,
            editing: hold == Hold::Editing,
            pressure: hold == Hold::Pressure,
            ..demand()
        };
        if hold == Hold::Disabled {
            scheduler.set_policy(Policy::default()).unwrap();
        }
        scheduler.update(next).unwrap();
        assert_eq!(scheduler.status().hold, hold);
        assert!(ticket.canceled());
        assert_eq!(scheduler.status().pending, 0);
        assert!(scheduler.take_next(true).unwrap().is_none());
        assert!(scheduler.finish(&ticket).is_none());
        assert_eq!(scheduler.status().counts.canceled, 1);
        assert_eq!(scheduler.status().counts.completed, 0);
        assert_eq!(scheduler.status().hold, hold);
    }
}

#[test]
fn canceled_worker_keeps_its_slot_until_acknowledged() {
    let mut scheduler = ready();
    let old = scheduler.take_next(true).unwrap().unwrap();
    for generation in 2..100 {
        let next = Demand {
            context: Context {
                document_generation: generation,
                ..context()
            },
            ..demand()
        };
        scheduler.update(next).unwrap();
        assert!(old.canceled());
        assert!(scheduler.take_next(true).unwrap().is_none());
        assert_eq!(scheduler.status().in_flight, Some(old.key()));
        assert_eq!(scheduler.status().counts.started, 1);
        assert_eq!(scheduler.status().counts.canceled, 1);
    }
    assert!(scheduler.finish(&old).is_none());
    let current = scheduler.take_next(true).unwrap().unwrap();
    assert_eq!(current.key().context().document_generation, 99);
    assert!(scheduler.finish(&old).is_none());
    assert_eq!(scheduler.status().in_flight, Some(current.key()));
    assert_eq!(scheduler.finish(&current), Some(current.key()));
}

#[test]
fn each_content_output_and_device_change_rejects_old_installation() {
    let contexts = [
        Context {
            content: [3; 32],
            ..context()
        },
        Context {
            output: [4; 32],
            ..context()
        },
        Context {
            device_generation: 2,
            ..context()
        },
        Context {
            document_generation: 2,
            ..context()
        },
    ];
    for context in contexts {
        let mut scheduler = ready();
        let old = scheduler.take_next(true).unwrap().unwrap();
        scheduler
            .update(Demand {
                context,
                ..demand()
            })
            .unwrap();
        assert!(old.canceled());
        assert!(scheduler.finish(&old).is_none());
        let new = scheduler.take_next(true).unwrap().unwrap();
        assert_eq!(new.key().context(), context);
        assert!(scheduler.finish(&old).is_none());
        assert_eq!(scheduler.finish(&new), Some(new.key()));
    }
}

#[test]
fn cross_scheduler_ticket_cannot_install_or_clear_a_flight() {
    let mut a = ready();
    let mut b = ready();
    let ta = a.take_next(true).unwrap().unwrap();
    let tb = b.take_next(true).unwrap().unwrap();
    assert_eq!(ta.id(), tb.id());
    assert_eq!(ta.key(), tb.key());
    assert!(b.finish(&ta).is_none());
    assert_eq!(b.status().in_flight, Some(tb.key()));
    assert_eq!(b.finish(&tb), Some(tb.key()));
}

#[test]
fn budget_refusal_preserves_pending_work_without_starting() {
    let mut scheduler = ready();
    for _ in 0..60 {
        assert!(scheduler.take_next(false).unwrap().is_none());
    }
    assert_eq!(scheduler.status().hold, Hold::Budget);
    assert_eq!(scheduler.status().pending, 3);
    assert_eq!(scheduler.status().counts.started, 0);
    assert_eq!(scheduler.status().counts.budget_holds, 60);
    let ticket = scheduler.take_next(true).unwrap().unwrap();
    assert_eq!(ticket.key().frame(), 11.25);
}

#[test]
fn fast_seek_cancels_and_recovers_after_idle_without_sleeping() {
    let mut scheduler = ready();
    let ticket = scheduler.take_next(true).unwrap().unwrap();
    let seek = Demand {
        frame: 80.25,
        now: Duration::from_millis(10),
        ..demand()
    };
    scheduler.update(seek).unwrap();
    assert_eq!(scheduler.status().hold, Hold::FastSeek);
    assert!(ticket.canceled());
    assert!(scheduler.finish(&ticket).is_none());
    assert!(scheduler.take_next(true).unwrap().is_none());
    scheduler
        .update(Demand {
            now: Duration::from_millis(210),
            ..seek
        })
        .unwrap();
    assert_eq!(
        scheduler.take_next(true).unwrap().unwrap().key().frame(),
        81.25
    );
}

#[test]
fn invalid_clock_or_frame_cancels_existing_work() {
    for frame in [f64::NAN, f64::INFINITY, -1.] {
        let mut scheduler = ready();
        let ticket = scheduler.take_next(true).unwrap().unwrap();
        assert_eq!(
            scheduler.update(Demand { frame, ..demand() }),
            Err(Error::InvalidDemand)
        );
        assert!(ticket.canceled());
        assert!(scheduler.finish(&ticket).is_none());
        assert_eq!(scheduler.status().hold, Hold::NoDemand);
    }
    let mut scheduler = ready();
    scheduler
        .update(Demand {
            now: Duration::from_secs(1),
            ..demand()
        })
        .unwrap();
    let ticket = scheduler.take_next(true).unwrap().unwrap();
    assert_eq!(scheduler.update(demand()), Err(Error::ClockReversed));
    assert!(ticket.canceled());
}

#[test]
fn policy_bounds_and_ticket_exhaustion_are_explicit() {
    assert!(matches!(
        Scheduler::new(Policy {
            window: MAX_WINDOW + 1,
            ..policy()
        }),
        Err(Error::InvalidPolicy)
    ));
    for speed in [0., -1., f64::NAN, f64::INFINITY] {
        assert!(matches!(
            Scheduler::new(Policy {
                max_seek_frames_per_second: speed,
                ..policy()
            }),
            Err(Error::InvalidPolicy)
        ));
    }
    let mut scheduler = ready();
    scheduler.next_ticket = u64::MAX;
    assert!(matches!(
        scheduler.take_next(true),
        Err(Error::TicketExhausted)
    ));
    assert_eq!(scheduler.status().pending, 3);
    assert_eq!(scheduler.status().counts.started, 0);
    scheduler
        .set_policy(Policy {
            window: 0,
            ..policy()
        })
        .unwrap();
    assert_eq!(scheduler.status().hold, Hold::Disabled);
}

#[test]
fn dropping_owner_publishes_cancellation_to_blocked_worker() {
    let mut scheduler = ready();
    let ticket = scheduler.take_next(true).unwrap().unwrap();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        entered_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        assert!(ticket.canceled());
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(scheduler.status().in_flight.is_some());
    drop(scheduler);
    release_tx.send(()).unwrap();
    worker.join().unwrap();
}
