//! Native GPU evidence for the temporary layout projection and floating rule.
use crate::*;
use motion_ui::dock::Panel;

fn backquote(input: &mut Input) {
    let key = translate_key(&WinitKey::Character("`".into())).expect("backquote adapter");
    motion_ui::ui::accumulate(
        input,
        Event::KeyPressed {
            key,
            modifiers: Modifiers::default(),
        },
    );
    motion_ui::ui::accumulate(input, Event::TextInput("`".into()));
}

impl Shell {
    pub(crate) fn panel_maximize_probe(&mut self, size: [f32; 2]) -> Result<Value, String> {
        let layout = serde_json::to_value(&self.workspace).map_err(|e| e.to_string())?;
        let project = self.engine.state()?;
        let input = std::mem::take(&mut self.input);
        let paint_input = self.paint_input.clone();
        let focus = self.panels.focused;
        let result = (|| {
            let mut screens = vec![];
            for panel in [Panel::Composition, Panel::Timeline] {
                let frame = panels::frames(self.workspace.display_root(), size)
                    .into_iter()
                    .find(|f| f.panel == panel)
                    .ok_or("maximize target not visible")?;
                self.input = Input {
                    mouse: [frame.content.min[0] + 20., frame.content.min[1] + 20.],
                    ..Default::default()
                };
                backquote(&mut self.input);
                let (paint, actions) = self.draw_commands(size);
                if !actions.is_empty() || self.workspace.maximized_panel() != Some(panel.dock()) {
                    return Err(format!("maximize input did not project {panel:?}"));
                }
                let ui = self.ui_frame(paint, size, true);
                if panels::frames(self.workspace.display_root(), size).len() != 1 {
                    return Err("maximized layout still includes other docks".into());
                }
                if panel == Panel::Composition && ui.composition.is_none()
                    || panel == Panel::Timeline && ui.composition.is_some()
                {
                    return Err("maximized GPU viewport differs from the drawn panel".into());
                }
                let path = self
                    .smoke
                    .as_ref()
                    .unwrap()
                    .with_extension(format!("maximize-{}.png", panel.dock().0));
                self.engine.capture_ui(ui, path.clone())?;
                screens.push(path);
                self.input = Input::default();
                backquote(&mut self.input);
                let (paint, actions) = self.draw_commands(size);
                if !actions.is_empty() || self.workspace.maximized_panel().is_some() {
                    return Err("second backquote did not restore layout".into());
                }
                let restored = self.ui_frame(paint, size, true);
                let path = self
                    .smoke
                    .as_ref()
                    .unwrap()
                    .with_extension(format!("maximize-{}-restored.png", panel.dock().0));
                self.engine.capture_ui(restored, path.clone())?;
                screens.push(path);
            }
            let after = self.engine.state()?;
            if serde_json::to_value(&self.workspace).map_err(|e| e.to_string())? != layout
                || ["project", "revision", "history"]
                    .iter()
                    .any(|key| after[*key] != project[*key])
            {
                return Err("temporary maximize changed persistent layout or project".into());
            }
            Ok(
                serde_json::json!({"restored":true,"layoutAndProjectUntouched":true,"screenshots":screens}),
            )
        })();
        self.workspace.restore_panel();
        self.input = input;
        self.paint_input = paint_input;
        self.panels.focused = focus;
        result
    }

    pub(crate) fn panel_maximize_float_probe(
        &mut self,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), String> {
        let (id, key, window) = self
            .floating
            .iter()
            .find(|(_, f)| f.panel == Panel::Composition)
            .map(|(id, f)| (*id, f.key, f.window.clone()))
            .ok_or("composition float missing")?;
        if let Some(receive) = self.rendering.remove(&key) {
            receive
                .recv_timeout(Duration::from_secs(10))
                .map_err(|e| e.to_string())??;
        }
        let before = (window.inner_size(), window.is_maximized());
        let layout = serde_json::to_value(&self.workspace).map_err(|e| e.to_string())?;
        let focus = self.panels.focused;
        let f = self.floating.get_mut(&id).unwrap();
        let input = std::mem::take(&mut f.input);
        let paint_input = f.paint_input.clone();
        backquote(&mut f.input);
        let result = (|| {
            self.present_float(id, event_loop)?;
            let f = self.floating.get(&id).ok_or("float closed by backquote")?;
            if f.input.pending || !f.input.keys.is_empty() {
                return Err("floating maximize probe did not consume an interaction frame".into());
            }
            let receive = self
                .rendering
                .remove(&key)
                .ok_or("floating maximize probe did not present")?;
            if !receive
                .recv_timeout(Duration::from_secs(10))
                .map_err(|e| e.to_string())??
            {
                return Err("floating maximize probe surface unavailable".into());
            }
            if (window.inner_size(), window.is_maximized()) != before
                || serde_json::to_value(&self.workspace).map_err(|e| e.to_string())? != layout
            {
                return Err("backquote changed native floating geometry or ownership".into());
            }
            self.capture_panel_float_named(Panel::Composition, "-backquote")?;
            Ok(())
        })();
        if let Some(f) = self.floating.get_mut(&id) {
            f.input = input;
            f.paint_input = paint_input;
        }
        self.panels.focused = focus;
        result
    }
}
