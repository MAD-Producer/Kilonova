#![cfg(feature = "ai-depth")]

use motion_host::depth::{Backend, Cancellation, Costs, Error, Estimate, Input, Jobs, Key, Result};
use motion_model::depth::{
    DepthDescriptor, DepthGeometry, DepthMeasure, DepthModel, DepthResource, DepthSource,
    DepthTime, NearDirection,
};
use std::{
    sync::{mpsc, Arc, Mutex, Once},
    time::{Duration, Instant},
};

fn key() -> Key {
    Key {
        source: DepthSource {
            asset: 7,
            revision: 8,
            pts: DepthTime {
                ticks: 3,
                timescale: 24,
            },
        },
        geometry: DepthGeometry {
            source_size: [2, 1],
            crop: [0, 0, 2, 1],
            quarter_turns: 0,
            mirror_x: false,
        },
        model: DepthModel {
            id: "controlled-depth".into(),
            revision: "r1".into(),
            sha256: [42; 32],
            processing: "rgb-test-v1".into(),
        },
    }
}
fn input(key: Key) -> Input {
    Input::new(key, [2, 1], Arc::from([1, 2, 3, 4, 5, 6])).unwrap()
}
fn estimate(input: &Input) -> Result<Estimate> {
    let k = input.key();
    Ok(Estimate {
        resource: DepthResource::new(
            DepthDescriptor {
                size: [2, 1],
                source: k.source.clone(),
                geometry: k.geometry.clone(),
                model: k.model.clone(),
                measure: DepthMeasure::Relative {
                    near: NearDirection::Increasing,
                },
                scale: 1.,
                offset: 0.,
                valid_range: None,
            },
            vec![0.25, 2.],
            None,
            None,
        )?,
        provider: "controlled_cpu".into(),
        costs: Costs::default(),
    })
}

struct Controlled {
    entered: mpsc::Sender<Key>,
    release: mpsc::Receiver<()>,
    calls: usize,
}
impl Backend for Controlled {
    fn infer(&mut self, input: &Input, cancellation: &Cancellation) -> Result<Estimate> {
        self.calls += 1;
        self.entered.send(input.key().clone()).unwrap();
        if self.calls == 1 {
            self.release.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        // Deliberately return an old success after cancellation: the host must
        // reject it even when a backend missed its own cooperative check.
        if self.calls > 1 {
            cancellation.check()?;
        }
        estimate(input)
    }
}
fn controlled() -> (Jobs, mpsc::Receiver<Key>, mpsc::Sender<()>) {
    let (entered, rx) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    (
        Jobs::start(Controlled {
            entered,
            release: blocked,
            calls: 0,
        })
        .unwrap(),
        rx,
        release,
    )
}
fn wait_done(jobs: &Jobs, count: u64) {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let c = jobs.counts().unwrap();
        if c.completed + c.failed + c.canceled >= count {
            return;
        }
        assert!(Instant::now() < until, "depth completion timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn blocked_depth_publishes_flight_before_backend_and_retains_only_latest_pending_sample() {
    init_log();
    let (mut jobs, entered, release) = controlled();
    let first = jobs.request(input(key())).unwrap();
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    let flight = jobs.in_flight().unwrap().unwrap();
    assert_eq!(flight.request, first);
    assert_eq!(flight.source, key().source);
    assert!(LOG
        .records
        .lock()
        .unwrap()
        .iter()
        .any(|(_, s)| s.contains("start #1 depth_infer asset=7")));
    assert_eq!(jobs.queue_depth().unwrap(), 0);
    let mut replacement = key();
    for tick in 4..=100 {
        replacement.source.pts.ticks = tick;
        jobs.request(input(replacement.clone())).unwrap();
        assert_eq!(jobs.queue_depth().unwrap(), 1);
    }
    assert_eq!(jobs.counts().unwrap().superseded_pending, 96);
    release.send(()).unwrap();
    let next = entered.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(next.source.pts.ticks, 100);
    wait_done(&jobs, 2);
    assert_eq!(jobs.counts().unwrap().canceled, 1);
    assert!(jobs.in_flight().unwrap().is_none());
    assert_eq!(jobs.queue_depth().unwrap(), 0);
    let output = jobs.take_ready(&replacement).unwrap().unwrap();
    assert_eq!(output.resource.descriptor().source, replacement.source);
    assert!(jobs.take_ready(&replacement).unwrap().is_none());
}

#[test]
fn cancel_without_replacement_rejects_late_success_then_recovers() {
    let (mut jobs, entered, release) = controlled();
    jobs.request(input(key())).unwrap();
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    jobs.cancel().unwrap();
    release.send(()).unwrap();
    wait_done(&jobs, 1);
    assert_eq!(jobs.counts().unwrap().canceled, 1);
    assert!(jobs.take_ready(&key()).unwrap().is_none());
    jobs.request(input(key())).unwrap();
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    wait_done(&jobs, 2);
    assert!(jobs.take_ready(&key()).unwrap().is_some());
}

struct Immediate;
impl Backend for Immediate {
    fn infer(&mut self, input: &Input, _: &Cancellation) -> Result<Estimate> {
        estimate(input)
    }
}

#[test]
fn live_source_revision_seek_model_processing_and_geometry_are_checked_at_installation() {
    let mut jobs = Jobs::start(Immediate).unwrap();
    let mut mutations = vec![key(); 7];
    mutations[0].source.asset += 1;
    mutations[1].source.revision += 1;
    mutations[2].source.pts.ticks += 1;
    mutations[3].model.revision.push('2');
    mutations[4].model.sha256[0] ^= 1;
    mutations[5].model.processing.push('2');
    mutations[6].geometry.mirror_x = true;
    for (i, live) in mutations.iter().enumerate() {
        jobs.request(input(key())).unwrap();
        wait_done(&jobs, i as u64 + 1);
        assert!(
            jobs.take_ready(live).unwrap().is_none(),
            "installed stale mutation {i}"
        );
    }
    assert_eq!(jobs.counts().unwrap().stale, 7);
    jobs.request(input(key())).unwrap();
    wait_done(&jobs, 8);
    let mut rational_same_time = key();
    rational_same_time.source.pts = DepthTime {
        ticks: 6,
        timescale: 48,
    };
    assert!(jobs.take_ready(&rational_same_time).unwrap().is_some());
}

struct InvalidThenGood {
    calls: usize,
}
impl Backend for InvalidThenGood {
    fn infer(&mut self, input: &Input, _: &Cancellation) -> Result<Estimate> {
        self.calls += 1;
        match self.calls {
            1 => Err(Error::Backend("missing model file: controlled.onnx".into())),
            2 => {
                let mut bad = input.key().clone();
                bad.source.revision += 1;
                estimate(&super_input(bad))
            }
            3 => {
                let mut result = estimate(input)?;
                result.provider.clear();
                Ok(result)
            }
            _ => estimate(input),
        }
    }
}
fn super_input(k: Key) -> Input {
    input(k)
}

#[test]
fn missing_model_wrong_output_and_unknown_provider_preserve_failure_and_default_info_logs() {
    init_log();
    let mut jobs = Jobs::start(InvalidThenGood { calls: 0 }).unwrap();
    let causes = [
        "missing model file",
        "result identity differs",
        "execution provider",
    ];
    for (i, cause) in causes.iter().enumerate() {
        jobs.request(input(key())).unwrap();
        wait_done(&jobs, i as u64 + 1);
        let err = jobs.take_ready(&key()).unwrap_err();
        assert!(err.to_string().contains(cause), "{err}");
        assert!(LOG
            .records
            .lock()
            .unwrap()
            .iter()
            .any(|(level, line)| *level == log::Level::Error
                && line.contains(cause)
                && line.contains("asset=7")
                && line.contains("elapsed_ms=")));
    }
    jobs.request(input(key())).unwrap();
    wait_done(&jobs, 4);
    assert!(jobs.take_ready(&key()).unwrap().is_some());
    assert_eq!(jobs.counts().unwrap().failed, 3);
}

struct Panics;
impl Backend for Panics {
    fn infer(&mut self, _: &Input, _: &Cancellation) -> Result<Estimate> {
        panic!("controlled depth backend panic")
    }
}
#[test]
fn backend_panic_is_a_terminal_visible_failure_instead_of_restarting_backend() {
    let mut jobs = Jobs::start(Panics).unwrap();
    jobs.request(input(key())).unwrap();
    wait_done(&jobs, 1);
    assert!(
        matches!(jobs.take_ready(&key()), Err(Error::Panic(reason)) if reason.contains("controlled depth"))
    );
    assert!(matches!(jobs.request(input(key())), Err(Error::Stopped)));
    assert!(jobs.in_flight().unwrap().is_none());
}

#[test]
fn frame_contract_rejects_wrong_byte_count_orientation_and_oversized_input() {
    assert!(Input::new(key(), [2, 1], Arc::from([1u8; 5])).is_err());
    assert!(Input::new(key(), [1, 2], Arc::from([1u8; 6])).is_err());
    let mut rotated = key();
    rotated.geometry.quarter_turns = 1;
    assert!(Input::new(rotated, [1, 2], Arc::from([1u8; 6])).is_ok());
    let mut big = key();
    big.geometry.source_size = [4097, 1];
    big.geometry.crop = [0, 0, 4097, 1];
    assert!(Input::new(big, [4097, 1], Arc::from([])).is_err());
}

struct Capture {
    records: Mutex<Vec<(log::Level, String)>>,
}
static LOG: Capture = Capture {
    records: Mutex::new(Vec::new()),
};
impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        let mut records = self.records.lock().unwrap();
        if records.len() == 1024 {
            records.remove(0);
        }
        records.push((record.level(), record.args().to_string()));
    }
    fn flush(&self) {}
}
fn init_log() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        log::set_logger(&LOG).unwrap();
        log::set_max_level(log::LevelFilter::Info);
    });
}
