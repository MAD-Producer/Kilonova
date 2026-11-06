//! One session-owned worker, one queued request, one retained pending request
//! plus one cache action and one completed-result channel slot. A worker may hold one more completed
//! result while delivery is backpressured. Polling never waits for compilation.
use motion_core::{Observer, Project};
use motion_render::{
    PipelinePreparationContext as Context, PipelinePreparationInstall, PipelinePreparationJob,
    PipelinePreparationTicket, PreparedEffectPipelines, Renderer, Scene,
};
use serde_json::{json, Value};
use std::sync::{mpsc, Arc};

mod driver_cache;
#[cfg(test)]
mod tests;
pub use driver_cache::DriverPipelineCacheStorage;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("pipeline preparation worker creation failed: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("pipeline preparation worker disconnected; it was not restarted")]
    Disconnected,
    #[error("pipeline preparation invariant: {0}")]
    Invariant(&'static str),
    #[error(transparent)]
    Render(#[from] motion_render::RenderError),
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ExposureScope {
    pub composition: String,
    pub frame: u64,
    pub layers: Vec<u64>,
}
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Key {
    pub context: Context,
    pub surface: u64,
    pub view: u64,
    pub width: u32,
    pub height: u32,
    pub preview: bool,
    pub window: u64,
    pub exposure: Option<ExposureScope>,
}
impl Key {
    fn same_revision(&self, other: &Self) -> bool {
        self.context == other.context
            && self.surface == other.surface
            && self.view == other.view
            && self.width == other.width
            && self.height == other.height
            && self.preview == other.preview
            && self.exposure == other.exposure
    }
}

struct Request {
    job: PipelinePreparationJob,
    project: Arc<Project>,
    frames: Vec<f64>,
    observer: Option<Observer>,
    #[cfg(test)]
    before_sample: Option<Box<dyn FnOnce() + Send>>,
}
struct Completion {
    ticket: PipelinePreparationTicket,
    result: Result<PreparedEffectPipelines, motion_render::RenderError>,
    sampling_failures: Vec<motion_render::PipelineLookaheadFailure>,
}
enum Work {
    Prepare(Request),
    Cache(driver_cache::Request),
}
enum Reply {
    Prepared(Completion),
    Cache(driver_cache::Completion),
}
struct Worker {
    sender: mpsc::SyncSender<Work>,
    completed: mpsc::Receiver<Reply>,
    thread: std::thread::JoinHandle<()>,
}
impl Worker {
    fn new() -> Result<Self, Error> {
        let (sender, requests) = mpsc::sync_channel::<Work>(1);
        let (completed, results) = mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("motion-pipeline-prepare".into())
            .spawn(move || {
                while let Ok(work) = requests.recv() {
                    let reply = match work {
                    Work::Cache(request) => Reply::Cache(request.run()),
                    Work::Prepare(request) => {
                    let Request {
                        job,
                        project,
                        frames,
                        observer,
                        #[cfg(test)]
                        before_sample,
                    } = request;
                    let ticket = job.ticket();
                    let sampling_ticket = ticket.clone();
                    let mut sampling_failures = Vec::new();
                    let result = job.run_with_scenes(|scenes| {
                        #[cfg(test)]
                        if let Some(before) = before_sample {
                            before();
                        }
                        motion_core::prepare_expression_sources(&project)?;
                        for frame in frames {
                            if sampling_ticket.status().cancelled {
                                break;
                            }
                            let mut next = scenes[0].clone();
                            if let Err(cause) = next.sample(
                                &project,
                                frame,
                                observer.as_ref(),
                                &motion_core::ExpressionEvaluator,
                            ) {
                                let status = sampling_ticket.status();
                                let bounded: String = cause.to_string().chars().take(2048).collect();
                                log::error!(target: "motion_host::pipeline_preparation",
                                    "pipeline lookahead sample failed request={} generation={} revision={} frame={} elapsed_ms={} cause={}",
                                    status.request, status.context.document_generation,
                                    status.context.project_revision, frame, status.elapsed_ms, bounded);
                                sampling_failures.push(motion_render::PipelineLookaheadFailure { frame, cause:cause.into() });
                                continue;
                            }
                            scenes.push(next);
                        }
                        Ok(())
                    });
                    Reply::Prepared(Completion { ticket, result, sampling_failures })
                    }
                    };
                    // The receiver can leave when a session closes. This is a
                    // protocol-defined abandoned reply, not a compiler failure.
                    if completed.send(reply).is_err() {
                        break;
                    }
                }
            })
            .map_err(Error::Spawn)?;
        Ok(Self {
            sender,
            completed: results,
            thread,
        })
    }
}

#[derive(Default)]
pub(crate) struct State {
    worker: Option<Worker>,
    worker_failure: Option<Arc<Error>>,
    key: Option<Key>,
    ready: Option<Key>,
    ticket: Option<PipelinePreparationTicket>,
    pending: Option<Request>,
    project: Option<(Context, Arc<Project>)>,
    failure: Option<Arc<Error>>,
    limited: bool,
    lookahead_failures: Vec<motion_render::PipelineLookaheadFailure>,
    cache: driver_cache::State,
    #[cfg(test)]
    before_sample: Option<Box<dyn FnOnce() + Send>>,
}
impl State {
    pub(crate) fn cancel(&mut self) {
        self.cache.cancel();
        if let Some(ticket) = self.ticket.take() {
            ticket.cancel();
        }
        self.pending = None;
        self.key = None;
        self.ready = None;
        self.project = None;
        self.failure = None;
        self.limited = false;
        self.lookahead_failures.clear();
        // Retain the single worker across detach/replacement. A driver that
        // never returns cannot cause unbounded replacement threads.
    }
    pub(crate) fn status(&self) -> Value {
        json!({"version":1,"task":self.ticket.as_ref().map(|ticket|ticket.status()),
            "preparing":(self.cache.preparing() || self.key.as_ref().is_some_and(|key|!self.ready.as_ref().is_some_and(|ready|ready.same_revision(key)))) && self.failure.is_none(),
            "limited":self.limited,"pendingEnqueue":self.pending.is_some(),"driverCache":self.cache.status(),
            "workerStopped":self.worker.as_ref().is_some_and(|worker|worker.thread.is_finished()),
            "lookaheadErrors":self.lookahead_failures.iter().map(|failure|json!({"frame":failure.frame,
                "cause":failure.cause.to_string().chars().take(2048).collect::<String>()})).collect::<Vec<_>>(),
            "workerLimit":1,"queueLimit":1,"pendingLimit":1,"completedLimit":1,
            "error":self.failure.as_ref().or(self.worker_failure.as_ref()).map(|error|error.to_string().chars().take(2048).collect::<String>())})
    }
    fn fail(&mut self, error: Error, worker: bool) -> crate::Error {
        let error = Arc::new(error);
        self.failure = Some(error.clone());
        if worker {
            self.worker_failure = Some(error.clone());
        }
        crate::Error::PipelinePreparation(error)
    }
    fn collect(&mut self, renderer: &mut Renderer, context: Context) -> crate::Result<()> {
        for _ in 0..2 {
            let result = match self
                .worker
                .as_ref()
                .map(|worker| worker.completed.try_recv())
            {
                None | Some(Err(mpsc::TryRecvError::Empty)) => break,
                Some(Err(mpsc::TryRecvError::Disconnected)) => {
                    return Err(self.fail(Error::Disconnected, true))
                }
                Some(Ok(result)) => result,
            };
            let result = match result {
                Reply::Cache(completion) => {
                    self.cache.collect(completion, renderer);
                    continue;
                }
                Reply::Prepared(result) => result,
            };
            if result.ticket.status().context != context
                || !self
                    .ticket
                    .as_ref()
                    .is_some_and(|ticket| ticket.same_request(&result.ticket))
            {
                continue;
            }
            match result.result {
                Ok(mut prepared) => {
                    let mut failures = result.sampling_failures;
                    failures.extend(prepared.take_lookahead_failures());
                    let limited = prepared.limited();
                    match renderer.install_prepared_effect_pipelines(prepared, context) {
                        Ok(PipelinePreparationInstall::Installed { .. }) => {
                            self.ready = self.key.clone();
                            self.limited = limited;
                            self.lookahead_failures = failures;
                        }
                        Ok(PipelinePreparationInstall::Superseded) => {}
                        Err(error) => return Err(self.fail(error.into(), false)),
                    }
                }
                Err(error) => return Err(self.fail(error.into(), false)),
            }
        }
        Ok(())
    }
    fn enqueue(&mut self) -> crate::Result<()> {
        let request = self
            .pending
            .take()
            .map(Work::Prepare)
            .or_else(|| self.cache.pending.take().map(Work::Cache));
        if let Some(request) = request {
            let Some(worker) = self.worker.as_ref() else {
                return Err(self.fail(Error::Invariant("enqueue without worker"), false));
            };
            match worker.sender.try_send(request) {
                Ok(()) => {}
                Err(mpsc::TrySendError::Full(request)) => match request {
                    Work::Prepare(request) => self.pending = Some(request),
                    Work::Cache(request) => self.cache.pending = Some(request),
                },
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    return Err(self.fail(Error::Disconnected, true))
                }
            }
        }
        Ok(())
    }
    pub(crate) fn configure_cache(&mut self, storage: DriverPipelineCacheStorage) {
        self.cancel();
        self.cache.configure(storage);
    }
    pub(crate) fn frame_presented(
        &mut self,
        renderer: &mut Renderer,
        context: Context,
    ) -> crate::Result<()> {
        self.cache.frame_presented(renderer, context);
        self.enqueue()
    }
    pub(crate) fn ensure(
        &mut self,
        renderer: &mut Renderer,
        project: &Project,
        scene: &Scene,
        observer: Option<&Observer>,
        key: Key,
    ) -> crate::Result<bool> {
        if let Some(error) = &self.worker_failure {
            return Err(crate::Error::PipelinePreparation(error.clone()));
        }
        self.collect(renderer, key.context)?;
        if !self.cache.begin_restore(renderer, key.context) {
            if self.worker.is_none() {
                self.worker = Some(Worker::new().map_err(|error| self.fail(error, true))?);
            }
            self.enqueue()?;
            return Ok(false);
        }
        let cancelled = self
            .ticket
            .as_ref()
            .is_some_and(|ticket| ticket.status().cancelled);
        if self.key.as_ref() != Some(&key) || cancelled {
            if let Some(ticket) = &self.ticket {
                ticket.cancel();
            }
            self.pending = None;
            self.key = Some(key.clone());
            self.failure = None;
            self.limited = false;
            self.lookahead_failures.clear();
            let job = match renderer.prepare_effect_pipeline_job(
                vec![scene.clone()],
                key.width,
                key.height,
                key.context,
            ) {
                Ok(job) => {
                    let job = job.with_best_effort_lookahead();
                    let job = if key.exposure.is_some() {
                        job.for_exposure()
                    } else {
                        job
                    };
                    if key.preview {
                        job.for_preview()
                    } else {
                        job
                    }
                }
                Err(motion_render::RenderError::PipelinePreparation(
                    motion_render::PipelinePreparationError::SnapshotBudget { .. },
                )) => {
                    // Speculative preparation has a smaller snapshot allowance
                    // than rendering. Its capacity miss must not reject a valid
                    // project or silently alter the ordinary render budget.
                    self.ticket = None;
                    self.ready = Some(key);
                    self.limited = true;
                    return Ok(true);
                }
                Err(error) => return Err(self.fail(error.into(), false)),
            };
            self.ticket = Some(job.ticket());
            if self
                .project
                .as_ref()
                .is_none_or(|(context, _)| *context != key.context)
            {
                self.project = Some((key.context, Arc::new(project.clone())));
            }
            let Some((_, snapshot)) = &self.project else {
                return Err(self.fail(Error::Invariant("project snapshot missing"), false));
            };
            self.pending = Some(Request {
                job,
                project: snapshot.clone(),
                frames: if key.exposure.is_some() {
                    Vec::new()
                } else {
                    upcoming(project, scene.frame)
                },
                observer: observer.cloned(),
                #[cfg(test)]
                before_sample: self.before_sample.take(),
            });
            if self.worker.is_none() {
                self.worker = Some(Worker::new().map_err(|error| self.fail(error, true))?);
            }
        }
        if let Some(error) = &self.failure {
            return Err(crate::Error::PipelinePreparation(error.clone()));
        }
        self.enqueue()?;
        Ok(self
            .ready
            .as_ref()
            .is_some_and(|ready| ready.same_revision(&key)))
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// At most two future samples, preferring actual root layer transitions in the
/// next two seconds. Nested-only/time-remapped transitions remain a documented
/// follow-up; periodic windows still refresh the currently sampled nested scene.
fn upcoming(project: &Project, frame: f64) -> Vec<f64> {
    let end = (frame + project.fps.as_f64() * 2.).min(f64::from(project.frames.saturating_sub(1)));
    let mut result = Vec::new();
    for layer in &project.layers {
        let interval = layer.interval(project.frames);
        for boundary in [interval.in_frame, interval.out_frame] {
            if boundary > frame && boundary <= end {
                result.push(boundary);
            }
        }
    }
    result.sort_by(f64::total_cmp);
    result.dedup();
    result.truncate(2);
    if result.is_empty() && end > frame {
        result.push(end);
    }
    result
}
