//! Explicit input-study attribution. Compiled out of the application build.
use std::{cell::RefCell, time::Instant};

#[derive(serde::Serialize)]
struct Timing {
    phase: &'static str,
    elapsed_us: u128,
}
#[derive(serde::Serialize)]
struct Trace {
    records: Vec<Timing>,
    dropped: u64,
}
thread_local! {
    static TRACE: RefCell<Option<Trace>> = const { RefCell::new(None) };
}

pub(crate) fn begin() {
    // Allocate before timing; markers never grow the buffer or perform IO.
    TRACE.with(|trace| *trace.borrow_mut() = Some(Trace {
        records: Vec::with_capacity(64),
        dropped: 0,
    }));
}
pub(crate) fn finish() -> serde_json::Value {
    TRACE.with(|trace| serde_json::to_value(trace.borrow_mut().take()).unwrap())
}

pub(super) struct DrawStage(Option<Instant>);
impl DrawStage {
    pub(super) fn start() -> Self {
        Self(TRACE.with(|trace| trace.borrow().is_some().then(Instant::now)))
    }
    pub(super) fn next(&mut self, phase: &'static str) {
        let Some(started) = self.0 else { return };
        let ended = Instant::now();
        TRACE.with(|trace| {
            if let Some(trace) = trace.borrow_mut().as_mut() {
                if trace.records.len() < 64 {
                    trace.records.push(Timing {
                        phase,
                        elapsed_us: ended.duration_since(started).as_micros(),
                    });
                } else {
                    trace.dropped = trace.dropped.saturating_add(1);
                }
            }
        });
        // Attribute marker overhead to the next stage; never overlap phases.
        self.0 = Some(ended);
    }
}

#[test]
fn draw_evidence_is_explicit_bounded_and_reset_between_samples() {
    let mut inactive = DrawStage::start();
    inactive.next("inactive");
    assert!(finish().is_null());
    begin();
    let mut draw = DrawStage::start();
    for _ in 0..70 { draw.next("controlled"); }
    let trace = finish();
    assert_eq!(trace["records"].as_array().unwrap().len(), 64);
    assert_eq!(trace["dropped"], 6);
    assert!(trace["records"].as_array().unwrap().iter().all(|r| r["phase"] == "controlled"));
    assert!(finish().is_null());
    begin();
    let trace = finish();
    assert_eq!(trace["records"].as_array().unwrap().len(), 0);
    assert_eq!(trace["dropped"], 0);
}
