//! Exercise the actual Blend mode menu, strength editor and GPU preview.
use super::*;
use motion_graph::{ColorBlendMode, InputValue, NodeOp};

impl Shell {
    pub(super) fn start_node_blend_smoke(&mut self) -> Result<(), String> {
        let graph = motion_graph::builtin_examples()
            .iter()
            .find(|e| e.id == "compact_blend")
            .ok_or("blend example missing")?
            .load()
            .map_err(|e| e.to_string())?;
        self.panels.node_editor = node_editor::NodeEditor::new(graph);
        self.capture_node_float()?;
        let canvas = self.panels.node_editor.canvas_bounds();
        self.panels.node_editor.fit_all(canvas);
        self.schedule_node_compile()?;
        self.smoke_stage = 46;
        Ok(())
    }

    pub(super) fn advance_node_blend_smoke(
        &mut self,
        directory: &std::path::Path,
    ) -> Result<(), String> {
        match self.smoke_stage {
            46 => {
                self.capture_node_float()?;
                let canvas = self.panels.node_editor.canvas_bounds();
                self.panels.node_editor.fit_all(canvas);
                let snapshot = self.panels.node_editor.snapshot();
                let compiled = snapshot.compile().map_err(|e| e.to_string())?;
                if snapshot.document.nodes.len() != 5
                    || snapshot.document.connections.len() != 5
                    || compiled.report.passes != 1
                    || compiled.report.graph_sample_sites != 0
                {
                    return Err("blend fixture is not a five-node single-pass graph".into());
                }
                self.smoke_node_report["blendFiveNodes"] = true.into();
                self.smoke_node_report["blendSoftLight"] = self
                    .engine
                    .node_capture(0., directory.join("blend-soft-light.png"))?;
                self.onboarding_screenshot("blend-soft-light-ui", "blendSoftLightScreenshot")?;
                let rect = self
                    .panels
                    .node_editor
                    .choice_bounds("blend")
                    .ok_or("blend mode bounds missing")?;
                let point = [
                    (rect.min[0] + rect.max[0]) * 0.5,
                    (rect.min[1] + rect.max[1]) * 0.5,
                ];
                // Capture the whole graph above, then inspect its real mode
                // control at 1:1 zoom rather than an unreadable fit-all scale.
                self.panels.node_editor.view.zoom_at(1., point, canvas);
                self.capture_node_float()?;
                let rect = self
                    .panels
                    .node_editor
                    .choice_bounds("blend")
                    .ok_or("zoomed blend mode bounds missing")?;
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
                self.onboarding_screenshot("blend-modes", "blendModesScreenshot")?;
                // Soft Light (last item) to Screen, exercising a long native menu.
                for key in std::iter::repeat_n(Key::Up, 11).chain([Key::Enter]) {
                    self.node_float_input(Input {
                        keys: vec![(key, Default::default())],
                        ..Default::default()
                    })?;
                }
                let changed = self.panels.node_editor.snapshot().document;
                if changed
                    .nodes
                    .iter()
                    .find(|n| n.id == "blend")
                    .map(|n| &n.operation)
                    != Some(&NodeOp::Blend {
                        mode: ColorBlendMode::Screen,
                    })
                    || changed.connections != snapshot.document.connections
                    || changed
                        .nodes
                        .iter()
                        .find(|n| n.id == "blend")
                        .map(|n| &n.input_defaults)
                        != snapshot
                            .document
                            .nodes
                            .iter()
                            .find(|n| n.id == "blend")
                            .map(|n| &n.input_defaults)
                {
                    return Err(
                        "blend mode menu changed wires/defaults or did not select Screen".into(),
                    );
                }
                self.schedule_node_compile()?;
                self.smoke_stage = 47;
            }
            47 => {
                let screen = self
                    .engine
                    .node_capture(0., directory.join("blend-screen.png"))?;
                if screen["fingerprint"] == self.smoke_node_report["blendSoftLight"]["fingerprint"]
                {
                    return Err("blend mode did not change GPU pixels".into());
                }
                self.smoke_node_report["blendScreen"] = screen;
                self.smoke_node_report["blendModesChangedPixels"] = true.into();
                self.onboarding_screenshot("blend-screen-ui", "blendScreenScreenshot")?;
                self.edit_node_default_smoke("blend", "factor", "0.25")?;
                self.schedule_node_compile()?;
                self.smoke_stage = 48;
            }
            48 => {
                let edited = self
                    .engine
                    .node_capture(0., directory.join("blend-edited.png"))?;
                let snapshot = self.panels.node_editor.snapshot();
                if snapshot
                    .document
                    .nodes
                    .iter()
                    .find(|n| n.id == "blend")
                    .and_then(|n| n.input_defaults.get("factor"))
                    != Some(&InputValue::Float(0.25))
                    || edited["fingerprint"] == self.smoke_node_report["blendScreen"]["fingerprint"]
                {
                    return Err("native blend strength edit did not change value and pixels".into());
                }
                self.smoke_node_report["blendEdited"] = edited;
                self.smoke_node_report["blendNativeStrengthChangedPixels"] = true.into();
                self.onboarding_screenshot("blend-edited-ui", "blendEditedScreenshot")?;
                let path = directory.join("blend-edited.msgraph");
                snapshot.save(&path).map_err(|e| e.to_string())?;
                if motion_graph::GraphDocument::load(&path).map_err(|e| e.to_string())?
                    != snapshot.document
                {
                    return Err("blend source did not reopen".into());
                }
                self.smoke_node_report["blendSourceReopened"] = true.into();
                if !self.panels.node_editor.undo() {
                    return Err("blend strength was not undoable".into());
                }
                self.schedule_node_compile()?;
                self.smoke_stage = 49;
            }
            49 => {
                let restored = self
                    .engine
                    .node_capture(0., directory.join("blend-restored.png"))?;
                if restored["fingerprint"] != self.smoke_node_report["blendScreen"]["fingerprint"] {
                    return Err("blend undo did not restore pixels".into());
                }
                self.smoke_node_report["blendUndoRestoredPixels"] = true.into();
                if !self.panels.node_editor.redo() {
                    return Err("blend strength was not redoable".into());
                }
                self.schedule_node_compile()?;
                self.smoke_stage = 50;
            }
            50 => {
                let redone = self
                    .engine
                    .node_capture(0., directory.join("blend-redone.png"))?;
                if redone["fingerprint"] != self.smoke_node_report["blendEdited"]["fingerprint"] {
                    return Err("blend redo did not restore pixels".into());
                }
                self.smoke_node_report["blendRedoRestoredPixels"] = true.into();
                for key in [
                    "gradientProjectUntouched",
                    "noiseProjectUntouched",
                    "blendProjectUntouched",
                ] {
                    self.smoke_node_report[key] = true.into();
                }
                self.start_compact_example_smoke(0)?;
            }
            _ => return Err("unexpected blend acceptance stage".into()),
        }
        Ok(())
    }
}
