//! Bounded, nonblocking expression checks and transactions. A closed editor
//! does not cancel an already accepted transaction or hide its true result.
use crate::*;
pub(crate) struct Task {
    ticket: panels::expressions::Ticket,
    apply: bool,
    began: Instant,
    receive: std::sync::mpsc::Receiver<Result<Value, String>>,
}
impl Shell {
    pub(crate) fn start_expression_task(
        &mut self,
        ticket: panels::expressions::Ticket,
        apply: bool,
        receive: Result<std::sync::mpsc::Receiver<Result<Value, String>>, String>,
    ) -> Result<(), String> {
        match receive {
            Ok(receive) => {
                self.expression_task = Some(Task {
                    ticket,
                    apply,
                    began: Instant::now(),
                    receive,
                });
                Ok(())
            }
            Err(cause) => {
                self.panels.expressions.expression_response(
                    &ticket,
                    &Err(cause.clone()),
                    self.workspace.locale,
                );
                Err(cause)
            }
        }
    }
    pub(crate) fn poll_expression_task(&mut self) {
        let Some(task) = &self.expression_task else {
            return;
        };
        let operation = if task.apply {
            "expression_apply"
        } else {
            "expression_check"
        };
        let Some(result) = self.engine.poll_reply(&task.receive, operation) else {
            return;
        };
        let task = self.expression_task.take().expect("polled expression task");
        log::debug!(
            "expression editor done operation={operation} elapsed_ms={}",
            task.began.elapsed().as_millis()
        );
        self.panels
            .expressions
            .expression_response(&task.ticket, &result, self.workspace.locale);
        if task.apply {
            match result {
                Ok(state) => self.accept_edit_state(state),
                Err(cause) => {
                    self.panels.message = Some(self.workspace.locale.error(&cause));
                    self.stop();
                }
            }
        }
        self.dirty = true;
        self.redraw();
    }
}
