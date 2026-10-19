//! Exercise the actual compact examples, nested editors, defaults and GPU output.
use super::*;

const EXAMPLES: [(&str, usize, &str); 3] = [
    ("soft_sphere", 9, "shadow_gradient"),
    ("soft_sphere_motion", 15, "shadow_gradient"),
    ("floating_lights", 7, "light_1_gradient"),
];

impl Shell {
    pub(super) fn start_compact_example_smoke(&mut self, index: usize) -> Result<(), String> {
        let graph = motion_graph::builtin_examples()
            .iter()
            .find(|e| e.id == EXAMPLES[index].0)
            .ok_or("compact example missing")?
            .load()
            .map_err(|e| e.to_string())?;
        self.panels.node_editor = node_editor::NodeEditor::new(graph);
        self.capture_node_float()?;
        let canvas = self.panels.node_editor.canvas_bounds();
        self.panels.node_editor.fit_all(canvas);
        self.schedule_node_compile()?;
        self.smoke_stage = 51 + index as u8 * 3;
        Ok(())
    }

    pub(super) fn advance_compact_example_smoke(
        &mut self,
        directory: &std::path::Path,
    ) -> Result<(), String> {
        let index = (self.smoke_stage as usize - 51) / 3;
        let step = (self.smoke_stage - 51) % 3;
        let (id, count, gradient) = EXAMPLES[index];
        match step {
            0 => {
                let document = self.panels.node_editor.snapshot().document;
                if document.nodes.len() != count
                    || document.public_parameters.len() != [5, 10, 3][index]
                {
                    return Err(format!("{id}: compact layout lost public controls"));
                }
                self.smoke_node_report[id]["initial"] = self
                    .engine
                    .node_capture(0., directory.join(format!("{id}-initial.png")))?;
                self.onboarding_screenshot(&format!("{id}-root"), &format!("{id}RootScreenshot"))?;
                if !self.panels.node_editor.enter_group("effect") {
                    return Err(format!("{id}: effect group did not open"));
                }
                self.capture_node_float()?;
                let canvas = self.panels.node_editor.canvas_bounds();
                self.panels.node_editor.fit_all(canvas);
                self.onboarding_screenshot(
                    &format!("{id}-internal"),
                    &format!("{id}InternalScreenshot"),
                )?;
                let position = document
                    .nodes
                    .iter()
                    .find_map(|n| match &n.operation {
                        motion_graph::NodeOp::Group { graph } => graph
                            .nodes
                            .iter()
                            .find(|n| n.id == gradient)
                            .map(|n| n.position),
                        _ => None,
                    })
                    .ok_or("nested gradient position missing")?;
                self.panels.node_editor.view.zoom = 1.;
                self.panels.node_editor.view.pan = [
                    canvas.width() * 0.5 - position[0] - 100.,
                    canvas.height() * 0.5 - position[1] - 100.,
                ];
                self.capture_node_float()?;
                self.edit_node_default_smoke(gradient, "falloff", "2")?;
                self.schedule_node_compile()?;
                self.smoke_stage += 1;
            }
            1 => {
                let edited = self
                    .engine
                    .node_capture(0., directory.join(format!("{id}-edited.png")))?;
                if edited["fingerprint"] == self.smoke_node_report[id]["initial"]["fingerprint"] {
                    return Err(format!(
                        "{id}: editing nested gradient did not change pixels"
                    ));
                }
                self.smoke_node_report[id]["edited"] = edited;
                self.onboarding_screenshot(
                    &format!("{id}-gradient-edited"),
                    &format!("{id}EditedScreenshot"),
                )?;
                let snapshot = self.panels.node_editor.snapshot();
                let path = directory.join(format!("{id}-edited.msgraph"));
                snapshot.save(&path).map_err(|e| e.to_string())?;
                if motion_graph::GraphDocument::load(&path).map_err(|e| e.to_string())?
                    != snapshot.document
                {
                    return Err(format!("{id}: saving inside the group lost root data"));
                }
                if !self.panels.node_editor.undo() {
                    return Err(format!("{id}: nested edit was not undoable"));
                }
                self.schedule_node_compile()?;
                self.smoke_stage += 1;
            }
            2 => {
                let restored = self
                    .engine
                    .node_capture(0., directory.join(format!("{id}-restored.png")))?;
                if restored["fingerprint"] != self.smoke_node_report[id]["initial"]["fingerprint"] {
                    return Err(format!("{id}: undo did not restore GPU pixels"));
                }
                if !self.panels.node_editor.leave_group() {
                    return Err(format!("{id}: return to root failed"));
                }
                let original = motion_graph::builtin_examples()
                    .iter()
                    .find(|e| e.id == id)
                    .unwrap()
                    .load()
                    .map_err(|e| e.to_string())?;
                if self.panels.node_editor.snapshot().document != original {
                    return Err(format!("{id}: group navigation/undo changed source"));
                }
                self.smoke_node_report[id]["groupEditUndoAndReopen"] = true.into();
                if index + 1 < EXAMPLES.len() {
                    self.start_compact_example_smoke(index + 1)?;
                } else {
                    self.finish_node_starter_smoke(directory)?;
                    self.smoke_node_report["compactExamplesProjectUntouched"] = true.into();
                }
            }
            _ => return Err("unexpected compact-example acceptance stage".into()),
        }
        Ok(())
    }
}
