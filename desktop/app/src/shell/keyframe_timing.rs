//! The native window that opens the editor retains its modal input ownership.
use crate::*;

impl Shell {
    pub(crate) fn open_keyframe_timing(
        &mut self,
        source: crate::layer_clipboard::KeyframeSource,
        selections: Vec<motion_core::KeyframeSelection>,
    ) -> Result<(), String> {
        self.stop();
        let owner = if self.window.as_ref().is_some_and(|w| w.has_focus()) {
            0
        } else {
            self.floating
                .values()
                .find(|f| f.window.has_focus())
                .map_or(0, |f| f.key)
        };
        let began = Instant::now();
        let revision = self.state["revision"].as_u64();
        let generation = self.state["document_generation"].as_u64();
        let object = selections.first().map(|s| s.target.object());
        log::info!(
            "keyframe timing start operation=open window={owner} revision={revision:?} generation={generation:?} object={object:?} channels={} keys={}",
            selections.len(),
            selections.iter().map(|s| s.frames.len()).sum::<usize>()
        );
        let dialog = dialogs::Dialog::keyframe_timing(&self.state, source, selections, owner);
        match dialog {
            Ok(dialog) => {
                log::debug!(
                    "keyframe timing done operation=open window={owner} revision={revision:?} elapsed_ms={}",
                    began.elapsed().as_millis()
                );
                self.dialog = Some(dialog);
                Ok(())
            }
            Err(cause) => {
                log::error!(
                    "keyframe timing failed operation=open window={owner} revision={revision:?} generation={generation:?} object={object:?} phase=profile elapsed_ms={} cause={cause}",
                    began.elapsed().as_millis()
                );
                Err(cause)
            }
        }
    }
    pub(crate) fn timing_dialog_event(&mut self, event: dialogs::Event) -> Result<(), String> {
        match event {
            dialogs::Event::Cancelled => self.dialog = None,
            dialogs::Event::KeyframeTiming {
                source,
                selections,
                settings,
            } => {
                if source != crate::layer_clipboard::KeyframeSource::of(&self.state) {
                    self.panels.message=Some(if self.workspace.locale==Locale::Zh {
                        "工程或选择已变化，关键帧设置未应用，请重新打开设置。"
                    }else{
                        "The document or selection changed. Keyframe settings were not applied; reopen the editor."
                    }.into());
                    self.dialog = None;
                    self.dirty = true;
                    return Ok(());
                }
                self.actions.push_back(panels::Action::EditorAction {
                    source,
                    commands: vec![motion_core::Command::KeyframeTiming {
                        selections,
                        settings,
                    }],
                });
                self.dialog = None;
            }
            _ => {
                let cause = "unexpected keyframe timing dialog event";
                log::error!(
                    "keyframe timing failed operation=apply phase=dialog_event cause={cause}"
                );
                return Err(cause.into());
            }
        }
        self.dirty = true;
        Ok(())
    }
}

#[cfg(all(test, target_os = "windows"))]
mod native_tests;
