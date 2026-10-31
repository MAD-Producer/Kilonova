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
            let normal_overlay = self.present_workspace_repaints(size, preset)?;
            // Compact layouts deliberately omit an embedded preview that cannot
            // fit its existing controls. Exercise the existing expanded view as
            // well, retaining the normal-layout result instead of faking it.
            let expanded = if independent_node
                && preset == workspace::Preset::Nodes
                && !normal_overlay
                && panels::node_view_bound(self.workspace.display_root(), size, false).is_none()
            {
                Some(self.expanded_workspace_preview(size, records.len())?)
            } else {
                None
            };
            let node_overlay = normal_overlay || expanded.is_some();
            records.push(
                serde_json::json!({"preset":preset,"nodeOverlay":node_overlay,
                "normalNodeOverlay":normal_overlay,"expandedPreview":expanded,
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

    fn present_workspace_repaints(
        &mut self,
        size: [f32; 2],
        preset: workspace::Preset,
    ) -> Result<bool, String> {
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
                let has_overlay = ui.node_preview.is_some();
                let rendered = self
                    .engine
                    .present_async(ui)?
                    .recv_timeout(Duration::from_secs(5))
                    .map_err(|e| e.to_string())??;
                if rendered {
                    // An unpresented request is not evidence of a visible overlay.
                    node_overlay |= has_overlay;
                    break;
                }
                if began.elapsed() >= Duration::from_secs(5) {
                    return Err(format!("workspace {preset:?} did not present"));
                }
            }
        }
        Ok(node_overlay)
    }

    fn expanded_workspace_preview(&mut self, size: [f32; 2], index: usize) -> Result<Value, String> {
        let layout = serde_json::to_value(&self.workspace).map_err(|e| e.to_string())?;
        let state = self.engine.state()?;
        let frames_before = state["desktop"]["nodePreview"]["frames"].as_u64()
            .ok_or("workspace preview frame counter unavailable")?;
        let input = self.input.clone();
        let paint_input = self.paint_input.clone();
        let focus = self.panels.focused;
        if self.workspace.maximized_panel().is_some() {
            return Err("workspace preview probe expected an unmaximized layout".into());
        }
        if !self.workspace.toggle_panel_maximize(motion_ui::dock::Panel::Nodes.dock()) {
            return Err("workspace preview probe cannot expand the Nodes panel".into());
        }
        let result: Result<(PathBuf, [f32; 4]), String> = (|| {
            if !self.present_workspace_repaints(size, workspace::Preset::Nodes)? {
                return Err("expanded Nodes panel did not present its live preview".into());
            }
            let (paint, actions) = self.draw_commands(size);
            if !actions.is_empty() {
                return Err("expanded workspace capture emitted an editor command".into());
            }
            let ui = self.ui_frame(paint, size, true);
            let viewport = ui.node_preview
                .ok_or("expanded workspace capture lost the live preview")?.rect;
            let screenshot = self.smoke.as_ref().ok_or("no smoke report path")?
                .with_extension(format!("workspace-nodes-{index}-expanded.png"));
            self.capture_presented_ui(ui, screenshot.clone())?;
            Ok((screenshot, viewport))
        })();
        // Restore even when presentation or capture fails; preserve that cause.
        self.workspace.restore_panel();
        self.input = input;
        self.paint_input = paint_input;
        self.panels.focused = focus;
        let (screenshot, viewport) = result?;
        let after = self.engine.state()?;
        let frames_after = after["desktop"]["nodePreview"]["frames"].as_u64()
            .ok_or("workspace preview completion counter unavailable")?;
        if after["desktop"]["nodePreview"]["active"].is_null() || frames_after <= frames_before {
            return Err(format!("expanded Nodes preview did not encode GPU frames: before={frames_before} after={frames_after}"));
        }
        if self.workspace.maximized_panel().is_some()
            || serde_json::to_value(&self.workspace).map_err(|e| e.to_string())? != layout
            || ["project", "revision", "history"].iter().any(|key| after[*key] != state[*key])
        {
            return Err("expanded workspace preview changed persistent state".into());
        }
        Ok(serde_json::json!({"panel":"nodes","screenshot":screenshot,
            "viewport":viewport,"encodedFramesBefore":frames_before,"encodedFramesAfter":frames_after,
            "presented":true,"layoutRestored":true,"projectAndHistoryUntouched":true}))
    }
}
