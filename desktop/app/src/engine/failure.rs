//! Terminal worker failures: keep a bounded recovery document, release owned
//! resources on the worker, and report the same diagnostic to queued callers.
use super::*;
use std::io::Write;

pub(super) type Failure = Arc<std::sync::Mutex<Option<String>>>;
pub(super) enum ErrorReply {
    Clipboard(Sender<Result<crate::layer_clipboard::Read, String>>),
    KeyframeClipboard(Sender<Result<crate::layer_clipboard::KeyframeRead, String>>),
    Value(Sender<Result<Value, String>>),
    Bool(Sender<Result<bool, String>>),
    Ticket(Sender<Result<crate::project_lifecycle::Ticket, String>>),
    Movie(Sender<Result<motion_desktop_media::movie::FrozenMovie, String>>),
    Frame(Sender<Result<ArcFrame, String>>),
}
impl ErrorReply {
    pub(super) fn send(self, error: String) {
        match self {
            Self::Clipboard(reply) => { let _ = reply.send(Err(error)); }
            Self::KeyframeClipboard(reply) => { let _ = reply.send(Err(error)); }
            Self::Value(reply) => {
                let _ = reply.send(Err(error));
            }
            Self::Bool(reply) => {
                let _ = reply.send(Err(error));
            }
            Self::Ticket(reply) => {
                let _ = reply.send(Err(error));
            }
            Self::Movie(reply) => {
                let _ = reply.send(Err(error));
            }
            Self::Frame(reply) => {
                let _ = reply.send(Err(error));
            }
        }
    }
}

pub(super) struct SessionLease(pub i64);
impl Drop for SessionLease {
    fn drop(&mut self) {
        if let Err(error) = motion_host::ops::catch_panic(|| motion_host::session::close(self.0)) {
            log::error!("[motion-engine] session cleanup panicked: {error}");
        }
    }
}

const MAX_RECOVERY_BYTES: u64 = 16 * 1024 * 1024;
struct RecoveryWriter<'a> {
    file: &'a mut std::fs::File,
    remaining: u64,
}
impl Write for RecoveryWriter<'_> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        if data.len() as u64 > self.remaining {
            return Err(std::io::Error::other("recovery JSON exceeds 16 MiB"));
        }
        let written = self.file.write(data)?;
        self.remaining -= written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

pub(super) fn recovery(id: i64) -> Result<PathBuf, String> {
    motion_host::session::with_session(id, |session| {
        // Do not require external assets to still exist. They may be the cause
        // of the failure; keep their project-relative references for repair.
        session
            .engine
            .project()
            .validate()
            .map_err(|error| error.to_string())?;
        let path = session.root.join(format!(
            ".motion-recovery-{}.json",
            motion_host::ops::stamp()?
        ));
        let mut file = crate::owned_file::OwnedFile::create(&path)
            .map_err(|error| error.to_string())?;
        let result = (|| {
            let mut writer = RecoveryWriter {
                file: file.file(),
                remaining: MAX_RECOVERY_BYTES,
            };
            serde_json::to_writer(&mut writer, session.engine.project())
                .map_err(|error| error.to_string())?;
            writer.flush().map_err(|error| error.to_string())?;
            file.file().sync_all().map_err(|error| error.to_string())
        })();
        result?;
        file.keep();
        Ok(path)
    }).map_err(String::from)
}

impl Engine {
    /// Empty means pending; a disconnected response is a terminal error even
    /// when the worker died before it could publish its failure message.
    pub(crate) fn poll_reply<T>(
        &self,
        receive: &std::sync::mpsc::Receiver<Result<T, String>>,
        operation: &str,
    ) -> Option<Result<T, String>> {
        match receive.try_recv() {
            Ok(result) => Some(result),
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                let error = format!("{operation} response disconnected: {}", self.stopped_error());
                log::error!("{error}");
                Some(Err(error))
            }
        }
    }
    pub(crate) fn failed(&self) -> bool {
        self.check_running().is_err()
    }
    pub(super) fn check_running(&self) -> Result<(), String> {
        static FAILURES: crate::fallback_observation::Counter = crate::fallback_observation::Counter::new();
        if let Some(error) = crate::fallback_observation::recovered(&self.failure, &FAILURES, "engine.failure.check", |value| value.clone())
        {
            return Err(error);
        }
        if self.shutdown.load(Ordering::Acquire) {
            return Err("engine shutdown requested; new commands are not accepted".into());
        }
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.is_finished())
        {
            return Err(self.stopped_error());
        }
        Ok(())
    }
    pub(super) fn stopped_error(&self) -> String {
        static FAILURES: crate::fallback_observation::Counter = crate::fallback_observation::Counter::new();
        crate::fallback_observation::recovered(&self.failure, &FAILURES, "engine.failure.stopped", |value| value.clone())
            .unwrap_or_else(|| "engine session has stopped; reopen the project".into())
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod composition_tests;
#[cfg(test)]
mod project_tests;
#[cfg(test)]
mod sampling_tests;
#[cfg(test)]
mod model_edit_tests;
#[cfg(test)]
mod audio_clock_tests;
#[cfg(test)]
mod video_frame_tests;
#[cfg(test)]
mod work_area_tests;
