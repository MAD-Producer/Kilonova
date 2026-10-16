//! Exercise workspace changes on the native surface, including the live node overlay.
use crate::*;

impl Shell {
    pub(crate) fn workspace_presentation_probe(&mut self, size: [f32; 2]) -> Result<Value, String> {
        if self.panels.node_preview.active.is_none() {
            return Err("workspace acceptance requires a live node preview".into());
        }
        let original = self.workspace.preset;
        // Layer-bound node editing uses the composition instead of a separate
        // node overlay. The ordinary smoke fixture exercises the independent one.
        let independent_node = self.panels.node_editor.layer_target.is_none();
        let project = self.engine.state()?["project"].clone();
        let mut records = vec![];
        for preset in [
            workspace::Preset::Default,
            workspace::Preset::Nodes,
            workspace::Preset::Drawing,
            workspace::Preset::All,
            workspace::Preset::Animation,
            original,
        ] {
            self.switch_workspace(preset, None)?;
            let began = Instant::now();
            let mut node_overlay = false;
            // Exercise both composition rendering and retained-frame repaint.
            for _ in 0..2 {
                loop {
                    let (paint, actions) = self.draw_commands(size);
                    if !actions.is_empty() {
                        return Err("workspace repaint emitted an editor command".into());
                    }
                    let ui = self.ui_frame(paint, size, false);
                    node_overlay |= ui.node_preview.is_some();
                    let rendered = self
                        .engine
                        .present_async(ui)?
                        .recv_timeout(Duration::from_secs(5))
                        .map_err(|e| e.to_string())??;
                    if rendered {
                        break;
                    }
                    if began.elapsed() >= Duration::from_secs(5) {
                        return Err(format!("workspace {preset:?} did not present"));
                    }
                }
            }
            records.push(
                serde_json::json!({"preset":preset,"nodeOverlay":node_overlay,
                "roundTripUs":began.elapsed().as_micros() as u64}),
            );
        }
        if independent_node && !records.iter().any(|record| record["nodeOverlay"] == true) {
            return Err("workspace acceptance did not present a node overlay".into());
        }
        if self.engine.state()?["project"] != project {
            return Err("workspace presentation changed the project".into());
        }
        Ok(serde_json::json!({"projectUntouched":true,"presentations":records}))
    }
}
