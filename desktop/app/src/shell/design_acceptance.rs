//! Capture real widgets using the same native renderer, for Ardot comparisons.
use crate::*;
impl Shell {
    pub(crate) fn capture_design_screens(&mut self, size: [f32; 2]) -> Result<Value, String> {
        let report = self
            .smoke
            .as_ref()
            .ok_or("missing acceptance output")?
            .clone();
        let mut output = serde_json::Map::new();
        let styles = self.panels.styles.clone();
        for screen in [
            "home",
            "composition-settings",
            "workspace-manager",
            "effect-library",
            "timeline-parent-menu",
            "timeline-blend-menu",
            "timeline-graph",
            "value-graph",
            "speed-graph",
            "info-panel",
            "preview-panel",
            "character-panel",
        ] {
            let graph_preset = if screen.starts_with("timeline-") || screen == "effect-library" {
                workspace::Preset::Default
            } else {
                workspace::Preset::Animation
            };
            let mut paint = PaintList::default();
            paint.stylesheet = styles.clone();
            match screen {
                "home" => {
                    let mut home = home::Home::default();
                    home.draw(
                        &mut paint,
                        &mut self.atlas,
                        size,
                        &self.workspace.recent_projects,
                        &Input::default(),
                        self.workspace.locale,
                    );
                }
                "composition-settings" => {
                    let mut dialog = dialogs::Dialog::new_composition(1920, 1080);
                    dialog.render(
                        &mut paint,
                        &mut self.atlas,
                        size,
                        &Input::default(),
                        self.workspace.locale,
                    );
                }
                "workspace-manager" => {
                    let mut manager = workspace::Manager::new(&self.workspace);
                    manager.draw(
                        &mut paint,
                        &mut self.atlas,
                        size,
                        &Input::default(),
                        self.workspace.locale,
                    );
                }
                "effect-library" => {
                    let old = std::mem::take(&mut self.panels.browser);
                    self.panels.browser.catalogue = self.panels.effect_controls.catalogue.clone();
                    self.panels.browser.favorites = self.workspace.favorites.clone();
                    self.panels
                        .browser
                        .preview_categories(self.workspace.locale);
                    let layout = workspace::Workspace::design_layout(workspace::Preset::Default);
                    let actions = panels::chrome(
                        &mut paint,
                        &mut self.atlas,
                        size,
                        &layout,
                        &mut self.panels,
                        &self.state,
                        &Input::default(),
                    );
                    self.panels.browser = old;
                    if !actions.is_empty() {
                        return Err("effect library capture must not edit the project".into());
                    }
                }
                "info-panel" | "preview-panel" | "character-panel" => {
                    let target = if screen == "info-panel" {
                        dock::Panel::Info
                    } else if screen == "preview-panel" {
                        dock::Panel::Preview
                    } else {
                        dock::Panel::Character
                    };
                    for (suffix, panel_size) in [("", [360., 520.]), ("-narrow", [220., 360.])] {
                        let name = format!("{screen}{suffix}");
                        let path = report.with_extension(format!("{name}.png"));
                        self.capture_auxiliary_panel(target, panel_size, size, &path)?;
                        output.insert(name, serde_json::json!(path));
                    }
                    continue;
                }
                "timeline-parent-menu" | "timeline-blend-menu" => {
                    let old = self.panels.timeline.clone();
                    let columns = std::mem::take(&mut self.panels.timeline_columns);
                    let focus = self.panels.focused;
                    self.panels.timeline.set_graph_mode(false);
                    self.panels.timeline.scroll = 0.;
                    // This fixture locates layers by ordinal, so property rows stay collapsed.
                    self.panels.timeline.expanded.clear();
                    let layout = workspace::Workspace::design_layout(workspace::Preset::Default);
                    let frame = panels::frames(&layout, size)
                        .into_iter()
                        .find(|f| f.panel == dock::Panel::Timeline)
                        .ok_or("timeline placement missing")?;
                    let gutter = 640f32.min(frame.content.width() * 0.52);
                    let rows = self.state["project"]["layers"]
                        .as_array()
                        .ok_or("missing smoke layers")?;
                    let ordinal = rows
                        .iter()
                        .rev()
                        .position(|l| l["id"].as_u64() == self.panels.selected)
                        .ok_or("missing smoke selection")?;
                    let point = [
                        frame.content.min[0] + gutter
                            - if screen == "timeline-parent-menu" {
                                58.
                            } else {
                                158.
                            },
                        frame.content.min[1]
                            + 64.
                            + ordinal as f32 * paint.styles().metric("row.height", 22.)
                            + 11.,
                    ];
                    let input = Input {
                        mouse: point,
                        pressed: Some((motion_ui::input::MouseButton::Left, point)),
                        ..Default::default()
                    };
                    let actions = panels::chrome(
                        &mut paint,
                        &mut self.atlas,
                        size,
                        &layout,
                        &mut self.panels,
                        &self.state,
                        &input,
                    );
                    if !actions.is_empty() || !self.panels.timeline_columns.editing() {
                        return Err(
                            "timeline dropdown capture must open without editing the project"
                                .into(),
                        );
                    }
                    self.panels.timeline = old;
                    self.panels.timeline_columns = columns;
                    self.panels.focused = focus;
                }
                _ => {
                    let old = self.panels.timeline.clone();
                    let controls = std::mem::take(&mut self.panels.animation_controls);
                    let preset = self.panels.workspace_preset;
                    self.panels.workspace_preset = graph_preset;
                    if let Some(object) = self.panels.selected {
                        if let Some(frame) = self.state["timeline_layers"]
                            .as_array()
                            .and_then(|ls| ls.iter().find(|l| l["object"].as_u64() == Some(object)))
                            .and_then(|l| l["properties"]["position"]["keys"].as_array())
                            .and_then(|ks| ks.first())
                            .and_then(|k| k["frame"].as_i64())
                        {
                            self.panels.animation_controls.select_key(
                                object,
                                motion_core::Property::Position,
                                None,
                                frame,
                            );
                        }
                    }

                    self.panels.timeline.set_graph_mode(true);
                    self.panels.timeline.speed_graph = screen == "speed-graph";
                    let layout = workspace::Workspace::design_layout(graph_preset);
                    let actions = panels::chrome(
                        &mut paint,
                        &mut self.atlas,
                        size,
                        &layout,
                        &mut self.panels,
                        &self.state,
                        &Input::default(),
                    );
                    if !actions.is_empty() {
                        return Err("design capture must not edit the project".into());
                    }
                    self.panels.timeline = old;
                    self.panels.animation_controls = controls;
                    self.panels.workspace_preset = preset;
                }
            }
            let mut ui = self.ui_frame(paint, size, true);
            if !screen.ends_with("graph") && !screen.ends_with("menu") && screen != "effect-library"
            {
                ui.composition = None;
                ui.node_preview = None;
            } else {
                let view = panels::composition_view(
                    &workspace::Workspace::design_layout(graph_preset),
                    size,
                );
                ui.composition = Some([
                    view.min[0] * self.scale,
                    view.min[1] * self.scale,
                    view.width() * self.scale,
                    view.height() * self.scale,
                ]);
                ui.node_preview = None;
            }
            let path = report.with_extension(format!("{screen}.png"));
            self.engine.capture_ui(ui, path.clone())?;
            output.insert(screen.into(), serde_json::json!(path));
        }
        Ok(Value::Object(output))
    }

    fn capture_auxiliary_panel(
        &mut self,
        panel: dock::Panel,
        panel_size: [f32; 2],
        window_size: [f32; 2],
        path: &std::path::Path,
    ) -> Result<(), String> {
        let before = self.engine.state()?;
        let selection = (
            self.panels.selected,
            self.panels.selected_layers.clone(),
            self.panels.selection_anchor,
        );
        let focus = self.panels.focused;
        let mut created_text = false;
        let captured = (|| {
            if panel == dock::Panel::Character {
                self.state = self.engine.create_text("Title".into(), 64., 1024, None)?;
                created_text = true;
                let object = self.state["project"]["layers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .rev()
                    .find(|layer| layer["content"]["kind"] == "text")
                    .and_then(|layer| layer["id"].as_u64())
                    .ok_or("character capture requires a real text layer")?;
                self.panels.select_only(Some(object));
            }
            let mut paint = PaintList::default();
            paint.stylesheet = self.panels.styles.clone();
            let actions = panels::floating_chrome(
                &mut paint,
                &mut self.atlas,
                panel,
                panel_size,
                &mut self.panels,
                &self.state,
                &Input::default(),
            );
            if !actions.is_empty() {
                return Err("auxiliary panel capture must not emit edits".into());
            }
            // Capture at the window's real pixel ratio, then crop. Changing only
            // logical_size would stretch this panel across the main surface.
            let mut ui = self.ui_frame(paint, window_size, true);
            ui.composition = None;
            ui.node_preview = None;
            self.engine.capture_ui(ui, path.to_path_buf())?;
            let pixels = image::open(path).map_err(|e| e.to_string())?;
            let width = (panel_size[0] * pixels.width() as f32 / window_size[0]).round() as u32;
            let height = (panel_size[1] * pixels.height() as f32 / window_size[1]).round() as u32;
            pixels
                .crop_imm(0, 0, width, height)
                .save(path)
                .map_err(|e| e.to_string())
        })();
        let restored = if created_text {
            self.engine.history(0)
        } else {
            self.engine.state()
        };
        self.panels.selected = selection.0;
        self.panels.selected_layers = selection.1;
        self.panels.selection_anchor = selection.2;
        self.panels.focused = focus;
        self.state = restored?;
        if self.state["project"] != before["project"] {
            return Err("auxiliary panel fixture changed the project".into());
        }
        captured
    }
}
