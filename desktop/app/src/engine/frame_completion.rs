//! Bounded continuations for frame commands awaiting pipeline preparation.
//! Only the existing task is polled. No extra thread, error retry, or blocking
//! driver wait is introduced on the serial engine worker.
use super::*;
use std::collections::VecDeque;

const POLL_INTERVAL: Duration = Duration::from_millis(8);
const PENDING_LIMIT: usize = COMMAND_QUEUE_CAPACITY;

#[derive(Clone)]
pub(super) struct Meta {
    pub queued: Instant,
    pub prerender_input: u64,
    pub flight: Flight,
    pub context: crate::diagnostics::Context,
}

#[derive(PartialEq)]
pub(super) struct Stamp {
    document: u64,
    revision: u64,
    composition: String,
    frame: u64,
    view: u64,
    surface: u64,
    extent: [u32; 2],
    window: u64,
}
impl Stamp {
    fn capture(
        id: i64,
        window: u64,
        windows: &std::collections::HashMap<u64, FloatingSurface>,
    ) -> motion_host::Result<Option<Self>> {
        motion_host::session::with_session(id, |session| {
            let Some(graphics) = &session.graphics else {
                return Ok(None);
            };
            let config = if window == 0 {
                &graphics.config
            } else if let Some(surface) = windows.get(&window) {
                &surface.config
            } else {
                return Ok(None);
            };
            Ok(Some(Self {
                document: session.document_generation,
                revision: session.engine.revision(),
                composition: session.engine.project().composition_id.clone(),
                frame: session.frame.to_bits(),
                view: session.view_revision,
                surface: session.surface_epoch,
                extent: [config.width, config.height],
                window,
            }))
        })
    }
}

pub(super) struct Pending {
    pub command: Command,
    pub meta: Meta,
    pub stamp: Stamp,
    pub waiting: bool,
}
pub(super) enum Progress {
    Ready,
    Waiting,
    Superseded,
}
impl Stamp {
    pub fn progress(
        &self,
        waiting: bool,
        id: i64,
        windows: &std::collections::HashMap<u64, FloatingSurface>,
    ) -> motion_host::Result<Progress> {
        if Stamp::capture(id, self.window, windows)?.as_ref() != Some(self) {
            return Ok(Progress::Superseded);
        }
        if waiting && !motion_host::session::with_session(id, |s| s.poll_pipeline_preparation())? {
            return Ok(Progress::Waiting);
        }
        Ok(Progress::Ready)
    }
}

pub(super) struct Queue {
    frames: VecDeque<Pending>,
    due: Instant,
}
impl Default for Queue {
    fn default() -> Self {
        Self {
            frames: VecDeque::new(),
            due: Instant::now(),
        }
    }
}
impl Queue {
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
    pub fn delay(&self) -> Option<Duration> {
        (!self.is_empty()).then(|| self.due.saturating_duration_since(Instant::now()))
    }
    pub fn take_due(&mut self) -> Option<Pending> {
        if Instant::now() < self.due {
            return None;
        }
        self.frames.pop_front()
    }
    pub fn put_back(&mut self, pending: Pending) {
        self.frames.push_front(pending);
        self.due = Instant::now() + POLL_INTERVAL;
    }
    pub fn insert(
        &mut self,
        command: Command,
        meta: Meta,
        waiting: bool,
        id: i64,
        windows: &std::collections::HashMap<u64, FloatingSurface>,
        paints: &Arc<Mutex<Vec<motion_ui::paint::PaintList>>>,
    ) -> bool {
        let Some(window) = window(&command) else {
            unreachable!("only frame commands can be deferred");
        };
        let stamp = Stamp::capture(id, window, windows);
        match stamp {
            Ok(Some(stamp)) if self.frames.len() < PENDING_LIMIT => {
                if self.is_empty() {
                    self.due = Instant::now();
                }
                let pending = Pending {
                    command,
                    meta,
                    stamp,
                    waiting,
                };
                if waiting {
                    self.put_back(pending);
                } else {
                    self.frames.push_back(pending);
                }
                true
            }
            Ok(None) => {
                finish(command, Ok(false), paints);
                false
            }
            Ok(Some(_)) => {
                finish(
                    command,
                    Err(format!(
                        "pending frame completion capacity exceeded: limit={PENDING_LIMIT}"
                    )
                    .into()),
                    paints,
                );
                false
            }
            Err(error) => {
                finish(command, Err(error), paints);
                false
            }
        }
    }
    pub fn finish_all(
        &mut self,
        error: Option<&Error>,
        paints: &Arc<Mutex<Vec<motion_ui::paint::PaintList>>>,
    ) {
        let previous = CURRENT_COMMAND.get();
        for pending in self.frames.drain(..) {
            CURRENT_COMMAND.set(Some(pending.meta.flight));
            if let Some(error) = error {
                if let Some(reply) = pending.command.panic_reply() {
                    reply.send(error.clone());
                }
                recycle(pending.command, paints);
            } else {
                finish(pending.command, Ok(false), paints);
            }
        }
        CURRENT_COMMAND.set(previous);
    }
    pub fn cancel_window(
        &mut self,
        window: u64,
        paints: &Arc<Mutex<Vec<motion_ui::paint::PaintList>>>,
    ) {
        // An owner key can be reused after CloseWindow. Cancel immediately,
        // before a same-sized replacement surface can match the old stamp.
        let previous = CURRENT_COMMAND.get();
        let mut retained = VecDeque::new();
        for pending in self.frames.drain(..) {
            if pending.stamp.window == window {
                CURRENT_COMMAND.set(Some(pending.meta.flight));
                finish(pending.command, Ok(false), paints);
            } else {
                retained.push_back(pending);
            }
        }
        self.frames = retained;
        CURRENT_COMMAND.set(previous);
    }
    pub fn status(&self) -> Value {
        serde_json::json!({"pending":self.frames.len(),"limit":PENDING_LIMIT,
            "requests":self.frames.iter().map(|p|serde_json::json!({"request":p.meta.flight.request,
                "operation":p.meta.flight.operation,"window":p.stamp.window,"waitingForPipeline":p.waiting,
                "elapsedMs":p.meta.flight.started.elapsed().as_millis()})).collect::<Vec<_>>()})
    }
}

pub(super) fn window(command: &Command) -> Option<u64> {
    match command {
        Command::Present { ui, .. } | Command::PresentFrozen { ui, .. }
            if ui.composition.is_some() =>
        {
            Some(ui.window)
        }
        Command::Render { .. } => Some(0),
        _ => None,
    }
}
pub(super) fn finish(
    command: Command,
    result: motion_host::Result<bool>,
    paints: &Arc<Mutex<Vec<motion_ui::paint::PaintList>>>,
) {
    match command {
        Command::Present { ui, reply } | Command::PresentFrozen { ui, reply } => {
            recycle_ui(ui, paints);
            respond(reply, result);
        }
        Command::Render { reply, .. } => respond(reply, result),
        _ => unreachable!("only frame commands have frame completions"),
    }
}
fn recycle(command: Command, paints: &Arc<Mutex<Vec<motion_ui::paint::PaintList>>>) {
    if let Command::Present { ui, .. } | Command::PresentFrozen { ui, .. } = command {
        recycle_ui(ui, paints);
    }
}
fn recycle_ui(ui: UiFrame, paints: &Arc<Mutex<Vec<motion_ui::paint::PaintList>>>) {
    let mut paint = ui.paint;
    paint.clear();
    recycle_paint_pool(paints, paint, true);
}

#[cfg(test)]
mod tests;
