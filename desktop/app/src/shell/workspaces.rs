use crate::*;
impl Shell {
    pub(crate) fn remember_project(&mut self) -> Result<(), String> {
        let root = std::path::PathBuf::from(self.state["root"].as_str().unwrap_or(""));
        if root.as_os_str().is_empty() {
            return Ok(());
        }
        let p = &self.state["project"];
        let fps: motion_core::FrameRate = serde_json::from_value(p["fps"].clone())
            .map_err(|error| format!("remember project frame rate: {error}"))?;
        self.workspace.recent_projects.retain(|r| r.root != root);
        self.workspace.recent_projects.insert(
            0,
            workspace::RecentProject {
                root,
                name: p["name"].as_str().unwrap_or("Untitled").into(),
                width: p["width"].as_u64().unwrap_or(1920) as u32,
                height: p["height"].as_u64().unwrap_or(1080) as u32,
                fps: fps.as_f64(),
            },
        );
        self.workspace.recent_projects.truncate(24);
        if self.smoke.is_none() {
            self.workspace.save(&self.layout_path)?;
        }
        Ok(())
    }

    pub(crate) fn switch_workspace(
        &mut self,
        preset: workspace::Preset,
        event_loop: Option<&ActiveEventLoop>,
    ) -> Result<(), String> {
        self.actions
            .extend(panels::cancel_interaction(&mut self.panels));
        self.workspace.cancel();
        self.workspace.select_preset(preset);
        for (_, floating) in std::mem::take(&mut self.floating) {
            if floating.panel == dock::Panel::Composition { self.panels.viewer_inspection.close_floating(); }
            self.engine.close_window(floating.key)?;
        }
        self.panels.workspace_preset = preset;
        self.panels
            .timeline
            .set_graph_mode(preset == workspace::Preset::Animation);
        self.panels.focused = Some(match preset {
            workspace::Preset::Nodes => dock::Panel::Nodes,
            workspace::Preset::Drawing => dock::Panel::Vector,
            _ => dock::Panel::Timeline,
        });
        if let Some(event_loop) = event_loop {
            for id in self.workspace.floating.clone() {
                self.float_panel(id, event_loop)?;
            }
        }
        if self.smoke.is_none() {
            self.workspace.save(&self.layout_path)?;
        }
        self.dirty = true;
        self.redraw();
        Ok(())
    }
}
