use super::*;

fn pending(request: u64) -> (Pending, std::sync::mpsc::Receiver<Result<bool, Error>>) {
    let (reply, receive) = channel();
    (
        Pending {
            command: Command::Render { frame: 0., reply },
            meta: Meta {
                prerender_input: 0,
                queued: Instant::now(),
                flight: Flight {
                    request,
                    operation: "render",
                    started: Instant::now(),
                },
                context: Default::default(),
            },
            stamp: Stamp {
                document: 1,
                revision: 1,
                composition: "fixture".into(),
                frame: 0,
                view: 0,
                surface: 1,
                extent: [64, 64],
                window: 0,
            },
            waiting: true,
        },
        receive,
    )
}

#[test]
fn shutdown_cancels_pending_replies_but_failure_keeps_the_original_error_identity() {
    let paints = Arc::new(Mutex::new(Vec::new()));
    let mut queue = Queue::default();
    let (frame, cancelled) = pending(1);
    queue.put_back(frame);
    queue.finish_all(None, &paints);
    assert!(!cancelled
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
        .unwrap());
    assert!(queue.is_empty());
    let (frame, failed) = pending(2);
    queue.put_back(frame);
    let primary = Error::from(motion_host::Error::from("original native failure"));
    queue.finish_all(Some(&primary), &paints);
    assert_eq!(
        failed
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .unwrap_err(),
        primary
    );
    assert!(queue.is_empty());
}

#[test]
fn waiting_frame_keeps_its_place_and_original_request_while_other_windows_wait() {
    let mut queue = Queue::default();
    let (first, _first_reply) = pending(12);
    let first_started = first.meta.flight.started;
    let (second, _second_reply) = pending(13);
    queue.frames.push_back(first);
    queue.frames.push_back(second);
    let first = queue.take_due().unwrap();
    queue.put_back(first);
    assert_eq!(queue.status()["pending"], 2);
    assert_eq!(queue.status()["requests"][0]["request"], 12);
    assert!(
        queue.take_due().is_none(),
        "pending tasks must not busy-spin"
    );
    queue.due = Instant::now();
    let first = queue.take_due().unwrap();
    assert_eq!(first.meta.flight.started, first_started);
    assert_eq!(queue.take_due().unwrap().meta.flight.request, 13);
}

#[test]
fn closing_one_window_cancels_its_reply_before_owner_reuse_and_preserves_dispatch_context() {
    let paints = Arc::new(Mutex::new(Vec::new()));
    let mut queue = Queue::default();
    let (main, main_reply) = pending(1);
    let (mut floating, floating_reply) = pending(2);
    floating.stamp.window = 7;
    queue.frames.extend([main, floating]);
    let close = Flight {
        request: 90,
        operation: "close_window",
        started: Instant::now(),
    };
    CURRENT_COMMAND.set(Some(close));
    queue.cancel_window(7, &paints);
    assert!(!floating_reply
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
        .unwrap());
    assert!(matches!(
        main_reply.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    assert_eq!(CURRENT_COMMAND.get().unwrap().request, close.request);
    assert_eq!(queue.status()["pending"], 1);
    queue.finish_all(None, &paints);
    assert!(!main_reply
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
        .unwrap());
    assert_eq!(CURRENT_COMMAND.get().unwrap().request, close.request);
    CURRENT_COMMAND.set(None);
}

#[cfg(windows)]
mod native;
