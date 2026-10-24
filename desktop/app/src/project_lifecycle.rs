//! Project replacement is revision checked on the engine worker. The prompt
//! retains its target until the worker acknowledges a save and replacement.
use crate::{engine::Engine, node_tasks::CloseChoice, panels::Action, Shell};
use serde_json::Value;
use std::{path::PathBuf, sync::mpsc::Receiver};

pub(crate) mod error_notice;

#[derive(Clone, Debug)]
pub(crate) enum Target {
    New(String),
    Open(PathBuf),
    Import(PathBuf),
    Close,
}

#[derive(Clone, Debug)]
pub(crate) struct Ticket {
    pub root: PathBuf,
    pub revision: u64,
    pub dirty: bool,
    pub name: String,
}

pub(crate) struct Prompt {
    pub target: Target,
    pub ticket: Ticket,
    pub error: Option<String>,
    pub error_scroll: motion_ui::widgets::ScrollArea,
}

pub(crate) type Pending = (Prompt, Receiver<Result<Value, String>>);

impl Shell {
    pub(crate) fn project_modal(&self) -> bool {
        self.project_prompt.is_some() || self.project_pending.is_some()
    }

    pub(crate) fn request_project_change(&mut self, target: Target) -> Result<(), String> {
        if self.project_modal() || self.closing {
            return Ok(());
        }
        self.cancel_layer_clipboard();
        self.flush_bound_source()?;
        self.node_tasks.cancel(&mut self.panels.node_editor);self.node_compile_due=None;self.engine.node_expect((u64::MAX,u64::MAX))?;
        self.stop();
        let ticket = self.engine.project_ticket()?;
        let prompt = Prompt {
            target,
            ticket,
            error: None,
            error_scroll: Default::default(),
        };
        self.dirty = true;
        if prompt.ticket.dirty && self.smoke.is_none() {
            self.project_prompt = Some(prompt);
        } else {
            self.submit_project_change(prompt, CloseChoice::Discard)?;
        }
        Ok(())
    }

    fn submit_project_change(&mut self, prompt: Prompt, choice: CloseChoice) -> Result<(), String> {
        let result = self.engine.change_project(
            prompt.target.clone(),
            prompt.ticket.clone(),
            choice == CloseChoice::Save,
        );
        match result {
            Ok(receiver) => self.project_pending = Some((prompt, receiver)),
            Err(error) => {
                let mut prompt = prompt;
                prompt.error = Some(self.workspace.locale.error(&error));
                self.project_prompt = Some(prompt);
            }
        }
        Ok(())
    }

    pub(crate) fn choose_project_change(&mut self, choice: CloseChoice) -> Result<(), String> {
        let Some(prompt) = self.project_prompt.take() else {
            return Ok(());
        };
        self.dirty = true;
        // A clean project's failed transition only needs acknowledgement. It
        // must not expose Save/Discard or resubmit a failed open implicitly.
        if choice == CloseChoice::Cancel || (!prompt.ticket.dirty && prompt.error.is_some()) {
            return Ok(());
        }
        self.submit_project_change(prompt, choice)
    }

    pub(crate) fn poll_project_change(&mut self) {
        let Some((_, receiver)) = &self.project_pending else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err("engine thread has stopped".into())
            }
        };
        let (mut prompt, _) = self.project_pending.take().unwrap();
        match result {
            Ok(state) => {
                self.state = state;
                if !matches!(prompt.target, Target::Close) {
                    self.home = None;
                    if let Err(error) = self.remember_project() { log::error!("project change remember failed operation=remember_project changed=true reason={error}"); }
                }
                if matches!(prompt.target, Target::Close) {
                    self.finish_close();
                } else {
                    // The old model/gesture has been replaced. Clear UI handles
                    // without sending its cancellation into the new project.
                    let _ = crate::panels::cancel_interaction(&mut self.panels);
                    self.workspace.cancel();
                    self.actions.clear();
                    self.frame = self.state["frame"].as_f64().unwrap_or_else(|| {
                        log::warn!("project change frame fallback root={:?} revision={} reason=missing_or_non_numeric_frame fallback=0", self.state["root"].as_str(), self.state["revision"]);
                        0.
                    });
                    self.panels.select_only(None);
                    self.present_due = true;
                }
            }
            Err(error) => {
                let error = self.workspace.locale.error(&error);
                self.panels.message = Some(error.clone());
                if prompt.ticket.dirty {
                    // A concurrent edit invalidates the choice. Show the fresh
                    // state and require another decision, never auto-discard it.
                    match self.engine.project_ticket() {
                        Ok(ticket) => prompt.ticket = ticket,
                        Err(reason) => log::error!("project prompt ticket refresh failed root={} ticket_revision={} retained_stale_ticket=true operation_error={error:?} reason={reason}", prompt.ticket.root.display(), prompt.ticket.revision),
                    }
                }
                prompt.error = Some(error);
                self.project_prompt = Some(prompt);
            }
        }
        self.dirty = true;
        self.redraw();
    }
}

impl Engine {
    pub(crate) fn project_ticket(&self) -> Result<Ticket, String> {
        self.call(crate::engine::Command::ProjectTicket)
    }
    pub(crate) fn change_project(
        &self,
        target: Target,
        ticket: Ticket,
        save: bool,
    ) -> Result<Receiver<Result<Value, String>>, String> {
        self.request(|reply| crate::engine::Command::ChangeProject {
            target,
            ticket,
            save,
            reply,
        })
    }
}

pub(crate) fn render(shell: &mut Shell, paint: &mut motion_ui::PaintList, size: [f32; 2]) {
    let Some(prompt) = &mut shell.project_prompt else {
        return;
    };
    if !prompt.ticket.dirty {
        if let Some(error) = &prompt.error {
            if error_notice::render(
                paint, &mut shell.atlas, size, &shell.input, shell.workspace.locale,
                &prompt.target, error, &mut prompt.error_scroll,
            ) {
                shell.actions.push_back(Action::ProjectChoice(CloseChoice::Cancel));
            }
        }
        return;
    }
    let zh = shell.workspace.locale == crate::i18n::Locale::Zh;
    let title = if zh {
        "工程有未保存的修改"
    } else {
        "Unsaved project changes"
    };
    let message = if zh {
        format!("保存“{}”的修改后再继续？", prompt.ticket.name)
    } else {
        format!(
            "Save changes to \"{}\" before continuing?",
            prompt.ticket.name
        )
    };
    let note = prompt.error.as_deref().unwrap_or(if zh {
        "放弃只丢弃未保存的编辑；取消会留在当前工程。"
    } else {
        "Discard leaves the saved project intact; Cancel keeps editing."
    });
    if let Some(choice) = crate::node_tasks::decision_prompt(
        paint,
        &mut shell.atlas,
        size,
        &shell.input,
        shell.workspace.locale,
        title,
        &message,
        note,
        prompt.error.is_some(),
    ) {
        shell.actions.push_back(Action::ProjectChoice(choice));
    }
}

#[cfg(test)]
mod tests;
