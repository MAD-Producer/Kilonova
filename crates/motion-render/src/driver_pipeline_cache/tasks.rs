//! The caller supplies one bounded worker and completion channel. Creating a job
//! never reads a file or calls the driver; a completed load cannot install itself.
use super::*;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc, Weak,
    },
    thread::ThreadId,
    time::Instant,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DriverPipelineCacheOperation {
    Restore,
    Save,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DriverPipelineCachePhase {
    Queued,
    Reading,
    Creating,
    Snapshotting,
    Writing,
    Ready,
    Saved,
    Installed,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverPipelineCacheStatus {
    pub request: u64,
    pub request_saturated: bool,
    pub operation: DriverPipelineCacheOperation,
    pub phase: DriverPipelineCachePhase,
    pub cancelled: bool,
    pub elapsed_ms: u64,
}

struct Progress {
    request: u64,
    operation: DriverPipelineCacheOperation,
    phase: AtomicU8,
    cancelled: AtomicBool,
    created: Instant,
}

#[derive(Clone)]
pub struct DriverPipelineCacheTicket(Arc<Progress>);
impl DriverPipelineCacheTicket {
    pub fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::Release);
    }
    pub fn same_request(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub fn status(&self) -> DriverPipelineCacheStatus {
        let phase = match self.0.phase.load(Ordering::Acquire) {
            0 => DriverPipelineCachePhase::Queued,
            1 => DriverPipelineCachePhase::Reading,
            2 => DriverPipelineCachePhase::Creating,
            3 => DriverPipelineCachePhase::Snapshotting,
            4 => DriverPipelineCachePhase::Writing,
            5 => DriverPipelineCachePhase::Ready,
            6 => DriverPipelineCachePhase::Saved,
            7 => DriverPipelineCachePhase::Installed,
            8 => DriverPipelineCachePhase::Cancelled,
            _ => DriverPipelineCachePhase::Failed,
        };
        DriverPipelineCacheStatus {
            request: self.0.request,
            request_saturated: self.0.request == u64::MAX,
            operation: self.0.operation,
            phase,
            cancelled: self.cancelled(),
            elapsed_ms: self
                .0
                .created
                .elapsed()
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
        }
    }
    fn cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }
    fn phase(&self, phase: u8) {
        self.0.phase.store(phase, Ordering::Release);
    }
}

#[derive(Default)]
pub(crate) struct TaskState {
    current: Option<DriverPipelineCacheTicket>,
    // Saturating renderer-lifetime diagnostic ordinal; never request identity.
    next_request: u64,
}
impl TaskState {
    pub(crate) fn cancel(&mut self) {
        if let Some(ticket) = self.current.take() {
            ticket.cancel();
        }
    }
    fn begin(&mut self, operation: DriverPipelineCacheOperation) -> DriverPipelineCacheTicket {
        self.cancel();
        self.next_request = self.next_request.saturating_add(1);
        let ticket = DriverPipelineCacheTicket(Arc::new(Progress {
            request: self.next_request,
            operation,
            phase: AtomicU8::new(0),
            cancelled: AtomicBool::new(false),
            created: Instant::now(),
        }));
        self.current = Some(ticket.clone());
        ticket
    }
    fn accepts(&self, ticket: &DriverPipelineCacheTicket) -> bool {
        !ticket.cancelled()
            && self
                .current
                .as_ref()
                .is_some_and(|t| t.same_request(ticket))
    }
}
impl Drop for TaskState {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// A single restore or save, with no internally spawned threads or retries.
/// Run on the host's bounded worker, never on the creating/renderer thread.
pub struct DriverPipelineCacheJob {
    owner: Weak<()>,
    owner_thread: ThreadId,
    target_format: wgpu::TextureFormat,
    device: wgpu::Device,
    cache: Option<wgpu::PipelineCache>,
    identity: [u8; 32],
    path: PathBuf,
    ticket: DriverPipelineCacheTicket,
    context: Option<crate::PipelinePreparationContext>,
    finished: bool,
    #[cfg(test)]
    before_operation: Option<Box<dyn FnOnce() + Send>>,
}

pub enum DriverPipelineCacheCompletion {
    Loaded(LoadedDriverPipelineCache),
    /// The atomic rename succeeded. Later cancellation does not undo publication.
    Saved {
        snapshot_bytes: usize,
    },
    Unsupported,
    Cancelled,
}

/// Validated driver data created on the worker, awaiting explicit owner install.
/// Missing and incompatible results carry no GPU cache and preserve the live one.
pub struct LoadedDriverPipelineCache {
    owner: Weak<()>,
    target_format: wgpu::TextureFormat,
    ticket: DriverPipelineCacheTicket,
    result: DriverPipelineCacheLoad,
    cache: Option<wgpu::PipelineCache>,
    installed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriverPipelineCacheInstall {
    Finished(DriverPipelineCacheLoad),
    Superseded,
}

impl LoadedDriverPipelineCache {
    pub fn result(&self) -> DriverPipelineCacheLoad {
        self.result
    }
    pub(crate) fn install(
        mut self,
        renderer: &mut crate::Renderer,
    ) -> Result<DriverPipelineCacheInstall> {
        if !self
            .owner
            .ptr_eq(&Arc::downgrade(&renderer.allocation_owner))
        {
            return Err(DriverPipelineCacheError::ForeignRenderer);
        }
        if !renderer.driver_pipeline_cache.tasks.accepts(&self.ticket)
            || renderer.target_format != self.target_format
        {
            return Ok(DriverPipelineCacheInstall::Superseded);
        }
        if let Some(cache) = self.cache.take() {
            renderer.pipeline_preparation.cancel();
            renderer.effect_gpu.state.driver_pipeline_cache = Some(cache.clone());
            renderer.driver_pipeline_cache.cache = Some(cache);
        }
        self.ticket.phase(7);
        self.installed = true;
        Ok(DriverPipelineCacheInstall::Finished(self.result))
    }
}
impl Drop for LoadedDriverPipelineCache {
    fn drop(&mut self) {
        if !self.installed {
            self.ticket.cancel();
            self.ticket.phase(8);
        }
    }
}

impl DriverPipelineCacheJob {
    // Restore's private-data provenance obligation is enforced by the public
    // unsafe Renderer entry point. No disk/driver work occurs in this constructor.
    pub(crate) fn new(
        renderer: &mut crate::Renderer,
        path: &Path,
        operation: DriverPipelineCacheOperation,
    ) -> Self {
        let ticket = renderer.driver_pipeline_cache.tasks.begin(operation);
        Self {
            owner: Arc::downgrade(&renderer.allocation_owner),
            owner_thread: std::thread::current().id(),
            target_format: renderer.target_format,
            device: renderer.device.clone(),
            cache: renderer.driver_pipeline_cache.cache.clone(),
            identity: renderer.driver_pipeline_cache.identity,
            path: path.to_owned(),
            ticket,
            context: None,
            finished: false,
            #[cfg(test)]
            before_operation: None,
        }
    }
    pub fn ticket(&self) -> DriverPipelineCacheTicket {
        self.ticket.clone()
    }
    pub fn with_context(mut self, context: crate::PipelinePreparationContext) -> Self {
        self.context = Some(context);
        self
    }
    fn current(&self) -> bool {
        !self.ticket.cancelled() && self.owner.strong_count() != 0
    }

    pub fn run(self) -> Result<DriverPipelineCacheCompletion> {
        self.run_with_storage(|_| Ok(()))
    }

    /// Prepare/validate the caller's application-owned storage inside the same
    /// published in-flight and panic/error boundary, before opening any file.
    /// The callback receives the final file path. It must not import user data;
    /// restore's provenance contract remains the caller's obligation.
    pub fn run_with_storage(
        mut self,
        prepare: impl FnOnce(&Path) -> std::io::Result<()>,
    ) -> Result<DriverPipelineCacheCompletion> {
        if !self.current() {
            self.ticket.phase(8);
            self.finished = true;
            return Ok(DriverPipelineCacheCompletion::Cancelled);
        }
        self.ticket.phase(match self.ticket.0.operation {
            DriverPipelineCacheOperation::Restore => 1,
            DriverPipelineCacheOperation::Save => 3,
        });
        let started = Instant::now();
        let path: String = self.path.to_string_lossy().chars().take(384).collect();
        log::info!(
            "driver pipeline cache start request={} operation={:?} path={} context={:?}",
            self.ticket.0.request,
            self.ticket.0.operation,
            path,
            self.context
        );
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if std::thread::current().id() == self.owner_thread {
                return Err(DriverPipelineCacheError::OwnerThread);
            }
            #[cfg(test)]
            if let Some(before) = self.before_operation.take() {
                before();
            }
            if !self.current() {
                return Ok(DriverPipelineCacheCompletion::Cancelled);
            }
            if self.cache.is_some() {
                prepare(&self.path).map_err(|error| io("prepare storage", &self.path, error))?;
            }
            self.execute()
        }))
        .unwrap_or_else(|panic| {
            let reason = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("non-text panic payload")
                .chars()
                .take(2048)
                .collect();
            Err(DriverPipelineCacheError::Panicked { reason })
        });
        match &result {
            Err(cause) => {
                self.ticket.phase(9);
                let cause: String = cause.to_string().chars().take(2048).collect();
                log::error!("driver pipeline cache failed request={} operation={:?} path={} elapsed_ms={} context={:?} cause={}",
                    self.ticket.0.request, self.ticket.0.operation, path, started.elapsed().as_millis(), self.context, cause);
            }
            Ok(DriverPipelineCacheCompletion::Cancelled) => self.ticket.phase(8),
            Ok(DriverPipelineCacheCompletion::Saved { .. }) => self.ticket.phase(6),
            Ok(_) => self.ticket.phase(5),
        }
        log::debug!(
            "driver pipeline cache done request={} operation={:?} phase={:?} elapsed_ms={}",
            self.ticket.0.request,
            self.ticket.0.operation,
            self.ticket.status().phase,
            started.elapsed().as_millis()
        );
        self.finished = true;
        result
    }

    fn execute(&self) -> Result<DriverPipelineCacheCompletion> {
        if !self.current() {
            return Ok(DriverPipelineCacheCompletion::Cancelled);
        }
        match self.ticket.0.operation {
            DriverPipelineCacheOperation::Restore => {
                let (result, cache) = if self.cache.is_none() {
                    (DriverPipelineCacheLoad::Unsupported, None)
                } else {
                    let (result, bytes) = read_candidate(&self.path, &self.identity)?;
                    if !self.current() {
                        return Ok(DriverPipelineCacheCompletion::Cancelled);
                    }
                    let cache = if result == DriverPipelineCacheLoad::Restored {
                        self.ticket.phase(2);
                        Some(create(&self.device, Some(&bytes[HEADER_BYTES..]))?)
                    } else {
                        None
                    };
                    (result, cache)
                };
                if !self.current() {
                    return Ok(DriverPipelineCacheCompletion::Cancelled);
                }
                Ok(DriverPipelineCacheCompletion::Loaded(
                    LoadedDriverPipelineCache {
                        owner: self.owner.clone(),
                        target_format: self.target_format,
                        ticket: self.ticket.clone(),
                        result,
                        cache,
                        installed: false,
                    },
                ))
            }
            DriverPipelineCacheOperation::Save => {
                let Some(cache) = &self.cache else {
                    return Ok(DriverPipelineCacheCompletion::Unsupported);
                };
                let data = cache
                    .get_data()
                    .ok_or(DriverPipelineCacheError::Unavailable)?;
                if !self.current() {
                    return Ok(DriverPipelineCacheCompletion::Cancelled);
                }
                let bytes = encode(&self.identity, &data)?;
                self.ticket.phase(4);
                if write_atomic_if(&self.path, &bytes, || self.current())? {
                    Ok(DriverPipelineCacheCompletion::Saved {
                        snapshot_bytes: bytes.len(),
                    })
                } else {
                    Ok(DriverPipelineCacheCompletion::Cancelled)
                }
            }
        }
    }
}
impl Drop for DriverPipelineCacheJob {
    fn drop(&mut self) {
        if !self.finished {
            self.ticket.cancel();
            self.ticket.phase(8);
        }
    }
}

#[cfg(test)]
mod tests;
