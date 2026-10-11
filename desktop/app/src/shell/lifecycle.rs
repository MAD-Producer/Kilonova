use crate::*;

impl Shell {
    pub(crate) fn begin_close(&mut self) {
        self.node_tasks.cancel(&mut self.panels.node_editor);
        self.node_compile_due = None;
        self.node_installing.clear();
        let _ = self.engine.node_expect((u64::MAX, u64::MAX));
        self.actions
            .extend(panels::cancel_interaction(&mut self.panels));
        self.stop();
        self.closing = true;
    }
    pub(crate) fn request_close(&mut self) {
        if self.node_close_prompt.is_some() || self.node_close_save.is_some() || self.closing {
            return;
        }
        let snapshot = match self.panels.node_editor.request(node_editor::Request::Save) {
            Ok(node_editor::Action::Save(snapshot)) => snapshot,
            Err(error) => {
                self.panels.node_editor.report_error(error.to_string());
                self.dirty = true;
                return;
            }
            _ => unreachable!(),
        };
        if self.smoke.is_none() && self.panels.node_editor.dirty() && snapshot.revision > 0 {
            self.node_tasks.cancel(&mut self.panels.node_editor);
            self.node_compile_due = None;
            self.node_installing.clear();
            let _ = self.engine.node_expect((u64::MAX, u64::MAX));
            self.node_close_prompt = Some(snapshot);
            self.dirty = true;
        } else {
            self.begin_close();
        }
    }
}
