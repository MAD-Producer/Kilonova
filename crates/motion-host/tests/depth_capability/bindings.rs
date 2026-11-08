use super::*;
use motion_host::depth::capability::policy::{self, Bindings, Context, Decision};
use std::sync::{Mutex, Once};

// External quality/memory/sustained reports remain explicitly controlled test
// fixtures. These tests qualify routing invariants, not a real model or device.
fn decision(k: Key, execution: Execution) -> Decision {
    let profile = sampled_profile(k, execution, [1, 2, 3, 4, 5, 6], 2);
    let qualification = policy_qualification(&profile);
    let mut request = policy_request();
    request.geometry = profile.summary().unwrap().geometry().clone();
    policy::select(
        request,
        &[policy::Input {
            profile: &profile,
            qualification: Some(&qualification),
        }],
    )
    .unwrap()
}
fn context() -> Context {
    Context {
        environment_sha256: [80; 32],
        measurement_protocol_sha256: [85; 32],
        domain: "authored-control-domain".into(),
    }
}
fn bindings() -> Bindings {
    Bindings::new(context(), &[&decision(key(), execution())]).unwrap()
}
fn cache(root: &std::path::Path) -> Config {
    Config {
        directory: root.into(),
        byte_limit: 1 << 20,
        entry_limit: 4,
    }
}

struct Logs(Mutex<Vec<(log::Level, String)>>);
static LOGS: Logs = Logs(Mutex::new(Vec::new()));
impl log::Log for Logs {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            let mut lines = self.0.lock().unwrap();
            if lines.len() == 128 {
                lines.remove(0);
            }
            lines.push((record.level(), record.args().to_string()));
        }
    }
    fn flush(&self) {}
}
fn init_log() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        log::set_logger(&LOGS).unwrap();
        log::set_max_level(log::LevelFilter::Info);
    });
}

#[test]
fn bindings_require_current_context_and_preserve_complete_decisions() {
    let selected = decision(key(), execution());
    for field in 0..6 {
        let mut changed = context();
        match field {
            0 => changed.environment_sha256 = [0; 32],
            1 => changed.measurement_protocol_sha256 = [0; 32],
            2 => changed.domain = "x".repeat(129),
            3 => changed.environment_sha256 = [81; 32],
            4 => changed.measurement_protocol_sha256 = [86; 32],
            _ => changed.domain.push_str("-different"),
        }
        assert!(Bindings::new(changed, &[&selected]).is_err());
    }
    let retained = Bindings::new(context(), &[&selected]).unwrap();
    drop(selected);
    assert_eq!(retained.context(), &context());
    let retained = &retained.decisions()[0];
    assert_eq!(retained.requested().maximum_host_peak_bytes, 1024);
    assert_eq!(retained.resolved().unwrap().execution(), &execution());
    assert_eq!(
        retained.candidates()[0]
            .qualification()
            .unwrap()
            .quality
            .as_ref()
            .unwrap()
            .independent_reference_sha256,
        [82; 32]
    );
    assert!(retained.candidates()[0].refusals().is_empty());
    assert!(Bindings::new(context(), &[]).is_err());
    let refused = policy::select(policy_request(), &[]).unwrap();
    assert!(Bindings::new(context(), &[&refused])
        .unwrap_err()
        .to_string()
        .contains("precomputed depth is required"));
}

#[test]
fn bindings_bound_distinct_routes_and_refuse_ambiguous_model_geometry() {
    let choices: Vec<_> = (1..=9)
        .map(|index| {
            let mut k = key();
            k.model.sha256 = [index; 32];
            decision(k, execution())
        })
        .collect();
    let refs: Vec<_> = choices.iter().collect();
    assert_eq!(
        Bindings::new(context(), &refs[..8])
            .unwrap()
            .decisions()
            .len(),
        8
    );
    assert!(Bindings::new(context(), &refs).is_err());
    let mut alternate = execution();
    alternate.provider = "another-provider".into();
    let a = decision(key(), execution());
    let b = decision(key(), alternate);
    assert!(Bindings::new(context(), &[&a, &b])
        .unwrap_err()
        .to_string()
        .contains("ambiguous"));
}

#[test]
fn qualified_worker_publishes_identity_resolution_before_blocking_at_default_info() {
    init_log();
    let mut controlled = backend(Fault::None);
    let resolves = controlled.resolves.clone();
    let infers = controlled.infers.clone();
    let (entered, rx) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    controlled.block = Some((entered, blocked));
    let mut jobs = Jobs::start_qualified(controlled, bindings(), None).unwrap();
    LOGS.0.lock().unwrap().clear();
    let guard = Release(release);
    let mut k = key();
    k.source.asset = 75001;
    let request = jobs
        .request(Input::new(k.clone(), [2, 1], Arc::from([1u8, 2, 3, 4, 5, 6])).unwrap())
        .unwrap();
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let flight = jobs.in_flight().unwrap().unwrap();
    assert_eq!(flight.request, request);
    assert_eq!(flight.source, Some(k.source.clone()));
    assert_eq!(flight.phase, "qualified_resolve");
    assert_eq!(infers.load(Ordering::SeqCst), 0);
    assert!(LOGS
        .0
        .lock()
        .unwrap()
        .iter()
        .any(|(level, line)| *level == log::Level::Info
            && line
                == &format!(
                    "start #{request} depth_qualification asset=75001 phase=qualified_resolve"
                )));
    drop(guard);
    wait_done(&jobs, 1);
    let ready = jobs.take_prepared_ready(&k).unwrap().unwrap();
    assert!(
        ready.measurement().is_none(),
        "qualified use is not a new capability probe"
    );
    assert_eq!(ready.estimate().provider, execution().provider);
    assert_eq!(resolves.load(Ordering::SeqCst), 1);
    assert_eq!(infers.load(Ordering::SeqCst), 1);
    assert_eq!(jobs.qualified_bindings().unwrap().decisions().len(), 1);
    assert!(jobs.in_flight().unwrap().is_none());
}

#[test]
fn qualified_worker_rejects_each_changed_execution_field_before_inference_or_cache_read() {
    let root = tempfile::tempdir().unwrap();
    let admitted = bindings();
    let mut populate = Jobs::start_qualified(
        backend(Fault::None),
        admitted.clone(),
        Some(cache(root.path())),
    )
    .unwrap();
    populate.request(input()).unwrap();
    take(&mut populate, 1);
    drop(populate);
    for cached in [false, true] {
        for field in 0..5 {
            let mut controlled = backend(Fault::None);
            match field {
                0 => controlled.execution.backend.push_str("-changed"),
                1 => controlled.execution.provider.push_str("-changed"),
                2 => controlled.execution.runtime_sha256 = [26; 32],
                3 => controlled.execution.plan_sha256 = [27; 32],
                _ => controlled.execution.precision = "float16".into(),
            }
            let infers = controlled.infers.clone();
            let mut jobs = Jobs::start_qualified(
                controlled,
                admitted.clone(),
                cached.then(|| cache(root.path())),
            )
            .unwrap();
            jobs.request(input()).unwrap();
            wait_done(&jobs, 1);
            let cause = jobs.take_prepared_ready(&key()).unwrap_err();
            assert!(
                matches!(cause, Error::Backend(ref message) if message.contains("qualified execution differs")
                && message.contains("required=") && message.contains("resolved="))
            );
            assert_eq!(infers.load(Ordering::SeqCst), 0);
            assert_eq!(jobs.counts().unwrap().cache_hits, 0);
            assert_eq!(jobs.counts().unwrap().cache_misses, 0);
            assert_eq!(jobs.counts().unwrap().failed, 1);
        }
    }
}

#[test]
fn qualified_worker_refuses_missing_invalid_or_unbound_identity_without_fallback() {
    let admitted = bindings();
    for fault in [Fault::Unsupported, Fault::InvalidIdentity] {
        let controlled = backend(fault);
        let infers = controlled.infers.clone();
        let mut jobs = Jobs::start_qualified(controlled, admitted.clone(), None).unwrap();
        jobs.request(input()).unwrap();
        wait_done(&jobs, 1);
        assert!(matches!(
            jobs.take_prepared_ready(&key()),
            Err(Error::Invalid(_))
        ));
        assert_eq!(infers.load(Ordering::SeqCst), 0);
    }
    let controlled = backend(Fault::None);
    let resolves = controlled.resolves.clone();
    let infers = controlled.infers.clone();
    let mut jobs = Jobs::start_qualified(controlled, admitted, None).unwrap();
    for field in 0..2 {
        let mut k = key();
        if field == 0 {
            k.model.processing.push_str("-unbound");
        } else {
            k.geometry.mirror_x = true;
        }
        jobs.request(Input::new(k.clone(), [2, 1], Arc::from([1u8, 2, 3, 4, 5, 6])).unwrap())
            .unwrap();
        wait_done(&jobs, field + 1);
        assert!(
            matches!(jobs.take_prepared_ready(&k), Err(Error::Invalid(message))
            if message.contains("no qualified inference binding"))
        );
    }
    assert_eq!(resolves.load(Ordering::SeqCst), 0);
    assert_eq!(infers.load(Ordering::SeqCst), 0);
    jobs.request(input()).unwrap();
    take(&mut jobs, 3);
    assert_eq!(
        infers.load(Ordering::SeqCst),
        1,
        "only the explicit valid request infers"
    );
}

#[test]
fn qualified_worker_preserves_primary_result_faults_and_panic_shutdown() {
    let admitted = bindings();
    for fault in [
        Fault::Provider,
        Fault::Resource,
        Fault::Backend,
        Fault::Panic,
    ] {
        let controlled = backend(fault);
        let infers = controlled.infers.clone();
        let mut jobs = Jobs::start_qualified(controlled, admitted.clone(), None).unwrap();
        jobs.request(input()).unwrap();
        wait_done(&jobs, 1);
        let cause = jobs.take_prepared_ready(&key()).unwrap_err();
        match fault {
            Fault::Backend => {
                assert!(matches!(cause, Error::Backend(ref c) if c == "original probe cause"))
            }
            Fault::Panic => {
                assert!(matches!(cause, Error::Panic(ref c) if c == "original probe panic"));
                assert!(matches!(jobs.request(input()), Err(Error::Stopped)));
            }
            _ => assert!(matches!(cause, Error::Invalid(_))),
        }
        assert_eq!(infers.load(Ordering::SeqCst), 1);
        assert_eq!(jobs.counts().unwrap().failed, 1);
        assert!(jobs.in_flight().unwrap().is_none());
    }
}

#[test]
fn qualified_cache_reuse_still_resolves_actual_execution_without_inference() {
    let root = tempfile::tempdir().unwrap();
    let admitted = bindings();
    let mut initial = Jobs::start_qualified(
        backend(Fault::None),
        admitted.clone(),
        Some(cache(root.path())),
    )
    .unwrap();
    initial.request(input()).unwrap();
    take(&mut initial, 1);
    drop(initial);
    let controlled = backend(Fault::None);
    let resolves = controlled.resolves.clone();
    let infers = controlled.infers.clone();
    let mut jobs =
        Jobs::start_qualified(controlled, admitted.clone(), Some(cache(root.path()))).unwrap();
    jobs.request(input()).unwrap();
    assert!(take(&mut jobs, 1).measurement().is_none());
    assert_eq!(resolves.load(Ordering::SeqCst), 1);
    assert_eq!(infers.load(Ordering::SeqCst), 0);
    assert_eq!(jobs.counts().unwrap().cache_hits, 1);
    let mut invalid = cache(root.path());
    invalid.entry_limit = 0;
    assert!(Jobs::start_qualified(backend(Fault::None), admitted, Some(invalid)).is_err());
}

#[test]
fn qualified_resolution_cancellation_prevents_inference_and_publication() {
    let mut controlled = backend(Fault::None);
    let infers = controlled.infers.clone();
    let (entered, rx) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    controlled.block = Some((entered, blocked));
    let mut jobs = Jobs::start_qualified(controlled, bindings(), None).unwrap();
    let guard = Release(release);
    jobs.request(input()).unwrap();
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    jobs.cancel().unwrap();
    drop(guard);
    wait_done(&jobs, 1);
    assert_eq!(infers.load(Ordering::SeqCst), 0);
    assert_eq!(jobs.counts().unwrap().canceled, 1);
    assert!(jobs.take_prepared_ready(&key()).unwrap().is_none());
    assert!(jobs.in_flight().unwrap().is_none());
}

#[test]
fn qualified_two_model_router_keeps_each_selected_provider_and_plan() {
    let mut other = key();
    other.model.processing.push_str("-settled");
    let mut actual = execution();
    actual.provider = "settled-provider".into();
    actual.plan_sha256 = [91; 32];
    let a = decision(key(), execution());
    let b = decision(other.clone(), actual.clone());
    let admitted = Bindings::new(context(), &[&a, &b]).unwrap();
    let first = backend(Fault::None);
    let infers_a = first.infers.clone();
    let mut second = backend(Fault::None);
    second.execution = actual.clone();
    let infers_b = second.infers.clone();
    let routed =
        motion_host::depth::routing::Routed::new(key().model, first, other.model.clone(), second)
            .unwrap();
    let mut jobs = Jobs::start_qualified(routed, admitted, None).unwrap();
    jobs.request(input()).unwrap();
    assert_eq!(take(&mut jobs, 1).estimate().provider, execution().provider);
    jobs.request(Input::new(other.clone(), [2, 1], Arc::from([1u8, 2, 3, 4, 5, 6])).unwrap())
        .unwrap();
    wait_done(&jobs, 2);
    assert_eq!(
        jobs.take_prepared_ready(&other)
            .unwrap()
            .unwrap()
            .estimate()
            .provider,
        actual.provider
    );
    assert_eq!(infers_a.load(Ordering::SeqCst), 1);
    assert_eq!(infers_b.load(Ordering::SeqCst), 1);
}
