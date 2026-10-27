use crate::{
    animation_presets::{Completion, Intent},
    i18n::Locale,
    layer_clipboard::KeyframeSource,
    panels::Action,
    Shell,
};
use std::path::PathBuf;
impl Shell {
    fn preset_library(&self) -> PathBuf {
        self.layout_path.with_extension("presets")
    }
    pub(crate) fn animation_preset_action(&mut self, action: Action) -> Result<(), String> {
        let zh = self.workspace.locale == Locale::Zh;
        if matches!(action, Action::CancelAnimationPreset) {
            self.animation_presets.cancel();
            return Ok(());
        }
        if self.project_modal() || self.closing {
            return Ok(());
        }
        if matches!(action, Action::RefreshAnimationPresets) {
            return self.animation_presets.scan(self.preset_library());
        }
        if self.animation_presets.busy() {
            return Err(if zh {
                "动画预设任务尚未结束"
            } else {
                "An animation preset operation is still running"
            }
            .into());
        }
        let source = KeyframeSource::of(&self.state);
        let intent = match action {
            Action::SaveAnimationPreset { object, effects } => {
                let layer = self.state["project"]["layers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|l| l["id"].as_u64() == Some(object))
                    .ok_or("preset source layer missing")?;
                let properties = if effects.is_none() {
                    vec![
                        motion_core::Property::Anchor,
                        motion_core::Property::Position,
                        motion_core::Property::Scale,
                        motion_core::Property::Rotation,
                        motion_core::Property::Opacity,
                    ]
                } else {
                    vec![]
                };
                let effects = effects.unwrap_or_else(|| {
                    layer["effects"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|e| e["id"].as_u64())
                        .collect()
                });
                let destination = rfd::FileDialog::new()
                    .set_title(if zh {
                        "保存动画预设"
                    } else {
                        "Save animation preset"
                    })
                    .set_directory(self.preset_library())
                    .add_filter("Motion Studio animation preset", &["msanim"])
                    .set_file_name(if zh {
                        "动画预设.msanim"
                    } else {
                        "Animation.msanim"
                    })
                    .save_file();
                let Some(destination) = destination else {
                    return Ok(());
                };
                Intent::Save {
                    object,
                    properties,
                    effects,
                    destination,
                }
            }
            Action::ApplyAnimationPreset { path, objects } => Intent::Apply {
                source: path,
                objects,
            },
            Action::OpenAnimationPreset => {
                let source = rfd::FileDialog::new()
                    .set_title(if zh {
                        "从文件应用动画预设"
                    } else {
                        "Apply animation preset from file"
                    })
                    .set_directory(self.preset_library())
                    .add_filter("Motion Studio animation preset", &["msanim"])
                    .pick_file();
                let Some(source) = source else {
                    return Ok(());
                };
                Intent::Apply {
                    source,
                    objects: self.panels.selection_ids(),
                }
            }
            _ => return Err("unexpected animation preset action".into()),
        };
        self.stop();
        self.animation_presets.start(&self.engine, intent, source)?;
        self.panels.message = Some(
            if zh {
                "正在准备动画预设…"
            } else {
                "Preparing animation preset…"
            }
            .into(),
        );
        self.dirty = true;
        Ok(())
    }
    pub(crate) fn poll_animation_presets(&mut self) {
        if !self.animation_presets.initialized && !self.animation_presets.busy() {
            if let Err(error) = self.animation_presets.scan(self.preset_library()) {
                log::error!("animation preset initialization failed reason={error}");
                self.panels.message = Some(self.workspace.locale.error(&error));
                // A failed initialization is visible; explicit Refresh can retry.
                self.animation_presets.initialized = true;
            }
        }
        if self
            .actions
            .iter()
            .any(|a| matches!(a, Action::CancelAnimationPreset))
        {
            self.actions
                .retain(|a| !matches!(a, Action::CancelAnimationPreset));
            self.animation_presets.cancel();
        }
        let allow_commit = self.editing.is_none()
            && self.attaching.is_none()
            && self.resizing.is_none()
            && self.project_pending.is_none()
            && self.importing_image.is_none()
            && self.importing_media.is_none()
            && self.node_sources.is_empty()
            && self.node_installing.is_empty()
            && self.node_save_project.is_none()
            && !self.layer_clipboard.busy()
            && self.actions.is_empty()
            && !self.project_modal()
            && !self.closing;
        let result = self.animation_presets.poll(&self.engine, allow_commit);
        self.panels.browser.presets_busy = self.animation_presets.phase().is_some();
        self.panels.effect_controls.presets_busy = self.animation_presets.busy();
        self.panels.properties.effects.presets_busy = self.animation_presets.busy();
        let Some(result) = result else {
            return;
        };
        let zh = self.workspace.locale == Locale::Zh;
        match result {
            Ok(Completion::Catalog { entries, failures }) => {
                self.panels.browser.presets = entries;
                if failures > 0 {
                    self.panels.message = Some(if zh {
                        format!("{failures} 个预设无法读取，详情已写入日志")
                    } else {
                        format!("Could not read {failures} presets; details are in the log")
                    });
                }
            }
            Ok(Completion::Applied(state)) => {
                self.panels.normalize_selection(&state);
                self.accept_edit_state(state);
                self.panels.message = Some(
                    if zh {
                        "动画预设已应用，可撤销"
                    } else {
                        "Animation preset applied; undo available"
                    }
                    .into(),
                );
            }
            Ok(Completion::Saved(path)) => {
                self.panels.message = Some(if zh {
                    format!("动画预设已保存：{}", path.display())
                } else {
                    format!("Animation preset saved: {}", path.display())
                });
                if let Err(error) = self.animation_presets.scan(self.preset_library()) {
                    log::error!("animation preset catalog refresh failed reason={error}");
                    self.panels.message = Some(self.workspace.locale.error(&error));
                }
            }
            Ok(Completion::Cancelled) => {
                self.panels.message = Some(
                    if zh {
                        "已取消动画预设操作"
                    } else {
                        "Animation preset operation cancelled"
                    }
                    .into(),
                )
            }
            Ok(Completion::Superseded) => {
                self.panels.message = Some(
                    if zh {
                        "工程已变化，未应用预设，请重试"
                    } else {
                        "The document changed; preset was not applied"
                    }
                    .into(),
                )
            }
            Err(error) => self.panels.message = Some(self.workspace.locale.error(&error)),
        }
        self.dirty = true;
    }
}
