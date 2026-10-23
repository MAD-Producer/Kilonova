use super::*;

#[test]
fn failed_decoder_closes_outside_state_lock_after_the_primary_error_is_recorded() {
    isolated("shutdown_tests::failed_decoder_closes_outside_state_lock_after_the_primary_error_is_recorded", "MOTION_VIDEO_CLOSE_LOCK_CHILD", |root| {
        let asset = fixture(root);
        let (entered, waiting) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let _release = Release(release.clone());
        let gate = Arc::new(Gate { phase: "close", entered, release: Mutex::new(released), error: Some("controlled failure before native cleanup") });
        let stream = Stream::new(root.into(), asset, Arc::new(Backend(gate)), 7).unwrap();
        stream.request(0, 1, false).unwrap();
        assert_eq!(waiting.recv_timeout(Duration::from_secs(5)).unwrap(), "close");
        {
            let state = stream.shared.0.try_lock().expect("native decoder cleanup retained the stop/status mutex");
            assert!(state.error.as_ref().unwrap().to_string().contains("controlled failure before native cleanup"));
            assert!(state.in_flight.is_none());
        }
        let text = std::fs::read_to_string(root.join("events.log")).unwrap();
        assert!(text.contains("video decode failed object=7 source=1"));
        assert!(text.contains("controlled failure before native cleanup"));
        release.send(()).unwrap();
        drop(stream);
    });
}

#[test]
fn stream_drop_joins_finished_worker_and_reports_its_original_panic() {
    isolated(
        "shutdown_tests::stream_drop_joins_finished_worker_and_reports_its_original_panic",
        "MOTION_VIDEO_DROP_PANIC_CHILD",
        |root| {
            let asset = fixture(root);
            let (entered, waiting) = mpsc::channel();
            let (release, released) = mpsc::channel();
            let _release = Release(release.clone());
            let gate = Arc::new(Gate {
                phase: "seeks",
                entered,
                release: Mutex::new(released),
                error: None,
            });
            let stream = Stream::new(root.into(), asset, Arc::new(Backend(gate)), 7).unwrap();
            stream.request(0, 1, false).unwrap();
            assert_eq!(
                waiting.recv_timeout(Duration::from_secs(5)).unwrap(),
                "seeks"
            );
            release.send(()).unwrap();
            wait_until(|| stream.worker.as_ref().unwrap().is_finished());
            drop(stream);
            log::logger().flush();
            let text = std::fs::read_to_string(root.join("events.log")).unwrap();
            let lines: Vec<_> = text
                .lines()
                .filter(|line| line.contains("video shutdown join failed"))
                .collect();
            assert_eq!(lines.len(), 1, "{text}");
            assert!(lines[0].contains("controlled seek telemetry panic"));
            assert!(
                lines[0].contains("owner=stream")
                    && lines[0].contains("object=Some(7)")
                    && lines[0].contains("source=Some(1)")
            );
            assert!(!text.contains("video shutdown timeout"));
        },
    );
}

#[test]
fn reader_drop_explicitly_reports_bounded_detachment_then_the_worker_can_finish() {
    isolated("shutdown_tests::reader_drop_explicitly_reports_bounded_detachment_then_the_worker_can_finish", "MOTION_VIDEO_DROP_TIMEOUT_CHILD", |root| {
        let asset = fixture(root);
        let (entered, waiting) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let _release = Release(release.clone());
        let gate = Arc::new(Gate { phase: "open", entered, release: Mutex::new(released), error: None });
        let stream = Stream::new(root.into(), asset, Arc::new(Backend(gate)), 7).unwrap();
        stream.request(0, 1, false).unwrap();
        assert_eq!(waiting.recv_timeout(Duration::from_secs(5)).unwrap(), "open");
        let shared = stream.shared.clone();
        let mut reader = VideoFrames::default();
        reader.streams.insert(7, stream);
        let began = Instant::now();
        drop(reader);
        assert!(began.elapsed() < Duration::from_secs(2), "reader teardown did not remain bounded");
        assert!(shared.0.lock().unwrap().stop);
        assert!(!shared.0.lock().unwrap().worker_stopped);
        log::logger().flush();
        let text = std::fs::read_to_string(root.join("events.log")).unwrap();
        let lines: Vec<_> = text.lines().filter(|line| line.contains("video shutdown timeout")).collect();
        assert_eq!(lines.len(), 1, "{text}");
        assert!(lines[0].contains("owner=reader") && lines[0].contains("disposition=detached"));
        assert!(lines[0].contains("object=Some(7)") && lines[0].contains("source=Some(1)"));
        assert!(text.find("video shutdown start").unwrap() < text.find("video shutdown timeout").unwrap());
        release.send(()).unwrap();
        wait_until(|| shared.0.lock().unwrap().worker_stopped);
    });
}
