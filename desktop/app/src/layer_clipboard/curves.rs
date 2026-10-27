//! The engine captures timing from the stamped source. OS clipboard IO belongs
//! to one long-lived thread, preserving Linux selection ownership while open.
use super::{KeyframeSource, Stamp};
use motion_core::{CurveClipboard, KeyframeSelection};
use motion_host::session::Session;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

pub(crate) struct Read {
    pub stamp: Stamp,
    pub clipboard: Option<CurveClipboard>,
    pub superseded: bool,
}
pub(crate) fn read(
    session: &Session,
    source: KeyframeSource,
    selection: Option<KeyframeSelection>,
) -> motion_host::Result<Read> {
    let stamp = Stamp::of(session);
    let superseded = !source.current(session);
    let clipboard = if superseded {
        None
    } else {
        selection
            .map(|selected| CurveClipboard::capture(session.engine.project(), &selected))
            .transpose()?
    };
    Ok(Read {
        stamp,
        clipboard,
        superseded,
    })
}

pub(crate) enum Work {
    Copy(CurveClipboard),
    Paste,
}
pub(crate) enum Output {
    Copied,
    Paste(CurveClipboard),
    Cancelled,
}
struct Request {
    work: Work,
    cancel: Arc<AtomicBool>,
    progress: Arc<Progress>,
    reply: mpsc::Sender<Result<Output, String>>,
    before: Option<Box<dyn FnOnce() + Send>>,
}
struct Progress {
    request: u64,
    phase: AtomicU8,
    began: Instant,
}
pub(crate) struct Job {
    progress: Arc<Progress>,
    receive: mpsc::Receiver<Result<Output, String>>,
}
#[derive(Default)]
pub(crate) struct Service {
    send: Option<mpsc::SyncSender<Request>>,
}

fn bounded(cause: impl std::fmt::Display) -> String {
    cause.to_string().chars().take(1024).collect()
}

impl Service {
    pub(crate) fn submit(&mut self, work: Work, cancel: Arc<AtomicBool>) -> Result<Job, String> {
        self.submit_with(work, cancel, None)
    }
    fn submit_with(
        &mut self,
        work: Work,
        cancel: Arc<AtomicBool>,
        before: Option<Box<dyn FnOnce() + Send>>,
    ) -> Result<Job, String> {
        if self.send.is_none() {
            let (send, receive) = mpsc::sync_channel::<Request>(1);
            std::thread::Builder::new()
                .name("motion-curve-clipboard".into())
                .spawn(move || {
                    let mut clipboard = None;
                    while let Ok(request) = receive.recv() {
                        run(request, &mut clipboard);
                    }
                    // Dropping the sender ends this loop without waiting on the UI.
                    // Linux clipboard managers decide whether data survives exit.
                })
                .map_err(|cause| {
                    log::error!("curve clipboard worker spawn failed reason={cause}");
                    cause.to_string()
                })?;
            self.send = Some(send);
        }
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let request = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| {
                log::error!("curve clipboard start failed reason=request_space_exhausted");
                "curve clipboard request space exhausted"
            })?;
        let operation = match &work {
            Work::Copy(_) => "copy",
            Work::Paste => "paste",
        };
        let progress = Arc::new(Progress {
            request,
            phase: AtomicU8::new(0),
            began: Instant::now(),
        });
        let (reply, receive) = mpsc::channel();
        let job = Job {
            progress: progress.clone(),
            receive,
        };
        // Start and in-flight state precede native clipboard initialization/IO.
        log::info!("curve clipboard start request={request} operation={operation} phase=queued");
        crate::logging::flush();
        let Some(send) = self.send.as_ref() else {
            log::error!("curve clipboard start failed request={request} operation={operation} reason=service_missing");
            return Err("curve clipboard service missing".into());
        };
        send.try_send(Request { work, cancel, progress, reply, before }).map_err(|cause| {
            log::error!("curve clipboard queue failed request={request} operation={operation} reason={cause}");
            bounded(cause)
        })?;
        Ok(job)
    }
}

fn run(request: Request, clipboard: &mut Option<arboard::Clipboard>) {
    let Request {
        work,
        cancel,
        progress,
        reply,
        before,
    } = request;
    let operation = match &work {
        Work::Copy(_) => "copy",
        Work::Paste => "paste",
    };
    progress.phase.store(1, Ordering::Release);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> Result<Output, String> {
            if let Some(before) = before {
                before();
            }
            if cancel.load(Ordering::Acquire) {
                return Ok(Output::Cancelled);
            }
            if clipboard.is_none() {
                *clipboard = Some(
                    arboard::Clipboard::new()
                        .map_err(|cause| format!("system clipboard initialization: {cause}"))?,
                );
            }
            let Some(clipboard) = clipboard.as_mut() else {
                return Err("system clipboard initialization returned no context".into());
            };
            if cancel.load(Ordering::Acquire) {
                return Ok(Output::Cancelled);
            }
            match work {
                Work::Copy(payload) => {
                    let text = payload
                        .to_text()
                        .map_err(|cause| format!("serialize easing clipboard: {cause}"))?;
                    clipboard
                        .set_text(text)
                        .map_err(|cause| format!("system clipboard write: {cause}"))?;
                    Ok(Output::Copied)
                }
                Work::Paste => {
                    let text = clipboard
                        .get_text()
                        .map_err(|cause| format!("system clipboard read: {cause}"))?;
                    if cancel.load(Ordering::Acquire) {
                        return Ok(Output::Cancelled);
                    }
                    CurveClipboard::from_text(&text)
                        .map(Output::Paste)
                        .map_err(|cause| format!("invalid easing clipboard: {cause}"))
                }
            }
        },
    ))
    .unwrap_or_else(|payload| {
        let cause = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or("non-text panic");
        Err(format!(
            "curve clipboard worker panicked: {}",
            bounded(cause)
        ))
    });
    progress.phase.store(2, Ordering::Release);
    let elapsed = progress.began.elapsed().as_millis();
    let request = progress.request;
    match &result {
        Err(cause) => log::error!("curve clipboard failed request={request} operation={operation} phase=io elapsed_ms={elapsed} reason={}", bounded(cause)),
        Ok(_) => log::debug!("curve clipboard done request={request} operation={operation} elapsed_ms={elapsed}"),
    }
    // Best-effort protocol reply after the UI closes; native context stays on
    // its owning thread until the service channel is closed.
    let _ = reply.send(result.map_err(bounded));
}

#[cfg(test)]
mod tests;

impl Job {
    pub(crate) fn status(&self) -> (u64, &'static str, Duration) {
        (
            self.progress.request,
            match self.progress.phase.load(Ordering::Acquire) {
                0 => "queued",
                1 => "io",
                _ => "complete",
            },
            self.progress.began.elapsed(),
        )
    }
    pub(crate) fn poll(&self) -> Option<Result<Output, String>> {
        match self.receive.try_recv() {
            Ok(value) => Some(value),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                let (request, phase, elapsed) = self.status();
                log::error!("curve clipboard worker disconnected request={request} phase={phase} elapsed_ms={}", elapsed.as_millis());
                Some(Err("curve clipboard worker stopped before replying".into()))
            }
        }
    }
}
