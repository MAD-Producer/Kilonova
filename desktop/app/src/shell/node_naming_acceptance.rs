//! Exercise native rename menus and text input against a disposable editor.
use crate::*;
use motion_ui::{input::MouseButton, Rect};

fn key(key: Key) -> Input {
    Input {
        keys: vec![(key, Default::default())],
        ..Default::default()
    }
}
fn text(value: &str) -> Input {
    Input {
        text: value.into(),
        ..Default::default()
    }
}
fn right(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        pressed: Some((MouseButton::Right, point)),
        ..Default::default()
    }
}

impl Shell {
    fn node_naming_frame(
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
                panels::Action::Node(node_editor::Action::Changed {
                    semantic: false, ..
                }) => {}
                panels::Action::Node(action @ node_editor::Action::ViewPreferencesChanged(_)) => {
                    self.node_action(action)?
                }
                _ => return Err(format!(
                    "node view input emitted an unexpected graph or project action during {suffix}"
                )),
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

    fn naming_preview(&mut self) -> Result<(), String> {
        let snapshot = self.panels.node_editor.snapshot();
        let compiled = snapshot.compile_preview().map_err(|e| e.to_string())?;
        let report = compiled.report.clone();
        self.engine.node_expect(snapshot.preview_stamp())?;
        if !self
            .engine
            .node_install(snapshot.clone(), compiled)?
            .recv_timeout(Duration::from_secs(30))
            .map_err(|e| format!("naming preview install: {e}"))??
        {
            return Err("naming preview installation was superseded".into());
        }
        self.panels
            .node_editor
            .accept_compile(&snapshot, Ok(report));
        self.panels.node_preview.active = Some(snapshot.preview_stamp());
        Ok(())
    }

    fn naming_node_point(&mut self, floating: bool, id: &str) -> Result<[f32; 2], String> {
        let panel = if floating {
            let f = self
                .floating
                .values()
                .find(|f| f.panel == dock::Panel::Nodes)
                .ok_or("node naming float missing")?;
            let size = f.window.inner_size();
            Rect::new(
                0.,
                30.,
                size.width as f32 / f.scale,
                (size.height as f32 / f.scale - 52.).max(1.),
            )
        } else {
            panels::frames(
                &self.workspace.root,
                self.size().ok_or("node naming window missing")?,
            )
            .into_iter()
            .find(|f| f.panel == dock::Panel::Nodes)
            .ok_or("node naming panel missing")?
            .content
        };
        let body = Rect::new(
            panel.min[0],
            panel.min[1] + 28.,
            panel.width(),
            panel.height() - 28.,
        );
        let canvas = self
            .panels
            .node_editor
            .layout(node_preview::regions(body).0)
            .canvas;
        let node = self
            .panels
            .node_editor
            .document()
            .nodes
            .iter()
            .find(|n| n.id == id)
            .ok_or("node naming target missing")?;
        // A minimum-height dock cannot legibly frame the entire demonstration.
        // Set up a normal panned view of the target before testing real input;
        // do not mistake an overlapping wire for the target's context menu.
        let position = node.position;
        self.panels.node_editor.view.zoom = 0.85;
        self.panels.node_editor.view.pan = [
            ((canvas.width() - 204. * 0.85) * 0.5).max(12.) - position[0] * 0.85,
            8. - position[1] * 0.85,
        ];
        let point = self
            .panels
            .node_editor
            .view
            .world_to_screen([position[0] + 70., position[1] + 14.], canvas);
        if !canvas.contains(point) {
            return Err("node naming header outside canvas".into());
        }
        Ok(point)
    }

    pub(super) fn node_naming_acceptance(&mut self, floating: bool) -> Result<Value, String> {
        let project = self.engine.state()?["revision"].clone();
        let original_graph = self.panels.node_editor.snapshot().document;
        let mut scratch = node_editor::NodeEditor::new(original_graph.clone());
        scratch.view_preferences = self.panels.node_editor.view_preferences;
        let original = std::mem::replace(&mut self.panels.node_editor, scratch);
        let result = (|| {
            self.naming_preview()?;
            self.panels.focused = Some(dock::Panel::Nodes);
            self.panels.node_editor.focus_panel();
            self.node_naming_frame(floating, key(Key::Home), "naming-initial")?;
            let before = self.panels.node_editor.snapshot();
            let shader = before.compile().map_err(|e| e.to_string())?.source;
            let point = self.naming_node_point(floating, "fill")?;
            let node_menu = self.node_naming_frame(floating, right(point), "naming-node-menu")?;
            let node_edit =
                self.node_naming_frame(floating, key(Key::Enter), "naming-node-edit")?;
            if !self.panels.node_editor.popup_open() {
                return Err("native rename did not open".into());
            }
            let mut ime = key(Key::Enter);
            ime.text = "球体填充".into();
            ime.ime_consumed = true;
            self.node_naming_frame(floating, ime, "naming-ime")?;
            if !self.panels.node_editor.popup_open()
                || self.panels.node_editor.snapshot().document != before.document
            {
                return Err("IME confirmation committed a node name".into());
            }
            let renamed =
                self.node_naming_frame(floating, key(Key::Enter), "naming-node-renamed")?;
            let snapshot = self.panels.node_editor.snapshot();
            if snapshot
                .document
                .nodes
                .iter()
                .find(|n| n.id == "fill")
                .map(|n| n.display_name.as_str())
                != Some("球体填充")
                || snapshot.preview_stamp() != before.preview_stamp()
                || snapshot.compile().map_err(|e| e.to_string())?.source != shader
            {
                return Err("native node rename changed identity, shader or preview".into());
            }
            self.panels.node_editor.select(Some("circle"), false);
            self.panels.node_editor.select(Some("fill"), true);
            self.panels
                .node_editor
                .group_selected()
                .map_err(|e| e.to_string())?;
            let id = self
                .panels
                .node_editor
                .selected()
                .ok_or("group selection missing")?
                .to_owned();
            self.naming_preview()?;
            self.node_naming_frame(floating, key(Key::Home), "naming-group-initial")?;
            let point = self.naming_node_point(floating, &id)?;
            self.node_naming_frame(floating, right(point), "naming-group-menu-before")?;
            self.node_naming_frame(floating, key(Key::Enter), "naming-group-edit")?;
            self.node_naming_frame(floating, text("Soft Sphere"), "naming-group-text")?;
            self.node_naming_frame(floating, key(Key::Enter), "naming-group-renamed")?;
            let group_menu = self.node_naming_frame(floating, right(point), "naming-group-menu")?;
            let mut ports_key = key(Key::Enter);
            ports_key.keys.insert(0, (Key::Down, Default::default()));
            ports_key.keys.insert(0, (Key::Down, Default::default()));
            self.node_naming_frame(floating, ports_key, "naming-port-open")?;
            let port_menu =
                self.node_naming_frame(floating, Input::default(), "naming-port-menu")?;
            let before = self.panels.node_editor.snapshot();
            let node = before
                .document
                .nodes
                .iter()
                .find(|n| n.id == id)
                .ok_or("renamed group missing")?;
            let ports = motion_graph::node_ports(node, &before.document.public_parameters)
                .map_err(|e| e.to_string())?;
            let input = ports
                .inputs
                .keys()
                .next()
                .ok_or("group input missing")?
                .clone();
            let port_edit =
                self.node_naming_frame(floating, key(Key::Enter), "naming-port-edit")?;
            self.node_naming_frame(floating, text("Position"), "naming-port-text")?;
            let group_renamed =
                self.node_naming_frame(floating, key(Key::Enter), "naming-group-result")?;
            let snapshot = self.panels.node_editor.snapshot();
            let node = snapshot
                .document
                .nodes
                .iter()
                .find(|n| n.id == id)
                .ok_or("renamed group missing")?;
            if motion_graph::group_port(&node.operation, &input, false).map(|p| p.name.as_str())
                != Some("Position")
                || snapshot.document.connections != before.document.connections
                || snapshot.preview_stamp() != before.preview_stamp()
                || snapshot.compile().map_err(|e| e.to_string())?.source
                    != before.compile().map_err(|e| e.to_string())?.source
            {
                return Err("native port rename changed keys, shader, wires or preview".into());
            }
            let mode = if floating { "float" } else { "main" };
            let saved = self
                .smoke
                .as_ref()
                .ok_or("smoke path missing")?
                .with_extension(format!("naming-{mode}.msgraph"));
            snapshot.save(&saved).map_err(|e| e.to_string())?;
            if motion_graph::GraphDocument::load(&saved).map_err(|e| e.to_string())?
                != snapshot.document
            {
                return Err("names did not survive graph save/reopen".into());
            }
            if self.engine.state()?["revision"] != project {
                return Err("naming probe changed project".into());
            }
            Ok(
                serde_json::json!({"nodeMenu":node_menu,"nodeEdit":node_edit,"renamedNode":renamed,"groupMenu":group_menu,"portMenu":port_menu,"portEdit":port_edit,"renamedGroup":group_renamed,"imeConfirmation":true,"keysAndWiresPreserved":true,"previewUnchanged":true,"saveReopen":true,"projectUntouched":true}),
            )
        })();
        self.panels.node_editor = original;
        let restored = self.naming_preview();
        match (result, restored) {
            (Err(error), Err(restore)) => Err(format!(
                "{error}; restoring node preview also failed: {restore}"
            )),
            (Err(error), _) | (_, Err(error)) => Err(error),
            (Ok(mut value), Ok(())) => {
                if self.panels.node_editor.snapshot().document != original_graph {
                    return Err("naming probe did not restore graph".into());
                }
                value["sourceRestored"] = Value::Bool(true);
                Ok(value)
            }
        }
    }
}
