//! Real preferences controls, rendered by the attached native GPU surface.
use crate::*;

impl Shell {
    pub(super) fn capture_preference_screens(
        &mut self,
        window_size: [f32; 2],
        report: &std::path::Path,
        output: &mut serde_json::Map<String, Value>,
    ) -> Result<(), String> {
        let original = self.engine.state()?;
        for (name, size) in [
            ("preferences", [900., 600.]),
            ("preferences-quality", [900., 600.]),
            ("preferences-interface", [900., 600.]),
            ("preferences-compact", [360., 400.]),
            ("preferences-compact-bottom", [360., 400.]),
            ("preferences-error", [900., 600.]),
        ] {
            let mut dialog = dialogs::Dialog::preferences(&self.workspace);
            let mut paint = PaintList::default();
            paint.stylesheet = self.panels.styles.clone();
            let styles = paint.styles();
            if name.ends_with("-interface") || name.ends_with("-bottom") {
                let pad = styles.metric("preferences.margin", 16.);
                let row = styles.metric("row.height", 22.);
                let gap = styles.metric("auxiliary.gap", 6.);
                let width = styles
                    .metric("preferences.width", 512.)
                    .min(size[0] - pad * 2.);
                let height = styles
                    .metric("preferences.max_height", 492.)
                    .min(size[1] - pad * 2.);
                let compact = width < styles.metric("preferences.sidebar_breakpoint", 440.);
                let point = if compact {
                    [
                        size[0] * 0.5,
                        (size[1] - height) * 0.5
                            + pad
                            + row * 2.5
                            + styles.metric("preferences.section_gap", 12.),
                    ]
                } else {
                    [
                        (size[0] - width) * 0.5
                            + pad
                            + styles.metric("preferences.sidebar_width", 100.) * 0.5,
                        (size[1] - height) * 0.5
                            + pad
                            + row * 3.5
                            + styles.metric("preferences.section_gap", 12.)
                            + gap,
                    ]
                };
                let input = Input {
                    mouse: point,
                    pressed: Some((MouseButton::Left, point)),
                    ..Default::default()
                };
                let mut discarded = PaintList::default();
                discarded.stylesheet = self.panels.styles.clone();
                if dialog
                    .render(
                        &mut discarded,
                        &mut self.atlas,
                        size,
                        &input,
                        self.workspace.locale,
                    )
                    .is_some()
                {
                    return Err("preferences category click emitted a commit".into());
                }
            }
            let input = if name.ends_with("-quality") {
                let pad = styles.metric("preferences.margin", 16.);
                let row = styles.metric("row.height", 22.);
                let top = (size[1] - styles.metric("preferences.max_height", 492.)) * 0.5;
                let point = [
                    size[0] * 0.5 + styles.metric("preferences.width", 512.) * 0.25,
                    top + pad
                        + row * 3.5
                        + styles.metric("preferences.section_gap", 12.)
                        + styles.metric("auxiliary.gap", 6.),
                ];
                Input {
                    mouse: point,
                    pressed: Some((MouseButton::Left, point)),
                    ..Default::default()
                }
            } else if name.ends_with("-bottom") {
                Input {
                    mouse: [size[0] * 0.5, size[1] * 0.5],
                    wheel: [0., 1000.],
                    ..Default::default()
                }
            } else {
                Input::default()
            };
            if name.ends_with("-error") {
                let mut values = self.workspace.preferences();
                values.style_path = Some(report.with_extension("missing-style.json"));
                let error = self
                    .apply_preferences(values, true)
                    .err()
                    .ok_or("missing style unexpectedly loaded")?;
                dialog.error = Some(self.workspace.locale.error(&error));
            }
            if dialog
                .render(
                    &mut paint,
                    &mut self.atlas,
                    size,
                    &input,
                    self.workspace.locale,
                )
                .is_some()
            {
                return Err(format!(
                    "{name} capture unexpectedly committed a dialog action"
                ));
            }
            let mut ui = self.ui_frame(paint, window_size, true);
            // This isolated capture overrides shell geometry; use the single-view frame protocol.
            ui.views = None;
            ui.composition = None;
            ui.composition_image = None;
            ui.node_preview = None;
            let path = report.with_extension(format!("{name}.png"));
            self.capture_presented_ui(ui, path.clone())?;
            let pixels = image::open(&path).map_err(|e| e.to_string())?;
            let width = (size[0] * pixels.width() as f32 / window_size[0]).round() as u32;
            let height = (size[1] * pixels.height() as f32 / window_size[1]).round() as u32;
            pixels
                .crop_imm(0, 0, width, height)
                .save(&path)
                .map_err(|e| e.to_string())?;
            output.insert(name.into(), serde_json::json!(path));
        }
        let after = self.engine.state()?;
        for field in ["project", "revision", "canUndo", "canRedo"] {
            if original[field] != after[field] {
                return Err(format!("preferences capture changed {field}"));
            }
        }
        Ok(())
    }
}
