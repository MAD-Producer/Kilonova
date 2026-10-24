use crate::*;

impl Shell {
    pub(crate) fn begin_close(&mut self) {
        if self.engine.failed() {
            self.finish_close();
            return;
        }
        if let Err(error) = self.request_project_change(project_lifecycle::Target::Close) {
            self.panels.message = Some(self.workspace.locale.error(&error));
            self.dirty = true;
        }
    }
    pub(crate) fn finish_close(&mut self) {
        self.cancel_layer_clipboard();
        self.cancel_audio_analysis();
        self.node_tasks.cancel(&mut self.panels.node_editor);
        self.node_compile_due = None;
        self.node_installing.clear();
        if let Err(error) = self.engine.node_expect((u64::MAX, u64::MAX)) { log::error!("node close invalidation failed operation=node_expect reason={error}"); }
        let cancellation = panels::cancel_interaction(&mut self.panels);
        if self.engine.failed() {
            self.actions.clear();
            self.resize_queue.clear();
            // Queued jobs lose their senders when the worker exits. They can
            // never acknowledge another frame and must not hold exit open.
            self.rendering.clear();
            self.exposure_presentations.clear();
            self.editing = None;
            self.resizing = None;
            self.attaching = None;
            self.project_pending = None;
            self.present_due = false;
        } else {
            self.actions.extend(cancellation);
        }
        self.stop();
        self.closing = true;
    }
    pub(crate) fn request_close(&mut self) {
        if self.engine.failed() && !self.closing {
            self.request_failed_close();
            return;
        }
        if self.node_close_prompt.is_some()
            || self.node_close_save.is_some()
            || self.project_modal()
            || self.closing
        {
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
        if snapshot.layer_target.is_some() {
            if let Err(error) = self.flush_bound_source() {
                self.panels.node_editor.report_error(error);
                self.dirty = true;
                return;
            }
            if self
                .node_standalone
                .as_ref()
                .is_some_and(|e| e.dirty() && e.snapshot().revision > 0)
            {
                let mut restored = self.node_standalone.take().unwrap();
                restored.take_clipboard_from(&mut self.panels.node_editor);
                self.panels.node_editor = restored;
                self.panels.node_editor.view_preferences = self.workspace.node_view;
                self.panels.node_editor.recent_nodes = self.workspace.node_recents.clone();
                self.request_close();
                return;
            }
            self.begin_close();
            return;
        }
        if self.smoke.is_none() && self.panels.node_editor.dirty() && snapshot.revision > 0 {
            self.node_tasks.cancel(&mut self.panels.node_editor);
            self.node_compile_due = None;
            self.node_installing.clear();
            if let Err(error) = self.engine.node_expect((u64::MAX, u64::MAX)) { log::error!("node close invalidation failed operation=node_expect reason={error}"); }
            self.node_close_prompt = Some(snapshot);
            self.dirty = true;
        } else {
            self.begin_close();
        }
    }

    // A stopped renderer cannot paint the usual save modal. Node source data
    // lives on the UI thread and can still be saved through a native dialog.
    fn request_failed_close(&mut self) {
        use node_tasks::CloseChoice;
        let pending = self.panels.node_editor.dirty() || self.panels.node_editor.editing();
        let choice = if pending {
            let zh = self.workspace.locale == Locale::Zh;
            match rfd::MessageDialog::new()
                .set_title(if zh { "Motion Studio — 引擎已停止" } else { "Motion Studio — Engine stopped" })
                .set_description(if zh {
                    "是否保存节点图后退出？是：保存；否：放弃节点图改动；取消：留在窗口。工程恢复副本的位置见错误信息。"
                } else {
                    "Save the node graph before exiting? Yes: save; No: discard node changes; Cancel: stay. The engine error reports the project recovery status."
                })
                .set_buttons(rfd::MessageButtons::YesNoCancel)
                .show()
            {
                rfd::MessageDialogResult::Yes => CloseChoice::Save,
                rfd::MessageDialogResult::No => CloseChoice::Discard,
                _ => CloseChoice::Cancel,
            }
        } else {
            CloseChoice::Discard
        };
        let path = if choice == CloseChoice::Save {
            self.panels.node_editor.snapshot().source_path.or_else(|| {
                rfd::FileDialog::new()
                    .add_filter("Motion Studio graph", &["msgraph"])
                    .save_file()
            })
        } else {
            None
        };
        if let Err(error) = self.close_failed_session(choice, path) {
            self.panels.node_editor.report_error(error.clone());
            rfd::MessageDialog::new()
                .set_title("Motion Studio")
                .set_description(&error)
                .set_level(rfd::MessageLevel::Error)
                .show();
        }
    }

    pub(crate) fn close_failed_session(
        &mut self,
        choice: node_tasks::CloseChoice,
        path: Option<PathBuf>,
    ) -> Result<(), String> {
        use node_tasks::CloseChoice;
        if !self.engine.failed() || choice == CloseChoice::Cancel {
            return Ok(());
        }
        if choice == CloseChoice::Save {
            let Some(path) = path else { return Ok(()) };
            let node_editor::Action::Save(snapshot) = self
                .panels
                .node_editor
                .request(node_editor::Request::Save)
                .map_err(|error| error.to_string())?
            else {
                unreachable!()
            };
            snapshot.save(&path).map_err(|error| error.to_string())?;
            self.panels.node_editor.accept_saved(&snapshot, path);
        }
        self.node_close_prompt = None;
        self.project_prompt = None;
        self.finish_close();
        Ok(())
    }
}
