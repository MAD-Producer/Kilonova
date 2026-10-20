//! Install a validated style atomically in every native window.
use crate::*;
use motion_ui::style::{default_stylesheet, StyleSheet};

#[derive(Default)]
pub(crate) struct StyleSource {
    pub path: Option<PathBuf>,
    stamp: Option<(std::time::SystemTime, u64)>,
    error: Option<String>,
    pub next_poll: Option<Instant>,
    stamp_failures: crate::fallback_observation::Counter,
    reload_failures: crate::fallback_observation::Counter,
}
impl Shell {
    pub(crate) fn remember_preference_style(&mut self, path: Option<PathBuf>) {
        self.style_source = StyleSource { path, ..Default::default() };
        self.style_source.stamp = self.style_stamp();
        if self.style_source.path.is_some() {
            self.style_source.next_poll = Some(Instant::now() + Duration::from_millis(250));
        }
    }
    pub(crate) fn install_styles(&mut self, sheet: StyleSheet) -> Result<(), String> {
        let mut atlas = TextAtlas::from_family(
            sheet.font_family(),
            sheet.base_font_size(),
            self.workspace.locale == Locale::Zh,
        )?;
        atlas.set_pixel_ratio(self.scale);
        // Font creation may fail: keep both the previous font and sheet in that case.
        self.actions
            .extend(panels::cancel_interaction(&mut self.panels));
        self.panels.set_stylesheet(sheet);
        self.atlas = atlas;
        if self
            .style_source
            .error
            .take()
            .is_some_and(|error| self.panels.message.as_ref() == Some(&error))
        {
            self.panels.message = None;
        }
        self.dirty = true;
        self.redraw();
        Ok(())
    }
    pub(crate) fn load_style_source(&mut self, path: PathBuf) -> Result<(), String> {
        // Keep watching a remembered file even if startup finds it missing or
        // malformed. Fixing it should recover without restarting the editor.
        let absolute = if path.is_absolute() {
            path
        } else {
            std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(path)
        };
        let path = absolute.canonicalize().unwrap_or(absolute);
        self.style_source.path = Some(path.clone());
        self.style_source.stamp = self.style_stamp();
        self.style_source.next_poll = Some(Instant::now() + Duration::from_millis(250));
        self.reload_styles()?;
        self.workspace.style_path = Some(path.clone());
        if self.smoke.is_none() {
            self.workspace.save(&self.layout_path)?;
        }
        self.style_source.path = Some(path);
        self.style_source.stamp = self.style_stamp();
        self.style_source.next_poll = Some(Instant::now() + Duration::from_millis(250));
        Ok(())
    }
    fn style_stamp(&self) -> Option<(std::time::SystemTime, u64)> {
        let Some(path) = self.style_source.path.as_ref() else {
            self.style_source.stamp_failures.success();
            return None;
        };
        let result = std::fs::metadata(path).map_err(|e| ("metadata", e)).and_then(|m|
            m.modified().map(|stamp| (stamp, m.len())).map_err(|e| ("modified", e)));
        match result {
            Ok(stamp) => { self.style_source.stamp_failures.success(); Some(stamp) }
            Err((phase, error)) => {
                self.style_source.stamp_failures.failure("styles.stamp", format_args!("phase={phase} path={} reason={error} fallback=no_stamp", crate::fallback_observation::short(path.display())));
                None
            }
        }
    }

    pub(crate) fn reload_styles(&mut self) -> Result<(), String> {
        let Some(path) = self.style_source.path.clone() else {
            return Ok(());
        };
        let began = Instant::now();
        log::info!("style reload start path={}", path.display().to_string().chars().take(256).collect::<String>());
        crate::logging::flush();
        let result = default_stylesheet()
            .load_overrides(&path)
            .and_then(|sheet| self.install_styles(sheet));
        if let Err(error) = &result {
            self.style_source.reload_failures.failure("styles.reload", format_args!("path={} elapsed_ms={} reason={error} fallback=previous_sheet", crate::fallback_observation::short(path.display()), began.elapsed().as_millis()));
            self.style_source.error = Some(error.clone());
            self.panels.message = Some(error.clone());
            self.dirty = true;
        }
        if result.is_ok() { self.style_source.reload_failures.success(); }
        log::debug!("style reload done elapsed_ms={} ok={}", began.elapsed().as_millis(), result.is_ok());
        result
    }
    pub(crate) fn poll_styles(&mut self) {
        if !self
            .style_source
            .next_poll
            .is_some_and(|due| Instant::now() >= due)
        {
            return;
        }
        self.style_source.next_poll = Some(Instant::now() + Duration::from_millis(250));
        let stamp = self.style_stamp();
        if stamp != self.style_source.stamp {
            self.style_source.stamp = stamp;
            let _ = self.reload_styles();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn override_reloads_every_window_and_invalid_replacement_keeps_previous_sheet() {
        let folder = tempfile::tempdir().unwrap();
        let mut shell = Shell::new(folder.path().join("project"), None, Some(Locale::En)).unwrap();
        shell.layout_path = folder.path().join("layout.json");
        let path = folder.path().join("style.json");
        std::fs::write(&path, br##"{"tokens":{"colors":{"focus":"#00ff88","panel":"#212428"},"sizes":{"motion.drag_threshold":12}}}"##).unwrap();
        shell.load_style_source(path.clone()).unwrap();
        let before = shell.panels.styles.clone().unwrap();
        assert_eq!(before.metric("motion.drag_threshold", 3.), 12.);
        let (paint, _) = shell.draw_commands([1600., 1000.]);
        assert!(paint.colors.contains(&motion_ui::Color::rgb(33, 36, 40)));
        assert!(Arc::ptr_eq(&paint.styles(), &before));
        let mut floating = PaintList::default();
        panels::floating_chrome(
            &mut floating,
            &mut shell.atlas,
            dock::Panel::Properties,
            [286., 600.],
            &mut shell.panels,
            &shell.state,
            &Input::default(),
        );
        assert!(Arc::ptr_eq(&floating.styles(), &before));
        assert!(floating.colors.contains(&motion_ui::Color::rgb(33, 36, 40)));
        std::fs::write(&path, br#"{"styles":{"button":{"radius":-1}}}"#).unwrap();
        assert!(shell.reload_styles().unwrap_err().contains("radius"));
        assert!(Arc::ptr_eq(&shell.panels.styles.clone().unwrap(), &before));
        std::fs::write(&path, br##"{"tokens":{"colors":{"focus":"#ff8800"}}}"##).unwrap();
        shell.style_source.next_poll = Some(Instant::now());
        shell.poll_styles();
        assert!(shell.panels.message.is_none());
        assert_eq!(
            shell.panels.styles.clone().unwrap().color("focus"),
            motion_ui::Color::rgb(255, 136, 0)
        );
    }
    #[test]
    fn missing_stylesheet_recovers_when_created_and_does_not_clear_unrelated_feedback() {
        let folder = tempfile::tempdir().unwrap();
        let mut shell = Shell::new(folder.path().join("project"), None, Some(Locale::En)).unwrap();
        shell.layout_path = folder.path().join("layout.json");
        let path = folder.path().join("late-style.json");
        assert!(shell.load_style_source(path.clone()).is_err());
        assert!(shell.style_source.next_poll.is_some());
        assert!(shell.panels.styles.is_none());
        shell.panels.message = Some("Unrelated save feedback".into());
        std::fs::write(&path, br##"{"tokens":{"colors":{"focus":"#60C5A0"}}}"##).unwrap();
        shell.style_source.next_poll = Some(Instant::now());
        shell.poll_styles();
        assert_eq!(
            shell.panels.styles.as_ref().unwrap().color("focus"),
            motion_ui::Color::rgb(96, 197, 160)
        );
        assert_eq!(
            shell.panels.message.as_deref(),
            Some("Unrelated save feedback")
        );
    }
}
