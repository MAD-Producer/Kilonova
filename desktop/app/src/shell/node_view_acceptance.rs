//! Native main-window and floating-window checks, run by --node-smoke.
use crate::*;
use motion_ui::Rect;

impl Shell {
    fn node_view_frame(
        &mut self,
        floating: bool,
        input: Input,
        suffix: &str,
    ) -> Result<PathBuf, String> {
        let (size, scale, key) = if floating {
            let f = self
                .floating
                .values()
                .find(|f| f.panel == dock::Panel::Nodes)
                .ok_or("node float missing")?;
            let s = f.window.inner_size();
            (
                [s.width as f32 / f.scale, s.height as f32 / f.scale],
                f.scale,
                f.key,
            )
        } else {
            (self.size().ok_or("main window missing")?, self.scale, 0)
        };
        let (paint, actions) = if floating {
            let mut paint = PaintList::default();
            let actions = panels::floating_chrome(
                &mut paint,
                &mut self.atlas,
                dock::Panel::Nodes,
                size,
                &mut self.panels,
                &self.state,
                &input,
            );
            (paint, actions)
        } else {
            let previous = std::mem::replace(&mut self.input, input);
            let result = self.draw_commands(size);
            self.input = previous;
            result
        };
        for action in actions {
            match action {
                panels::Action::Node(action @ node_editor::Action::ViewPreferencesChanged(_)) => {
                    self.node_action(action)?
                }
                _ => return Err("node view input emitted a graph or project action".into()),
            }
        }
        let config = self.atlas.config();
        let ui = UiFrame {
            paint,
            atlas: Some(AtlasUpload {
                pixels: self.atlas.pixel_data().to_vec(),
                width: config.width,
                height: config.height,
            }),
            logical_size: size,
            window: key,
            composition_image: None,
            composition: None,
            node_preview: if floating {
                self.node_viewport(panels::floating_node_view(dock::Panel::Nodes, size), scale)
            } else {
                self.node_viewport(panels::node_view(&self.workspace.root, size), scale)
            },
        };
        let mode = if floating { "float" } else { "main" };
        let path = self
            .smoke
            .as_ref()
            .ok_or("smoke report missing")?
            .with_extension(format!("node-view-{mode}-{suffix}.png"));
        self.engine.capture_ui(ui, path.clone())?;
        Ok(path)
    }

    pub(super) fn node_view_acceptance(&mut self, floating: bool) -> Result<Value, String> {
        let saved = self.panels.node_editor.view_preferences;
        let view = self.panels.node_editor.view;
        let preferences = self.workspace.node_view;
        let result = (|| {
            let stamp = self.panels.node_editor.stamp();
            let graph = self.panels.node_editor.snapshot().document;
            let project = self.engine.state()?["revision"].clone();
            self.panels.focused = Some(dock::Panel::Nodes);
            self.panels.node_editor.focus_panel();
            self.panels.node_editor.view_preferences = node_editor::ViewPreferences::default();
            let key = |key| Input {
                keys: vec![(key, Default::default())],
                ..Default::default()
            };
            let overview = self.node_view_frame(floating, key(Key::N), "overview")?;
            if !self.panels.node_editor.view_preferences.minimap {
                return Err("N did not enable minimap in native node panel".into());
            }
            let no_grid = self.node_view_frame(floating, key(Key::G), "no-grid")?;
            if self.panels.node_editor.view_preferences.grid {
                return Err("G did not hide node grid in native node panel".into());
            }
            let panel = if floating {
                let f = self
                    .floating
                    .values()
                    .find(|f| f.panel == dock::Panel::Nodes)
                    .ok_or("node float missing")?;
                let size = f.window.inner_size();
                Rect::new(
                    0.,
                    30.,
                    size.width as f32 / f.scale,
                    (size.height as f32 / f.scale - 52.).max(1.),
                )
            } else {
                panels::frames(&self.workspace.root, self.size().ok_or("window missing")?)
                    .into_iter()
                    .find(|f| f.panel == dock::Panel::Nodes)
                    .ok_or("main node panel missing")?
                    .content
            };
            let body = Rect::new(
                panel.min[0],
                panel.min[1] + 28.,
                panel.width(),
                panel.height() - 28.,
            );
            let editor = if self.panels.node_editor.layer_target.is_some() {
                body
            } else {
                node_preview::regions(body).0
            };
            let canvas = self.panels.node_editor.layout(editor).canvas;
            // Make a genuinely empty canvas target, including in the minimum
            // window where the fitted nodes can cover the original test point.
            let minimum = graph
                .nodes
                .iter()
                .fold([f32::INFINITY; 2], |minimum, node| {
                    std::array::from_fn(|axis| minimum[axis].min(node.position[axis]))
                });
            if minimum.iter().all(|v| v.is_finite()) {
                self.panels.node_editor.view.pan = [
                    canvas.width() * 0.25 - minimum[0] * self.panels.node_editor.view.zoom,
                    canvas.height() * 0.5 - minimum[1] * self.panels.node_editor.view.zoom,
                ];
            }
            let point = [canvas.min[0] + 12., canvas.min[1] + 12.];
            self.node_view_frame(
                floating,
                Input {
                    mouse: point,
                    pressed: Some((motion_ui::input::MouseButton::Right, point)),
                    ..Default::default()
                },
                "menu-open",
            )?;
            if !self.panels.node_editor.popup_open() {
                return Err("node canvas context menu did not open".into());
            }
            let menu = self.node_view_frame(
                floating,
                Input {
                    mouse: point,
                    ..Default::default()
                },
                "menu",
            )?;
            self.node_view_frame(floating, key(Key::Escape), "menu-cancelled")?;
            if self.panels.node_editor.popup_open() {
                return Err("Escape did not close node view menu".into());
            }
            self.node_view_frame(
                floating,
                Input {
                    mouse: point,
                    pressed: Some((motion_ui::input::MouseButton::Right, point)),
                    ..Default::default()
                },
                "menu-reopen",
            )?;
            self.node_view_frame(
                floating,
                Input {
                    keys: vec![
                        (Key::Down, Default::default()),
                        (Key::Down, Default::default()),
                        (Key::Enter, Default::default()),
                    ],
                    ..Default::default()
                },
                "menu-toggled",
            )?;
            if !self.panels.node_editor.view_preferences.grid
                || !self.panels.node_editor.view_preferences.minimap
                || self.panels.node_editor.popup_open()
            {
                return Err("canvas menu did not toggle grid without changing minimap".into());
            }
            if self.panels.node_editor.stamp() != stamp
                || self.panels.node_editor.snapshot().document != graph
                || self.engine.state()?["revision"] != project
            {
                return Err("node view toggles changed model or compile revision".into());
            }
            if workspace::Workspace::load(&self.layout_path).node_view
                != self.panels.node_editor.view_preferences
            {
                return Err("node view preference did not survive workspace reload".into());
            }
            Ok(
                serde_json::json!({"overview":overview,"noGrid":no_grid,"contextMenu":menu,
                "projectAndCompileUnchanged":true,"workspaceReload":true,"menuCancelled":true,"menuToggled":true}),
            )
        })();
        self.panels.node_editor.cancel_interaction();
        self.panels.node_editor.view_preferences = saved;
        self.panels.node_editor.view = view;
        self.workspace.node_view = preferences;
        let cleanup = self.workspace.save(&self.layout_path);
        match (result, cleanup) {
            (Err(reason), Err(cleanup)) => Err(format!(
                "{reason}; restoring node view preferences also failed: {cleanup}"
            )),
            (Err(reason), _) | (_, Err(reason)) => Err(reason),
            (Ok(record), Ok(())) => Ok(record),
        }
    }
}
