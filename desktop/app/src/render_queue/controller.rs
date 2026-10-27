//! Window callbacks only submit work and poll published state. Retiring a
//! project's queue never joins its worker until the handle has finished.
use super::{
    model::Template,
    worker::{Queue, Ticket, View},
};
use crate::{engine::Engine, layer_clipboard::KeyframeSource};
use motion_desktop_media::movie::{ExportControl, FrozenMovie};
use serde_json::Value;
use std::{path::PathBuf, sync::mpsc::Receiver};

#[derive(Clone, Default)]
pub(crate) struct Presentation {
    pub requested: bool,
    pub view: Option<View>,
    pub pending: bool,
    pub retiring: bool,
    pub error: Option<String>,
    pub last_added: Option<u64>,
    pub last_template: Option<(u64, u64)>,
}
#[derive(Debug)]
pub(crate) enum Intent {
    Reload,
    Add(Template),
    Start,
    Pause,
    Cancel(u64),
    CancelPreparation,
    Retry(u64),
    RetryAs(u64),
    Remove(u64),
    SaveTemplate(Template),
    RemoveTemplate(u64),
}
struct Freeze {
    receive: Receiver<Result<FrozenMovie, String>>,
    template: Template,
    output: PathBuf,
    control: ExportControl,
}
struct Pending {
    ticket: Ticket,
    adding: bool,
}
#[derive(Default)]
pub(crate) struct Controller {
    queue: Option<Queue>,
    root: Option<PathBuf>,
    generation: Option<u64>,
    requested: bool,
    attempted: bool,
    retiring: bool,
    closing: bool,
    pending: Vec<Pending>,
    freeze: Option<Freeze>,
    preparing: Option<ExportControl>,
    error: Option<String>,
    last_added: Option<u64>,
    last_template: Option<(u64, u64)>,
}
impl Controller {
    pub fn presentation(&self) -> Presentation {
        Presentation {
            requested: self.requested,
            view: if self.retiring {
                None
            } else {
                self.queue.as_ref().map(Queue::view)
            },
            pending: self.freeze.is_some() || !self.pending.is_empty(),
            retiring: self.retiring,
            error: self.error.clone(),
            last_added: self.last_added,
            last_template: self.last_template,
        }
    }
    pub fn busy(&self) -> bool {
        self.retiring
            || self.freeze.is_some()
            || !self.pending.is_empty()
            || self.queue.as_ref().is_some_and(|queue| {
                let view = queue.view();
                !view.stopped && (!view.ready || view.running || view.flight.is_some())
            })
    }
    pub fn preparing(&self) -> bool {
        self.freeze.is_some() || self.preparing.is_some()
    }
    pub fn open(&mut self) {
        self.requested = true;
    }
    pub fn cancel_preparation(&self) {
        if let Some(freeze) = &self.freeze {
            freeze.control.cancel();
        }
        if let Some(control) = &self.preparing {
            control.cancel();
        }
    }
    pub fn shutdown(&mut self) {
        self.closing = true;
        self.retire();
    }
    fn retire(&mut self) {
        self.cancel_preparation();
        self.freeze = None;
        if let Some(queue) = &self.queue {
            queue.request_shutdown();
            self.retiring = true;
        }
    }
    pub fn reload(&mut self) {
        self.requested = true;
        self.attempted = false;
        self.retire();
    }
    pub fn queue(&self) -> Result<&Queue, String> {
        self.queue
            .as_ref()
            .filter(|queue| {
                let view = queue.view();
                view.ready && !view.stopped && !self.retiring && !self.closing
            })
            .ok_or_else(|| "render queue is not ready".into())
    }
    pub fn accept(&mut self, ticket: Ticket, adding: bool) -> Result<(), String> {
        // Callers check capacity before dispatching. Accepted replies must
        // always remain owned until completion or explicit project retirement.
        if self.pending.len() >= 16 {
            return Err("render queue UI command capacity reached".into());
        }
        self.pending.push(Pending { ticket, adding });
        Ok(())
    }
    pub fn has_capacity(&self) -> bool {
        self.pending.len() + usize::from(self.freeze.is_some()) < 16
    }
    pub fn freeze(
        &mut self,
        engine: &Engine,
        state: &Value,
        template: Template,
        output: PathBuf,
    ) -> Result<(), String> {
        self.queue()?;
        if self.preparing() {
            return Err("render queue is already preparing a task".into());
        }
        if !self.has_capacity() {
            return Err("render queue UI command capacity reached".into());
        }
        template.validate()?;
        let control = ExportControl::default();
        let receive = engine.freeze_movie_for_queue(KeyframeSource::of(state), control.clone())?;
        self.freeze = Some(Freeze {
            receive,
            template,
            output,
            control,
        });
        Ok(())
    }
    pub fn poll(&mut self, engine: &Engine, state: &Value) -> bool {
        let mut changed = false;
        let root = state["root"].as_str().map(PathBuf::from);
        let generation = state["document_generation"].as_u64();
        if self.root != root {
            self.retire();
            self.root = root;
            self.attempted = false;
            self.last_added = None;
            self.last_template = None;
            self.error = None;
            changed = true;
        } else if self.generation != generation {
            self.cancel_preparation();
            self.freeze = None;
            changed = true;
        }
        self.generation = generation;
        if self.retiring {
            if self
                .queue
                .as_ref()
                .is_some_and(|queue| !queue.worker_finished())
            {
                return true;
            }
            // The worker has ended; joining here cannot wait for IO/encoding.
            self.queue = None;
            self.pending.clear();
            self.preparing = None;
            self.retiring = false;
            changed = true;
        }
        if self.closing {
            return changed;
        }
        if self.requested && !self.attempted && self.queue.is_none() {
            self.attempted = true;
            match self
                .root
                .clone()
                .ok_or_else(|| "render queue project root is missing".into())
                .and_then(Queue::open)
            {
                Ok(queue) => self.queue = Some(queue),
                Err(cause) => {
                    log::error!(
                        "render queue open dispatch failed cause={}",
                        crate::fallback_observation::error_cause(&cause)
                    );
                    self.error = Some(cause);
                }
            }
            changed = true;
        }
        if let Some(freeze) = &self.freeze {
            if let Some(result) = engine.poll_reply(&freeze.receive, "freeze_movie_for_queue") {
                let freeze = self.freeze.take().expect("polled queue freeze");
                let result = result.and_then(|snapshot| {
                    freeze.control.check()?;
                    let ticket = self.queue()?.add(
                        snapshot,
                        freeze.template,
                        freeze.output,
                        freeze.control.clone(),
                    )?;
                    self.accept(ticket, true)?;
                    self.preparing = Some(freeze.control);
                    Ok(())
                });
                if let Err(cause) = result {
                    if cause != "movie export cancelled" {
                        log::error!(
                            "render queue add dispatch failed phase=freeze_reply cause={}",
                            crate::fallback_observation::error_cause(&cause)
                        );
                        self.error = Some(cause);
                    }
                }
                changed = true;
            }
        }
        let mut index = 0;
        while index < self.pending.len() {
            let result = match self.pending[index].ticket.poll() {
                Ok(None) => {
                    index += 1;
                    continue;
                }
                Ok(Some(value)) => Ok(value),
                Err(cause) => Err(cause),
            };
            let pending = self.pending.remove(index);
            if pending.adding {
                self.preparing = None;
            }
            match result {
                Ok(id) if pending.adding => self.last_added = id,
                Ok(Some(id)) if pending.ticket.operation == "save_template" => {
                    self.last_template = Some((pending.ticket.request, id));
                }
                Ok(_) => {}
                Err(cause) if cause == "movie export cancelled" => {}
                Err(cause) => {
                    if cause.contains("lost its worker reply") {
                        log::error!(
                            "render queue reply failed request={} cause={}",
                            pending.ticket.request,
                            crate::fallback_observation::error_cause(&cause)
                        );
                    }
                    self.error = Some(cause);
                }
            }
            changed = true;
        }
        changed || self.busy()
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        self.cancel_preparation();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    fn until(
        controller: &mut Controller,
        engine: &Engine,
        state: &Value,
        condition: impl Fn(&Presentation) -> bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            controller.poll(engine, state);
            if condition(&controller.presentation()) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "controller did not complete its asynchronous transition"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    #[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
    fn queue_controller_opens_on_demand_retires_before_switching_and_keeps_saved_templates() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first");
        let second = root.path().join("second");
        let a = Engine::start(first.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
        let b = Engine::start(second.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
        let state_a = a.state().unwrap();
        let state_b = b.state().unwrap();
        let mut controller = Controller::default();
        controller.poll(&a, &state_a);
        assert!(!first.join(".motion-render-queue").exists());
        assert!(!controller.busy());
        controller.open();
        until(&mut controller, &a, &state_a, |p| {
            p.view.as_ref().is_some_and(|v| v.ready)
        });
        let mut template = controller.queue().unwrap().view().document.templates[0].clone();
        template.name = "Saved template".into();
        let ticket = controller
            .queue()
            .unwrap()
            .save_template(template.clone())
            .unwrap();
        let saved_request = ticket.request;
        controller.accept(ticket, false).unwrap();
        until(&mut controller, &a, &state_a, |p| !p.pending);
        assert_eq!(
            controller.presentation().last_template,
            Some((saved_request, template.id))
        );
        assert_eq!(
            controller.queue().unwrap().view().document.templates[0],
            template
        );
        let mut invalid = template.clone();
        invalid.pattern = "../outside.mp4".into();
        let ticket = controller.queue().unwrap().save_template(invalid).unwrap();
        controller.accept(ticket, false).unwrap();
        until(&mut controller, &a, &state_a, |p| !p.pending);
        let cause = controller.presentation().error.unwrap();
        assert_eq!(
            controller.presentation().last_template,
            Some((saved_request, template.id)),
            "a failed write must not acknowledge a successful template save"
        );
        assert_eq!(
            controller.queue().unwrap().view().document.templates[0],
            template
        );
        let ticket = controller
            .queue()
            .unwrap()
            .save_template(template.clone())
            .unwrap();
        let recovered_request = ticket.request;
        controller.accept(ticket, false).unwrap();
        until(&mut controller, &a, &state_a, |p| !p.pending);
        assert_eq!(
            controller.presentation().last_template,
            Some((recovered_request, template.id))
        );
        assert_eq!(
            controller.presentation().error.as_deref(),
            Some(cause.as_str())
        );
        assert_eq!(
            controller.queue().unwrap().view().document.templates[0],
            template
        );
        controller.poll(&b, &state_b);
        until(&mut controller, &b, &state_b, |p| {
            !p.retiring && p.view.as_ref().is_some_and(|v| v.ready)
        });
        assert!(controller.presentation().last_template.is_none());
        assert_ne!(
            controller.queue().unwrap().view().document.templates[0],
            template
        );
        controller.poll(&a, &state_a);
        until(&mut controller, &a, &state_a, |p| {
            !p.retiring && p.view.as_ref().is_some_and(|v| v.ready)
        });
        assert_eq!(
            controller.queue().unwrap().view().document.templates[0],
            template
        );
        controller.shutdown();
        until(&mut controller, &a, &state_a, |p| !p.retiring);
        assert!(!controller.busy());
        assert!(controller.queue().is_err());
    }
}
