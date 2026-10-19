//! Real shape grips, engine transactions and GPU captures; no alternate painter.
use crate::*;
use motion_ui::{dock::Panel, MouseButton};

impl Shell {
    pub(super) fn capture_shape_resize_screens(
        &mut self,
        window_size: [f32; 2],
        report: &std::path::Path,
        output: &mut serde_json::Map<String, Value>,
    ) -> Result<(), String> {
        let before = self.engine.state()?;
        let object = before["project"]["layers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|l| l["id"].as_u64())
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("resize fixture ID exhausted")?;
        let center = [
            before["project"]["width"]
                .as_f64()
                .ok_or("resize fixture width missing")? as f32
                * 0.5,
            before["project"]["height"]
                .as_f64()
                .ok_or("resize fixture height missing")? as f32
                * 0.5,
            0.,
        ];
        let command = Command::AddShape {
            id: object,
            name: if self.workspace.locale == i18n::Locale::Zh {
                "矩形宽高"
            } else {
                "Rectangle dimensions"
            }
            .into(),
            shape: motion_core::vector::ShapeKind::Rectangle,
            size: [320., 200.],
            position: center,
        };
        let mut state = self
            .engine
            .edit(serde_json::to_string(&command).map_err(|e| e.to_string())?)?;
        let mut undos = 1;
        let mut active = false;
        let result = (|| {
            let mut panels = panels::Panels {
                styles: self.panels.styles.clone(),
                locale: self.workspace.locale,
                focused: Some(Panel::Composition),
                ..Default::default()
            };
            panels.select_only(Some(object));
            panels.path_controls.tool = panels::path_controls::Tool::Pen;
            let styles = panels
                .styles
                .clone()
                .unwrap_or_else(motion_ui::style::default_stylesheet_arc);
            let size = [900., 520.];
            let view = panels::floating_view_styled(Panel::Composition, size, &styles)
                .ok_or("resize fixture viewport missing")?;
            let projection = panels::path_controls::layer_projection(&state, object, view)
                .ok_or("resize fixture projection missing")?;
            let start = projection
                .screen([160., 100.])
                .ok_or("resize corner not visible")?;
            let release = projection
                .screen([260., 160.])
                .ok_or("resize destination not visible")?;
            for phase in 0..3 {
                if phase == 1 {
                    for input in [
                        Input {
                            mouse: start,
                            pressed: Some((MouseButton::Left, start)),
                            held: Some(MouseButton::Left),
                            ..Default::default()
                        },
                        Input {
                            mouse: [size[0] - 1., size[1] - 1.],
                            released: Some((MouseButton::Left, release)),
                            ..Default::default()
                        },
                    ] {
                        let actions = panels::floating_chrome(
                            &mut PaintList::default(),
                            &mut self.atlas,
                            Panel::Composition,
                            size,
                            &mut panels,
                            &state,
                            &input,
                        );
                        if input.released.is_some() && actions.iter().filter(|a|
                            matches!(a,panels::Action::EditBatch(commands) if commands.len()==2)).count()!=1 {
                            return Err(format!("resize must publish size and position atomically: {actions:?}"));
                        }
                        for action in actions {
                            match action {
                                panels::Action::History(2) => {
                                    self.engine.history(2)?;
                                    active = true;
                                }
                                panels::Action::History(3) => {
                                    self.engine.history(3)?;
                                    active = false;
                                    undos += 1;
                                }
                                panels::Action::Edit(command) => {
                                    state = self.engine.edit(
                                        serde_json::to_string(&command)
                                            .map_err(|e| e.to_string())?,
                                    )?
                                }
                                panels::Action::EditBatch(commands) => {
                                    state = self.engine.edit(
                                        serde_json::to_string(&commands)
                                            .map_err(|e| e.to_string())?,
                                    )?;
                                }
                                other => {
                                    return Err(format!(
                                        "resize capture unexpected action: {other:?}"
                                    ))
                                }
                            }
                        }
                    }
                    let layer = state["project"]["layers"]
                        .as_array()
                        .and_then(|a| a.iter().find(|l| l["id"] == object))
                        .ok_or("resized layer missing")?;
                    let layer: Layer =
                        serde_json::from_value(layer.clone()).map_err(|e| e.to_string())?;
                    let motion_core::Content::Vector {
                        vector:
                            motion_core::vector::VectorContent {
                                source: motion_core::vector::VectorSource::Group { group },
                                ..
                            },
                    } = &layer.content
                    else {
                        return Err("resize capture lost parametric geometry".into());
                    };
                    let motion_core::vector::groups::GroupItem::Geometry {
                        size,
                        position,
                        vector,
                        ..
                    } = &group.items[0]
                    else {
                        return Err("resize geometry missing".into());
                    };
                    let f =
                        layer.local_frame(state["frame"].as_f64().ok_or("resize frame missing")?);
                    for (actual, expected) in size
                        .sample(f)
                        .into_iter()
                        .chain(position.sample(f))
                        .zip([420., 260., 50., 30.])
                    {
                        if (actual - expected).abs() > 0.01 {
                            return Err(format!(
                                "resize dimensions mismatch: {actual} expected {expected}"
                            ));
                        }
                    }
                    if !matches!(
                        vector.source,
                        motion_core::vector::VectorSource::Shape {
                            shape: motion_core::vector::ShapeKind::Rectangle,
                            ..
                        }
                    ) {
                        return Err("resize converted the rectangle to a path".into());
                    }
                }
                let (name, panel, panel_size) = match phase {
                    0 => ("rectangle-resize-before", Panel::Composition, size),
                    1 => ("rectangle-resize-after", Panel::Composition, size),
                    _ => (
                        "rectangle-resize-properties",
                        Panel::Properties,
                        [360., 560.],
                    ),
                };
                panels.focused = Some(panel);
                panels.properties.transform_collapsed = true;
                if panel == Panel::Properties {
                    panels.path_controls.reveal_vector_item(&state, object, 2);
                }
                let mut paint = PaintList::default();
                paint.stylesheet = panels.styles.clone();
                let actions = panels::floating_chrome(
                    &mut paint,
                    &mut self.atlas,
                    panel,
                    panel_size,
                    &mut panels,
                    &state,
                    &Input::default(),
                );
                if !actions.is_empty() {
                    return Err(format!("{name} idle emitted actions: {actions:?}"));
                }
                if panel == Panel::Composition {
                    self.present_property_capture_scene(
                        window_size,
                        [view.min[0], view.min[1], view.width(), view.height()],
                    )?;
                }
                let mut ui = self.ui_frame(paint, window_size, true);
                ui.node_preview = None;
                ui.composition = None;
                ui.composition_image = None;
                if panel == Panel::Composition {
                    ui.composition = Some(
                        [view.min[0], view.min[1], view.width(), view.height()]
                            .map(|v| v * self.scale),
                    );
                    ui.composition_image = Some(
                        [
                            projection.image.min[0],
                            projection.image.min[1],
                            projection.image.width(),
                            projection.image.height(),
                        ]
                        .map(|v| v * self.scale),
                    );
                }
                let path = report.with_extension(format!("{name}.png"));
                self.engine.capture_ui(ui, path.clone())?;
                let pixels = image::open(&path).map_err(|e| e.to_string())?;
                let width = (panel_size[0] * pixels.width() as f32 / window_size[0]).round() as u32;
                let height =
                    (panel_size[1] * pixels.height() as f32 / window_size[1]).round() as u32;
                pixels
                    .crop_imm(0, 0, width, height)
                    .save(&path)
                    .map_err(|e| e.to_string())?;
                output.insert(name.into(), serde_json::json!(path));
            }
            Ok(())
        })();
        // Keep the primary failure if gesture closure or fixture removal fails.
        let mut cleanup_errors = vec![];
        if active {
            if let Err(e) = self.engine.history(4) {
                cleanup_errors.push(e);
            }
        }
        for _ in 0..undos {
            if let Err(e) = self.engine.history(0) {
                cleanup_errors.push(e);
                break;
            }
        }
        match self.engine.state() {
            Ok(state) if state["project"] == before["project"] => {}
            Ok(_) => cleanup_errors.push("resize capture did not restore project".into()),
            Err(e) => cleanup_errors.push(e),
        }
        match (result, cleanup_errors.is_empty()) {
            (Err(e), false) => Err(format!(
                "{e}; resize cleanup: {}",
                cleanup_errors.join("; ")
            )),
            (Err(e), true) => Err(e),
            (Ok(()), false) => Err(format!("resize cleanup: {}", cleanup_errors.join("; "))),
            (Ok(()), true) => Ok(()),
        }
    }
}
