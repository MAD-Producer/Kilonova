//! U/UU captures use real shortcut routing and the attached native GPU surface.
use super::*;

impl Shell {
    pub(crate) fn capture_modified_property_screens(
        &mut self,
        window_size: [f32; 2],
        report: &std::path::Path,
        output: &mut serde_json::Map<String, Value>,
    ) -> Result<(), String> {
        let before = self.engine.state()?;
        let mut project =
            motion_core::Project::new(640, 360, 30, 120).map_err(|e| e.to_string())?;
        let mut layer = Layer::solid(
            7,
            if self.workspace.locale == i18n::Locale::Zh {
                "已修改属性"
            } else {
                "Modified properties"
            },
            [100.; 2],
            [320., 180., 0.],
            [1.; 4],
        );
        layer.transform.rotation.value = [0., 0., 12.];
        layer
            .transform
            .opacity
            .upsert(0, 1., Ease::Linear)
            .map_err(|e| e.to_string())?;
        project.layers.push(layer);
        let engine = motion_core::Engine::new(project).map_err(|e| e.to_string())?;
        let mut state = before.clone();
        state["project"] = serde_json::to_value(engine.project()).map_err(|e| e.to_string())?;
        state["timeline_layers"] = serde_json::to_value(engine.project().timeline_layers(0.).unwrap())
            .map_err(|e| e.to_string())?;
        state["sampledLayers"] = serde_json::json!([{"id":7,"anchor":[0.5,0.5],"position":[320.,180.,0.],
            "scale":[100.,100.,100.],"rotation":[0.,0.,12.],"opacity":1.,"active":true,"three_d":false}]);
        state["frame"] = 0.into();
        state["observing"] = false.into();
        for panel in [Panel::Timeline, Panel::Properties] {
            for floating in [false, true] {
                let size = if panel == Panel::Timeline {
                    [1000., 520.]
                } else {
                    [400., 560.]
                };
                let mut panels = panels::Panels {
                    locale: self.workspace.locale,
                    styles: self.panels.styles.clone(),
                    focused: Some(panel),
                    ..Default::default()
                };
                panels.select_only(Some(7));
                panels
                    .animation_controls
                    .begin_row_probe(7, Property::Rotation, None);
                let mut captures = vec![];
                for (index, mode) in ["animated", "modified"].into_iter().enumerate() {
                    let input = Input {
                        keys: vec![(Key::U, Modifiers::default())],
                        ..Default::default()
                    };
                    let mut paint = PaintList::default();
                    paint.stylesheet = panels.styles.clone();
                    let actions = if floating {
                        panels::floating_chrome(
                            &mut paint,
                            &mut self.atlas,
                            panel,
                            size,
                            &mut panels,
                            &state,
                            &input,
                        )
                    } else {
                        panels::chrome(
                            &mut paint,
                            &mut self.atlas,
                            size,
                            &dock::Node::dock(panel.dock()),
                            &mut panels,
                            &state,
                            &input,
                        )
                    };
                    if !actions.is_empty() {
                        return Err(format!("U/UU capture emitted actions: {actions:?}"));
                    }
                    if panels.animation_controls.probed_row(None).is_some() != (index == 1) {
                        return Err(format!("{panel:?} floating={floating} {mode}: rotation visibility differs from authored filter"));
                    }
                    let name = format!(
                        "{}-{}-{mode}",
                        if panel == Panel::Timeline {
                            "timeline"
                        } else {
                            "properties"
                        },
                        if floating { "floating" } else { "main" }
                    );
                    captures.push((name, paint));
                }
                // GPU readback and disk IO must not consume the double-key
                // interval between the two real routed input frames.
                for (name, paint) in captures {
                    let path = report.with_extension(format!("uu-{name}.png"));
                    let mut ui = self.ui_frame(paint, window_size, true);
                    ui.composition = None;
                    ui.node_preview = None;
                    self.engine.capture_ui(ui, path.clone())?;
                    let pixels = image::open(&path).map_err(|e| e.to_string())?;
                    let width = (size[0] * pixels.width() as f32 / window_size[0]).round() as u32;
                    let height = (size[1] * pixels.height() as f32 / window_size[1]).round() as u32;
                    pixels
                        .crop_imm(0, 0, width, height)
                        .save(&path)
                        .map_err(|e| e.to_string())?;
                    output.insert(format!("uu-{name}"), serde_json::json!(path));
                }
            }
        }
        if self.engine.state()?["project"] != before["project"] {
            return Err("U/UU view-only captures changed the project".into());
        }
        Ok(())
    }
}
