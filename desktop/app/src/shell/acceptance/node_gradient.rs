//! Exercise the real Gradient dropdown, input editor and installed GPU preview.
use super::*;
use motion_graph::{GradientMode, InputValue, NodeOp};

impl Shell {
    pub(super) fn start_node_gradient_smoke(&mut self) -> Result<(), String> {
        let graph = motion_graph::builtin_examples()
            .iter()
            .find(|e| e.id == "compact_gradient")
            .ok_or("gradient example missing")?
            .load()
            .map_err(|e| e.to_string())?;
        self.panels.node_editor = node_editor::NodeEditor::new(graph);
        if std::env::var_os("MOTION_SMOKE_MINIMUM").is_some() {
            let window = &self
                .floating
                .values()
                .find(|f| f.panel == dock::Panel::Nodes)
                .ok_or("gradient floating window missing")?
                .window;
            let _ = window.request_inner_size(winit::dpi::LogicalSize::new(900., 600.));
        }
        self.capture_node_float()?;
        let canvas = self.panels.node_editor.canvas_bounds();
        self.panels.node_editor.fit_all(canvas);
        self.schedule_node_compile()?;
        self.smoke_stage = 36;
        Ok(())
    }

    fn gradient_mode_smoke(
        &mut self,
        mode: GradientMode,
        capture_menu: bool,
    ) -> Result<(), String> {
        let rect = self
            .panels
            .node_editor
            .choice_bounds("gradient")
            .ok_or("gradient mode bounds missing")?;
        let point = [
            (rect.min[0] + rect.max[0]) * 0.5,
            (rect.min[1] + rect.max[1]) * 0.5,
        ];
        for input in [
            Input {
                mouse: point,
                pressed: Some((MouseButton::Left, point)),
                held: Some(MouseButton::Left),
                ..Default::default()
            },
            Input {
                mouse: point,
                released: Some((MouseButton::Left, point)),
                ..Default::default()
            },
        ] {
            self.node_float_input(input)?;
        }
        if capture_menu {
            self.onboarding_screenshot("gradient-modes", "gradientModesScreenshot")?;
        }
        for key in [Key::Down, Key::Enter] {
            self.node_float_input(Input {
                keys: vec![(key, Default::default())],
                ..Default::default()
            })?;
        }
        let document = self.panels.node_editor.snapshot().document;
        if document
            .nodes
            .iter()
            .find(|node| node.id == "gradient")
            .map(|node| &node.operation)
            != Some(&NodeOp::Gradient { mode })
        {
            return Err("native gradient dropdown did not commit the requested mode".into());
        }
        self.schedule_node_compile()
    }

    pub(super) fn advance_node_gradient_smoke(
        &mut self,
        directory: &std::path::Path,
    ) -> Result<(), String> {
        match self.smoke_stage {
            36 => {
                self.capture_node_float()?;
                let canvas = self.panels.node_editor.canvas_bounds();
                self.panels.node_editor.fit_all(canvas);
                let f = self
                    .floating
                    .values()
                    .find(|f| f.panel == dock::Panel::Nodes)
                    .ok_or("gradient floating window missing")?;
                let physical = f.window.inner_size();
                self.smoke_node_report["gradientWindowSize"] = serde_json::json!([
                    (physical.width as f32 / f.scale).round() as u32,
                    (physical.height as f32 / f.scale).round() as u32,
                ]);
                let snapshot = self.panels.node_editor.snapshot();
                let compiled = snapshot.compile().map_err(|e| e.to_string())?;
                if snapshot.document.nodes.len() != 3
                    || snapshot.document.connections.len() != 2
                    || compiled.report.passes != 1
                    || compiled.report.graph_sample_sites != 0
                {
                    return Err("gradient is not the actual three-node single-pass graph".into());
                }
                self.smoke_node_report["gradientThreeNodes"] = true.into();
                self.smoke_node_report["gradientLinear"] = self
                    .engine
                    .node_capture(0., directory.join("gradient-linear.png"))?;
                self.onboarding_screenshot("gradient-linear-ui", "gradientLinearScreenshot")?;
                self.gradient_mode_smoke(GradientMode::Radial, true)?;
                self.edit_node_default_smoke("gradient", "start", "0.5, 0.5")?;
                self.schedule_node_compile()?;
                self.smoke_stage = 37;
            }
            37 => {
                let radial = self
                    .engine
                    .node_capture(0., directory.join("gradient-radial.png"))?;
                if radial["fingerprint"] == self.smoke_node_report["gradientLinear"]["fingerprint"]
                {
                    return Err("radial gradient did not change GPU pixels".into());
                }
                self.smoke_node_report["gradientRadial"] = radial;
                self.onboarding_screenshot("gradient-radial-ui", "gradientRadialScreenshot")?;
                self.gradient_mode_smoke(GradientMode::Conic, false)?;
                self.smoke_stage = 38;
            }
            38 => {
                let conic = self
                    .engine
                    .node_capture(0., directory.join("gradient-conic.png"))?;
                if conic["fingerprint"] == self.smoke_node_report["gradientRadial"]["fingerprint"]
                    || conic["fingerprint"]
                        == self.smoke_node_report["gradientLinear"]["fingerprint"]
                {
                    return Err("conic gradient did not produce distinct pixels".into());
                }
                self.smoke_node_report["gradientConic"] = conic;
                self.smoke_node_report["gradientModesChangedPixels"] = true.into();
                self.onboarding_screenshot("gradient-conic-ui", "gradientConicScreenshot")?;
                self.edit_node_default_smoke("gradient", "falloff", "2")?;
                self.schedule_node_compile()?;
                self.smoke_stage = 39;
            }
            39 => {
                let edited = self
                    .engine
                    .node_capture(0., directory.join("gradient-edited.png"))?;
                let snapshot = self.panels.node_editor.snapshot();
                if snapshot
                    .document
                    .nodes
                    .iter()
                    .find(|node| node.id == "gradient")
                    .and_then(|node| node.input_defaults.get("falloff"))
                    != Some(&InputValue::Float(2.))
                    || edited["fingerprint"]
                        == self.smoke_node_report["gradientConic"]["fingerprint"]
                {
                    return Err(
                        "native gradient falloff edit did not change value and pixels".into(),
                    );
                }
                self.smoke_node_report["gradientNativeFalloffChangedPixels"] = true.into();
                self.onboarding_screenshot("gradient-edited-ui", "gradientEditedScreenshot")?;
                let path = directory.join("gradient-edited.msgraph");
                snapshot.save(&path).map_err(|e| e.to_string())?;
                if motion_graph::GraphDocument::load(&path).map_err(|e| e.to_string())?
                    != snapshot.document
                {
                    return Err("gradient source did not reopen".into());
                }
                self.smoke_node_report["gradientSourceReopened"] = true.into();
                if !self.panels.node_editor.undo() {
                    return Err("gradient edit was not undoable".into());
                }
                self.schedule_node_compile()?;
                self.smoke_stage = 40;
            }
            40 => {
                let restored = self
                    .engine
                    .node_capture(0., directory.join("gradient-restored.png"))?;
                if restored["fingerprint"] != self.smoke_node_report["gradientConic"]["fingerprint"]
                {
                    return Err("gradient undo did not restore GPU pixels".into());
                }
                self.smoke_node_report["gradientUndoRestoredPixels"] = true.into();
                self.finish_node_starter_smoke(directory)?;
                self.smoke_node_report["gradientProjectUntouched"] = true.into();
            }
            _ => return Err("unexpected gradient acceptance stage".into()),
        }
        Ok(())
    }
}
