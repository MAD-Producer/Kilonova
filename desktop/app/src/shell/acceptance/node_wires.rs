//! Real floating-window pointer input, saved source and GPU evidence for wire targeting.
use super::*;
use motion_graph::{Connection, InputValue, MathOp, NodeOp, ValueType};
use std::path::Path;

impl Shell {
    fn wire_port(&self, node: &str, port: &str, output: bool) -> Result<[f32; 2], String> {
        self.panels
            .node_editor
            .port_position(node, port, output)
            .ok_or_else(|| format!("native wire port missing: {node}.{port}"))
    }
    fn wire_body(&self, node: &str) -> Result<[f32; 2], String> {
        self.panels
            .node_editor
            .node_header_center(node)
            .ok_or_else(|| format!("native wire card missing: {node}"))
    }
    fn begin_wire_smoke(&mut self, from: [f32; 2], to: [f32; 2]) -> Result<(), String> {
        self.node_float_input(Input {
            mouse: from,
            pressed: Some((MouseButton::Left, from)),
            held: Some(MouseButton::Left),
            ..Default::default()
        })?;
        self.node_float_input(Input {
            mouse: to,
            held: Some(MouseButton::Left),
            ..Default::default()
        })
    }
    fn release_wire_smoke(&mut self, point: [f32; 2]) -> Result<(), String> {
        self.node_float_input(Input {
            mouse: point,
            released: Some((MouseButton::Left, point)),
            ..Default::default()
        })
    }
    fn wire_smoke_key(&mut self, key: Key) -> Result<(), String> {
        self.node_float_input(Input {
            keys: vec![(key, Default::default())],
            ..Default::default()
        })
    }
    fn wire_screenshot(
        &mut self,
        name: &str,
        key: &str,
        held: Option<[f32; 2]>,
    ) -> Result<(), String> {
        let input = held
            .map(|point| Input {
                mouse: point,
                held: Some(MouseButton::Left),
                ..Default::default()
            })
            .unwrap_or_default();
        let screenshot = self.capture_node_float_input(&input)?;
        let path = self
            .smoke
            .as_ref()
            .unwrap()
            .with_extension(format!("{name}.png"));
        std::fs::copy(screenshot, &path).map_err(|e| e.to_string())?;
        self.smoke_node_report[key] = serde_json::to_value(path).map_err(|e| e.to_string())?;
        Ok(())
    }
    fn wire_node_id(&self) -> Result<String, String> {
        self.smoke_node_report["wireNode"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "native wire node ID missing".into())
    }
    pub(super) fn start_node_wire_smoke(&mut self) -> Result<(), String> {
        self.smoke_node_report["wireOriginalDocument"] =
            serde_json::to_value(self.panels.node_editor.snapshot().document)
                .map_err(|e| e.to_string())?;
        self.smoke_node_report["wireOriginalZoom"] = self.panels.node_editor.view.zoom.into();
        self.smoke_node_report["wireOriginalPan"] =
            serde_json::to_value(self.panels.node_editor.view.pan).map_err(|e| e.to_string())?;
        let id = self
            .panels
            .node_editor
            .add_node(
                NodeOp::Math {
                    operation: MathOp::Add,
                    value_type: ValueType::Float,
                },
                [960., 190.],
            )
            .map_err(|e| e.to_string())?;
        self.panels
            .node_editor
            .set_input_default(&id, "a", InputValue::Float(7.))
            .map_err(|e| e.to_string())?;
        // Keep the arranged radius source and both destination cards inside the canvas.
        self.panels.node_editor.view.zoom = 0.5;
        self.panels.node_editor.view.pan = [8., 16.];
        self.node_float_input(Input::default())?;
        let from = self.wire_port(&id, "value", true)?;
        let to = self.wire_body("circle")?;
        self.begin_wire_smoke(from, to)?;
        self.wire_screenshot("wire-unique", "wireUniqueScreenshot", Some(to))?;
        self.release_wire_smoke(to)?;
        if !self
            .panels
            .node_editor
            .snapshot()
            .document
            .connections
            .contains(&Connection::new(&id, "value", "circle", "radius"))
        {
            return Err("unique native body drop did not choose circle.radius".into());
        }
        self.smoke_node_report["wireNode"] = id.into();
        Ok(())
    }
    pub(super) fn advance_node_wire_smoke(&mut self, directory: &Path) -> Result<(), String> {
        let id = self.wire_node_id()?;
        let capture = self.engine.node_capture(
            0.,
            directory.join(format!("node-wire-{}.png", self.smoke_stage)),
        )?;
        if self.smoke_stage == 28 {
            if capture["fingerprint"] == self.smoke_node_report["parameter"]["fingerprint"] {
                return Err("unique body drop did not change real GPU pixels".into());
            }
            self.smoke_node_report["wireUniqueBodyChangedPixels"] = true.into();
            let before = self.panels.node_editor.snapshot().document;
            let from = self.wire_port("radius", "value", true)?;
            let to = self.wire_body(&id)?;
            self.begin_wire_smoke(from, to)?;
            self.wire_screenshot("wire-candidates", "wireCandidatesScreenshot", Some(to))?;
            self.release_wire_smoke(to)?;
            self.wire_screenshot("wire-port-menu", "wirePortMenuScreenshot", None)?;
            if self.panels.node_editor.snapshot().document != before {
                return Err("port chooser mutated native graph before choosing".into());
            }
            self.wire_smoke_key(Key::Escape)?;
            if self.panels.node_editor.snapshot().document != before {
                return Err("cancelling native chooser mutated graph".into());
            }
            self.smoke_node_report["wireChoiceDeferredAndCancelled"] = true.into();
            self.begin_wire_smoke(from, to)?;
            self.release_wire_smoke(to)?;
            self.wire_smoke_key(Key::Enter)?;
            if !self
                .panels
                .node_editor
                .snapshot()
                .document
                .connections
                .contains(&Connection::new("radius", "value", &id, "a"))
            {
                return Err("native port chooser did not connect selected input A".into());
            }
            self.schedule_node_compile()?;
            self.smoke_stage = 29;
        } else if self.smoke_stage == 29 {
            if capture["fingerprint"] != self.smoke_node_report["parameter"]["fingerprint"] {
                return Err("native port choice did not restore the expected GPU result".into());
            }
            self.smoke_node_report["wireChoiceRestoredPixels"] = true.into();
            let before = self.panels.node_editor.snapshot().document;
            let from = self.wire_port(&id, "a", false)?;
            let b = self.wire_port(&id, "b", false)?;
            let to = [self.wire_body(&id)?[0], b[1]];
            self.begin_wire_smoke(from, to)?;
            self.release_wire_smoke(to)?;
            self.wire_screenshot("wire-replace-menu", "wireReplaceMenuScreenshot", None)?;
            self.wire_smoke_key(Key::Escape)?;
            if self.panels.node_editor.snapshot().document != before {
                return Err("cancelled native rewire discarded the original edge".into());
            }
            self.smoke_node_report["wireRewireCancelPreservedEdges"] = true.into();
            let from = self.wire_port("radius", "value", true)?;
            let to = self.wire_body("fill")?;
            self.begin_wire_smoke(from, to)?;
            self.wire_screenshot("wire-rejected", "wireRejectedScreenshot", Some(to))?;
            self.release_wire_smoke(to)?;
            if self.panels.node_editor.snapshot().document != before {
                return Err("incompatible native body drop was not rejected explicitly".into());
            }
            self.smoke_node_report["wireInvalidBodyPreservedEdges"] = true.into();
            self.panels.node_editor.view.zoom = 0.35;
            self.node_float_input(Input::default())?;
            let from = self.wire_port("radius", "value", true)?;
            let b = self.wire_port(&id, "b", false)?;
            let to = [b[0], b[1] - 1.];
            self.begin_wire_smoke(from, to)?;
            self.release_wire_smoke(to)?;
            let document = self.panels.node_editor.snapshot().document;
            if !document
                .connections
                .contains(&Connection::new("radius", "value", &id, "b"))
                || !document
                    .connections
                    .contains(&Connection::new("radius", "value", &id, "a"))
            {
                return Err("native 35% drop did not select nearest input B and preserve A".into());
            }
            let source = directory.join("node-wires.msgraph");
            self.panels
                .node_editor
                .snapshot()
                .save(&source)
                .map_err(|e| e.to_string())?;
            if motion_graph::GraphDocument::load(&source).map_err(|e| e.to_string())? != document {
                return Err("native wire source did not reopen exactly".into());
            }
            self.smoke_node_report["wireSourceReopened"] = true.into();
            self.schedule_node_compile()?;
            self.smoke_stage = 30;
        } else {
            if capture["fingerprint"] == self.smoke_node_report["parameter"]["fingerprint"] {
                return Err("nearest native B connection did not change GPU pixels".into());
            }
            self.smoke_node_report["wireNearestSocketChangedPixels"] = true.into();
            self.wire_screenshot("wire-nearest", "wireNearestScreenshot", None)?;
            let original: motion_graph::GraphDocument =
                serde_json::from_value(self.smoke_node_report["wireOriginalDocument"].clone())
                    .map_err(|e| e.to_string())?;
            let mut count = 0;
            while self.panels.node_editor.snapshot().document != original && count < 8 {
                if !self.panels.node_editor.undo() {
                    return Err("native wire history exhausted".into());
                }
                count += 1;
            }
            if self.panels.node_editor.snapshot().document != original || count != 5 {
                return Err(format!(
                    "native wire history did not restore the exact graph in five edits: {count}"
                ));
            }
            self.panels.node_editor.view.zoom =
                self.smoke_node_report["wireOriginalZoom"].as_f64().unwrap() as f32;
            self.panels.node_editor.view.pan =
                serde_json::from_value(self.smoke_node_report["wireOriginalPan"].clone())
                    .map_err(|e| e.to_string())?;
            self.smoke_node_report
                .as_object_mut()
                .unwrap()
                .remove("wireOriginalDocument");
            self.schedule_node_compile()?;
            self.smoke_stage = 31;
        }
        Ok(())
    }
}
