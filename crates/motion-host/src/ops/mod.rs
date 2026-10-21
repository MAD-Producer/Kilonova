//! Versioned editing operations shared by every host.
//!
//! Each function takes a [`Session`] and returns the JSON payload the UI
//! receives. Android wraps these in JNI entry points; the desktop shell calls
//! them through [`Host`]. Keeping the wire shapes identical is what makes the
//! two applications SDK-equivalent: the desktop UI cannot accidentally depend on
//! a capability that only exists on one platform.
use crate::platform::Platform;
use crate::session::Result;
use serde_json::{json, Value};
use std::sync::Arc;

pub mod composition;
pub mod alignment;
pub mod editing;
pub mod effects;
pub mod export;
pub mod geometry;
pub mod fonts;
pub mod images;
pub mod media;
pub mod preview;
pub mod preview_inputs;
pub mod project;
pub mod snapshot;

/// A host application bound to one platform backend.
///
/// Every editing operation lives on this type, so the desktop shell and the
/// Android bridge call exactly the same code with the same wire formats.
pub struct Host {
    platform: Arc<dyn Platform>,
}

impl Host {
    pub fn new(platform: Arc<dyn Platform>) -> Self {
        Self { platform }
    }

    pub fn platform(&self) -> &Arc<dyn Platform> {
        &self.platform
    }

    pub fn create(&self, root: std::path::PathBuf, project_text: &str) -> Result<i64> {
        project::create(root, project_text, self.platform.clone())
    }
}

/// Standard `{"ok":…}` envelope shared with the Android bridge.
pub fn envelope<T: serde::Serialize>(value: T) -> Value {
    json!({"ok": true, "data": value})
}

/// Legacy text-only failure envelope. New native operations use
/// [`typed_error_envelope`]; the prefix decoder remains only for old text callers.
pub fn error_envelope(error: &str) -> Value {
    json!({
        "ok": false,
        "error": error,
        "error_detail": error
            .strip_prefix("composition_error:")
            .and_then(|v| serde_json::from_str::<Value>(v).ok()),
    })
}

/// Preserve native identity without parsing the display text.
pub fn typed_error_envelope(error: &crate::Error) -> Value {
    json!({"ok": false, "error": error.to_string(),
        "error_detail": error.composition_detail()})
}

/// Wrap a host operation in the envelope and in a panic boundary.
///
/// Every host entry point routes through this so a panic in one operation can
/// never take down the process or leave a session wedged.
pub fn dispatch(operation: impl FnOnce() -> Result<Value>) -> Value {
    match catch_panic(operation) {
        Ok(Ok(value)) => envelope(value),
        Ok(Err(error)) => typed_error_envelope(&error),
        Err(reason) => typed_error_envelope(&crate::Error::Message(reason)),
    }
}

/// Shared native panic formatting. Normal errors and panics remain distinct so
/// a stateful worker can close a failed session without stopping on ordinary Err.
pub fn catch_panic<T>(operation: impl FnOnce() -> T) -> std::result::Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation)).map_err(|payload| {
        let reason = payload.downcast_ref::<String>().map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or("unknown native error");
        reason.chars().take(2048).collect()
    })
}

/// Parse a host request body, rejecting oversized payloads before any work.
pub fn parse_request(text: &str, limit: usize, what: &str) -> Result<Value> {
    if text.len() > limit {
        return Err(format!("{what} too large").into());
    }
    serde_json::from_str(text).map_err(crate::Error::from)
}

/// Nanosecond suffix used for import/export sibling directories.
pub fn stamp() -> Result<u128> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(crate::Error::from)
        .map(|d| d.as_nanos())
}
