//! Application-only settings, committed after validation and successful persistence.
use crate::workspace::preferences::{Preferences, PreviewQuality};
use crate::*;

#[cfg(test)]
mod tests;

impl Shell {
    pub(crate) fn restore_preview_preference(&mut self) -> Result<(), String> {
        if self.workspace.preview_quality != PreviewQuality::Auto {
            self.state["preview"] = self
                .engine
                .preview_mode(self.workspace.preview_quality.id(), 0)?;
        }
        Ok(())
    }
    pub(crate) fn open_preferences(&mut self) {
        self.stop();
        self.workspace.cancel();
        self.actions
            .extend(panels::cancel_interaction(&mut self.panels));
        if let Some(editor) = &mut self.node_standalone {
            editor.cancel_interaction();
        }
        self.dialog = Some(dialogs::Dialog::preferences(&self.workspace));
        if let Some(window) = &self.window {
            window.focus_window();
        }
        self.dirty = true;
        self.redraw();
    }

    pub(crate) fn apply_preferences(
        &mut self,
        values: Preferences,
        refresh_style: bool,
    ) -> Result<(), String> {
        let began = Instant::now();
        log::info!(
            "preferences start operation=apply path={}",
            crate::fallback_observation::short(self.layout_path.display())
        );
        crate::logging::flush();
        let result = self.commit_preferences(&values, refresh_style);
        match &result {
            Ok(()) => log::debug!(
                "preferences done operation=apply elapsed_ms={}",
                began.elapsed().as_millis()
            ),
            Err(error) => log::error!(
                "preferences failed operation=apply elapsed_ms={} path={} cause={}",
                began.elapsed().as_millis(),
                crate::fallback_observation::short(self.layout_path.display()),
                crate::fallback_observation::short(error)
            ),
        }
        result
    }

    fn commit_preferences(
        &mut self,
        values: &Preferences,
        refresh_style: bool,
    ) -> Result<(), String> {
        // Prepare fallible fonts/styles before changing the engine or the saved settings.
        let prepared = if refresh_style {
            let base = motion_ui::style::default_stylesheet();
            let sheet = if let Some(path) = &values.style_path {
                base.load_overrides(path)
                    .map_err(|error| format!("preferences phase=style cause={error}"))?
            } else {
                base.clone()
            };
            let mut atlas = TextAtlas::from_family(
                sheet.font_family(),
                sheet.base_font_size(),
                values.locale == Locale::Zh,
            )
            .map_err(|error| format!("preferences phase=font cause={error}"))?;
            atlas.set_pixel_ratio(self.scale);
            Some((sheet, atlas))
        } else {
            None
        };
        let before = self.workspace.preview_quality;
        let preview = if before != values.preview {
            Some(self.engine.preview_mode(values.preview.id(), 0)?)
        } else {
            None
        };
        if let Err(primary) = self.workspace.save_preferences(&self.layout_path, values) {
            if preview.is_some() {
                match self.engine.preview_mode(before.id(), 0) {
                    Ok(previous) => self.state["preview"] = previous,
                    Err(rollback) => log::error!(
                        "preferences rollback failed phase=preview primary={} cause={}",
                        crate::fallback_observation::short(&primary),
                        crate::fallback_observation::short(rollback)
                    ),
                }
            }
            return Err(primary);
        }
        self.workspace.set_preferences(values);
        if let Some(preview) = preview {
            self.state["preview"] = preview;
        }
        if let Some((sheet, atlas)) = prepared {
            self.panels.set_stylesheet(sheet);
            self.atlas = atlas;
            self.remember_preference_style(values.style_path.clone());
        }
        self.panels.locale = values.locale;
        self.panels.node_editor.view_preferences = values.node_view;
        if let Some(editor) = &mut self.node_standalone {
            editor.view_preferences = values.node_view;
        }
        self.panels.timeline.column_visibility = values.columns;
        for floating in self.floating.values() {
            floating.window.set_title(&format!(
                "{} — Motion Studio",
                values.locale.panel(floating.panel.dock())
            ));
        }
        self.dirty = true;
        self.redraw();
        Ok(())
    }

    pub(crate) fn set_preview_preference(&mut self, mode: i32) -> Result<(), String> {
        let mut values = self.workspace.preferences();
        values.preview = PreviewQuality::from_id(mode)?;
        self.apply_preferences(values, false)
    }
}
