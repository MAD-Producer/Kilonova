use super::*;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn read_only_observation_tracks_blocked_success_and_retains_terminal_cause() {
    for fail in [false, true] {
        let (project, scene) = fixture();
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        let mut state = State::default();
        let observation = state.observation();
        assert!(!observation.preparing());
        let (entered, entry) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        state.before_sample = Some(Box::new(move || {
            entered.send(()).unwrap();
            gate.recv_timeout(Duration::from_secs(30)).unwrap();
            assert!(!fail, "observation controlled preparation failure");
        }));
        assert!(!state
            .ensure(&mut renderer, &project, &scene, None, key())
            .unwrap());
        entry.recv_timeout(Duration::from_secs(5)).unwrap();
        // The owner retains its state while another thread reads the handle;
        // there is no Session access, worker command or GPU wait on the reader.
        let reader = observation.clone();
        std::thread::spawn(move || {
            for _ in 0..1000 {
                assert!(reader.preparing());
            }
        })
        .join()
        .unwrap();
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        let terminal = loop {
            match state.poll(&mut renderer, key().context) {
                Ok(false) => assert!(Instant::now() < deadline, "{}", state.status()),
                value => break value,
            }
            std::thread::yield_now();
        };
        assert!(!observation.preparing());
        if fail {
            let crate::Error::PipelinePreparation(primary) = terminal.unwrap_err() else {
                panic!("lost typed cause")
            };
            assert!(primary
                .to_string()
                .contains("observation controlled preparation failure"));
            for _ in 0..65 {
                let crate::Error::PipelinePreparation(next) =
                    state.poll(&mut renderer, key().context).unwrap_err()
                else {
                    panic!("lost terminal cause")
                };
                assert!(Arc::ptr_eq(&primary, &next));
                assert!(!observation.preparing());
            }
        } else {
            assert!(terminal.unwrap());
            assert!(state.ready.is_some());
        }
        drop(state);
        assert!(!observation.preparing());
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn cancellation_and_drop_clear_the_observation_before_a_blocked_worker_returns() {
    for drop_owner in [false, true] {
        let (project, scene) = fixture();
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        let mut state = State::default();
        let observation = state.observation();
        let (entered, entry) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        state.before_sample = Some(Box::new(move || {
            entered.send(()).unwrap();
            gate.recv_timeout(Duration::from_secs(30)).unwrap();
        }));
        assert!(!state
            .ensure(&mut renderer, &project, &scene, None, key())
            .unwrap());
        entry.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(observation.preparing());
        if drop_owner {
            drop(state);
            assert!(!observation.preparing());
            release.send(()).unwrap();
        } else {
            state.cancel();
            assert!(!observation.preparing());
            release.send(()).unwrap();
            for _ in 0..65 {
                assert!(state.poll(&mut renderer, key().context).unwrap());
                assert!(!observation.preparing());
                std::thread::yield_now();
            }
            finish(&mut state, &mut renderer, &project, &scene, key());
            assert!(!observation.preparing());
        }
    }
}
