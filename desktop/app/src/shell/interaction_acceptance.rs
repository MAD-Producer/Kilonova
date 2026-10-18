//! Capture the actual path editor and a translated timeline through native drawing.
use crate::*;
use motion_core::{vector::{PathNode, VectorPath}, Track};

impl Shell {
    pub(crate) fn capture_interaction_regressions(&mut self, window_size: [f32; 2]) -> Result<serde_json::Map<String, Value>, String> {
        let before = self.engine.state()?;
        let object = before["project"]["layers"].as_array().into_iter().flatten()
            .filter_map(|layer| layer["id"].as_u64()).max().unwrap_or(0)
            .checked_add(1).ok_or("interaction fixture object ID exhausted")?;
        self.engine.history(2)?;
        let saved_panels = std::mem::take(&mut self.panels);
        let saved_root = self.workspace.root.clone();
        self.panels.styles = saved_panels.styles.clone();
        self.panels.locale = saved_panels.locale;
        let captured = (|| {
            let mut layer = Layer::solid(object, "Bezier", [200.; 2],
                [before["project"]["width"].as_f64().ok_or("fixture width missing")? as f32 * 0.5,
                 before["project"]["height"].as_f64().ok_or("fixture height missing")? as f32 * 0.5, 0.], [1.; 4]);
            layer.content = motion_core::Content::Vector { vector: panels::path_controls::new_vector(VectorPath {
                id: 1, closed: false,
                nodes: [[0., 0., -120., -30., 120., 30.], [250., 100., 0., 0., 0., 0.]]
                    .into_iter().enumerate().map(|(index, geometry)| PathNode {
                        id: index as u64 + 1, geometry: Track::constant(geometry),
                    }).collect(),
            }) };
            self.state = self.engine.edit(serde_json::to_string(&Command::Add { layer }).map_err(|e| e.to_string())?)?;
            let size = [900., 520.];
            let mut screens = serde_json::Map::new();
            self.panels.select_only(Some(object));
            self.panels.focused = Some(dock::Panel::Composition);
            self.panels.path_controls.tool = panels::path_controls::Tool::Pen;
            self.workspace.root = dock::Node::dock(dock::Panel::Composition.dock());
            let styles = self.panels.styles.clone().unwrap_or_else(motion_ui::style::default_stylesheet_arc);
            let view = panels::composition_view_styled(&self.workspace.root, size, &styles);
            let projection = panels::path_controls::layer_projection(&self.state, object, view).ok_or("fixture projection unavailable")?;
            let node = projection.screen([0.; 2]).ok_or("fixture node is outside projection")?;
            // Use real picking to select the node; no alternate hover or painting implementation.
            for input in [Input { mouse: node, pressed: Some((motion_ui::MouseButton::Left, node)), ..Default::default() },
                Input { mouse: node, released: Some((motion_ui::MouseButton::Left, node)), ..Default::default() }] {
                let mut actions = vec![];
                self.panels.path_controls.draw_in_viewport(&mut PaintList::default(), &mut self.atlas, view, view, &self.state,
                    Some(object), &input, self.panels.locale, &mut actions);
                if !actions.is_empty() { return Err("path selection capture unexpectedly edited data".into()); }
            }
            for (name, position, locked) in [
                ("path-hover-incoming", [-120., -30.], false),
                ("path-hover-outgoing", [120., 30.], false),
                ("path-hover-locked", [0., 0.], true),
            ] {
                if locked {
                    self.state = self.engine.edit(serde_json::to_string(&Command::Flags { object, visible: true, locked }).map_err(|e| e.to_string())?)?;
                }
                let mouse = projection.screen(position).ok_or("fixture grip is outside projection")?;
                let mut paint = PaintList::default();
                let actions = panels::chrome(&mut paint, &mut self.atlas, size, &self.workspace.root,
                    &mut self.panels, &self.state, &Input { mouse, ..Default::default() });
                if !actions.is_empty() { return Err(format!("{name} capture unexpectedly edited data")); }
                let mut frame = self.ui_frame(paint, window_size, true);
                frame.node_preview = None;
                frame.composition = Some([view.min[0] * self.scale, view.min[1] * self.scale,
                    view.width() * self.scale, view.height() * self.scale]);
                frame.composition_image = Some([projection.image.min[0] * self.scale, projection.image.min[1] * self.scale,
                    projection.image.width() * self.scale, projection.image.height() * self.scale]);
                let path = self.smoke.as_ref().ok_or("interaction report missing")?.with_extension(format!("{name}.png"));
                self.engine.capture_ui(frame, path.clone())?;
                Self::crop_interaction_capture(&path, size, window_size)?;
                screens.insert(name.into(), serde_json::json!(path));
            }
            self.workspace.root = dock::Node::Split {
                edge: dock::Edge::Left, split: 0.28,
                first: Box::new(dock::Node::dock(dock::Panel::Properties.dock())),
                second: Box::new(dock::Node::dock(dock::Panel::Timeline.dock())),
            };
            let mut canceled = vec![];
            self.panels.path_controls.choose(panels::path_controls::Tool::Select, &mut canceled);
            if !canceled.is_empty() { return Err("timeline fixture canceled an unexpected gesture".into()); }
            self.panels.focused = Some(dock::Panel::Timeline);
            let mut paint = PaintList::default();
            let actions = panels::chrome(&mut paint, &mut self.atlas, size, &self.workspace.root,
                &mut self.panels, &self.state, &Input::default());
            if !actions.is_empty() { return Err("translated timeline capture unexpectedly edited data".into()); }
            let frame = self.ui_frame(paint, window_size, true);
            let path = self.smoke.as_ref().ok_or("interaction report missing")?.with_extension("timeline-translated.png");
            self.engine.capture_ui(frame, path.clone())?;
            Self::crop_interaction_capture(&path, size, window_size)?;
            screens.insert("timeline-translated".into(), serde_json::json!(path));
            Ok(screens)
        })();
        let restored = self.engine.history(4);
        self.panels = saved_panels;
        self.workspace.root = saved_root;
        match (captured, restored) {
            (Ok(screens), Ok(state)) => {
                self.state = state;
                if self.state["project"] != before["project"] { return Err("interaction capture did not restore project".into()); }
                Ok(screens)
            }
            (Err(error), Err(cleanup)) => {
                log::error!("interaction capture rollback failed: {cleanup}; primary failure: {error}");
                Err(error)
            }
            (Err(error), Ok(state)) => { self.state = state; Err(error) }
            (Ok(_), Err(error)) => Err(error),
        }
    }

    fn crop_interaction_capture(path: &std::path::Path, size: [f32; 2], window_size: [f32; 2]) -> Result<(), String> {
        let pixels = image::open(path).map_err(|error| error.to_string())?;
        let width = (size[0] * pixels.width() as f32 / window_size[0]).round() as u32;
        let height = (size[1] * pixels.height() as f32 / window_size[1]).round() as u32;
        if width > pixels.width() || height > pixels.height() {
            return Err("interaction fixture exceeds the native window".into());
        }
        pixels.crop_imm(0, 0, width, height).save(path).map_err(|error| error.to_string())
    }
}
