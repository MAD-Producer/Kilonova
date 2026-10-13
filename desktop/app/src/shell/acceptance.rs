use crate::*;

impl Shell {
    pub(crate) fn panel_smoke_step(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        if self.smoke_stage != 3 || self.smoke_frames < 6 {
            return Ok(());
        }
        if let Some(panel) = self.smoke_panel_current {
            if self
                .smoke_float_frames
                .get(&(panel as u32))
                .copied()
                .unwrap_or(0)
                < 2
            {
                return Ok(());
            }
            if matches!(panel, dock::Panel::Nodes | dock::Panel::NodePreview)
                && self.panels.node_preview.active.is_none()
            {
                return Ok(());
            }
            let revision = self.state["revision"].clone();
            let screenshot = self.capture_panel_float(panel)?;
            let value_graph = if panel == dock::Panel::Timeline {
                let previous = self.panels.timeline.clone();
                self.panels.timeline.set_graph_mode(true);
                let graph = self.capture_panel_float(panel);
                self.panels.timeline = previous;
                Some(graph?)
            } else {
                None
            };
            self.redock_panel(panel.dock())?;
            if !self.workspace.valid() {
                return Err("panel lost unique workspace ownership".into());
            }
            if self.state["revision"] != revision {
                return Err("layout change edited the project".into());
            }
            self.smoke_panel_records.push(serde_json::json!({"id":panel as u32,"title":self.workspace.locale.panel(panel.dock()),"frames":self.smoke_float_frames[&(panel as u32)],"screenshot":screenshot,"valueGraphScreenshot":value_graph,"projectRevision":revision}));
            self.smoke_panel_current = None;
            self.smoke_panel_index += 1;
        } else if let Some(panel) = dock::Panel::ALL.get(self.smoke_panel_index).copied() {
            self.action(panels::Action::ShowPanel(panel.dock()), Some(event_loop))?;
            if panel == dock::Panel::Vector {
                self.action(
                    panels::Action::AddShape(motion_core::vector::ShapeKind::Rectangle),
                    Some(event_loop),
                )?;
            }
            self.float_panel(panel.dock(), event_loop)?;
            self.smoke_float_frames.insert(panel as u32, 0);
            self.smoke_panel_current = Some(panel);
        } else {
            let locale = self.workspace.locale;
            self.workspace = workspace::Workspace::default();
            self.workspace.locale = locale;
            self.finish_smoke()?;
            self.smoke_stage = 4;
        }
        Ok(())
    }
    fn capture_panel_float(&mut self, panel: dock::Panel) -> Result<PathBuf, String> {
        let f = self
            .floating
            .values()
            .find(|f| f.panel == panel)
            .ok_or("panel window missing")?;
        let physical = f.window.inner_size();
        let scale = f.scale;
        let key = f.key;
        let size = [
            physical.width as f32 / scale,
            physical.height as f32 / scale,
        ];
        let mut paint = PaintList::default();
        panels::floating_chrome(
            &mut paint,
            &mut self.atlas,
            panel,
            size,
            &mut self.panels,
            &self.state,
            &Input::default(),
        );
        let atlas = self.atlas.config();
        let ui = UiFrame {
            paint,
            atlas: Some(AtlasUpload {
                pixels: self.atlas.pixel_data().to_vec(),
                width: atlas.width,
                height: atlas.height,
            }),
            logical_size: size,
            composition: panels::floating_view(panel, size).map(|r| {
                [
                    r.min[0] * scale,
                    r.min[1] * scale,
                    r.width() * scale,
                    r.height() * scale,
                ]
            }),
            window: key,
            node_preview: self.node_viewport(panels::floating_node_view(panel, size), scale),
        };
        let path = self.smoke.as_ref().unwrap().with_extension(format!(
            "panel-{}{}.png",
            panel as u32,
            if panel == dock::Panel::Timeline && self.panels.timeline.graph_mode {
                "-graph"
            } else {
                ""
            }
        ));
        self.engine.capture_ui(ui, path.clone())?;
        Ok(path)
    }
    pub(crate) fn prepare_smoke(&mut self) -> Result<(), String> {
        self.add_solid()?;
        let object = self.panels.selected.unwrap();
        self.panels.timeline.expanded.insert(object);
        let controller_id = self.state["project"]["layers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|l| l["id"].as_u64())
            .max()
            .unwrap_or(0)
            + 10;
        let mut controller = motion_core::Layer::solid(
            controller_id,
            if self.workspace.locale == Locale::Zh {
                "控制器"
            } else {
                "Controller"
            },
            [100.; 2],
            [960., 540., 0.],
            [0.; 4],
        );
        controller.content = motion_core::Content::Null;
        self.state = self.engine.edit(
            serde_json::to_string(&vec![
                Command::Add { layer: controller },
                Command::Animate {
                    object,
                    property: motion_core::Property::Position,
                    axis: None,
                    frame: 0,
                    enabled: true,
                },
                Command::SetVector {
                    object,
                    property: motion_core::Property::Position,
                    frame: 90,
                    value: [1280.0, 540.0, 0.0],
                },
                Command::Curve {
                    object,
                    property: motion_core::Property::Position,
                    axis: None,
                    frame: 0,
                    easing: motion_core::Easing {
                        ease: motion_core::Ease::InOut,
                        curve: Some(motion_core::Curve {
                            space: motion_core::CurveSpace::Progress,
                            shape: motion_core::CurveShape::Cubic {
                                control1: [1. / 3., 0.],
                                control2: [2. / 3., 1.],
                                start: 0.,
                                end: 1.,
                            },
                        }),
                    },
                },
            ])
            .map_err(|e| e.to_string())?,
        )?;
        self.state = self.engine.seek(45.0)?;
        self.frame = 45.0;
        self.state = self.engine.save()?;
        let root = PathBuf::from(self.state["root"].as_str().ok_or("missing project root")?);
        let mut other = Project::new(1920, 1080, 30, 300).map_err(|e| e.to_string())?;
        other.name = "Reopen check".into();
        self.state = self
            .engine
            .new_project(serde_json::to_string(&other).map_err(|e| e.to_string())?)?;
        self.state = self.engine.open_directory(root)?;
        self.state = self.engine.seek(45.0)?;
        Ok(())
    }
    pub(crate) fn finish_smoke(&mut self) -> Result<(), String> {
        let path = self.smoke.as_ref().ok_or("no smoke report path")?.clone();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let size = self.size().ok_or("window has no size")?;
        let (paint, _) = self.draw_commands(size);
        let ui = self.ui_frame(paint, size, true);
        let screenshot = path.with_extension("png");
        self.engine.capture_ui(ui, screenshot.clone())?;
        // Exercise actual surface presentation, not a CPU estimate. UI-only
        // repaint must reuse the submitted composition without executing effects.
        let view = panels::composition_view(&self.workspace.root, size);
        let reuse_probe = if view.width() > 0. && view.height() > 0. {
            let mut reuse_times = vec![];
            let before = self.engine.state()?;
            for index in 0..9 {
                let (paint, _) = self.draw_commands(size);
                let ui = self.ui_frame(paint, size, false);
                let began = Instant::now();
                if !self
                    .engine
                    .present_async(ui)?
                    .recv_timeout(Duration::from_secs(10))
                    .map_err(|e| e.to_string())??
                {
                    return Err("retained UI presentation failed".into());
                }
                if index > 0 {
                    reuse_times.push(began.elapsed().as_micros() as u64);
                }
            }
            let state = self.engine.state()?;
            let reuse_delta = state["desktop"]["uiPerformance"]["compositionReuses"]
                .as_u64()
                .unwrap_or(0)
                - before["desktop"]["uiPerformance"]["compositionReuses"]
                    .as_u64()
                    .unwrap_or(0);
            let render_delta = state["desktop"]["uiPerformance"]["compositionRenders"]
                .as_u64()
                .unwrap_or(0)
                - before["desktop"]["uiPerformance"]["compositionRenders"]
                    .as_u64()
                    .unwrap_or(0);
            if reuse_delta != 8 || render_delta != 1 {
                return Err("UI repaint re-executed the composition".into());
            }
            serde_json::json!({"reuses":reuse_delta,"renders":render_delta,"roundTripUs":reuse_times})
        } else {
            Value::Null
        };
        let design_screens = self.capture_design_screens(size)?;
        let state = self.engine.state()?;
        let styles = self
            .panels
            .styles
            .clone()
            .unwrap_or_else(motion_ui::style::default_stylesheet_arc);
        let style_report = serde_json::json!({"focus":styles.color("focus").0,"panel":styles.color("panel").0,"header":styles.color("header").0,"rowHeight":styles.metric("row.height",22.),"dragThreshold":styles.metric("motion.drag_threshold",3.),"smallFont":styles.metric("font.small",11.),"easing":styles.easing(),"overrideActive":self.style_source.path.is_some()});
        let result = serde_json::json!({"designScreens":design_screens,"style":style_report,"panels":self.smoke_panel_records,"ok":true,"framesPresented":state["presented"],"frame":state["frame"],"preview":state["preview"],
            "retainedUiProbe":reuse_probe,"root":state["root"],"screenshot":screenshot,"floatingFrames":self.smoke_float_frames,"locale":self.workspace.locale,"desktop":state["desktop"],"nodePreview":self.smoke_node_report,"checks":["create_layer","animate","seek","save","reopen","single_surface_preview","chrome","floating_properties","floating_composition","resize","redock"]});
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub(crate) fn capture_node_float(&mut self) -> Result<PathBuf, String> {
        let f = self
            .floating
            .values()
            .find(|f| f.panel == dock::Panel::Nodes)
            .ok_or("node floating window missing")?;
        let physical = f.window.inner_size();
        let scale = f.scale;
        let key = f.key;
        let size = [
            physical.width as f32 / scale,
            physical.height as f32 / scale,
        ];
        let mut paint = PaintList::default();
        let _ = panels::floating_chrome(
            &mut paint,
            &mut self.atlas,
            dock::Panel::Nodes,
            size,
            &mut self.panels,
            &self.state,
            &Input::default(),
        );
        let atlas = self.atlas.config();
        let ui = UiFrame {
            paint,
            atlas: Some(AtlasUpload {
                pixels: self.atlas.pixel_data().to_vec(),
                width: atlas.width,
                height: atlas.height,
            }),
            logical_size: size,
            composition: None,
            window: key,
            node_preview: self
                .node_viewport(panels::floating_node_view(dock::Panel::Nodes, size), scale),
        };
        let path = self.smoke.as_ref().unwrap().with_extension("nodes.png");
        self.engine.capture_ui(ui, path.clone())?;
        Ok(path)
    }
    fn capture_bound_composition(&self, name: &str) -> Result<Value, String> {
        let capture = self.engine.capture()?;
        let source = PathBuf::from(
            capture["path"]
                .as_str()
                .ok_or("composition capture path missing")?,
        );
        let path = self
            .smoke
            .as_ref()
            .unwrap()
            .with_extension(format!("{name}.png"));
        std::fs::copy(source, &path).map_err(|e| e.to_string())?;
        let pixels = image::open(&path).map_err(|e| e.to_string())?.to_rgba8();
        let fingerprint = pixels.as_raw().iter().fold(0xcbf29ce484222325u64, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
        });
        Ok(serde_json::json!({"path":path,"fingerprint":format!("{fingerprint:016x}")}))
    }
    pub(crate) fn node_smoke_step(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let directory = self
            .smoke
            .as_ref()
            .unwrap()
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf();
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        if self.smoke_stage == 3
            && self.smoke_frames >= 6
            && self.panels.node_preview.active == Some(self.panels.node_editor.stamp())
        {
            self.action(
                panels::Action::ShowPanel(dock::Panel::Nodes.dock()),
                Some(event_loop),
            )?;
            self.float_panel(dock::Panel::Nodes.dock(), event_loop)?;
            let before = self.engine.state()?;
            let first = self
                .engine
                .node_capture(0., directory.join("node-clock-zero.png"))?;
            let later = self
                .engine
                .node_capture(0.65, directory.join("node-clock-later.png"))?;
            if first["fingerprint"] == later["fingerprint"] {
                return Err("node Time did not change actual GPU pixels".into());
            }
            self.smoke_node_report = serde_json::json!({"initial":first,"later":later,"projectRevision":before["revision"],"projectEffects":before["project"]["layers"],"clockPixelsChanged":true});
            self.panels
                .node_editor
                .set_parameter_default("color", 0, 0.9)
                .map_err(|d| d.to_string())?;
            self.schedule_node_compile()?;
            self.smoke_stage = 5;
        } else if self.smoke_stage == 5
            && self.smoke_float_frames.get(&10).copied().unwrap_or(0) >= 3
            && self.panels.node_preview.active == Some(self.panels.node_editor.stamp())
        {
            let changed = self
                .engine
                .node_capture(0., directory.join("node-parameter.png"))?;
            if changed["fingerprint"] == self.smoke_node_report["initial"]["fingerprint"] {
                return Err("node parameter did not change actual GPU pixels".into());
            }
            self.smoke_node_report["parameter"] = changed.clone();
            self.smoke_node_report["parameterPixelsChanged"] = Value::Bool(true);
            self.smoke_node_report["floatingScreenshot"] =
                serde_json::to_value(self.capture_node_float()?).map_err(|e| e.to_string())?;
            self.panels.node_editor.select(Some("circle"), false);
            self.panels.node_editor.select(Some("fill"), true);
            self.panels
                .node_editor
                .group_selected()
                .map_err(|error| error.to_string())?;
            self.smoke_node_report["groupId"] =
                serde_json::to_value(self.panels.node_editor.selected())
                    .map_err(|error| error.to_string())?;
            self.schedule_node_compile()?;
            self.smoke_stage = 10;
        } else if self.smoke_stage == 10
            && self.panels.node_preview.active == Some(self.panels.node_editor.stamp())
        {
            let grouped = self.engine.node_capture(
                0.,
                self.smoke.as_ref().unwrap().with_extension("grouped.png"),
            )?;
            if grouped["fingerprint"] != self.smoke_node_report["parameter"]["fingerprint"] {
                return Err("grouping changed actual GPU pixels".into());
            }
            self.smoke_node_report["groupPixelsPreserved"] = Value::Bool(true);
            let id = self.smoke_node_report["groupId"]
                .as_str()
                .ok_or("group id missing")?
                .to_string();
            if !self.panels.node_editor.enter_group(&id)
                || !self.panels.node_editor.toggle_collapsed("circle")
            {
                return Err("native group navigation/collapse failed".into());
            }
            let snapshot = self.panels.node_editor.snapshot();
            let path = self.smoke.as_ref().unwrap().with_extension("group.msgraph");
            snapshot.save(&path).map_err(|error| error.to_string())?;
            if motion_graph::GraphDocument::load(&path).map_err(|error| error.to_string())?
                != snapshot.document
            {
                return Err("saving inside a group lost root data or folded state".into());
            }
            self.smoke_node_report["groupSourceReopened"] = Value::Bool(true);
            self.schedule_node_compile()?;
            self.smoke_stage = 11;
        } else if self.smoke_stage == 11
            && self.panels.node_preview.active == Some(self.panels.node_editor.stamp())
        {
            let screenshot = self.capture_node_float()?;
            let path = self.smoke.as_ref().unwrap().with_extension("group.png");
            std::fs::copy(screenshot, &path).map_err(|error| error.to_string())?;
            self.smoke_node_report["groupEditorScreenshot"] =
                serde_json::to_value(path).map_err(|error| error.to_string())?;
            if !self.panels.node_editor.leave_group() {
                return Err("native parent navigation failed".into());
            }
            self.panels
                .node_editor
                .ungroup_selected()
                .map_err(|error| error.to_string())?;
            self.panels.node_editor.toggle_collapsed("circle");
            self.schedule_node_compile()?;
            self.smoke_stage = 12;
        } else if self.smoke_stage == 12
            && self.panels.node_preview.active == Some(self.panels.node_editor.stamp())
        {
            let ungrouped = self.engine.node_capture(
                0.,
                self.smoke.as_ref().unwrap().with_extension("ungrouped.png"),
            )?;
            if ungrouped["fingerprint"] != self.smoke_node_report["parameter"]["fingerprint"] {
                return Err("ungrouping changed actual GPU pixels".into());
            }
            self.smoke_node_report["ungroupPixelsPreserved"] = Value::Bool(true);
            self.panels
                .node_editor
                .disconnect_input(&motion_graph::PortRef::new("output", "color"));
            self.schedule_node_compile()?;
            self.smoke_stage = 6;
        } else if self.smoke_stage == 6
            && !self.node_tasks.busy()
            && self.node_compile_due.is_none()
            && self.node_installing.is_empty()
            && self.panels.node_editor.diagnostic().is_some()
        {
            let retained = self
                .engine
                .node_capture(0., directory.join("node-retained.png"))?;
            if retained["fingerprint"] != self.smoke_node_report["parameter"]["fingerprint"] {
                return Err("invalid graph did not preserve the valid GPU preview".into());
            }
            let state = self.engine.state()?;
            if state["revision"] != self.smoke_node_report["projectRevision"]
                || state["project"]["layers"] != self.smoke_node_report["projectEffects"]
            {
                return Err("node preview mutated the user project".into());
            }
            self.smoke_node_report["retained"] = retained;
            self.smoke_node_report["invalidGraphKeptValidPixels"] = Value::Bool(true);
            self.smoke_node_report["projectUntouched"] = Value::Bool(true);
            self.redock_panel(dock::Panel::Nodes.dock())?;
            self.smoke_stage = 7;
            self.dirty = true;
        } else if self.smoke_stage == 7 && self.smoke_frames >= 8 {
            let snapshot = self.panels.node_editor.snapshot();
            let size = self.size().ok_or("node example smoke window missing")?;
            let mut paint = PaintList::default();
            node_examples::Browser::new(snapshot.clone(), false).draw(
                &mut paint,
                &mut self.atlas,
                size,
                &Input::default(),
                self.workspace.locale,
            );
            let config = self.atlas.config();
            let examples_path = self.smoke.as_ref().unwrap().with_extension("examples.png");
            self.engine.capture_ui(
                UiFrame {
                    paint,
                    atlas: Some(AtlasUpload {
                        pixels: self.atlas.pixel_data().to_vec(),
                        width: config.width,
                        height: config.height,
                    }),
                    logical_size: size,
                    composition: None,
                    window: 0,
                    node_preview: None,
                },
                examples_path.clone(),
            )?;
            self.smoke_node_report["exampleLibraryScreenshot"] =
                serde_json::to_value(examples_path).map_err(|error| error.to_string())?;
            self.open_node_example(&snapshot, "uv_gradient")?;
            self.smoke_stage = 8;
        } else if self.smoke_stage == 8
            && self.panels.node_preview.active == Some(self.panels.node_editor.stamp())
        {
            let gradient = self
                .engine
                .node_capture(0., directory.join("node-uv-gradient.png"))?;
            if gradient["fingerprint"] == self.smoke_node_report["parameter"]["fingerprint"] {
                return Err("builtin example did not replace actual GPU pixels".into());
            }
            self.smoke_node_report["example"] = gradient;
            self.smoke_node_report["exampleOpenedAndRendered"] = Value::Bool(true);
            let initial = self.capture_bound_composition("layer-before")?;
            self.smoke_node_report["layerBefore"] = initial;
            let object = self
                .panels
                .selected
                .ok_or("bound node smoke selection missing")?;
            self.open_node_effect(object, None, false, Some(event_loop))?;
            self.smoke_stage = 13;
        } else if self.smoke_stage == 13
            && self.panels.node_preview.active == Some(self.panels.node_editor.stamp())
        {
            let identity = self.capture_bound_composition("layer-identity")?;
            if identity["fingerprint"] != self.smoke_node_report["layerBefore"]["fingerprint"] {
                return Err("identity node effect changed the layer composition".into());
            }
            self.smoke_node_report["layerIdentityPreservedPixels"] = Value::Bool(true);
            let target = self.panels.node_editor.layer_target.clone();
            let mut document = self.panels.node_editor.snapshot().document;
            document
                .public_parameters
                .push(motion_graph::PublicParameter {
                    id: "amount".into(),
                    name: "反相强度".into(),
                    english_name: "Invert Amount".into(),
                    value_type: motion_graph::ValueType::Float,
                    default: [0.5, 0., 0., 0.],
                    min: 0.,
                    max: 1.,
                    step: 0.01,
                    units: "".into(),
                    animatable: true,
                });
            document.nodes.extend([
                motion_graph::Node::new(
                    "invert",
                    motion_graph::NodeOp::ColorMath {
                        operation: motion_graph::ColorOp::Invert,
                    },
                ),
                motion_graph::Node::new(
                    "mix",
                    motion_graph::NodeOp::ColorMath {
                        operation: motion_graph::ColorOp::Mix,
                    },
                ),
                motion_graph::Node::new(
                    "amount",
                    motion_graph::NodeOp::Parameter {
                        parameter: "amount".into(),
                    },
                ),
            ]);
            document.connections.retain(|c| c.to.node != "output");
            document.connections.extend([
                motion_graph::Connection::new("sample", "color", "invert", "color"),
                motion_graph::Connection::new("sample", "color", "mix", "a"),
                motion_graph::Connection::new("invert", "color", "mix", "b"),
                motion_graph::Connection::new("amount", "value", "mix", "factor"),
                motion_graph::Connection::new("mix", "color", "output", "color"),
            ]);
            for node in &mut document.nodes {
                node.position = match node.id.as_str() {
                    "input" => [0., 0.],
                    "uv" => [0., 190.],
                    "sample" => [260., 50.],
                    "invert" => [520., 190.],
                    "amount" => [520., 390.],
                    "mix" => [780., 50.],
                    _ => [1040., 50.],
                };
            }
            self.panels.node_editor.replace_document(document, None);
            self.panels.node_editor.layer_target = target;
            self.schedule_node_compile()?;
            self.smoke_stage = 14;
        } else if self.smoke_stage == 14
            && self.panels.node_preview.active == Some(self.panels.node_editor.stamp())
        {
            let changed = self.capture_bound_composition("layer-invert")?;
            if changed["fingerprint"] == self.smoke_node_report["layerBefore"]["fingerprint"] {
                return Err("bound graph did not update main composition pixels".into());
            }
            self.smoke_node_report["layerGraphChangedPixels"] = Value::Bool(true);
            self.smoke_node_report["layerInvert"] = changed;
            self.panels
                .node_editor
                .set_parameter_default("amount", 0, 0.9)
                .map_err(|e| e.to_string())?;
            self.schedule_node_compile()?;
            self.action(panels::Action::Save, Some(event_loop))?;
            self.smoke_stage = 15;
        } else if self.smoke_stage == 15
            && self.node_save_project.is_none()
            && self.panels.node_preview.active == Some(self.panels.node_editor.stamp())
        {
            let changed = self.capture_bound_composition("layer-parameter")?;
            if changed["fingerprint"] == self.smoke_node_report["layerInvert"]["fingerprint"] {
                return Err("bound public default did not update composition pixels".into());
            }
            self.smoke_node_report["layerPublicParameterChangedPixels"] = Value::Bool(true);
            self.smoke_node_report["layerParameter"] = changed;
            let target = self.panels.node_editor.layer_target.as_ref().unwrap();
            let layer = self.state["project"]["layers"]
                .as_array()
                .unwrap()
                .iter()
                .find(|l| l["id"] == target.object)
                .unwrap();
            let effect = layer["effects"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["id"] == target.effect)
                .unwrap();
            if !effect["params"]["amount"].is_object() {
                return Err("bound parameters missing from effect controls".into());
            }
            let size = self.size().ok_or("bound node window missing")?;
            let (paint, _) = self.draw_commands(size);
            let ui = self.ui_frame(paint, size, true);
            let path = self
                .smoke
                .as_ref()
                .unwrap()
                .with_extension("layer-nodes.png");
            self.engine.capture_ui(ui, path.clone())?;
            self.smoke_node_report["layerWorkspaceScreenshot"] =
                serde_json::to_value(path).map_err(|e| e.to_string())?;
            self.state = self.engine.history(0)?;
            self.reconcile_node_target()?;
            if self
                .panels
                .node_editor
                .snapshot()
                .document
                .public_parameters[0]
                .default[0]
                != 0.5
            {
                return Err(
                    "undo did not restore the public default in the editable source".into(),
                );
            }
            self.smoke_stage = 18;
        } else if self.smoke_stage == 18
            && self.node_compile_due.is_none()
            && self.node_installing.is_empty()
            && !self.node_tasks.busy()
            && self.panels.node_preview.active == Some(self.panels.node_editor.stamp())
        {
            let undone = self.capture_bound_composition("layer-undo")?;
            if undone["fingerprint"] != self.smoke_node_report["layerInvert"]["fingerprint"] {
                return Err(
                    "project undo failed to restore node source and composition pixels".into(),
                );
            }
            self.state = self.engine.history(1)?;
            self.reconcile_node_target()?;
            self.smoke_stage = 19;
        } else if self.smoke_stage == 19
            && self.node_compile_due.is_none()
            && self.node_installing.is_empty()
            && !self.node_tasks.busy()
            && self.panels.node_preview.active == Some(self.panels.node_editor.stamp())
        {
            let redone = self.capture_bound_composition("layer-redo")?;
            if redone["fingerprint"] != self.smoke_node_report["layerParameter"]["fingerprint"] {
                return Err(
                    "project redo failed to restore node source and composition pixels".into(),
                );
            }
            self.smoke_node_report["layerProjectUndoRedoRestoresSourceAndPixels"] =
                Value::Bool(true);
            self.panels
                .node_editor
                .toggle_preview(motion_graph::PortRef::new("sample", "color"))
                .map_err(|e| e.to_string())?;
            self.schedule_node_compile()?;
            self.smoke_stage = 16;
        } else if self.smoke_stage == 16
            && self.panels.node_preview.active == Some(self.panels.node_editor.stamp())
        {
            let preview = self.capture_bound_composition("layer-port-preview")?;
            if preview["fingerprint"] != self.smoke_node_report["layerParameter"]["fingerprint"] {
                return Err("temporary port preview overwrote canonical layer output".into());
            }
            self.smoke_node_report["layerPortPreviewIsObservational"] = Value::Bool(true);
            self.panels.node_editor.restore_preview();
            self.panels
                .node_editor
                .disconnect_input(&motion_graph::PortRef::new("output", "color"));
            self.schedule_node_compile()?;
            self.smoke_stage = 17;
        } else if self.smoke_stage == 17
            && !self.node_tasks.busy()
            && self.node_installing.is_empty()
            && self.node_sources.is_empty()
            && self.node_compile_due.is_none()
            && self.panels.node_editor.diagnostic().is_some()
        {
            let retained = self.capture_bound_composition("layer-retained")?;
            if retained["fingerprint"] != self.smoke_node_report["layerParameter"]["fingerprint"] {
                return Err("invalid layer graph replaced valid composition pixels".into());
            }
            self.smoke_node_report["layerInvalidGraphKeptValidPixels"] = Value::Bool(true);
            self.action(panels::Action::Save, Some(event_loop))?;
            self.smoke_stage = 20;
        } else if self.smoke_stage == 20
            && self.node_save_project.is_none()
            && self.node_sources.is_empty()
        {
            let target = self.panels.node_editor.layer_target.clone().unwrap();
            let expected = self.panels.node_editor.snapshot().document;
            self.state = self.engine.open_directory(target.root.clone())?;
            let layer = self.state["project"]["layers"]
                .as_array()
                .unwrap()
                .iter()
                .find(|l| l["id"] == target.object)
                .unwrap();
            let effect = layer["effects"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["id"] == target.effect)
                .unwrap();
            self.panels
                .node_editor
                .load_layer_source(&effect["node_graph"], target)?;
            if self.panels.node_editor.snapshot().document != expected {
                return Err("project reopen lost editable graph source".into());
            }
            let package = self.engine.pack()?;
            let isolated_root = directory.join(format!(
                "bound-package-reopened-{}/project",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            let imported = motion_core::storage::import_package(
                std::path::Path::new(package["path"].as_str().unwrap()),
                &isolated_root,
            )
            .map_err(|e| e.to_string())?;
            if !imported.layers.iter().flat_map(|l| &l.effects).any(|e| {
                e.node_graph
                    .as_ref()
                    .is_some_and(|s| s["document"] == serde_json::to_value(&expected).unwrap())
            }) {
                return Err("packed project lost graph source or runtime package".into());
            }
            let offline = Engine::start(isolated_root, motion_desktop_media::platform(), 8 << 30)?;
            offline.seek(self.frame)?;
            let captured = offline.capture()?;
            let image = image::open(captured["path"].as_str().ok_or("offline capture missing")?)
                .map_err(|e| e.to_string())?
                .to_rgba8();
            let fingerprint = image.as_raw().iter().fold(0xcbf29ce484222325u64, |h, b| {
                (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
            });
            if format!("{fingerprint:016x}")
                != self.smoke_node_report["layerParameter"]["fingerprint"]
                    .as_str()
                    .unwrap()
            {
                return Err(
                    "offline project package did not restore the last valid node effect pixels"
                        .into(),
                );
            }
            self.smoke_node_report["layerOfflinePackageRestoresPixels"] = Value::Bool(true);
            self.smoke_node_report["layerSourceAndRuntimePackedAndReopened"] = Value::Bool(true);
            self.finish_smoke()?;
            self.smoke_stage = 4;
        }
        Ok(())
    }
}
