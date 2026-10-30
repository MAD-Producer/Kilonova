//! Same-process author window. It owns no host selection, playback or commands.
//! One bounded worker performs dialogs, startup and teardown outside the UI loop.
use super::*;
use std::{
    sync::Mutex,
    thread::{self, JoinHandle},
};
#[cfg(all(test, windows))]
#[path = "window_host_tests.rs"]
mod tests;
struct Owner {
    // The author workspace contains a complete node editor. Keep it out of the
    // already large Shell value and Windows' default main-thread stack.
    app: Box<App>,
    reader: Option<JoinHandle<()>>,
    frozen: bool,
}
#[derive(Clone)]
struct Flight {
    request: u64,
    phase: &'static str,
    began: Instant,
}
enum Job {
    Open {
        path: Option<PathBuf>,
        zh: bool,
    },
    Retire(Owner),
    Recover {
        project: motion_graph::plugin_project::PluginProject,
        path: PathBuf,
        zh: bool,
    },
    #[cfg(test)]
    Block {
        entered: mpsc::SyncSender<()>,
        release: Receiver<()>,
    },
    #[cfg(test)]
    Panic,
}
enum Done {
    Opened(Option<Owner>),
    Retired,
    Recovered(bool),
}
struct Queued {
    flight: Flight,
    job: Job,
}
struct Worker {
    send: mpsc::SyncSender<Queued>,
    reply: Receiver<Result<Done, String>>,
    flight: Arc<Mutex<Option<Flight>>>,
    thread: Option<JoinHandle<()>>,
}
impl Worker {
    fn new() -> Result<Self, String> {
        let (send, jobs) = mpsc::sync_channel::<Queued>(1);
        let (out, reply) = mpsc::sync_channel(1);
        let flight: Arc<Mutex<Option<Flight>>> = Default::default();
        let state = flight.clone();
        let thread = thread::Builder::new()
            .name("plugin-author-window".into())
            .spawn(move || {
                while let Ok(queued) = jobs.recv() {
                    let Flight{request, phase, began} = queued.flight.clone();
                    match state.lock() {
                        Ok(mut s) => *s=Some(queued.flight),
                        Err(_) => {log::error!("plugin.host phase=dispatch cause=flight poisoned");break;}
                    }
                    log::info!("plugin.host start request={request} phase={phase} in_flight=true");
                    crate::logging::flush();
                    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||execute(queued.job)))
                        .unwrap_or_else(|_|Err("author window worker panicked; original cause recorded by process hook".into()));
                    if let Err(error)=&result {
                        log::error!("plugin.host error request={request} phase={phase} elapsed_ms={} cause={}",began.elapsed().as_millis(),crate::fallback_observation::error_cause(error));
                    } else {log::debug!("plugin.host done request={request} phase={phase} elapsed_ms={}",began.elapsed().as_millis());}
                    if let Ok(mut s)=state.lock(){*s=None;}
                    // Closing the owner intentionally abandons this protocol reply.
                    if out.send(result).is_err(){break;}
                }
            }).map_err(|e|format!("start author window worker: {e}"))?;
        Ok(Self {
            send,
            reply,
            flight,
            thread: Some(thread),
        })
    }
}
fn execute(job: Job) -> Result<Done, String> {
    match job {
        Job::Open { path, zh } => {
            let path = path.or_else(|| {
                rfd::FileDialog::new()
                    .add_filter("Motion Studio Plugin", &["msplugin"])
                    .pick_file()
            });
            let Some(path) = path else {
                return Ok(Done::Opened(None));
            };
            let (mut app, reader) = prepare_window(&path, zh, true)?;
            app.embedded = true;
            Ok(Done::Opened(Some(Owner {
                app: Box::new(app),
                reader: Some(reader),
                frozen: false,
            })))
        }
        Job::Retire(owner) => {
            drop(owner);
            Ok(Done::Retired)
        }
        Job::Recover { project, path, zh } => {
            let choice=rfd::MessageDialog::new()
                .set_title(tr(zh,"插件预览已停止","Plugin preview stopped"))
                .set_description(tr(zh,"保存作者草稿后关闭？选择“否”放弃修改，“取消”保留草稿和窗口。","Save the author draft before closing? No discards edits; Cancel keeps the draft and window."))
                .set_buttons(rfd::MessageButtons::YesNoCancel).show();
            match choice {
                rfd::MessageDialogResult::Yes => {
                    project.save(&path).map_err(|e| e.to_string())?;
                    Ok(Done::Recovered(true))
                }
                rfd::MessageDialogResult::No => Ok(Done::Recovered(true)),
                _ => Ok(Done::Recovered(false)),
            }
        }
        #[cfg(test)]
        Job::Block { entered, release } => {
            let _ = entered.send(());
            release.recv().map_err(|e| e.to_string())?;
            Ok(Done::Retired)
        }
        #[cfg(test)]
        Job::Panic => panic!("controlled author window panic"),
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(t) = self.thread.take().filter(|t| t.is_finished()) {
            if let Err(e) = t.join() {
                log::error!(
                    "plugin.host worker panic cause={}",
                    crate::fallback_observation::error_cause(motion_media::panic_reason(e))
                );
            }
        }
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        if let Some(reader) = self.reader.take().filter(|t| t.is_finished()) {
            if let Err(payload) = reader.join() {
                log::error!(
                    "plugin.host source reader panic cause={}",
                    crate::fallback_observation::error_cause(motion_media::panic_reason(payload))
                );
            }
        }
    }
}
#[derive(Default)]
pub struct Hosted {
    worker: Option<Worker>,
    active: Option<Owner>,
    busy: bool,
    failed: bool,
    request: u64,
    owner_close: bool,
    prompt_requested: bool,
    pub message: Option<String>,
}
impl Hosted {
    fn submit(&mut self, job: Job, phase: &'static str) -> Result<(), Job> {
        let Some(worker) = &self.worker else {
            return Err(job);
        };
        let Some(request) = self.request.checked_add(1) else {
            self.report("author window request identity exhausted".into());
            return Err(job);
        };
        self.request = request;
        let observation = Flight {
            request: self.request,
            phase,
            began: Instant::now(),
        };
        let published = match worker.flight.lock() {
            Ok(mut flight) => {
                *flight = Some(observation.clone());
                true
            }
            Err(_) => false,
        };
        if !published {
            self.failed = true;
            self.report("author window in-flight state poisoned; job not dispatched".into());
            return Err(job);
        }
        match worker.send.try_send(Queued {
            flight: observation,
            job,
        }) {
            Ok(()) => {
                self.busy = true;
                Ok(())
            }
            Err(error) => {
                if let Ok(mut flight) = worker.flight.lock() {
                    *flight = None;
                }
                self.failed = true;
                self.report(format!("author window queue: {error}"));
                Err(match error {
                    mpsc::TrySendError::Full(queued) | mpsc::TrySendError::Disconnected(queued) => {
                        queued.job
                    }
                })
            }
        }
    }
    fn report(&mut self, error: String) {
        log::error!(
            "plugin.host phase=controller request={} cause={}",
            self.request,
            crate::fallback_observation::error_cause(&error)
        );
        self.message = Some(error);
    }
    pub fn open(&mut self, path: Option<PathBuf>, zh: bool) -> Result<(), String> {
        if let Some(owner) = &self.active {
            if let Some(window) = &owner.app.window {
                window.focus_window();
            }
            return Ok(());
        }
        if self.busy {
            return Ok(());
        }
        if self.failed {
            return Err("author window worker stopped; draft retained".into());
        }
        if self.worker.is_none() {
            self.worker = Some(Worker::new().map_err(|e| {
                log::error!("plugin.host phase=start cause={e}");
                e
            })?);
        }
        self.submit(Job::Open { path, zh }, "open")
            .map_err(|_| "author window launch not queued".into())
    }
    pub fn pending(&self) -> bool {
        self.busy
            || self.active.as_ref().is_some_and(|o| {
                if o.frozen {
                    o.app.author.as_ref().is_some_and(|a| a.pending())
                } else {
                    o.app.pending()
                }
            })
    }
    pub fn contains(&self, id: WindowId) -> bool {
        self.active
            .as_ref()
            .is_some_and(|o| o.app.window.as_ref().is_some_and(|w| w.id() == id))
    }
    pub fn event(&mut self, events: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if !self.contains(id) {
            return;
        }
        if self.active.as_ref().is_some_and(|o| o.frozen) {
            if matches!(event, WindowEvent::CloseRequested) {
                self.request_close();
            }
            return;
        }
        if let Some(owner) = &mut self.active {
            owner.app.window_event(events, id, event);
        }
    }
    /// Returns true while the child has first refusal over closing the owner.
    pub fn request_owner_close(&mut self) -> bool {
        if self.active.is_none() && !self.busy {
            return false;
        }
        if !self.owner_close {
            self.owner_close = true;
            self.request_close();
        }
        true
    }
    fn request_close(&mut self) {
        let Some(owner) = &mut self.active else {
            return;
        };
        if let Some(w) = &owner.app.window {
            w.focus_window();
        }
        if owner.frozen {
            if self.busy {
                return;
            }
            if let Some(author) = &mut owner.app.author {
                match author.recovery_source() {
                    Ok((project, path)) => {
                        let zh = owner.app.zh;
                        let _ = self.submit(Job::Recover { project, path, zh }, "recover_close");
                    }
                    Err(e) => {
                        self.report(e);
                        self.owner_close = false;
                    }
                }
            }
        } else if let Some(author) = &mut owner.app.author {
            author.request_close();
            owner.app.dirty = true;
            self.prompt_requested = true;
        } else {
            owner.app.finished = true;
        }
    }
    fn retire(&mut self) {
        if self.failed || self.busy {
            return;
        }
        if let Some(owner) = self.active.take() {
            if let Err(Job::Retire(owner)) = self.submit(Job::Retire(owner), "close") {
                self.active = Some(owner);
            }
        }
    }
    /// Polls only this tool and never changes the host event-loop control flow.
    /// True means its close completed and the shell may resume its own close.
    pub fn poll(&mut self, events: &ActiveEventLoop) -> bool {
        let received = self.worker.as_ref().map(|w| w.reply.try_recv());
        let result = match received {
            Some(Ok(result)) => Some(result),
            Some(Err(TryRecvError::Disconnected)) if !self.failed => {
                self.failed = true;
                Some(Err("author window worker disconnected".into()))
            }
            _ => None,
        };
        if let Some(result) = result {
            self.busy = false;
            match result {
                Ok(Done::Opened(owner)) => {
                    self.active = owner;
                    if let Some(o) = &mut self.active {
                        o.app.resumed(events);
                    }
                    if self.owner_close {
                        self.request_close();
                    }
                }
                Ok(Done::Retired) => {
                    let close = self.owner_close;
                    self.owner_close = false;
                    self.prompt_requested = false;
                    return close;
                }
                Ok(Done::Recovered(close)) => {
                    if close {
                        self.retire();
                    } else {
                        self.owner_close = false;
                    }
                }
                Err(e) => {
                    self.report(e);
                    self.owner_close = false;
                }
            }
            if self.owner_close && self.active.is_none() && !self.busy {
                self.owner_close = false;
                return true;
            }
        }
        if let Some(owner) = &mut self.active {
            if owner.frozen {
                if let Some(author) = &mut owner.app.author {
                    if let Err(error) = author.poll() {
                        self.message = Some(error);
                    }
                }
            }
            if !owner.frozen {
                owner.app.about_to_wait(events);
            }
            if owner.app.finished && !owner.frozen {
                let dirty = owner.app.author.as_ref().is_some_and(|a| a.unsaved());
                let approved = owner.app.author.as_ref().is_some_and(|a| a.close_ready);
                if owner.app.error.is_some() && dirty && !approved {
                    owner.frozen = true;
                    self.message=Some("Plugin preview stopped; author draft retained. Close the plugin window to save or discard it.".into());
                } else {
                    self.retire();
                }
            } else if self.owner_close
                && self.prompt_requested
                && owner
                    .app
                    .author
                    .as_ref()
                    .is_some_and(|a| !a.close_prompt && !a.close_ready)
            {
                self.owner_close = false;
                self.prompt_requested = false;
            }
        }
        false
    }
}
impl Drop for Hosted {
    fn drop(&mut self) {
        // Normal shell close drains retirement first. On an exceptional unwind,
        // still transfer a live owner to the existing worker instead of joining GPU IO here.
        if !self.busy {
            self.retire();
        }
        if let (Some(owner), Some(worker)) = (self.active.take(), &self.worker) {
            if let Err(e) = worker.send.try_send(Queued {
                flight: Flight {
                    request: self.request.saturating_add(1),
                    phase: "abandon_close",
                    began: Instant::now(),
                },
                job: Job::Retire(owner),
            }) {
                log::error!("plugin.host cleanup cause=retirement channel unavailable: {e}");
            }
        }
    }
}
