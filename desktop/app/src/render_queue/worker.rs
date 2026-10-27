//! One queue owner performs disk work away from native window callbacks.
//! Export cancellation has a separate control path so it also interrupts a
//! blocked snapshot restore; it never waits behind a command in this mailbox.
use super::{
    model::{Document, State, Template},
    store::Store,
};
use crate::{
    fallback_observation as observation,
    movie_export::{ExportStatus, MovieExport},
};
use motion_desktop_media::movie::{ExportControl, FrozenMovie};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, String>;
const CAPACITY: usize = 16;

#[derive(Clone)]
pub(crate) struct Flight {
    pub request: u64,
    pub operation: &'static str,
    pub job: Option<u64>,
    pub began: Instant,
}
#[derive(Clone)]
struct Active {
    job: u64,
    request: u64,
    control: ExportControl,
}
#[derive(Clone)]
pub(crate) struct View {
    pub ready: bool,
    pub running: bool,
    pub stopped: bool,
    pub document: Arc<Document>,
    pub flight: Option<Flight>,
    pub active_job: Option<u64>,
    pub active_request: Option<u64>,
    pub progress: f64,
    pub error: Option<String>,
}
struct Published {
    view: View,
    active: Option<Active>,
    preparing: Option<ExportControl>,
}
struct Shared {
    published: Mutex<Published>,
    shutdown: AtomicBool,
    ids: AtomicU64,
}
impl Shared {
    fn update<T>(&self, f: impl FnOnce(&mut Published) -> T) -> T {
        static FAILURES: observation::Counter = observation::Counter::new();
        observation::recovered(&self.published, &FAILURES, "render_queue.view", f)
    }
    fn next(&self) -> Result<u64> {
        self.ids
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| "render queue request ID exhausted".into())
    }
    fn cancel_active(&self, job: Option<u64>) {
        let active = self.update(|p| p.active.clone());
        if let Some(active) = active.filter(|a| job.is_none_or(|id| id == a.job)) {
            active.control.cancel();
        }
    }
    fn cancel_all(&self) {
        self.cancel_active(None);
        if let Some(control) = self.update(|p| p.preparing.clone()) {
            control.cancel();
        }
    }
}

pub(crate) struct Ticket {
    pub request: u64,
    reply: Receiver<Result<Option<u64>>>,
}
impl Ticket {
    /// None means the accepted request is still pending, not a failure.
    pub fn poll(&self) -> Result<Option<Option<u64>>> {
        match self.reply.try_recv() {
            Ok(reply) => reply.map(Some),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(format!(
                "render queue request {} lost its worker reply",
                self.request
            )),
        }
    }
}
enum Command {
    Add {
        snapshot: FrozenMovie,
        template: Template,
        output: PathBuf,
        control: ExportControl,
    },
    SaveTemplate(Template),
    RemoveTemplate(u64),
    Start,
    Pause,
    Cancel(u64),
    Retry(u64, Option<PathBuf>),
    Remove(u64),
}
impl Command {
    fn operation(&self) -> &'static str {
        match self {
            Self::Add { .. } => "add",
            Self::SaveTemplate(_) => "save_template",
            Self::RemoveTemplate(_) => "remove_template",
            Self::Start => "start",
            Self::Pause => "pause",
            Self::Cancel(_) => "cancel",
            Self::Retry(..) => "retry",
            Self::Remove(_) => "remove",
        }
    }
    fn job(&self) -> Option<u64> {
        match self {
            Self::Cancel(id) | Self::Retry(id, _) | Self::Remove(id) => Some(*id),
            _ => None,
        }
    }
}
struct Request {
    id: u64,
    command: Command,
    reply: SyncSender<Result<Option<u64>>>,
}

pub(crate) struct Queue {
    shared: Arc<Shared>,
    sender: SyncSender<Request>,
    worker: Option<JoinHandle<()>>,
}
impl Queue {
    pub fn open(root: PathBuf) -> Result<Self> {
        let shared = Arc::new(Shared {
            published: Mutex::new(Published {
                view: View {
                    ready: false,
                    running: false,
                    stopped: false,
                    document: Arc::new(Document::default()),
                    flight: None,
                    active_job: None,
                    active_request: None,
                    progress: 0.,
                    error: None,
                },
                active: None,
                preparing: None,
            }),
            shutdown: AtomicBool::new(false),
            ids: AtomicU64::new(1),
        });
        let (sender, receiver) = mpsc::sync_channel(CAPACITY);
        let worker_state = shared.clone();
        let worker = std::thread::Builder::new().name("motion-render-queue".into()).spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run(&root, &receiver, &worker_state)
            }));
            if let Err(payload) = result {
                worker_state.cancel_all();
                let flight = worker_state.update(|p| p.view.flight.clone());
                let cause = format!("render queue worker panicked: {}", motion_media::panic_reason(payload));
                log::error!("render queue failed phase=worker request={:?} operation={:?} job={:?} elapsed_ms={:?} path={} cause={}",
                    flight.as_ref().map(|f| f.request), flight.as_ref().map(|f| f.operation),
                    flight.as_ref().and_then(|f| f.job), flight.as_ref().map(|f| f.began.elapsed().as_millis()),
                    observation::short(root.display()), observation::error_cause(&cause));
                worker_state.update(|p| p.view.error = Some(retain_cause(&cause)));
            }
            worker_state.update(|p| {
                p.view.stopped = true; p.view.running = false; p.view.flight = None;
                p.view.active_job = None; p.active = None; p.preparing = None;
            });
            // Replies after a requester exits are protocol-defined best effort.
            for request in receiver.try_iter() {
                let _ = request.reply.send(Err("render queue worker stopped before this request".into()));
            }
        }).map_err(|error| {
            log::error!("render queue spawn failed cause={error}"); error.to_string()
        })?;
        Ok(Self {
            shared,
            sender,
            worker: Some(worker),
        })
    }
    pub fn view(&self) -> View {
        self.shared.update(|p| {
            let mut view = p.view.clone();
            view.progress = p.active.as_ref().map_or(0., |a| a.control.progress());
            view.active_request = p.active.as_ref().map(|a| a.request);
            view
        })
    }
    fn request(&self, command: Command) -> Result<Ticket> {
        if self.shared.shutdown.load(Ordering::Acquire) || self.view().stopped {
            return Err("render queue worker is stopped".into());
        }
        let id = self.shared.next()?;
        let (reply, receive) = mpsc::sync_channel(1);
        self.sender
            .try_send(Request { id, command, reply })
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => "render queue command capacity reached".to_string(),
                mpsc::TrySendError::Disconnected(_) => {
                    "render queue worker is unavailable".to_string()
                }
            })?;
        Ok(Ticket {
            request: id,
            reply: receive,
        })
    }
    pub fn add(
        &self,
        snapshot: FrozenMovie,
        template: Template,
        output: PathBuf,
        control: ExportControl,
    ) -> Result<Ticket> {
        self.request(Command::Add {
            snapshot,
            template,
            output,
            control,
        })
    }
    pub fn save_template(&self, template: Template) -> Result<Ticket> {
        self.request(Command::SaveTemplate(template))
    }
    pub fn remove_template(&self, id: u64) -> Result<Ticket> {
        self.request(Command::RemoveTemplate(id))
    }
    pub fn start(&self) -> Result<Ticket> {
        self.request(Command::Start)
    }
    /// Finish the active task, then leave all other jobs queued.
    pub fn pause(&self) -> Result<Ticket> {
        self.request(Command::Pause)
    }
    pub fn cancel(&self, id: u64) -> Result<Ticket> {
        let ticket = self.request(Command::Cancel(id))?;
        self.shared.cancel_active(Some(id));
        Ok(ticket)
    }
    pub fn retry(&self, id: u64, output: Option<PathBuf>) -> Result<Ticket> {
        self.request(Command::Retry(id, output))
    }
    pub fn remove(&self, id: u64) -> Result<Ticket> {
        self.request(Command::Remove(id))
    }
    /// Signal cancellation without joining a worker from a window callback.
    pub fn request_shutdown(&self) {
        self.shared.shutdown.store(true, Ordering::Release);
        self.shared.cancel_all();
    }
    pub fn worker_finished(&self) -> bool {
        self.worker.as_ref().is_none_or(JoinHandle::is_finished)
    }
}
impl Drop for Queue {
    fn drop(&mut self) {
        self.request_shutdown();
        if let Some(worker) = self.worker.take() {
            let view = self.view();
            log::info!(
                "render queue shutdown start phase=join active_job={:?} in_flight_request={:?}",
                view.active_job,
                view.flight.as_ref().map(|f| f.request)
            );
            crate::logging::flush();
            let began = Instant::now();
            if let Err(payload) = worker.join() {
                log::error!(
                    "render queue shutdown failed phase=join elapsed_ms={} cause={}",
                    began.elapsed().as_millis(),
                    observation::error_cause(motion_media::panic_reason(payload))
                );
            }
            log::debug!(
                "render queue shutdown done phase=join elapsed_ms={}",
                began.elapsed().as_millis()
            );
        }
    }
}

fn observe<T>(
    shared: &Shared,
    request: u64,
    operation: &'static str,
    job: Option<u64>,
    f: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let began = Instant::now();
    shared.update(|p| {
        p.view.flight = Some(Flight {
            request,
            operation,
            job,
            began,
        })
    });
    log::info!("render queue start request={request} operation={operation} job={job:?}");
    crate::logging::flush();
    let result = f();
    match &result {
        Ok(_) => log::debug!("render queue done request={request} operation={operation} job={job:?} elapsed_ms={}", began.elapsed().as_millis()),
        Err(cause) if cause == "movie export cancelled" => log::info!("render queue cancelled request={request} operation={operation} job={job:?} elapsed_ms={}", began.elapsed().as_millis()),
        Err(cause) => log::error!("render queue failed request={request} operation={operation} job={job:?} elapsed_ms={} cause={}", began.elapsed().as_millis(), observation::error_cause(cause)),
    }
    shared.update(|p| {
        p.view.flight = None;
        if let Err(cause) = &result {
            if cause != "movie export cancelled" {
                p.view.error = Some(retain_cause(cause));
            }
        }
    });
    result
}
fn retain_cause(cause: &str) -> String {
    let mut end = cause.len().min(8 * 1024);
    while !cause.is_char_boundary(end) {
        end -= 1;
    }
    cause[..end].to_owned()
}
struct Worker<'a> {
    shared: &'a Shared,
    store: Store,
    running: bool,
    export: Option<(u64, u64, MovieExport)>,
}
impl Worker<'_> {
    fn publish(&self) {
        let document = Arc::new(self.store.document.clone());
        self.shared.update(|p| {
            p.view.document = document;
            p.view.running = self.running;
        });
    }
    fn command(&mut self, request: Request) {
        let operation = request.command.operation();
        let job = request.command.job();
        let result = observe(self.shared, request.id, operation, job, || {
            match request.command {
                Command::Add {
                    snapshot,
                    template,
                    output,
                    control,
                } => {
                    self.shared.update(|p| p.preparing = Some(control.clone()));
                    if self.shared.shutdown.load(Ordering::Acquire) {
                        control.cancel();
                    }
                    let scope = template.scope;
                    let prepared = snapshot
                        .with_scope(scope)
                        .prepare_persistent(&self.store.snapshots(), &control)?;
                    control.check()?;
                    self.store.add(prepared, template, output).map(Some)
                }
                Command::SaveTemplate(template) => self.store.save_template(template).map(Some),
                Command::RemoveTemplate(id) => self.store.remove_template(id).map(|()| None),
                Command::Start => {
                    if !cfg!(feature = "ffmpeg") {
                        return Err("MP4 render queue requires a build with --ffmpeg".into());
                    }
                    self.running = true;
                    Ok(None)
                }
                Command::Pause => {
                    self.running = false;
                    Ok(None)
                }
                Command::Cancel(id) => self.store.cancel(id).map(|_| None),
                Command::Retry(id, output) => self.store.retry(id, output).map(|()| None),
                Command::Remove(id) => self.store.remove(id).map(|()| None),
            }
        });
        self.shared.update(|p| p.preparing = None);
        self.publish();
        // The caller may have closed its project/window after accepting work.
        let _ = request.reply.send(result);
    }
    fn finish(&mut self, id: u64, request: u64, status: ExportStatus) {
        let terminal = match status.state {
            "succeeded" => status
                .result
                .map(|r| State::Succeeded {
                    bytes: r.bytes,
                    encoder: r.video_encoder,
                })
                .ok_or_else(|| "completed render has no published result".to_owned()),
            "cancelled" => Ok(State::Cancelled),
            "failed" => status
                .error
                .map(|cause| State::Failed {
                    cause: retain_cause(&cause),
                })
                .ok_or_else(|| "render failed without a cause".to_owned()),
            _ => Err("render stopped without a terminal status".into()),
        };
        let result = observe(self.shared, request, "finish", Some(id), || {
            let terminal = terminal?;
            self.store
                .finish(id, terminal.clone())
                .map_err(|secondary| {
                    if let State::Failed { cause } = terminal {
                        format!("{cause}; saving failed render status also failed: {secondary}")
                    } else {
                        format!("save render completion status: {secondary}")
                    }
                })
        });
        if result.is_err() {
            self.running = false;
        }
        self.shared.update(|p| {
            p.active = None;
            p.view.active_job = None;
        });
        self.publish();
    }
    fn tick(&mut self) -> Result<()> {
        if self
            .export
            .as_ref()
            .is_some_and(|(_, _, task)| task.finished())
        {
            if let Some((id, request, task)) = self.export.take() {
                let status = task.status();
                drop(task);
                self.finish(id, request, status);
            }
        }
        if !self.running || self.export.is_some() || self.shared.shutdown.load(Ordering::Acquire) {
            return Ok(());
        }
        let request = self.shared.next()?;
        let job = observe(self.shared, request, "begin", None, || {
            self.store.begin_next()
        })?;
        let Some(job) = job else {
            self.running = false;
            self.publish();
            return Ok(());
        };
        let control = ExportControl::default();
        self.shared.update(|p| {
            p.view.active_job = Some(job.id);
            p.active = Some(Active {
                job: job.id,
                request,
                control: control.clone(),
            });
        });
        self.publish();
        let restored = observe(self.shared, request, "restore", Some(job.id), || {
            if self.shared.shutdown.load(Ordering::Acquire) {
                control.cancel();
            }
            control.check()?;
            let path = self.store.snapshot_path(&job)?;
            let snapshot = FrozenMovie::restore_persistent_with_request(&path, &control, request)?;
            if snapshot.snapshot_info() != job.source {
                return Err("queued task metadata differs from its frozen input".into());
            }
            MovieExport::start_new_file_with_control(snapshot, job.output.clone(), control)
        });
        match restored {
            Ok(task) => self.export = Some((job.id, request, task)),
            Err(error) => self.finish(
                job.id,
                request,
                ExportStatus {
                    state: if error == "movie export cancelled" {
                        "cancelled"
                    } else {
                        "failed"
                    },
                    progress: 0.,
                    revision: job.source.revision,
                    result: None,
                    error: Some(error),
                },
            ),
        }
        Ok(())
    }
}
fn run(root: &std::path::Path, receiver: &Receiver<Request>, shared: &Shared) {
    let store = match observe(shared, 0, "open", None, || Store::open(root)) {
        Ok(store) => store,
        Err(_) => return,
    };
    let mut worker = Worker {
        shared,
        store,
        running: false,
        export: None,
    };
    worker.publish();
    shared.update(|p| p.view.ready = true);
    while !shared.shutdown.load(Ordering::Acquire) {
        match receiver.recv_timeout(Duration::from_millis(16)) {
            Ok(request) => worker.command(request),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if let Err(cause) = worker.tick() {
            // A journal failure stops scheduling. Recovery is explicit, never a
            // loop that retries the same failed operation on every poll.
            worker.running = false;
            shared.update(|p| p.view.error = Some(retain_cause(&cause)));
            worker.publish();
        }
    }
    if let Some((id, request, task)) = worker.export.take() {
        let status = observe(shared, request, "shutdown_export", Some(id), || {
            Ok(task.cancel_and_join())
        });
        if let Ok(status) = status {
            worker.finish(id, request, status);
        }
    }
}

#[cfg(test)]
mod tests;
