use super::*;

impl App {
    /// Exercise the real native redraw path with normalized splitter input.
    /// This is the independent component gallery, not an editor project.
    pub(super) fn verify_dock_resize(
        &mut self,
        main: WindowId,
        event_loop: &ActiveEventLoop,
        path: &std::path::Path,
    ) -> Result<serde_json::Value, String> {
        let saved = serde_json::to_value(&self.workspace).map_err(|e| e.to_string())?;
        let before_model = self.model.borrow().clone();
        let scale = self.windows[&main].window.scale_factor() as f32;
        let size = self.windows[&main].window.inner_size();
        let area = Rect::new(
            0.,
            0.,
            size.width as f32 / scale,
            size.height as f32 / scale,
        );
        let original_frames = self.windows[&main].frames;
        let original = std::mem::replace(
            &mut self.workspace,
            Workspace::from_root(dock::Node::Split {
                edge: Edge::Left,
                split: 0.5,
                first: Box::new(dock::Node::dock(DockId(1))),
                second: Box::new(dock::Node::dock(DockId(2))),
            }),
        );
        self.windows.get_mut(&main).unwrap().input = Input::default();
        self.redraw(main, event_loop)?;
        self.capture(main, &path.with_extension("dock-before.png"))?;
        let start = [area.width() * 0.5, area.height() * 0.5];
        let end = [-100., start[1]];
        for input in [
            Input {
                mouse: start,
                pressed: Some((motion_ui::MouseButton::Left, start)),
                held: Some(motion_ui::MouseButton::Left),
                ..Default::default()
            },
            Input {
                mouse: end,
                held: Some(motion_ui::MouseButton::Left),
                ..Default::default()
            },
            Input {
                mouse: end,
                released: Some((motion_ui::MouseButton::Left, end)),
                ..Default::default()
            },
        ] {
            self.windows.get_mut(&main).unwrap().input = input;
            self.redraw(main, event_loop)?;
        }
        let placements = dock::solve(&self.workspace.root, area);
        let width = placements
            .iter()
            .find(|p| p.id == DockId(1))
            .ok_or("resized dock disappeared")?
            .area
            .width();
        let expected = metrics::MIN_DOCK.min(area.width() * 0.45);
        if (width - expected).abs() > 0.01 || self.windows[&main].frames < original_frames + 4 {
            return Err(format!("native splitter did not present its pixel minimum: width={width} expected={expected}"));
        }
        let before_rejected =
            serde_json::to_value(&self.workspace.root).map_err(|e| e.to_string())?;
        if self.workspace.root.split(Edge::Top, DockId(3))
            || serde_json::to_value(&self.workspace.root).map_err(|e| e.to_string())?
                != before_rejected
        {
            return Err("rejected split changed the native layout".into());
        }
        let resized_path = path.with_extension("dock-layout.json");
        self.workspace.save(&resized_path)?;
        let reopened = Workspace::load(&resized_path);
        if !reopened.valid_for(self.host.registry())
            || serde_json::to_value(&reopened).map_err(|e| e.to_string())?
                != serde_json::to_value(&self.workspace).map_err(|e| e.to_string())?
        {
            return Err("native resized layout failed to reopen unchanged".into());
        }
        self.capture(main, &path.with_extension("dock-minimum.png"))?;
        self.workspace = original;
        self.redraw(main, event_loop)?;
        if serde_json::to_value(&self.workspace).map_err(|e| e.to_string())? != saved
            || *self.model.borrow() != before_model
            || self.windows[&main].window.inner_size() != size
        {
            return Err(
                "splitter probe changed model, history, original layout or native size".into(),
            );
        }
        Ok(
            serde_json::json!({"pixelMinimum":true,"rejectedSplitUnchanged":true,"savedLayoutReopened":true,"originalLayoutRestored":true,"modelAndHistoryUnchanged":true,"logicalSize":[area.width(),area.height()],"scaleFactor":scale,"minimumWidth":width}),
        )
    }
}
