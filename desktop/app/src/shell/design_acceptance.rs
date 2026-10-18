//! Capture real widgets using the same native renderer, for Ardot comparisons.
use crate::*;
impl Shell {
    pub(crate) fn capture_design_screens(&mut self, size: [f32; 2]) -> Result<Value, String> {
        let report = self
            .smoke
            .as_ref()
            .ok_or("missing acceptance output")?
            .clone();
        let mut output = serde_json::Map::new();
        let styles = self.panels.styles.clone();
        for screen in [
            "home",
            "project-open-error",
            "composition-settings",
            "workspace-manager",
            "effect-library",
            "timeline-parent-menu",
            "timeline-blend-menu",
            "timeline-graph",
            "value-graph",
            "speed-graph",
            "info-panel",
            "preview-panel",
            "character-panel",
            "expression-inline",
            "effect-controls",
            "effect-controls-narrow",
            "effect-controls-bottom",
            "effect-curve-inline",
            "effect-expression-unavailable-menu",
            "layer-properties-tree",
            "layer-properties-effects",
            "timeline-properties-tree",
        ] {
            if self.smoke_properties && screen != "effect-expression-unavailable-menu" {
                continue;
            }
            if screen == "effect-expression-unavailable-menu" && !self.smoke_properties {
                continue;
            }
            let graph_preset = if screen.starts_with("timeline-") || screen == "effect-library" {
                workspace::Preset::Default
            } else {
                workspace::Preset::Animation
            };
            let mut paint = PaintList::default();
            paint.stylesheet = styles.clone();
            match screen {
                "home" => {
                    let mut home = home::Home::default();
                    home.draw(
                        &mut paint,
                        &mut self.atlas,
                        size,
                        &self.workspace.recent_projects,
                        &Input::default(),
                        self.workspace.locale,
                    );
                }
                "project-open-error" => {
                    let mut home = home::Home::default();
                    home.draw(&mut paint, &mut self.atlas, size,
                        &self.workspace.recent_projects, &Input::default(), self.workspace.locale);
                    let missing = report.with_extension("moved-project");
                    let error = match missing.canonicalize() {
                        Err(error) => self.workspace.locale.error(&error.to_string()),
                        Ok(_) => return Err("error-notice capture requires an absent fixture path".into()),
                    };
                    project_lifecycle::error_notice::render(&mut paint, &mut self.atlas,
                        size, &Input::default(), self.workspace.locale,
                        &project_lifecycle::Target::Open(missing), &error, &mut Default::default());
                }
                "composition-settings" => {
                    let mut dialog = dialogs::Dialog::new_composition(1920, 1080);
                    dialog.render(
                        &mut paint,
                        &mut self.atlas,
                        size,
                        &Input::default(),
                        self.workspace.locale,
                    );
                }
                "workspace-manager" => {
                    let mut manager = workspace::Manager::new(&self.workspace);
                    manager.draw(
                        &mut paint,
                        &mut self.atlas,
                        size,
                        &Input::default(),
                        self.workspace.locale,
                    );
                }
                "effect-library" => {
                    let old = std::mem::take(&mut self.panels.browser);
                    self.panels.browser.catalogue = self.panels.effect_controls.catalogue.clone();
                    self.panels.browser.favorites = self.workspace.favorites.clone();
                    self.panels
                        .browser
                        .preview_categories(self.workspace.locale);
                    let layout = workspace::Workspace::design_layout(workspace::Preset::Default);
                    let actions = panels::chrome(
                        &mut paint,
                        &mut self.atlas,
                        size,
                        &layout,
                        &mut self.panels,
                        &self.state,
                        &Input::default(),
                    );
                    self.panels.browser = old;
                    if !actions.is_empty() {
                        return Err("effect library capture must not edit the project".into());
                    }
                }
                "layer-properties-tree"
                | "layer-properties-effects"
                | "timeline-properties-tree" => {
                    let target = if screen.starts_with("timeline-") {
                        dock::Panel::Timeline
                    } else {
                        dock::Panel::Properties
                    };
                    let panel_size = if target == dock::Panel::Timeline {
                        [720., 520.]
                    } else {
                        [360., 560.]
                    };
                    let path = report.with_extension(format!("{screen}.png"));
                    self.capture_auxiliary_panel(target, panel_size, size, &path, screen)?;
                    output.insert(screen.into(), serde_json::json!(path));
                    continue;
                }
                "effect-controls"
                | "effect-controls-narrow"
                | "effect-controls-bottom"
                | "effect-curve-inline"
                | "effect-expression-unavailable-menu" => {
                    let panel_size = [
                        if screen.ends_with("narrow") {
                            220.
                        } else {
                            360.
                        },
                        if screen == "effect-controls-bottom" {
                            360.
                        } else {
                            560.
                        },
                    ];
                    let path = report.with_extension(format!("{screen}.png"));
                    self.capture_auxiliary_panel(
                        dock::Panel::EffectControls,
                        panel_size,
                        size,
                        &path,
                        screen,
                    )?;
                    output.insert(screen.into(), serde_json::json!(path));
                    continue;
                }
                "expression-inline" => {
                    for mode in ["enabled", "disabled", "locked"] {
                        for (suffix, panel_size) in [("", [900., 520.]), ("-narrow", [600., 360.])]
                        {
                            let name = format!("expression-inline-{mode}{suffix}");
                            let path = report.with_extension(format!("{name}.png"));
                            self.capture_auxiliary_panel(
                                dock::Panel::Timeline,
                                panel_size,
                                size,
                                &path,
                                &name,
                            )?;
                            output.insert(name, serde_json::json!(path));
                        }
                    }
                    continue;
                }
                "info-panel" | "preview-panel" | "character-panel" => {
                    let target = if screen == "info-panel" {
                        dock::Panel::Info
                    } else if screen == "preview-panel" {
                        dock::Panel::Preview
                    } else {
                        dock::Panel::Character
                    };
                    for (suffix, panel_size) in [("", [360., 520.]), ("-narrow", [220., 360.])] {
                        let name = format!("{screen}{suffix}");
                        let path = report.with_extension(format!("{name}.png"));
                        self.capture_auxiliary_panel(target, panel_size, size, &path, &name)?;
                        output.insert(name, serde_json::json!(path));
                    }
                    continue;
                }
                "timeline-parent-menu" | "timeline-blend-menu" => {
                    let old = self.panels.timeline.clone();
                    let columns = std::mem::take(&mut self.panels.timeline_columns);
                    let focus = self.panels.focused;
                    self.panels.timeline.set_graph_mode(false);
                    self.panels.timeline.scroll = 0.;
                    // This fixture locates layers by ordinal, so property rows stay collapsed.
                    self.panels.timeline.expanded.clear();
                    let layout = workspace::Workspace::design_layout(workspace::Preset::Default);
                    let frame = panels::frames(&layout, size)
                        .into_iter()
                        .find(|f| f.panel == dock::Panel::Timeline)
                        .ok_or("timeline placement missing")?;
                    let gutter = 640f32.min(frame.content.width() * 0.52);
                    let rows = self.state["project"]["layers"]
                        .as_array()
                        .ok_or("missing smoke layers")?;
                    let ordinal = rows
                        .iter()
                        .rev()
                        .position(|l| l["id"].as_u64() == self.panels.selected)
                        .ok_or("missing smoke selection")?;
                    let point = [
                        frame.content.min[0] + gutter
                            - if screen == "timeline-parent-menu" {
                                58.
                            } else {
                                158.
                            },
                        frame.content.min[1]
                            + 64.
                            + ordinal as f32 * paint.styles().metric("row.height", 22.)
                            + 11.,
                    ];
                    let input = Input {
                        mouse: point,
                        pressed: Some((motion_ui::input::MouseButton::Left, point)),
                        ..Default::default()
                    };
                    let actions = panels::chrome(
                        &mut paint,
                        &mut self.atlas,
                        size,
                        &layout,
                        &mut self.panels,
                        &self.state,
                        &input,
                    );
                    if !actions.is_empty() || !self.panels.timeline_columns.editing() {
                        return Err(
                            "timeline dropdown capture must open without editing the project"
                                .into(),
                        );
                    }
                    self.panels.timeline = old;
                    self.panels.timeline_columns = columns;
                    self.panels.focused = focus;
                }
                _ => {
                    let old = self.panels.timeline.clone();
                    let controls = std::mem::take(&mut self.panels.animation_controls);
                    let preset = self.panels.workspace_preset;
                    self.panels.workspace_preset = graph_preset;
                    if let Some(object) = self.panels.selected {
                        if let Some(frame) = self.state["timeline_layers"]
                            .as_array()
                            .and_then(|ls| ls.iter().find(|l| l["object"].as_u64() == Some(object)))
                            .and_then(|l| l["properties"]["position"]["keys"].as_array())
                            .and_then(|ks| ks.first())
                            .and_then(|k| k["frame"].as_i64())
                        {
                            self.panels.animation_controls.select_key(
                                object,
                                motion_core::Property::Position,
                                None,
                                frame,
                            );
                        }
                    }

                    self.panels.timeline.set_graph_mode(true);
                    self.panels.timeline.speed_graph = screen == "speed-graph";
                    let layout = workspace::Workspace::design_layout(graph_preset);
                    let actions = panels::chrome(
                        &mut paint,
                        &mut self.atlas,
                        size,
                        &layout,
                        &mut self.panels,
                        &self.state,
                        &Input::default(),
                    );
                    if !actions.is_empty() {
                        return Err("design capture must not edit the project".into());
                    }
                    self.panels.timeline = old;
                    self.panels.animation_controls = controls;
                    self.panels.workspace_preset = preset;
                }
            }
            let mut ui = self.ui_frame(paint, size, true);
            if !screen.ends_with("graph") && !screen.ends_with("menu") && screen != "effect-library"
            {
                ui.composition = None;
                ui.node_preview = None;
            } else {
                let view = panels::composition_view(
                    &workspace::Workspace::design_layout(graph_preset),
                    size,
                );
                ui.composition = Some([
                    view.min[0] * self.scale,
                    view.min[1] * self.scale,
                    view.width() * self.scale,
                    view.height() * self.scale,
                ]);
                ui.node_preview = None;
            }
            let path = report.with_extension(format!("{screen}.png"));
            self.engine.capture_ui(ui, path.clone())?;
            output.insert(screen.into(), serde_json::json!(path));
        }
        if self.smoke_properties {
            self.capture_property_followup_screens(size, &report, &mut output)?;
        }
        Ok(Value::Object(output))
    }

    fn capture_auxiliary_panel(
        &mut self,
        panel: dock::Panel,
        panel_size: [f32; 2],
        window_size: [f32; 2],
        path: &std::path::Path,
        pose: &str,
    ) -> Result<(), String> {
        let before = self.engine.state()?;
        self.state = before.clone();
        let selection = (
            self.panels.selected,
            self.panels.selected_layers.clone(),
            self.panels.selection_anchor,
        );
        let focus = self.panels.focused;
        let old_animation = std::mem::take(&mut self.panels.animation_controls);
        let old_properties = std::mem::take(&mut self.panels.properties);
        let old_timeline = self.panels.timeline.clone();
        let old_anchor = std::mem::take(&mut self.panels.timeline_anchor);
        let old_scroll = self.panels.auxiliary.scroll.clone();
        let old_controls = if panel == dock::Panel::EffectControls {
            let old = std::mem::take(&mut self.panels.effect_controls);
            self.panels.effect_controls.catalogue = old.catalogue.clone();
            Some(old)
        } else {
            None
        };
        let old_expression = pose
            .starts_with("expression-inline")
            .then(|| std::mem::take(&mut self.panels.expressions));
        let mut created_text = false;
        let mut effect_gesture = false;
        let captured = (|| {
            if pose.starts_with("expression-inline") {
                let object = before["project"]["layers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|l| l["id"].as_u64())
                    .max()
                    .unwrap_or(0)
                    + 1;
                self.state = self.engine.edit(
                    serde_json::to_string(&vec![
                        Command::Add {
                            layer: Layer::solid(object, "Expression", [100.; 2], [0.; 3], [1.; 4]),
                        },
                        Command::SetExpression {
                            expression: motion_core::PropertyExpression {
                                target: motion_core::ExpressionTarget::Property {
                                    object,
                                    property: motion_core::Property::Position,
                                    axis: None,
                                },
                                source: "[100, 200, 0]".into(),
                                enabled: !pose.contains("disabled"),
                                seed: 42,
                                profile: motion_core::EXPRESSION_PROFILE.into(),
                            },
                            frame: before["frame"].as_f64().unwrap_or(0.).floor() as u32,
                        },
                        Command::Flags {
                            object,
                            visible: true,
                            locked: pose.contains("locked"),
                        },
                    ])
                    .map_err(|e| e.to_string())?,
                )?;
                created_text = true;
                self.panels.select_only(Some(object));
                self.panels.focused = Some(panel);
                self.panels.timeline.expanded.insert(object);
                self.panels.expressions.open(
                    &self.state,
                    motion_core::ExpressionTarget::Property {
                        object,
                        property: motion_core::Property::Position,
                        axis: None,
                    },
                    dock::Panel::Timeline,
                );
            }
            if panel == dock::Panel::Character {
                self.state = self.engine.create_text("Title".into(), 64., 1024, None)?;
                created_text = true;
                let object = self.state["project"]["layers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .rev()
                    .find(|layer| layer["content"]["kind"] == "text")
                    .and_then(|layer| layer["id"].as_u64())
                    .ok_or("character capture requires a real text layer")?;
                self.panels.select_only(Some(object));
            }
            if !pose.starts_with("expression-inline")
                && matches!(
                    panel,
                    dock::Panel::EffectControls | dock::Panel::Properties | dock::Panel::Timeline
                )
            {
                self.engine.history(2)?;
                effect_gesture = true;
                let object = before["project"]["layers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|layer| layer["id"].as_u64())
                    .max()
                    .unwrap_or(0)
                    + 1;
                let layer = Layer::solid(
                    object,
                    if self.panels.locale == i18n::Locale::Zh {
                        "纯色层 1"
                    } else {
                        "Solid 1"
                    },
                    [100.; 2],
                    [960., 540., 0.],
                    [1.; 4],
                );
                self.state = self.engine.edit(
                    serde_json::to_string(&Command::Add { layer }).map_err(|e| e.to_string())?,
                )?;
                let catalogue = self.panels.effect_controls.catalogue.clone();
                for name in ["drop_shadow", "curves", "brightness_contrast"]
                    .into_iter()
                    .filter(|name| pose != "effect-curve-inline" || *name == "curves")
                    .filter(|name| pose != "effect-expression-unavailable-menu" || *name == "brightness_contrast")
                {
                    let package = catalogue["packages"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|package| {
                            package["manifest"]["effects"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .any(|effect| effect["id"] == name)
                        })
                        .ok_or("capture effect missing")?;
                    self.engine.plugin(serde_json::json!({"op":"add","object":object,"plugin":package["manifest"]["id"],"version":package["manifest"]["version"],"hash":package["hash"],"effect":name}).to_string())?;
                }
                if matches!(panel, dock::Panel::Properties | dock::Panel::Timeline) {
                    self.engine.edit(
                        serde_json::to_string(&vec![
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
                                frame: 60,
                                value: [1120., 540., 0.],
                            },
                        ])
                        .map_err(|error| error.to_string())?,
                    )?;
                }
                self.state = self.engine.history(3)?;
                effect_gesture = false;
                created_text = true;
                self.panels.select_only(Some(object));
                if panel == dock::Panel::Properties {
                    self.panels.properties.transform_collapsed = pose.ends_with("effects");
                    self.panels.properties.effects_collapsed = !pose.ends_with("effects");
                }
                if panel == dock::Panel::Timeline {
                    self.panels.timeline.expanded.clear();
                    self.panels.timeline.expanded.insert(object);
                    self.panels.expressions.open(
                        &self.state,
                        motion_core::ExpressionTarget::Property {
                            object,
                            property: motion_core::Property::Position,
                            axis: None,
                        },
                        dock::Panel::Timeline,
                    );
                    self.panels.focused = Some(dock::Panel::Timeline);
                }
                if pose == "effect-controls-bottom" {
                    self.panels
                        .auxiliary
                        .scroll
                        .insert(panel as u32, panel_size[1]);
                }
            }
            let mut paint = PaintList::default();
            paint.stylesheet = self.panels.styles.clone();
            let mut input = Input::default();
            if pose == "effect-expression-unavailable-menu" {
                let styles = paint.styles();
                let point = [80., 30. + styles.metric("padding.panel", 8.)
                    + styles.metric("effect.toolbar.height", 28.)
                    + styles.metric("effect.toolbar.gap", 6.)
                    + styles.metric("effect.group.gap", 8.)
                    + styles.metric("effect.header.height", 30.).max(styles.metric("row.height", 22.))
                    + styles.metric("effect.row.height", 26.).max(styles.metric("row.height", 22.)) * 3.5];
                input.mouse = point;
                input.pressed = Some((motion_ui::input::MouseButton::Right, point));
            }
            let actions = panels::floating_chrome(
                &mut paint,
                &mut self.atlas,
                panel,
                panel_size,
                &mut self.panels,
                &self.state,
                &input,
            );
            if !actions.is_empty() {
                return Err("auxiliary panel capture must not emit edits".into());
            }
            if pose == "effect-expression-unavailable-menu" && !self.panels.effect_controls.editing() {
                return Err("discrete effect capture did not open the parameter menu".into());
            }
            // Capture at the window's real pixel ratio, then crop. Changing only
            // logical_size would stretch this panel across the main surface.
            let mut ui = self.ui_frame(paint, window_size, true);
            ui.composition = None;
            ui.node_preview = None;
            self.engine.capture_ui(ui, path.to_path_buf())?;
            let pixels = image::open(path).map_err(|e| e.to_string())?;
            let width = (panel_size[0] * pixels.width() as f32 / window_size[0]).round() as u32;
            let height = (panel_size[1] * pixels.height() as f32 / window_size[1]).round() as u32;
            pixels
                .crop_imm(0, 0, width, height)
                .save(path)
                .map_err(|e| e.to_string())
        })();
        let restored = if effect_gesture {
            self.engine.history(4)
        } else if created_text {
            self.engine.history(0)
        } else {
            self.engine.state()
        };
        self.panels.selected = selection.0;
        self.panels.selected_layers = selection.1;
        self.panels.selection_anchor = selection.2;
        self.panels.focused = focus;
        self.panels.animation_controls = old_animation;
        self.panels.properties = old_properties;
        self.panels.timeline = old_timeline;
        self.panels.timeline_anchor = old_anchor;
        self.panels.auxiliary.scroll = old_scroll;
        if let Some(old) = old_expression {
            self.panels.expressions = old;
        }
        if let Some(old) = old_controls {
            self.panels.effect_controls = old;
        }
        self.state = restored?;
        if self.state["project"] != before["project"] {
            return Err("auxiliary panel fixture changed the project".into());
        }
        captured
    }
}
