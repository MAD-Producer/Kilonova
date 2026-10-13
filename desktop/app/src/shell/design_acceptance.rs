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
            "timeline-graph",
            "value-graph",
            "speed-graph",
        ] {
            let graph_preset = if screen == "timeline-graph" {
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
            if !screen.ends_with("graph") {
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
}
