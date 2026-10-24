use crate::{
    layer_clipboard::controller::{Intent, Status},
    Shell,
};
#[cfg(test)]
mod tests;

impl Shell {
    pub(crate) fn request_layer_clipboard(&mut self, intent: Intent) -> Result<(), String> {
        if self.project_modal() || self.closing {
            return Ok(());
        }
        let started = self.layer_clipboard.start(
            &self.engine,
            intent,
            self.panels.selection_ids(),
            self.layout_path.with_extension("clipboard"),
        )?;
        if started {
            if intent != Intent::Copy {
                self.stop();
            }
            self.panels.message = Some(
                if self.workspace.locale == crate::i18n::Locale::Zh {
                    "正在准备图层剪贴板…"
                } else {
                    "Preparing layer clipboard…"
                }
                .into(),
            );
            self.dirty = true;
        }
        Ok(())
    }

    pub(crate) fn poll_layer_clipboard(&mut self) {
        // Cancellation must reach a queued engine commit even while ordinary
        // action dispatch is waiting for that commit's acknowledgement.
        if self
            .actions
            .iter()
            .any(|a| matches!(a, crate::panels::Action::CancelLayerClipboard))
        {
            self.actions
                .retain(|a| !matches!(a, crate::panels::Action::CancelLayerClipboard));
            self.cancel_layer_clipboard();
        }
        let allow_commit = self.editing.is_none()
            && self.attaching.is_none()
            && self.resizing.is_none()
            && self.importing_image.is_none()
            && self.importing_media.is_none()
            && self.node_sources.is_empty()
            && self.node_installing.is_empty()
            && self.node_save_project.is_none()
            && self.project_pending.is_none()
            && !self.project_modal()
            && self.actions.is_empty()
            && !self.closing;
        let result = self.layer_clipboard.poll(&self.engine, allow_commit);
        let Some(result) = result else {
            return;
        };
        let zh = self.workspace.locale == crate::i18n::Locale::Zh;
        match result {
            Ok(completed) => {
                if let Some(state) = completed.state {
                    self.panels.normalize_selection(&state);
                    if matches!(completed.intent, Intent::Paste { .. }) {
                        let objects: Vec<_> = state["edit_result"]["objects"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|v| v.as_u64())
                            .collect();
                        self.panels.select_only(objects.first().copied());
                        self.panels.selected_layers = objects;
                    }
                    self.accept_edit_state(state);
                }
                self.panels.message = Some(
                    match completed.status {
                        Status::Copied => {
                            if zh {
                                "已复制图层"
                            } else {
                                "Layers copied"
                            }
                        }
                        Status::Applied if completed.intent == Intent::Cut => {
                            if zh {
                                "已剪切图层"
                            } else {
                                "Layers cut"
                            }
                        }
                        Status::Applied => {
                            if zh {
                                "已粘贴图层"
                            } else {
                                "Layers pasted"
                            }
                        }
                        Status::Cancelled => {
                            if zh {
                                "已取消剪贴板操作"
                            } else {
                                "Clipboard operation cancelled"
                            }
                        }
                        Status::Superseded => {
                            if zh {
                                "工程已改变，剪贴板操作未提交"
                            } else {
                                "Project changed; clipboard operation was not applied"
                            }
                        }
                    }
                    .into(),
                );
            }
            Err(error) => self.panels.message = Some(self.workspace.locale.error(&error)),
        }
        self.dirty = true;
    }

    pub(crate) fn cancel_layer_clipboard(&mut self) {
        self.layer_clipboard.cancel();
        if self.layer_clipboard.busy() {
            self.panels.message = Some(
                if self.workspace.locale == crate::i18n::Locale::Zh {
                    "正在取消剪贴板操作…"
                } else {
                    "Cancelling clipboard operation…"
                }
                .into(),
            );
            self.dirty = true;
        }
    }
}
