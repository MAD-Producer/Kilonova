use super::*;
use motion_host::depth::{
    cache::Execution,
    capability::{policy as admission, Profile},
    Jobs,
};
use motion_host::depth_snapshot::DepthSnapshot;
use motion_model::depth::{DepthSource, DepthTime};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Attested {
    scaled: Scaled,
    epoch: Arc<AtomicUsize>,
}
fn execution(model: &DepthModel) -> Execution {
    Execution {
        backend: "controlled".into(),
        provider: if model.processing == "settled" {
            "actual_settled_provider"
        } else {
            "controlled_cpu"
        }
        .into(),
        runtime_sha256: [24; 32],
        plan_sha256: [25; 32],
        precision: "float32".into(),
    }
}
impl Backend for Attested {
    fn execution_identity(
        &mut self,
        input: &Input,
        cancel: &Cancellation,
    ) -> motion_host::depth::Result<Option<Execution>> {
        cancel.check()?;
        let mut actual = execution(&input.key().model);
        if self.epoch.load(Ordering::SeqCst) != 0 {
            actual.runtime_sha256 = [26; 32];
        }
        Ok(Some(actual))
    }
    fn infer(
        &mut self,
        input: &Input,
        cancel: &Cancellation,
    ) -> motion_host::depth::Result<Estimate> {
        self.scaled.infer(input, cancel)
    }
}
fn worker(epoch: Arc<AtomicUsize>) -> (Attested, mpsc::Receiver<Seen>) {
    let (echo, seen, _) = backend(false);
    (
        Attested {
            scaled: Scaled {
                echo,
                blocked: None,
                fail: false,
            },
            epoch,
        },
        seen,
    )
}
fn context() -> admission::Context {
    admission::Context {
        environment_sha256: [80; 32],
        measurement_protocol_sha256: [85; 32],
        domain: "authored-session-control".into(),
    }
}
fn bindings() -> admission::Bindings {
    let mut choices = Vec::new();
    for model in [model(), settled()] {
        let (backend, _seen) = worker(Arc::new(AtomicUsize::new(0)));
        let mut jobs = Jobs::start_measured(backend).unwrap();
        let mut profile = Profile::new(2).unwrap();
        let key = Key {
            source: DepthSource {
                asset: 7,
                revision: 1,
                pts: DepthTime {
                    ticks: 0,
                    timescale: 1,
                },
            },
            geometry: geometry(),
            model,
        };
        let rgb: Vec<u8> = (1..=9).flat_map(|i| [i * 10, 2, 3]).collect();
        for count in 1..=2 {
            jobs.request(Input::new(key.clone(), [3, 3], Arc::from(rgb.clone())).unwrap())
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            while jobs.counts().unwrap().completed < count {
                assert!(Instant::now() < deadline);
                assert_eq!(jobs.counts().unwrap().failed, 0);
                std::thread::sleep(Duration::from_millis(1));
            }
            profile
                .record(&jobs.take_prepared_ready(&key).unwrap().unwrap())
                .unwrap();
        }
        // Controlled reports prove Session plumbing, not actual quality, whole
        // memory, thermal certification or automatic device-tier eligibility.
        let qualification = admission::Qualification {
            model: key.model.clone(),
            geometry: key.geometry.clone(),
            execution: execution(&key.model),
            cohort_sha256: admission::cohort_sha256(&profile).unwrap(),
            environment_sha256: context().environment_sha256,
            measurement_protocol_sha256: context().measurement_protocol_sha256,
            domain: context().domain,
            quality: Some(admission::Quality {
                record_sha256: [81; 32],
                independent_reference_sha256: [82; 32],
                samples: 2,
                normalized_rms: 0.,
                normalized_maximum: 0.,
                maximum_additional_transition_jump: 0.,
            }),
            memory: Some(admission::Memory {
                record_sha256: [83; 32],
                host_peak_bytes: 1024,
                device_peak_bytes: 2048,
                weight_bytes: 256,
                activation_peak_bytes: 512,
            }),
            sustained: Some(admission::Sustained {
                record_sha256: [84; 32],
                samples: 2,
                failed: 0,
                canceled: 0,
                duration: Duration::from_secs(2),
                maximum_worker_latency: Duration::from_secs(1),
                maximum_temperature_celsius: 80.,
            }),
        };
        let requested = admission::Request {
            tier: admission::Tier::Standard,
            domain: context().domain,
            environment_sha256: context().environment_sha256,
            measurement_protocol_sha256: context().measurement_protocol_sha256,
            geometry: key.geometry,
            model: Some(key.model),
            backend: None,
            minimum_samples: 2,
            maximum_worker_latency: Duration::from_secs(10),
            maximum_host_peak_bytes: 1024,
            maximum_device_peak_bytes: 2048,
            maximum_quality_rms: 0.1,
            maximum_quality_error: 0.2,
            maximum_transition_jump: 0.05,
            minimum_sustained_duration: Duration::from_secs(2),
            maximum_temperature_celsius: 80.,
        };
        choices.push(
            admission::select(
                requested,
                &[admission::Input {
                    profile: &profile,
                    qualification: Some(&qualification),
                }],
            )
            .unwrap(),
        );
    }
    admission::Bindings::new(context(), &choices.iter().collect::<Vec<_>>()).unwrap()
}
fn prepared(root: &Path) -> (Session, mpsc::Receiver<Seen>, Arc<AtomicUsize>) {
    let initial = calibrated_source_session(root);
    let mut project = initial.engine.snapshot();
    project.layers[0].three_d = true;
    drop(initial);
    let mut session = Session::new(project, root.into(), Arc::new(NoMedia)).unwrap();
    let epoch = Arc::new(AtomicUsize::new(0));
    let (backend, seen) = worker(epoch.clone());
    session
        .configure_qualified_depth_in(backend, root.join("depth"), bindings(), None)
        .unwrap();
    assert_eq!(
        session
            .depth_qualified_bindings()
            .unwrap()
            .unwrap()
            .decisions()
            .len(),
        2
    );
    session.request_image_depth(7, geometry(), model()).unwrap();
    wait_done(&mut session, 1);
    session.poll_depth().unwrap().unwrap();
    let command = calibration_command(&mut session);
    session.apply_command_batch(vec![command]).unwrap();
    session.request_image_depth(7, geometry(), model()).unwrap();
    wait_done(&mut session, 2);
    session.poll_depth().unwrap().unwrap();
    session
        .begin_image_depth_transition(settled(), policy(), limits())
        .unwrap();
    session
        .request_image_depth_transition(Activity::Interactive, Duration::ZERO)
        .unwrap()
        .unwrap();
    wait_done(&mut session, 3);
    assert!(session.poll_image_depth_transition().unwrap());
    (session, seen, epoch)
}

#[test]
fn qualified_session_transitions_and_frozen_roundtrip_keep_calibration_and_exact_routes() {
    let root = tempfile::tempdir().unwrap();
    let (mut session, seen, _epoch) = prepared(root.path());
    assert!(session
        .request_image_depth_transition(Activity::Idle, Duration::from_millis(100))
        .unwrap()
        .is_none());
    session
        .request_image_depth_transition(Activity::Idle, Duration::from_millis(400))
        .unwrap()
        .unwrap();
    wait_done(&mut session, 4);
    assert!(session.poll_image_depth_transition().unwrap());
    assert!(session.bound_depth_calibration(1).unwrap().is_some());
    let project = session.engine.snapshot();
    let snapshot = DepthSnapshot::capture(&session).unwrap();
    snapshot.validate_project(&project).unwrap();
    assert_eq!(snapshot.layer_count(), 1);
    let mut wire = Vec::new();
    snapshot.write_to(&mut wire, &|| Ok(())).unwrap();
    let restored = DepthSnapshot::read_from(&mut &wire[..], &|| Ok(())).unwrap();
    restored.validate_project(&project).unwrap();
    assert_eq!(restored.layer_count(), 1);
    assert_eq!(
        seen.try_iter().map(|v| v.key.model).collect::<Vec<_>>(),
        vec![model(), model(), model(), settled()]
    );
    assert_eq!(session.depth_status().unwrap().unwrap().counts.submitted, 4);
}

#[test]
fn qualified_session_changed_runtime_cannot_freeze_old_reference_or_retry_settled_backend() {
    let root = tempfile::tempdir().unwrap();
    let (mut session, seen, epoch) = prepared(root.path());
    epoch.store(1, Ordering::SeqCst);
    assert!(session
        .request_image_depth_transition(Activity::Idle, Duration::from_millis(100))
        .unwrap()
        .is_none());
    session
        .request_image_depth_transition(Activity::Idle, Duration::from_millis(400))
        .unwrap()
        .unwrap();
    wait_done(&mut session, 4);
    assert!(session
        .poll_image_depth_transition()
        .unwrap_err()
        .to_string()
        .contains("qualified execution differs"));
    assert!(DepthSnapshot::capture(&session)
        .unwrap_err()
        .to_string()
        .contains("pending"));
    let status = session.depth_status().unwrap().unwrap();
    assert_eq!(status.counts.submitted, 4);
    assert_eq!(status.counts.failed, 1);
    assert_eq!(
        seen.try_iter().count(),
        3,
        "changed identity cannot enter inference or trigger a retry"
    );
}
