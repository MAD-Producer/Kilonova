//! One bounded text-layout transaction uses the observed serial engine worker.
use crate::*;
pub(crate) struct Task {
    ticket: panels::auxiliary::text_style::Ticket,
    receive: std::sync::mpsc::Receiver<Result<Value, String>>,
    began: Instant,
}
impl Shell {
    pub(crate) fn start_text_layout(
        &mut self,
        ticket: panels::auxiliary::text_style::Ticket,
        source: crate::layer_clipboard::KeyframeSource,
        request: Value,
    ) -> Result<(), String> {
        self.stop();
        let receive = self.engine.request(|reply| engine::Command::TextLayout {
            source,
            request,
            reply,
        });
        match receive {
            Ok(receive) => {
                self.text_style_task = Some(Task {
                    ticket,
                    receive,
                    began: Instant::now(),
                });
                Ok(())
            }
            Err(cause) => {
                self.panels.auxiliary.text_style.response(
                    &ticket,
                    &Err(cause.clone()),
                    self.workspace.locale,
                );
                Err(cause)
            }
        }
    }
    pub(crate) fn poll_text_layout(&mut self) {
        let Some(task) = &self.text_style_task else {
            return;
        };
        let Some(result) = self.engine.poll_reply(&task.receive, "text_layout") else {
            return;
        };
        let task = self.text_style_task.take().expect("polled text layout");
        log::debug!(
            "text layout done object={} elapsed_ms={}",
            task.ticket.object,
            task.began.elapsed().as_millis()
        );
        self.panels
            .auxiliary
            .text_style
            .response(&task.ticket, &result, self.workspace.locale);
        match result {
            Ok(state) => {
                let applied = state["textEdit"]["cancelled"] == false;
                self.accept_edit_state(state);
                if applied {
                    // A later successful transaction replaces the earlier UI
                    // result; its original Error record remains in both sinks.
                    self.panels.message = Some(
                        if self.workspace.locale == i18n::Locale::Zh {
                            "文字排版已应用"
                        } else {
                            "Text layout applied"
                        }
                        .into(),
                    );
                }
            }
            Err(cause) => {
                self.panels.message = Some(self.workspace.locale.error(&cause));
                self.stop();
            }
        }
        self.dirty = true;
        self.redraw();
    }
}
