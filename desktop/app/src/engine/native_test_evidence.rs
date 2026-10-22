//! Nonblocking context for a native test whose reply deadline expired.
use super::*;

impl Engine {
    pub(crate) fn native_wait_snapshot(&self) -> Value {
        let (flight, lock) = match self.in_flight.try_lock() {
            Ok(guard) => (*guard, "available"),
            Err(std::sync::TryLockError::Poisoned(error)) => (*error.into_inner(), "poisoned"),
            Err(std::sync::TryLockError::WouldBlock) => (None, "busy"),
        };
        serde_json::json!({
            "queue_depth":self.queue_depth(),"flight_lock":lock,
            "in_flight":flight.map(|flight|serde_json::json!({
                "request":flight.request,"operation":flight.operation,
                "elapsed_ms":flight.started.elapsed().as_millis()
            })),
            "native":self.diagnostics.native_test_snapshot()
        })
    }
}
