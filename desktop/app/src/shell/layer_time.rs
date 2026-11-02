//! Freeze the opening document and native owner; worker guards the final batch.
use crate::*;
impl Shell {
    pub(super) fn reject_stale_layer_time(&mut self) {
        self.panels.message = Some(
            if self.workspace.locale == Locale::Zh {
                "工程已变化，图层时间设置未应用，请重新打开。"
            } else {
                "The document changed. Layer time settings were not applied; reopen the editor."
            }
            .into(),
        );
        self.dirty = true;
    }
    pub(crate) fn open_layer_time(
        &mut self,
        source: crate::layer_clipboard::KeyframeSource,
        objects: Vec<u64>,
    ) -> Result<(), String> {
        if source != crate::layer_clipboard::KeyframeSource::of(&self.state) {
            self.reject_stale_layer_time();
            return Ok(());
        }
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
        let first = objects.first().copied();
        log::info!("layer time start operation=open window={owner} revision={revision:?} generation={generation:?} object={first:?} layers={}",objects.len());
        match dialogs::Dialog::layer_time(&self.state, source, objects, owner) {
            Ok(dialog) => {
                self.dialog = Some(dialog);
                self.dirty = true;
                log::debug!(
                    "layer time done operation=open window={owner} elapsed_ms={}",
                    began.elapsed().as_millis()
                );
                Ok(())
            }
            Err(cause) => {
                let observed = crate::fallback_observation::error_cause(&cause);
                log::error!("layer time failed operation=open window={owner} revision={revision:?} generation={generation:?} object={first:?} phase=draft elapsed_ms={} cause={observed}",began.elapsed().as_millis());
                Err(cause)
            }
        }
    }
    pub(crate) fn owned_dialog_event(&mut self, event: dialogs::Event) -> Result<(), String> {
        match event {
            dialogs::Event::LayerTime { source, commands } => {
                if source != crate::layer_clipboard::KeyframeSource::of(&self.state) {
                    self.reject_stale_layer_time();
                } else if !commands.is_empty() {
                    self.actions
                        .push_back(panels::Action::EditorAction { source, commands });
                }
                self.dialog = None;
                self.dirty = true;
                Ok(())
            }
            other => self.timing_dialog_event(other),
        }
    }
}

#[cfg(all(test, target_os = "windows"))]
mod native_tests;

#[cfg(test)]
mod tests;
