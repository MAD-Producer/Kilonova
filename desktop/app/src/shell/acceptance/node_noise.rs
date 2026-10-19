//! Exercise both actual Noise dropdowns, native seed editing and GPU preview.
use super::*;
use motion_graph::{InputValue, NodeOp, NoiseMode};

impl Shell {
    pub(super) fn start_node_noise_smoke(&mut self) -> Result<(), String> {
        let graph = motion_graph::builtin_examples()
            .iter()
            .find(|e| e.id == "compact_noise")
            .ok_or("noise example missing")?
            .load()
            .map_err(|e| e.to_string())?;
        self.panels.node_editor = node_editor::NodeEditor::new(graph);
        self.capture_node_float()?;
        let canvas = self.panels.node_editor.canvas_bounds();
        self.panels.node_editor.fit_all(canvas);
        self.schedule_node_compile()?;
        self.smoke_stage = 41;
        Ok(())
    }

    fn noise_choice_smoke(
        &mut self,
        index: usize,
        downs: usize,
        expected: NodeOp,
        name: &str,
        key: &str,
    ) -> Result<(), String> {
        let rect = self
            .panels
            .node_editor
            .choice_bounds_at("noise", index)
            .ok_or("noise choice bounds missing")?;
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
        self.onboarding_screenshot(name, key)?;
        for key in std::iter::repeat_n(Key::Down, downs).chain([Key::Enter]) {
            self.node_float_input(Input {
                keys: vec![(key, Default::default())],
                ..Default::default()
            })?;
        }
        let document = self.panels.node_editor.snapshot().document;
        if document
            .nodes
            .iter()
            .find(|n| n.id == "noise")
            .map(|n| &n.operation)
            != Some(&expected)
        {
            return Err("native noise dropdown did not commit requested choice".into());
        }
        self.schedule_node_compile()
    }

    pub(super) fn advance_node_noise_smoke(
        &mut self,
        directory: &std::path::Path,
    ) -> Result<(), String> {
        match self.smoke_stage {
            41 => {
                self.capture_node_float()?;
                let canvas = self.panels.node_editor.canvas_bounds();
                self.panels.node_editor.fit_all(canvas);
                let snapshot = self.panels.node_editor.snapshot();
                let compiled = snapshot.compile().map_err(|e| e.to_string())?;
                if snapshot.document.nodes.len() != 3
                    || snapshot.document.connections.len() != 2
                    || compiled.report.passes != 1
                    || compiled.report.graph_sample_sites != 0
                {
                    return Err("noise is not the actual three-node single-pass graph".into());
                }
                self.smoke_node_report["noiseThreeNodes"] = true.into();
                self.smoke_node_report["noiseValue"] = self
                    .engine
                    .node_capture(0., directory.join("noise-value.png"))?;
                self.onboarding_screenshot("noise-value-ui", "noiseValueScreenshot")?;
                self.noise_choice_smoke(
                    0,
                    1,
                    NodeOp::Noise {
                        mode: NoiseMode::Perlin,
                        octaves: 1,
                    },
                    "noise-modes",
                    "noiseModesScreenshot",
                )?;
                self.smoke_stage = 42;
            }
            42 => {
                let perlin = self
                    .engine
                    .node_capture(0., directory.join("noise-perlin.png"))?;
                if perlin["fingerprint"] == self.smoke_node_report["noiseValue"]["fingerprint"] {
                    return Err("Perlin mode did not change GPU pixels".into());
                }
                self.smoke_node_report["noisePerlin"] = perlin;
                self.smoke_node_report["noiseModesChangedPixels"] = true.into();
                self.onboarding_screenshot("noise-perlin-ui", "noisePerlinScreenshot")?;
                self.noise_choice_smoke(
                    1,
                    3,
                    NodeOp::Noise {
                        mode: NoiseMode::Perlin,
                        octaves: 4,
                    },
                    "noise-octaves",
                    "noiseOctavesScreenshot",
                )?;
                self.smoke_stage = 43;
            }
            43 => {
                let layered = self
                    .engine
                    .node_capture(0., directory.join("noise-four-octaves.png"))?;
                if layered["fingerprint"] == self.smoke_node_report["noisePerlin"]["fingerprint"] {
                    return Err("octave dropdown did not change GPU pixels".into());
                }
                self.smoke_node_report["noiseLayered"] = layered;
                self.smoke_node_report["noiseOctavesChangedPixels"] = true.into();
                self.onboarding_screenshot("noise-layered-ui", "noiseLayeredScreenshot")?;
                self.edit_node_default_smoke("noise", "seed", "17.5")?;
                self.schedule_node_compile()?;
                self.smoke_stage = 44;
            }
            44 => {
                let edited = self
                    .engine
                    .node_capture(0., directory.join("noise-edited.png"))?;
                let snapshot = self.panels.node_editor.snapshot();
                if snapshot
                    .document
                    .nodes
                    .iter()
                    .find(|n| n.id == "noise")
                    .and_then(|n| n.input_defaults.get("seed"))
                    != Some(&InputValue::Float(17.5))
                    || edited["fingerprint"]
                        == self.smoke_node_report["noiseLayered"]["fingerprint"]
                {
                    return Err("native noise seed edit did not change value and pixels".into());
                }
                self.smoke_node_report["noiseNativeSeedChangedPixels"] = true.into();
                self.onboarding_screenshot("noise-edited-ui", "noiseEditedScreenshot")?;
                let path = directory.join("noise-edited.msgraph");
                snapshot.save(&path).map_err(|e| e.to_string())?;
                if motion_graph::GraphDocument::load(&path).map_err(|e| e.to_string())?
                    != snapshot.document
                {
                    return Err("noise source did not reopen".into());
                }
                self.smoke_node_report["noiseSourceReopened"] = true.into();
                if !self.panels.node_editor.undo() {
                    return Err("noise seed edit was not undoable".into());
                }
                self.schedule_node_compile()?;
                self.smoke_stage = 45;
            }
            45 => {
                let restored = self
                    .engine
                    .node_capture(0., directory.join("noise-restored.png"))?;
                if restored["fingerprint"] != self.smoke_node_report["noiseLayered"]["fingerprint"]
                {
                    return Err("noise undo did not restore GPU pixels".into());
                }
                self.smoke_node_report["noiseUndoRestoredPixels"] = true.into();
                self.start_node_blend_smoke()?;
            }
            _ => return Err("unexpected noise acceptance stage".into()),
        }
        Ok(())
    }
}
