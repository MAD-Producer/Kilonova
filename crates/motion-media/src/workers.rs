//! Own native worker handles independently of user-visible task records.
use crate::{task_logging::TaskLog, Result};
use std::{io, sync::Mutex, thread::JoinHandle, time::Instant};

// Includes cancelled/released tasks and threads still dropping their captures.
// Existing process-wide running-worker limits remain unchanged.
const MAX_RETAINED_WORKERS: usize = 64;

struct Worker {
    handle: JoinHandle<()>,
    log: TaskLog,
}

#[derive(Default)]
pub(crate) struct Workers {
    handles: Mutex<Vec<Worker>>,
    shutdown_started: bool,
}

impl Workers {
    /// Call before taking task/registry guards. Even a completed main function
    /// may still have thread-local destructors to join; no guard spans the wait.
    pub(crate) fn reap(&self) -> Result<()> {
        if self.shutdown_started {
            return Err("media workers already shut down".into());
        }
        let finished = {
            let mut handles = self
                .handles
                .lock()
                .map_err(|_| "media worker registry poisoned")?;
            let (finished, pending) = std::mem::take(&mut *handles)
                .into_iter()
                .partition(|worker: &Worker| worker.handle.is_finished());
            *handles = pending;
            finished
        };
        join(finished, "reap")
    }

    pub(crate) fn spawn(
        &self,
        name: &'static str,
        log: TaskLog,
        work: impl FnOnce() + Send + 'static,
    ) -> io::Result<()> {
        if self.shutdown_started {
            return Err(io::Error::other("media workers already shut down"));
        }
        let mut handles = self
            .handles
            .lock()
            .map_err(|_| io::Error::other("media worker registry poisoned"))?;
        if handles.len() >= MAX_RETAINED_WORKERS {
            return Err(io::Error::other("media worker handle budget exhausted"));
        }
        let handle = std::thread::Builder::new().name(name.into()).spawn(work)?;
        handles.push(Worker { handle, log });
        Ok(())
    }

    /// Owners first notify every task of cancellation and release their guards.
    /// Draining handles also makes the fallback Drop path harmless.
    pub(crate) fn shutdown(&mut self) {
        if std::mem::replace(&mut self.shutdown_started, true) {
            return;
        }
        let (handles, poisoned) = match self.handles.get_mut() {
            Ok(handles) => (std::mem::take(handles), false),
            Err(error) => (std::mem::take(error.into_inner()), true),
        };
        if poisoned {
            // One shutdown per instance; this is not a polling counter.
            log::error!("media worker shutdown operation=join phase=registry cause=poisoned_mutex recovery=into_inner workers={}", handles.len());
        }
        // join already records each original cause. Drop cannot return an error.
        let _reported = join(handles, "shutdown");
    }
}

impl Drop for Workers {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn join(handles: Vec<Worker>, phase: &'static str) -> Result<()> {
    let mut primary = None;
    for worker in handles {
        let began = Instant::now();
        worker.log.join_start(phase);
        if let Err(payload) = worker.handle.join() {
            let error = crate::Error::Panic {
                operation: "media worker join",
                reason: crate::panic_reason(payload),
            };
            worker.log.finish(phase, Some(&error.to_string()));
            if primary.is_none() {
                primary = Some(error);
            }
        } else {
            worker.log.join_done(phase, began.elapsed());
        }
    }
    primary.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests;
