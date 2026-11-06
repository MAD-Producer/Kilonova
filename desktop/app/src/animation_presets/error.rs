//! Preserve provider identity while keeping existing bounded diagnostics.
use crate::engine::Error;
use std::{error::Error as StdError, fmt, path::Path};

#[derive(Debug)]
pub(super) struct Operation {
    message: String,
    pub(super) cause: motion_host::Error,
}
impl fmt::Display for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl StdError for Operation {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.cause)
    }
}
pub(super) fn io_error(
    operation: &'static str,
    path: &Path,
    cause: impl Into<motion_host::Error>,
) -> Error {
    let cause = cause.into();
    let path: String = path.to_string_lossy().chars().take(384).collect();
    let diagnostic: String = cause.to_string().chars().take(512).collect();
    motion_host::Error::from_source(Operation {
        message: format!("{operation} path={path}: {diagnostic}"),
        cause,
    })
    .into()
}

#[derive(Debug)]
struct WorkerPanic(String);
impl fmt::Display for WorkerPanic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "preset worker panicked: {}", self.0)
    }
}
impl StdError for WorkerPanic {}
pub(super) fn panic_error(reason: &str) -> Error {
    motion_host::Error::from_source(WorkerPanic(super::bounded(reason))).into()
}

pub(super) fn cancelled(error: &Error) -> bool {
    // The storage checkpoint currently exposes this exact validation reason,
    // not a cancellation code. Match only that native provider contract: an IO
    // failure, a panic or matching display text cannot impersonate it.
    let Some(motion_host::Error::NativeSource { cause }) = error.host() else {
        return false;
    };
    let Some(operation) = cause.downcast_ref::<Operation>() else {
        return false;
    };
    matches!(&operation.cause,
        motion_host::Error::Model(motion_core::Error::Invalid(reason))
        if reason == "animation preset operation cancelled")
}
