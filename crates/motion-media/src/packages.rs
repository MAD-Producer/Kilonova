//! Frozen project packages. Payload I/O never runs on the engine owner thread.
use crate::Result;
use motion_model::storage::{MAX_PACKAGE, MAX_PACKAGE_ARCHIVE};
use motion_model::{PackageExportSource, PackagePublication};
use serde::Serialize;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Condvar, Mutex,
    },
};

static WORKERS: AtomicUsize = AtomicUsize::new(0);
struct Worker;
impl Drop for Worker {
    fn drop(&mut self) {
        WORKERS.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PackageTaskStatus {
    pub schema_version: u32,
    pub request_id: String,
    pub operation: String,
    pub state: String,
    pub phase: String,
    pub progress: f64,
    pub bytes_processed: u64,
    pub total_bytes: u64,
    pub source_count: usize,
    pub frozen_revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
impl PackageTaskStatus {
    fn terminal(&self) -> bool {
        matches!(self.state.as_str(), "succeeded" | "failed" | "cancelled")
    }
}
struct Task {
    status: PackageTaskStatus,
    cancel: bool,
    completed: Arc<Condvar>,
}
#[derive(Default)]
pub struct PackageJobs {
    tasks: Mutex<HashMap<String, Arc<Mutex<Task>>>>,
    workers: crate::workers::Workers,
}
impl PackageJobs {
    fn task(&self, id: &str) -> Result<Arc<Mutex<Task>>> {
        self.tasks
            .lock()
            .map_err(|_| "package registry poisoned")?
            .get(id)
            .cloned()
            .ok_or_else(|| "package task does not exist".into())
    }
    pub fn contains(&self, id: &str) -> bool {
        static OBSERVATION: crate::registry_observation::RegistryObservation =
            crate::registry_observation::RegistryObservation::new();
        OBSERVATION.contains(&self.tasks, id, "package.registry.contains")
    }
    pub fn status(&self, id: &str) -> Result<PackageTaskStatus> {
        Ok(self
            .task(id)?
            .lock()
            .map_err(|_| "package task poisoned")?
            .status
            .clone())
    }
    /// Cancellation is acknowledged as running/cancelling until the worker has
    /// closed and removed its staging file. Publication and cancellation share a lock.
    /// Finalizing follows the publication decision; cancellation cannot override it.
    pub fn cancel(&self, id: &str) -> Result<PackageTaskStatus> {
        let task = self.task(id)?;
        let mut task = task.lock().map_err(|_| "package task poisoned")?;
        // Once publication begins, preserve the existing acknowledgement: a
        // concurrent cancel observes its terminal decision after cleanup. Waiting
        // releases the task mutex, so status/release/admission remain available.
        let completed = task.completed.clone();
        while task.status.phase == "finalizing" {
            task = completed.wait(task).map_err(|_| "package task poisoned")?;
        }
        if !task.status.terminal() {
            task.cancel = true;
            task.status.phase = "cancelling".into();
        }
        Ok(task.status.clone())
    }
    pub fn release(&self, id: &str) -> Result<()> {
        let mut tasks = self.tasks.lock().map_err(|_| "package registry poisoned")?;
        if !tasks
            .get(id)
            .ok_or("package task does not exist")?
            .lock()
            .map_err(|_| "package task poisoned")?
            .status
            .terminal()
        {
            return Err("wait for package task completion before release".into());
        }
        tasks.remove(id);
        Ok(())
    }
    pub fn start_export(
        &self,
        id: &str,
        snapshot: impl PackageExportSource,
        output: PathBuf,
        frozen_revision: u64,
    ) -> Result<PackageTaskStatus> {
        use crate::poison_observation::{RecoveryScope, RecoverySite};
        static ACTIVE: RecoverySite = RecoverySite::new("package.start.active_task");
        let mut recoveries = RecoveryScope::new();
        if id.is_empty() || id.len() > 128 {
            return Err("invalid package request id".into());
        }
        self.workers.reap()?;
        let mut tasks = self.tasks.lock().map_err(|_| "package registry poisoned")?;
        if tasks.contains_key(id) {
            return Err("media request id already exists".into());
        }
        if tasks.len() >= 64 {
            return Err("release completed package tasks before creating more".into());
        }
        if tasks.iter().any(|(request, t)| {
            !recoveries
                .recover(t.lock(), &ACTIVE, request, "export_project")
                .status
                .terminal()
        }) {
            return Err("at most one package task may run per session".into());
        }
        if WORKERS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 2).then_some(n + 1)
            })
            .is_err()
        {
            return Err("package worker budget exhausted".into());
        }
        let worker = Worker;
        let initial = PackageTaskStatus {
            schema_version: 1,
            request_id: id.into(),
            operation: "export_project".into(),
            state: "running".into(),
            phase: "queued".into(),
            progress: 0.,
            bytes_processed: 0,
            total_bytes: snapshot.total_bytes(),
            source_count: snapshot.source_count(),
            frozen_revision,
            path: None,
            error: None,
        };
        let task = Arc::new(Mutex::new(Task {
            status: initial.clone(),
            cancel: false,
            completed: Arc::new(Condvar::new()),
        }));
        tasks.insert(id.into(), task.clone());
        let task_log = crate::task_logging::TaskLog::new(&initial.request_id, &initial.operation);
        let worker_log = task_log.clone();
        let spawn_task = task.clone();
        let spawned = self.workers.spawn("motion-package-export", task_log.clone(), move || {
                let _worker = worker;
                worker_log.start();
                let mut result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let parent = output.parent().ok_or_else(|| {
                        motion_model::Error::Invalid("invalid package destination".into())
                    })?;
                    std::fs::create_dir_all(parent)?;
                    if fs2::available_space(parent)?
                        < snapshot
                            .total_bytes()
                            .saturating_add(MAX_PACKAGE_ARCHIVE - MAX_PACKAGE)
                    {
                        return Err(motion_model::Error::Invalid(
                            "insufficient package output storage".into(),
                        ));
                    }
                    snapshot.prepare(&output, |done, total| {
                        static PROGRESS: RecoverySite =
                            RecoverySite::new("package.worker.progress_task");
                        let mut recoveries = RecoveryScope::new();
                        let mut task = recoveries.recover(
                            spawn_task.lock(), &PROGRESS, worker_log.request_id(), "export_project",
                        );
                        if task.cancel {
                            return Err(motion_model::Error::Invalid(
                                "package export cancelled".into(),
                            ));
                        }
                        task.status.phase = "packing".into();
                        task.status.bytes_processed = done;
                        task.status.progress = done as f64 / total.max(1) as f64;
                        Ok(())
                    })
                })).map(|result| result.map_err(crate::Error::from));
                static PUBLISH: RecoverySite = RecoverySite::new("package.worker.result_task");
                let mut recoveries = RecoveryScope::new();
                let mut task = recoveries.recover(
                    spawn_task.lock(), &PUBLISH, worker_log.request_id(), "export_project",
                );
                if task.cancel {
                    // Cleanup may do filesystem IO or call the provider's logger.
                    // Keep cancellation nonterminal while releasing the task lock.
                    drop(task);
                    let primary = crate::task_logging::cancelled_error(
                        &result,
                        &["package export cancelled"],
                    );
                    // Drop/close the prepared archive before advertising terminal cancellation.
                    let cleanup = cleanup_package(result).err().map(|error| error.to_string());
                    let cleanup_failed = cleanup.is_some();
                    let error = match (primary, cleanup) {
                        (Some(primary), Some(cleanup)) => Some(format!("{primary}; {cleanup}")),
                        (primary, cleanup) => primary.or(cleanup),
                    };
                    let mut task = recoveries.recover(
                        spawn_task.lock(), &PUBLISH, worker_log.request_id(), "export_project",
                    );
                    task.status.state = "cancelled".into();
                    task.status.phase = "cancelled".into();
                    // Preserve existing cancellation status for preparation
                    // errors; a new destructor fault also belongs in status.
                    task.status.error = if cleanup_failed { error.clone() } else { None };
                    drop(task);
                    worker_log.finish("cancelled", error.as_deref());
                    return;
                }
                let published = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    match &mut result {
                        Ok(Ok(prepared)) => prepared.publish().map_err(crate::Error::from),
                        Ok(Err(error)) => Err(error.clone()),
                        Err(payload) => Err(crate::Error::Panic {
                            operation: "package worker",
                            reason: crate::panic_reason_ref(payload.as_ref()),
                        }),
                    }
                }))
                .unwrap_or_else(|payload| {
                    Err(crate::Error::Panic {
                        operation: "package publication",
                        reason: crate::panic_reason(payload),
                    })
                });
                // The publication decision is fixed under the cancellation lock.
                // Keep the task active until archive cleanup has actually returned.
                // Late cancellation must not override a completed publication.
                task.status.phase = "finalizing".into();
                drop(task);
                let cleanup = cleanup_package(result);
                let published = match cleanup {
                    Ok(()) => published,
                    Err(cleanup) => Err(match published {
                        Ok(path) => crate::Error::Context {
                            message: format!("{cleanup}; archive published at {}", path.display()),
                            cause: Arc::new(cleanup),
                        },
                        Err(primary) => primary.rollback(vec![cleanup]),
                    }),
                };
                let mut task = recoveries.recover(
                    spawn_task.lock(), &PUBLISH, worker_log.request_id(), "export_project",
                );
                match published {
                    Ok(path) => {
                        task.status.state = "succeeded".into();
                        task.status.phase = "complete".into();
                        task.status.progress = 1.;
                        task.status.path = Some(path);
                    }
                    Err(error) => {
                        task.status.state = "failed".into();
                        task.status.phase = "failed".into();
                        task.status.error = Some(error.to_string());
                    }
                }
                let phase = task.status.phase.clone();
                let error = task.status.error.clone();
                let completed = task.completed.clone();
                drop(task);
                completed.notify_all();
                worker_log.finish(&phase, error.as_deref());
            });
        if let Err(error) = spawned {
            return Err(mark_spawn_failed(&mut recoveries, &task, id, task_log, error).into());
        }
        Ok(initial)
    }
}

fn cleanup_package<T>(prepared: T) -> Result<()> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(prepared)))
        .map_err(|payload| crate::Error::Panic {
            operation: "package cleanup", reason: crate::panic_reason(payload),
        })
}

fn mark_spawn_failed(
    recoveries: &mut crate::poison_observation::RecoveryScope,
    task: &Mutex<Task>,
    id: &str,
    task_log: crate::task_logging::TaskLog,
    error: std::io::Error,
) -> String {
    static SPAWN: crate::poison_observation::RecoverySite =
        crate::poison_observation::RecoverySite::new("package.start.spawn_failure_task");
    let reason = error.to_string();
    recoveries.defer_spawn_error(task_log, reason.clone());
    let mut task = recoveries.recover(task.lock(), &SPAWN, id, "export_project");
    task.status.state = "failed".into();
    task.status.phase = "failed".into();
    task.status.error = Some(reason.clone());
    reason
}

impl Drop for PackageJobs {
    fn drop(&mut self) {
        use crate::poison_observation::{RecoveryScope, RecoverySite};
        static REGISTRY: RecoverySite = RecoverySite::new("package.drop.registry");
        static TASK: RecoverySite = RecoverySite::new("package.drop.task");
        let mut recoveries = RecoveryScope::new();
        for (id, task) in recoveries
            .recover(self.tasks.get_mut(), &REGISTRY, "session", "drop_package")
            .iter()
        {
            let mut task = recoveries.recover(task.lock(), &TASK, id, "drop_package");
            if !task.status.terminal() && task.status.phase != "finalizing" {
                task.cancel = true;
                task.status.phase = "cancelling".into();
            }
        }
        drop(recoveries);
        self.workers.shutdown();
    }
}

#[cfg(test)]
mod poison_tests;

#[cfg(test)]
mod cleanup_tests;

#[cfg(test)]
mod publication_tests;

#[cfg(test)]
mod registry_tests {
    use super::*;

    #[test]
    fn poisoned_package_registry_membership_is_observed_after_unlock() {
        use crate::registry_observation::tests::{check, child};
        if !child(
            "packages::registry_tests::poisoned_package_registry_membership_is_observed_after_unlock",
        ) {
            return;
        }
        let jobs = Box::leak(Box::new(PackageJobs::default()));
        jobs.tasks.lock().unwrap().insert(
            "present".into(),
            Arc::new(Mutex::new(Task {
                status: PackageTaskStatus {
                    schema_version: 1,
                    request_id: "present".into(),
                    operation: "export_project".into(),
                    state: "succeeded".into(),
                    phase: "complete".into(),
                    progress: 1.,
                    bytes_processed: 0,
                    total_bytes: 0,
                    source_count: 0,
                    frozen_revision: 0,
                    path: None,
                    error: None,
                },
                cancel: false,
                completed: Arc::new(Condvar::new()),
            })),
        );
        let healthy = PackageJobs::default();
        check(
            &jobs.tasks,
            "package.registry.contains",
            |id| jobs.contains(id),
            |id| healthy.contains(id),
        );
        assert_eq!(
            jobs.status("present").unwrap_err().to_string(),
            "package registry poisoned"
        );
    }
}
