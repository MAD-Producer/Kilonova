//! Bounded source IO/compiler ownership. No source text enters diagnostic logs.
use motion_graph::plugin_project::PluginProject;
use std::{
    path::PathBuf,
    sync::{
        mpsc::{self, Receiver, SyncSender},
        Arc, Mutex,
    },
    time::Instant,
};
pub struct Ready {
    pub prepared: crate::node_preview::PreparedPlugin,
    pub generated: String,
}
#[derive(Clone, Debug)]
pub struct Fault {
    pub phase: &'static str,
    pub message: String,
    pub location: Option<(usize, usize)>,
}
pub enum Job {
    Load(Option<PathBuf>),
    Compile(u64, PluginProject),
    Save(u64, PluginProject, Option<PathBuf>),
    #[cfg(test)]
    Block(Receiver<()>),
    #[cfg(test)]
    Panic,
}
pub enum Event {
    Loaded(PathBuf, PluginProject),
    Compiled(u64, Result<Ready, Fault>),
    Saved(u64, PathBuf, PluginProject),
    Cancelled,
    Failed(Fault),
}
#[derive(Clone, Debug)]
pub struct Flight {
    pub request: u64,
    pub phase: &'static str,
    pub began: Instant,
}
pub struct Worker {
    send: Option<SyncSender<Job>>,
    pub reply: Receiver<Event>,
    pub flight: Arc<Mutex<Option<Flight>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
fn fault(phase: &'static str, error: impl std::fmt::Display) -> Fault {
    Fault {
        phase,
        message: crate::fallback_observation::error_cause(error),
        location: None,
    }
}
fn compile_staged(
    project: PluginProject,
    before_decode: impl FnOnce() -> Result<(), Fault>,
) -> Result<Ready, Fault> {
    let compiled = project.compile_logic().map_err(|error| {
        let mut result = fault("compile", &error);
        result.location = project.compile_error_location(&error);
        result
    })?;
    let generated = compiled.source().to_owned();
    before_decode()?;
    let prepared = crate::node_preview::PreparedPlugin::new(compiled)
        .map_err(|e| fault("resource_decode", e))?;
    Ok(Ready {
        prepared,
        generated,
    })
}
impl Worker {
    pub fn new() -> Result<Self, String> {
        let (send, jobs) = mpsc::sync_channel(1);
        let (out, reply) = mpsc::sync_channel(1);
        let flight = Arc::new(Mutex::new(None));
        let state = flight.clone();
        let thread = std::thread::Builder::new()
            .name("plugin-author-source".into())
            .spawn(move || run(jobs, out, state))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            send: Some(send),
            reply,
            flight,
            thread: Some(thread),
        })
    }
    pub fn submit(&self, job: Job) -> Result<(), String> {
        self.send
            .as_ref()
            .ok_or("author source worker closed")?
            .try_send(job)
            .map_err(|e| format!("author source queue: {e}"))
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        drop(self.send.take());
        // A compiler/native IO already in progress is not killed or restarted.
        // Drop the response receiver after join only when no work is pending:
        // callers keep draining replies while their close prompt is active.
        if let Some(thread) = self.thread.take() {
            if thread.is_finished() {
                if let Err(payload) = thread.join() {
                    log::error!(
                        "plugin.author worker panic cause={}",
                        crate::fallback_observation::error_cause(motion_media::panic_reason(
                            payload
                        ))
                    );
                }
            }
        }
    }
}

fn run(jobs: Receiver<Job>, out: SyncSender<Event>, state: Arc<Mutex<Option<Flight>>>) {
    let mut request = 0u64;
    while let Ok(job) = jobs.recv() {
        request = request.wrapping_add(1);
        let phase = match &job {
            Job::Load(_) => "read",
            Job::Compile(..) => "compile",
            Job::Save(..) => "write",
            #[cfg(test)]
            Job::Block(_) => "controlled_block",
            #[cfg(test)]
            Job::Panic => "controlled_panic",
        };
        let began = Instant::now();
        match state.lock() {
            Ok(mut f) => {
                *f = Some(Flight {
                    request,
                    phase,
                    began,
                })
            }
            Err(_) => {
                log::error!("plugin.author error request={request} phase={phase} cause=in-flight lock poisoned");
                break;
            }
        }
        let revision = match &job {
            Job::Compile(revision, _) | Job::Save(revision, ..) => Some(*revision),
            _ => None,
        };
        let object = match &job {
            Job::Compile(_, p) | Job::Save(_, p, _) => {
                crate::fallback_observation::short(&p.package.id).to_string()
            }
            _ => String::new(),
        };
        let source = match &job {
            Job::Load(Some(p)) | Job::Save(_, _, Some(p)) => {
                crate::fallback_observation::short(p.display()).to_string()
            }
            _ => String::new(),
        };
        // Publish before a native dialog, filesystem call or compiler.
        log::info!("plugin.author start request={request} phase={phase} in_flight=true revision={revision:?} object={object} source={source}");
        crate::logging::flush();
        struct Scope(Arc<Mutex<Option<Flight>>>, u64, &'static str, Instant);
        impl Drop for Scope {
            fn drop(&mut self) {
                let (poisoned, final_phase) = match self.0.lock() {
                    Ok(mut f) => (false, f.take().map_or(self.2, |flight| flight.phase)),
                    Err(_) => (true, self.2),
                };
                if poisoned {
                    log::error!("plugin.author phase={} request={} cause=in-flight lock poisoned during completion",self.2,self.1);
                }
                if std::thread::panicking() {
                    log::error!("plugin.author aborted request={} phase={} elapsed_ms={} cause=panic; original cause recorded by process hook",self.1,final_phase,self.3.elapsed().as_millis());
                }
            }
        }
        let scope = Scope(state.clone(), request, phase, began);
        let event = match job {
            Job::Load(path) => {
                let path = path.or_else(|| {
                    rfd::FileDialog::new()
                        .add_filter("Motion Studio plugin", &["msplugin"])
                        .pick_file()
                });
                match path {
                    None => Event::Cancelled,
                    Some(path) => match PluginProject::load(&path) {
                        Ok(project) => Event::Loaded(path, project),
                        Err(e) => Event::Failed(fault("read", e)),
                    },
                }
            }
            Job::Compile(revision, project) => Event::Compiled(
                revision,
                compile_staged(project, || {
                    match state.lock() {
                        Ok(mut f) => {
                            *f = Some(Flight {
                                request,
                                phase: "resource_decode",
                                began: Instant::now(),
                            })
                        }
                        Err(_) => return Err(fault("resource_decode", "in-flight lock poisoned")),
                    }
                    log::info!("plugin.author start request={request} revision={revision} phase=resource_decode in_flight=true");
                    crate::logging::flush();
                    Ok(())
                }),
            ),
            #[cfg(test)]
            Job::Block(gate) => {
                let _abandoned = gate.recv();
                Event::Cancelled
            }
            #[cfg(test)]
            Job::Panic => panic!("controlled plugin author handler panic"),
            Job::Save(revision, project, path) => {
                let path = path.or_else(|| {
                    rfd::FileDialog::new()
                        .add_filter("Motion Studio plugin", &["msplugin"])
                        .set_file_name("plugin.msplugin")
                        .save_file()
                });
                match path {
                    None => Event::Cancelled,
                    Some(path) => match project.save(&path) {
                        Ok(()) => Event::Saved(revision, path, project),
                        Err(e) => Event::Failed(fault("write", e)),
                    },
                }
            }
        };
        let failure = match &event {
            Event::Failed(f) => Some(f),
            Event::Compiled(_, Err(f)) => Some(f),
            _ => None,
        };
        if let Some(f) = failure {
            log::error!("plugin.author error request={request} phase={} elapsed_ms={} location={:?} cause={}",f.phase,began.elapsed().as_millis(),f.location,f.message);
        }
        log::debug!(
            "plugin.author done request={request} phase={phase} elapsed_ms={}",
            began.elapsed().as_millis()
        );
        drop(scope);
        // Closing the owner abandons only its protocol reply.
        if out.send(event).is_err() {
            break;
        }
    }
}
