//! Persistent compilation data is optional. Cache faults retain their cause and
//! allow ordinary preparation; they never turn a failed pipeline into success.
use super::{Context, Error};
use motion_render::{
    DriverPipelineCacheCompletion, DriverPipelineCacheError, DriverPipelineCacheInstall,
    DriverPipelineCacheJob, DriverPipelineCacheLoad, DriverPipelineCacheOperation,
    DriverPipelineCacheTicket, Renderer,
};
use serde_json::{json, Value};
use std::{
    io,
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

type Prepare = dyn Fn(&Path) -> io::Result<()> + Send + Sync;

#[derive(Clone)]
pub struct DriverPipelineCacheStorage {
    root: PathBuf,
    prepare: Arc<Prepare>,
}
impl DriverPipelineCacheStorage {
    /// Configure an application-private directory, never an import location.
    /// No filesystem or native calls occur here or while choosing the file name.
    ///
    /// # Safety
    /// The root and its parent chain must be protected by the current account.
    /// `prepare` must maintain that protection and reject links/imported data for
    /// every supplied final file path, including while worker work is queued.
    /// Only this application's unchanged get_data snapshots may be restored.
    pub unsafe fn new(root: PathBuf, prepare: Arc<Prepare>) -> Self {
        Self { root, prepare }
    }
    fn path(&self, renderer: &Renderer) -> Result<Option<PathBuf>, DriverPipelineCacheError> {
        let info = renderer.driver_pipeline_cache_info();
        if !info.supported {
            return Ok(None);
        }
        let Some(key) = info.adapter_key else {
            return Err(DriverPipelineCacheError::Invalid(
                "supported cache has no adapter key",
            ));
        };
        if key.len() > 160
            || !matches!(
                Path::new(&key).components().collect::<Vec<_>>().as_slice(),
                [Component::Normal(_)]
            )
        {
            return Err(DriverPipelineCacheError::Invalid(
                "adapter key is not a file name",
            ));
        }
        Ok(Some(self.root.join(format!("{key}.cache"))))
    }
}

pub(super) struct Request {
    job: DriverPipelineCacheJob,
    storage: DriverPipelineCacheStorage,
}
pub(super) struct Completion {
    ticket: DriverPipelineCacheTicket,
    result: Result<DriverPipelineCacheCompletion, DriverPipelineCacheError>,
}
impl Request {
    pub(super) fn run(self) -> Completion {
        let ticket = self.job.ticket();
        let result = self
            .job
            .run_with_storage(|path| (self.storage.prepare)(path));
        Completion { ticket, result }
    }
}

#[derive(Default)]
pub(super) struct State {
    storage: Option<DriverPipelineCacheStorage>,
    pub(super) pending: Option<Request>,
    ticket: Option<DriverPipelineCacheTicket>,
    context: Option<Context>,
    restored: bool,
    load: Option<DriverPipelineCacheLoad>,
    disabled: bool,
    failure: Option<Arc<Error>>,
    requested_creations: u64,
    saved_creations: u64,
    // At most one checkpoint per 30 s, after a completed visible frame and
    // additional actual native creations. No disk polling in healthy Present.
    next_checkpoint: Option<Instant>,
}
impl State {
    pub(super) fn configure(&mut self, storage: DriverPipelineCacheStorage) {
        self.cancel();
        self.storage = Some(storage);
    }
    pub(super) fn cancel(&mut self) {
        if let Some(ticket) = &self.ticket {
            ticket.cancel();
        }
        let storage = self.storage.take();
        *self = Self {
            storage,
            ..Default::default()
        };
    }
    pub(super) fn status(&self) -> Value {
        json!({"enabled":self.storage.is_some(),"restored":self.restored,"load":self.load,
            "disabled":self.disabled,"context":self.context,
            "task":self.ticket.as_ref().map(|ticket|ticket.status()),
            "pendingEnqueue":self.pending.is_some(),"pendingLimit":1,
            "savedCreations":self.saved_creations,"requestedCreations":self.requested_creations,
            "checkpointIntervalMs":30_000,
            "lastError":self.failure.as_ref().map(|cause|cause.to_string().chars().take(2048).collect::<String>())})
    }
    pub(super) fn preparing(&self) -> bool {
        self.storage.is_some() && !self.restored && !self.disabled
    }
    #[cfg(test)]
    pub(super) fn failure(&self) -> Option<&Arc<Error>> {
        self.failure.as_ref()
    }
    pub(super) fn begin_restore(&mut self, renderer: &mut Renderer, context: Context) -> bool {
        if self.restored || self.disabled || self.storage.is_none() {
            return true;
        }
        if self.ticket.is_some() {
            return false;
        }
        let Some(storage) = self.storage.as_ref() else {
            return true;
        };
        let path = match storage.path(renderer) {
            Ok(Some(path)) => path,
            Ok(None) => {
                self.load = Some(DriverPipelineCacheLoad::Unsupported);
                self.restored = true;
                return true;
            }
            Err(error) => {
                self.record_setup_failure(error, context);
                return true;
            }
        };
        // SAFETY: storage's unsafe construction establishes private provenance;
        // its callback validates protection inside the job's observable boundary.
        let job =
            unsafe { renderer.driver_pipeline_cache_restore_job(&path) }.with_context(context);
        self.ticket = Some(job.ticket());
        self.context = Some(context);
        self.pending = Some(Request {
            job,
            storage: storage.clone(),
        });
        false
    }
    fn record_setup_failure(&mut self, error: DriverPipelineCacheError, context: Context) {
        let cause: String = error.to_string().chars().take(2048).collect();
        log::error!(target:"motion_host::pipeline_preparation", "driver cache setup failed generation={} revision={} cause={cause}",
            context.document_generation, context.project_revision);
        self.disabled = true;
        self.failure = Some(Arc::new(Error::Render(error.into())));
    }
    pub(super) fn collect(&mut self, completion: Completion, renderer: &mut Renderer) {
        if !self
            .ticket
            .as_ref()
            .is_some_and(|ticket| ticket.same_request(&completion.ticket))
        {
            return;
        }
        let operation = completion.ticket.status().operation;
        match completion.result {
            Ok(DriverPipelineCacheCompletion::Loaded(loaded)) => {
                match renderer.install_loaded_driver_pipeline_cache(loaded) {
                    Ok(DriverPipelineCacheInstall::Finished(load)) => self.load = Some(load),
                    Ok(DriverPipelineCacheInstall::Superseded) => {}
                    Err(error) => {
                        log::error!(target:"motion_host::pipeline_preparation", "driver cache install failed request={} context={:?} cause={}",
                            completion.ticket.status().request, self.context,
                            error.to_string().chars().take(2048).collect::<String>());
                        self.failure = Some(Arc::new(Error::Render(error)));
                        self.disabled = true;
                    }
                }
            }
            Ok(DriverPipelineCacheCompletion::Saved { .. }) => {
                self.saved_creations = self.requested_creations
            }
            Ok(DriverPipelineCacheCompletion::Unsupported) => {
                self.load = Some(DriverPipelineCacheLoad::Unsupported)
            }
            Ok(DriverPipelineCacheCompletion::Cancelled) => {}
            Err(error) => {
                // The job already logged the original failure. A malformed
                // envelope may be replaced by a later successful checkpoint;
                // no failed read or save is retried by polling.
                let replace_corrupt = operation == DriverPipelineCacheOperation::Restore
                    && matches!(
                        error,
                        DriverPipelineCacheError::Invalid(_) | DriverPipelineCacheError::TooLarge
                    );
                self.disabled = !replace_corrupt;
                self.failure = Some(Arc::new(Error::Render(error.into())));
            }
        }
        if operation == DriverPipelineCacheOperation::Restore {
            self.restored = true;
        }
        // Retain terminal ticket and original failure for observation. Save
        // admission checks the phase rather than replaying this operation.
    }
    pub(super) fn frame_presented(&mut self, renderer: &mut Renderer, context: Context) {
        if self.disabled || !self.restored || self.pending.is_some() {
            return;
        }
        let Some(storage) = &self.storage else {
            return;
        };
        if self.ticket.as_ref().is_some_and(|ticket| {
            matches!(
                ticket.status().phase,
                motion_render::DriverPipelineCachePhase::Queued
                    | motion_render::DriverPipelineCachePhase::Reading
                    | motion_render::DriverPipelineCachePhase::Creating
                    | motion_render::DriverPipelineCachePhase::Snapshotting
                    | motion_render::DriverPipelineCachePhase::Writing
                    | motion_render::DriverPipelineCachePhase::Ready
            )
        }) {
            return;
        }
        let creations = renderer
            .driver_pipeline_cache_info()
            .effect_pipeline_creations;
        if creations <= self.saved_creations
            || self.next_checkpoint.is_some_and(|at| Instant::now() < at)
        {
            return;
        }
        let path = match storage.path(renderer) {
            Ok(Some(path)) => path,
            Ok(None) => return,
            Err(error) => {
                self.record_setup_failure(error, context);
                return;
            }
        };
        let job = renderer
            .driver_pipeline_cache_save_job(&path)
            .with_context(context);
        self.ticket = Some(job.ticket());
        self.context = Some(context);
        self.requested_creations = creations;
        self.next_checkpoint = Some(Instant::now() + Duration::from_secs(30));
        self.pending = Some(Request {
            job,
            storage: storage.clone(),
        });
    }
}
