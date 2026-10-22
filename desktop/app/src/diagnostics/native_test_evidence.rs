//! Test-only native wait evidence. Reads never wait for a worker or its locks.
use super::*;

#[derive(Clone, Copy)]
pub(super) struct Phase {
    request: u64,
    operation: &'static str,
    name: &'static str,
    began: Instant,
}

fn snapshot<T>(state: &Mutex<T>, value: impl FnOnce(&T) -> Value) -> Value {
    match state.try_lock() {
        Ok(guard) => json!({"lock":"available","value":value(&guard)}),
        Err(std::sync::TryLockError::Poisoned(error)) => {
            json!({"lock":"poisoned","value":value(&error.into_inner())})
        }
        Err(std::sync::TryLockError::WouldBlock) => json!({"lock":"busy","value":null}),
    }
}

impl Diagnostics {
    pub(crate) fn native_test_phase(&self, name: &'static str) {
        let phase = crate::engine::current_command().map(|(request, operation, _)| Phase {
            request,
            operation,
            name,
            began: Instant::now(),
        });
        static FAILURES: crate::fallback_observation::Counter =
            crate::fallback_observation::Counter::new();
        crate::fallback_observation::recovered(
            &self.native_phase,
            &FAILURES,
            "diagnostics.native_test_phase",
            |current| *current = phase,
        );
    }

    pub(crate) fn native_test_snapshot(&self) -> Value {
        json!({
            "gpu":snapshot(&self.gpu, |gpu| gpu.clone().unwrap_or(Value::Null)),
            "progress":snapshot(&self.native_phase, |phase| phase.map(|phase| json!({
                "request":phase.request,"operation":phase.operation,"phase":phase.name,
                "elapsed_ms":phase.began.elapsed().as_millis()
            })).unwrap_or(Value::Null))
        })
    }
}

#[test]
fn native_evidence_reads_busy_poisoned_and_recovered_state_without_waiting() {
    let diagnostics = Diagnostics::new(None, None);
    let empty = diagnostics.native_test_snapshot();
    assert_eq!(empty["gpu"]["lock"], "available");
    assert!(empty["gpu"]["value"].is_null());
    {
        let _gpu = diagnostics.gpu.lock().unwrap();
        let _phase = diagnostics.native_phase.lock().unwrap();
        let busy = diagnostics.native_test_snapshot();
        assert_eq!(busy["gpu"]["lock"], "busy");
        assert_eq!(busy["progress"]["lock"], "busy");
    }
    let state = Mutex::new(17);
    let _ = std::panic::catch_unwind(|| {
        let _guard = state.lock().unwrap();
        panic!("controlled evidence lock poison");
    });
    assert_eq!(snapshot(&state, |value| json!(value))["lock"], "poisoned");
    assert_eq!(snapshot(&state, |value| json!(value))["value"], 17);
    assert_eq!(
        diagnostics.native_test_snapshot()["gpu"]["lock"],
        "available"
    );
}
