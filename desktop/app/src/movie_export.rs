//! Background movie export: start/poll/cancel are nonblocking; teardown joins.
use motion_desktop_media::movie::{ExportControl, FrozenMovie, MovieResult};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Clone, Serialize)]
pub struct ExportStatus {
    pub state: &'static str,
    pub progress: f64,
    pub revision: u64,
    pub result: Option<MovieResult>,
    pub error: Option<String>,
}
pub struct MovieExport {
    control: ExportControl,
    status: Arc<Mutex<ExportStatus>>,
    worker: Option<std::thread::JoinHandle<()>>,
    revision: u64,
    output: PathBuf,
    began: std::time::Instant,
}
impl MovieExport {
    pub fn start(snapshot: FrozenMovie, output: PathBuf) -> Result<Self, String> {
        Self::start_with_control(snapshot, output, ExportControl::default())
    }
    pub fn start_with_control(
        snapshot: FrozenMovie,
        output: PathBuf,
        control: ExportControl,
    ) -> Result<Self, String> {
        Self::start_with_mode(snapshot, output, control, false)
    }
    pub fn start_new_file_with_control(
        snapshot: FrozenMovie,
        output: PathBuf,
        control: ExportControl,
    ) -> Result<Self, String> {
        Self::start_with_mode(snapshot, output, control, true)
    }
    fn start_with_mode(
        snapshot: FrozenMovie,
        output: PathBuf,
        control: ExportControl,
        new_file: bool,
    ) -> Result<Self, String> {
        if !cfg!(feature = "ffmpeg") {
            return Err("MP4 export requires a build with --ffmpeg".into());
        }
        let status = Arc::new(Mutex::new(ExportStatus {
            state: "running",
            progress: 0.0,
            revision: snapshot.revision,
            result: None,
            error: None,
        }));
        let update = status.clone();
        let cancel = control.clone();
        let revision = snapshot.revision;
        let began = std::time::Instant::now();
        let owned_output = output.clone();
        log::info!(
            "movie export start: revision={revision} path={}",
            output.display()
        );
        crate::logging::flush();
        let worker = std::thread::Builder::new()
            .name("motion-movie-export".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let progress = |progress| {
                        static FAILURES: crate::fallback_observation::Counter =
                            crate::fallback_observation::Counter::new();
                        crate::fallback_observation::recovered(
                            &update,
                            &FAILURES,
                            "movie.status.progress",
                            |value| value.progress = progress,
                        );
                    };
                    if new_file {
                        snapshot.export_new_file(&output, &cancel, progress)
                    } else {
                        snapshot.export(&output, &cancel, progress)
                    }
                }))
                .unwrap_or_else(|payload| {
                    Err(format!(
                        "movie export worker failed: {}",
                        motion_media::panic_reason(payload)
                    ))
                });
                match &result {
                    Ok(result) => log::info!(
                        "movie export saved: revision={revision} path={} bytes={} {}ms",
                        result.path.display(),
                        result.bytes,
                        began.elapsed().as_millis()
                    ),
                    Err(error) if error == "movie export cancelled" => log::info!(
                        "movie export cancelled: revision={revision} path={} {}ms",
                        output.display(),
                        began.elapsed().as_millis()
                    ),
                    Err(error) => log::error!(
                        "movie export failed: revision={revision} path={} {}ms reason={error}",
                        output.display(),
                        began.elapsed().as_millis()
                    ),
                }
                static FAILURES: crate::fallback_observation::Counter =
                    crate::fallback_observation::Counter::new();
                crate::fallback_observation::recovered(
                    &update,
                    &FAILURES,
                    "movie.status.finish",
                    |state| match result {
                        Ok(result) => {
                            state.state = "succeeded";
                            state.progress = 1.0;
                            state.result = Some(result);
                        }
                        Err(error) => {
                            state.state = if error == "movie export cancelled" {
                                "cancelled"
                            } else {
                                "failed"
                            };
                            state.error = Some(error);
                        }
                    },
                );
            })
            .map_err(|error| {
                log::error!("movie export spawn failed: revision={revision} reason={error}");
                error.to_string()
            })?;
        Ok(Self {
            control,
            status,
            worker: Some(worker),
            revision,
            output: owned_output,
            began,
        })
    }
    pub fn status(&self) -> ExportStatus {
        static FAILURES: crate::fallback_observation::Counter =
            crate::fallback_observation::Counter::new();
        crate::fallback_observation::recovered(
            &self.status,
            &FAILURES,
            "movie.status.read",
            |value| value.clone(),
        )
    }
    pub fn cancel(&self) -> bool {
        self.control.cancel()
    }
    pub fn finished(&self) -> bool {
        self.status().state != "running"
    }
    pub(crate) fn cancel_and_join(self) -> ExportStatus {
        let status = self.status.clone();
        drop(self);
        static FAILURES: crate::fallback_observation::Counter =
            crate::fallback_observation::Counter::new();
        crate::fallback_observation::recovered(&status, &FAILURES, "movie.status.joined", |value| {
            value.clone()
        })
    }
}
impl Drop for MovieExport {
    fn drop(&mut self) {
        self.control.cancel();
        if let Some(worker) = self.worker.take() {
            if !worker.is_finished() {
                log::info!(
                    "movie export shutdown start: revision={} path={} phase=join elapsed_ms={}",
                    self.revision,
                    crate::fallback_observation::short(self.output.display()),
                    self.began.elapsed().as_millis()
                );
                crate::logging::flush();
            }
            let joined = std::time::Instant::now();
            if let Err(payload) = worker.join() {
                let reason = format!(
                    "movie export worker closure failed: {}",
                    motion_media::panic_reason(payload)
                );
                log::error!("movie export shutdown failed: revision={} path={} phase=join elapsed_ms={} cause={reason}",
                    self.revision, crate::fallback_observation::short(self.output.display()), self.began.elapsed().as_millis());
                static FAILURES: crate::fallback_observation::Counter =
                    crate::fallback_observation::Counter::new();
                crate::fallback_observation::recovered(
                    &self.status,
                    &FAILURES,
                    "movie.status.join",
                    |state| {
                        if state.error.is_none() {
                            state.state = "failed";
                            state.error = Some(reason);
                        }
                    },
                );
            }
            log::debug!(
                "movie export shutdown done: revision={} phase=join elapsed_ms={} join_ms={}",
                self.revision,
                self.began.elapsed().as_millis(),
                joined.elapsed().as_millis()
            );
        }
    }
}

#[cfg(test)]
mod tests;
