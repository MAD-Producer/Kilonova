//! Collect an existing preparation even when no frame continuation owns it.
//! This owner-only step never samples, creates a task, waits for a driver or
//! retries a failure. The preparation's original start/ticket remain in use.
use super::*;

pub(super) const POLL_INTERVAL: Duration = Duration::from_millis(8);

// A one-shot owner-thread boundary for native fault acceptance. No hook or
// injected operation is compiled into the application binary.
#[cfg(test)]
thread_local! {
    static BEFORE_ADVANCE: std::cell::RefCell<Option<Box<dyn FnOnce(i64)>>> =
        const { std::cell::RefCell::new(None) };
}

pub(super) fn advance(id: i64) -> Result<(), Error> {
    #[cfg(test)]
    if let Some(before) = BEFORE_ADVANCE.with_borrow_mut(Option::take) {
        before(id);
    }
    let (result, context) = motion_host::session::with_session(id, |session| {
        let result = session.poll_pipeline_preparation();
        let context = result.as_ref().err().map(|_| {
            session.preview_info()["pipelinePreparation"]["task"].clone()
        });
        Ok((result, context))
    })?;
    if let Err(error) = result {
        // State retains the original Arc error for the next actual frame and
        // clears the observation. Log outside the session lock, once; this
        // branch does not clear a failure or turn a frame reply into success.
        let context = context.unwrap_or(Value::Null);
        log::error!("pipeline preparation owner completion failed session={id} request={} generation={} revision={} phase={} elapsed_ms={} cause={}",
            context["request"], context["context"]["documentGeneration"],
            context["context"]["projectRevision"], context["phase"], context["elapsedMs"],
            crate::fallback_observation::error_cause(&error));
    }
    Ok(())
}

#[cfg(all(test, target_os = "windows"))]
mod native_tests;
