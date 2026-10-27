use crate::{
    layer_clipboard::controller::{Domain, Intent, Status},
    Shell,
};
#[cfg(test)]
mod tests;

impl Shell {
    pub(crate) fn request_curve_clipboard(
        &mut self, intent: Intent, source: crate::layer_clipboard::KeyframeSource,
        selections: Vec<motion_core::KeyframeSelection>,
    ) -> Result<(), String> {
        if self.project_modal() || self.closing { return Ok(()); }
        if self.layer_clipboard.start_curves(&self.engine, intent, source, selections)? {
            if intent != Intent::Copy { self.stop(); }
            self.panels.message = Some(if self.workspace.locale == crate::i18n::Locale::Zh {
                "正在读取系统曲线剪贴板…"
            } else { "Preparing system easing clipboard…" }.into());
            self.dirty = true;
        }
        Ok(())
    }
    pub(crate) fn request_path_clipboard(
        &mut self, intent: Intent, source: crate::layer_clipboard::PathSource,
        selections: Vec<motion_core::PathSelection>, target: motion_core::PathDestination,
    ) -> Result<(), String> {
        if self.project_modal() || self.closing { return Ok(()); }
        if self.layer_clipboard.start_paths(&self.engine, intent, source, selections, Some(target))? {
            if intent != Intent::Copy { self.stop(); }
            self.panels.message = Some(if self.workspace.locale == crate::i18n::Locale::Zh {
                "正在准备路径剪贴板…"
            } else { "Preparing path clipboard…" }.into());
            self.dirty = true;
        }
        Ok(())
    }
    pub(crate) fn request_keyframe_clipboard(
        &mut self,
        intent: Intent,
        source: crate::layer_clipboard::KeyframeSource,
        selections: Vec<motion_core::KeyframeSelection>,
        targets: Vec<motion_core::KeyframeTarget>,
    ) -> Result<(), String> {
        if self.project_modal() || self.closing {
            return Ok(());
        }
        if self
            .layer_clipboard
            .start_keyframes(&self.engine, intent, source, selections, targets)?
        {
            if intent != Intent::Copy {
                self.stop();
            }
            self.panels.message = Some(
                if self.workspace.locale == crate::i18n::Locale::Zh {
                    "正在准备关键帧剪贴板…"
                } else {
                    "Preparing keyframe clipboard…"
                }
                .into(),
            );
            self.dirty = true;
        }
        Ok(())
    }

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
                    if completed.domain == Domain::Layers
                        && matches!(completed.intent, Intent::Paste { .. })
                    {
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
                            if completed.domain == Domain::Curves {
                                if zh { "已复制缓动曲线到系统剪贴板" } else { "Easing copied to system clipboard" }
                            } else if completed.domain == Domain::Paths {
                                if zh { "已复制路径" } else { "Paths copied" }
                            } else if completed.domain == Domain::Keyframes {
                                if zh {
                                    "已复制关键帧"
                                } else {
                                    "Keyframes copied"
                                }
                            } else if zh {
                                "已复制图层"
                            } else {
                                "Layers copied"
                            }
                        }
                        Status::Applied if completed.intent == Intent::Cut => {
                            if completed.domain == Domain::Paths {
                                self.panels.path_controls.clipboard_cut_completed();
                                if zh { "已剪切路径" } else { "Paths cut" }
                            } else if completed.domain == Domain::Keyframes {
                                if zh {
                                    "已剪切关键帧"
                                } else {
                                    "Keyframes cut"
                                }
                            } else if zh {
                                "已剪切图层"
                            } else {
                                "Layers cut"
                            }
                        }
                        Status::Applied => {
                            if completed.domain == Domain::Curves {
                                if zh { "已将缓动曲线粘贴到选定区间" } else { "Easing pasted to selected intervals" }
                            } else if completed.domain == Domain::Paths {
                                if zh { "已粘贴路径" } else { "Paths pasted" }
                            } else if completed.domain == Domain::Keyframes {
                                if zh {
                                    "已粘贴关键帧"
                                } else {
                                    "Keyframes pasted"
                                }
                            } else if zh {
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
