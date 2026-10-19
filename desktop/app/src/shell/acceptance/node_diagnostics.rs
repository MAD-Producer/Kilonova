//! Exercise a real MissingInput diagnostic through floating-window pointer routing.
use super::*;
use motion_ui::{input::MouseButton, Rect};

fn center(rect: Rect) -> [f32; 2] {
    [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]
}
impl Shell {
    fn diagnostic_click(&mut self, point: [f32; 2]) -> Result<(), String> {
        self.node_float_input(Input {
            mouse: point,
            pressed: Some((MouseButton::Left, point)),
            held: Some(MouseButton::Left),
            ..Default::default()
        })?;
        self.node_float_input(Input {
            mouse: point,
            released: Some((MouseButton::Left, point)),
            ..Default::default()
        })
    }
    fn diagnostic_screenshot(&mut self, name: &str, key: &str) -> Result<(), String> {
        let screenshot = self.capture_node_float()?;
        let path = self
            .smoke
            .as_ref()
            .ok_or("diagnostic smoke report missing")?
            .with_extension(format!("{name}.png"));
        std::fs::copy(screenshot, &path).map_err(|e| e.to_string())?;
        self.smoke_node_report[key] = serde_json::to_value(path).map_err(|e| e.to_string())?;
        Ok(())
    }
    pub(super) fn node_diagnostic_smoke(&mut self) -> Result<(), String> {
        let styles = PaintList::default().styles();
        let snapshot = self.panels.node_editor.snapshot();
        let view = self.panels.node_editor.view;
        let selected = self.panels.node_editor.selected().map(str::to_owned);
        let diagnostic = self
            .panels
            .node_editor
            .diagnostic()
            .ok_or("real diagnostic missing")?
            .clone();
        if diagnostic.code != motion_graph::ErrorCode::MissingInput
            || diagnostic.node.as_deref() != Some("output")
            || diagnostic.port.as_deref() != Some("color")
        {
            return Err(format!("unexpected native failure fixture: {diagnostic}"));
        }
        self.panels.node_editor.view.pan = [-6000., -4000.];
        self.capture_node_float()?;
        let (toggle, locate) = self.panels.node_editor.diagnostic_controls(&styles);
        if locate.is_some() {
            return Err("diagnostic drawer unexpectedly starts open".into());
        }
        self.diagnostic_click(center(toggle))?;
        let (_, locate) = self.panels.node_editor.diagnostic_controls(&styles);
        let locate = locate.ok_or("real diagnostic toggle did not expand details")?;
        self.smoke_node_report["diagnosticDrawerOpened"] = true.into();
        self.diagnostic_screenshot("diagnostic-details", "diagnosticDetailsScreenshot")?;
        self.diagnostic_click(center(locate))?;
        let point = self
            .panels
            .node_editor
            .port_position("output", "color", false)
            .ok_or("locate did not reveal required socket")?;
        let after = self.panels.node_editor.snapshot();
        if self.panels.node_editor.selected() != Some("output")
            || point[1] >= locate.min[1]
            || after.document != snapshot.document
            || after.revision != snapshot.revision
            || after.preview_stamp() != snapshot.preview_stamp()
        {
            return Err(
                "diagnostic locate changed source/history/preview or hid the target".into(),
            );
        }
        self.smoke_node_report["diagnosticLocatedWithoutMutation"] = true.into();
        self.smoke_node_report["diagnosticTarget"] = serde_json::json!({"node": diagnostic.node, "port":diagnostic.port,"code":diagnostic.code,"message":diagnostic.message,"socket":point});
        self.diagnostic_screenshot("diagnostic-located", "diagnosticLocatedScreenshot")?;
        let retained = self.engine.node_capture(
            0.,
            self.smoke
                .as_ref()
                .unwrap()
                .with_extension("diagnostic-retained.png"),
        )?;
        if retained["fingerprint"] != self.smoke_node_report["parameter"]["fingerprint"] {
            return Err("diagnostic navigation changed last valid GPU pixels".into());
        }
        self.smoke_node_report["diagnosticDrawerKeptValidPixels"] = true.into();
        self.node_float_input(Input {
            keys: vec![(motion_ui::input::Key::Escape, Default::default())],
            ..Default::default()
        })?;
        if self
            .panels
            .node_editor
            .diagnostic_controls(&styles)
            .1
            .is_some()
        {
            return Err("Escape did not close diagnostic drawer".into());
        }
        self.smoke_node_report["diagnosticDrawerClosed"] = true.into();
        self.diagnostic_screenshot("diagnostic-closed", "diagnosticClosedScreenshot")?;
        self.panels.node_editor.view = view;
        self.panels.node_editor.select(selected.as_deref(), false);
        Ok(())
    }
}
