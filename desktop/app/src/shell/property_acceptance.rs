//! Acceptance fixtures rendered by the real panel widgets and attached GPU surface.
use crate::*;
use motion_core::{
    masks::LayerMask,
    vector::{PathNode, VectorPath},
    Axis, Ease, ExpressionTarget, Property, PropertyExpression, Track,
};
use motion_ui::{
    dock::Panel,
    input::{Key, Modifiers, MouseButton, Rect},
};

impl Shell {
    pub(crate) fn capture_property_followup_screens(
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
            + 1;
        let zh = self.workspace.locale == i18n::Locale::Zh;
        let mut layer = Layer::solid(
            object,
            if zh {
                "属性验收"
            } else {
                "Property acceptance"
            },
            [320., 240.],
            [100., 200., 0.],
            [1.; 4],
        );
        layer
            .transform
            .position
            .upsert(-10, [80., 200., 0.], Ease::Linear)
            .map_err(|e| e.to_string())?;
        layer
            .transform
            .position
            .upsert(60, [300., 200., 0.], Ease::Linear)
            .map_err(|e| e.to_string())?;
        layer
            .transform
            .position
            .separate()
            .map_err(|e| e.to_string())?;
        layer.masks.push(LayerMask::new(
            1,
            VectorPath {
                id: 1,
                closed: true,
                nodes: [[20., 20.], [280., 20.], [160., 220.]]
                    .into_iter()
                    .enumerate()
                    .map(|(i, p)| PathNode {
                        id: i as u64 + 1,
                        geometry: Track::constant([p[0], p[1], -20., 0., 20., 0.]),
                    })
                    .collect(),
            },
        ));
        let mut locked = Layer::solid(
            object + 1,
            if zh {
                "已锁定图层"
            } else {
                "Locked layer"
            },
            [100.; 2],
            [0.; 3],
            [1.; 4],
        );
        locked.locked = true;
        locked.three_d = true;
        let commands = vec![
            Command::Add { layer },
            Command::Add { layer: locked },
            Command::SetExpression {
                expression: PropertyExpression {
                    target: ExpressionTarget::Property {
                        object,
                        property: Property::Position,
                        axis: Some(Axis::X),
                    },
                    source: "value + time * 20".into(),
                    enabled: true,
                    seed: 42,
                    profile: motion_core::EXPRESSION_PROFILE.into(),
                },
                frame: 0,
            },
        ];
        let state = self
            .engine
            .edit(serde_json::to_string(&commands).map_err(|e| e.to_string())?)?;
        let captured = (|| {
            for (name, panel, size) in [
                ("property-key-navigation", Panel::Properties, [360., 560.]),
                ("property-key-navigation-main", Panel::Properties, [360., 560.]),
                ("property-key-navigation-narrow", Panel::Properties, [220., 560.]),
                ("timeline-key-navigation", Panel::Timeline, [900., 520.]),
                ("timeline-key-navigation-graph", Panel::Timeline, [900., 520.]),
                ("property-merge-menu", Panel::Properties, [360., 560.]),
                (
                    "property-merge-menu-narrow",
                    Panel::Properties,
                    [220., 560.],
                ),
                ("property-axis-expression", Panel::Properties, [360., 560.]),
                ("property-mask-group", Panel::Properties, [360., 560.]),
                ("timeline-mask-group", Panel::Timeline, [900., 520.]),
                ("timeline-mask-group-narrow", Panel::Timeline, [720., 520.]),
                ("timeline-layer-columns", Panel::Timeline, [720., 520.]),
                ("timeline-layer-columns-minimum", Panel::Timeline, [360., 520.]),
                ("timeline-mask-add-menu", Panel::Timeline, [900., 520.]),
                (
                    "timeline-mask-add-menu-narrow",
                    Panel::Timeline,
                    [720., 520.],
                ),
                ("layer-lock-menu", Panel::Timeline, [900., 520.]),
                ("layer-unlock-menu", Panel::Timeline, [900., 520.]),
                ("layer-lock-main-menu", Panel::Timeline, [900., 520.]),
                ("mask-mode-menu", Panel::Properties, [360., 560.]),
                ("mask-mode-menu-narrow", Panel::Properties, [220., 560.]),
                ("graph-raw-readonly", Panel::Timeline, [900., 520.]),
                ("mask-handles-selected", Panel::Composition, [900., 520.]),
            ] {
                let mut panels = panels::Panels {
                    styles: self.panels.styles.clone(),
                    locale: self.workspace.locale,
                    focused: Some(panel),
                    ..Default::default()
                };
                panels.select_only(Some(if name == "layer-unlock-menu" {
                    object + 1
                } else {
                    object
                }));
                panels.timeline.expanded.insert(object);
                if name == "timeline-key-navigation" { panels.timeline.set_graph_mode(false); }
                if name == "timeline-key-navigation-graph" { panels.timeline.set_graph_mode(true); }
                if name.starts_with("timeline-layer-columns") { panels.timeline.expanded.remove(&object); }
                panels.properties.transform_collapsed =
                    name.starts_with("mask-") || name.contains("mask-group");
                if name.starts_with("timeline-mask-") {
                    panels.timeline.collapsed_transforms.insert(object);
                }
                panels.path_controls.masks_open =
                    name.starts_with("mask-") || name.contains("mask-group");
                let mut style_context = PaintList::default();
                style_context.stylesheet = panels.styles.clone();
                let styles = style_context.styles();
                let row = styles.metric("row.height", 22.);
                let pad = styles.metric("padding.panel", 8.);
                let mut draw =
                    |panels: &mut panels::Panels, input: &Input| -> Result<PaintList, String> {
                        let mut paint = PaintList::default();
                        paint.stylesheet = panels.styles.clone();
                        let actions = if matches!(name, "layer-lock-main-menu" | "property-key-navigation-main") {
                            panels::chrome(
                                &mut paint,
                                &mut self.atlas,
                                size,
                                &dock::Node::dock(panel.dock()),
                                panels,
                                &state,
                                input,
                            )
                        } else {
                            panels::floating_chrome(
                                &mut paint,
                                &mut self.atlas,
                                panel,
                                size,
                                panels,
                                &state,
                                input,
                            )
                        };
                        if !actions.is_empty() {
                            return Err(format!("{name} capture emitted actions: {actions:?}"));
                        }
                        Ok(paint)
                    };
                draw(&mut panels, &Input::default())?;
                if name.starts_with("property-merge-") {
                    let point = [pad + styles.metric("icon.slot", 20.) * 3.5 + styles.metric("tree.indent", 14.), 30. + pad + row * 4.5];
                    draw(
                        &mut panels,
                        &Input {
                            mouse: point,
                            pressed: Some((MouseButton::Right, point)),
                            ..Default::default()
                        },
                    )?;
                    if !panels.expressions.editing() {
                        return Err(format!("{name} did not open the property menu"));
                    }
                } else if name == "property-axis-expression" {
                    panels.expressions.open(
                        &state,
                        ExpressionTarget::Property {
                            object,
                            property: Property::Position,
                            axis: Some(Axis::X),
                        },
                        panel,
                    );
                } else if name == "layer-lock-main-menu" {
                    panels.menu = Some(3);
                } else if name.starts_with("layer-") {
                    panels
                        .layer_actions
                        .open(panels.selected, [100., 400.], &state);
                    draw(&mut panels, &Input::default())?;
                    draw(
                        &mut panels,
                        &Input {
                            keys: vec![(Key::Right, Modifiers::default())],
                            ..Default::default()
                        },
                    )?;
                    if !panels.layer_actions.editing() {
                        return Err(format!("{name} did not open the layer menu"));
                    }
                } else if name.starts_with("timeline-mask-add-menu") {
                    let add = panels
                        .path_controls
                        .mask_add_rect(Panel::Timeline)
                        .ok_or("timeline mask add control missing")?;
                    let point = [
                        (add.min[0] + add.max[0]) * 0.5,
                        (add.min[1] + add.max[1]) * 0.5,
                    ];
                    draw(
                        &mut panels,
                        &Input {
                            mouse: point,
                            pressed: Some((MouseButton::Left, point)),
                            ..Default::default()
                        },
                    )?;
                    if !panels.path_controls.editing() {
                        return Err(format!("{name} did not open the add menu"));
                    }
                } else if name == "mask-handles-selected" {
                    panels.path_controls.tool = panels::path_controls::Tool::Mask;
                    panels.path_controls.mask = Some(1);
                    let view = Rect::new(8., 38., size[0] - 16., size[1] - 94.);
                    let point = panels::path_controls::layer_projection(&state, object, view)
                        .and_then(|p| p.screen([120., -100.]))
                        .ok_or("mask capture node projection missing")?;
                    draw(
                        &mut panels,
                        &Input {
                            mouse: point,
                            pressed: Some((MouseButton::Left, point)),
                            ..Default::default()
                        },
                    )?;
                } else if name.starts_with("mask-") {
                    let add = panels
                        .path_controls
                        .mask_add_rect(panel)
                        .ok_or("mask heading missing")?;
                    let point = [size[0] * 0.62, add.min[1] + row * 2.5];
                    draw(
                        &mut panels,
                        &Input {
                            mouse: point,
                            pressed: Some((MouseButton::Left, point)),
                            ..Default::default()
                        },
                    )?;
                    if !panels.path_controls.editing() {
                        return Err(format!("{name} did not open the mode dropdown"));
                    }
                } else if !name.contains("mask-group") && name != "timeline-key-navigation" {
                    panels.timeline.set_graph_mode(true);
                    panels.animation_controls.select_key(
                        object,
                        Property::Position,
                        Some(Axis::X),
                        -10,
                    );
                }
                let paint = draw(&mut panels, &Input::default())?;
                if name.starts_with("timeline-key-navigation")
                    && panels.timeline.graph_mode != name.ends_with("-graph")
                {
                    return Err(format!("{name} captured the wrong timeline mode"));
                }
                if panel == Panel::Composition {
                    self.present_property_capture_scene(
                        window_size,
                        [8., 38., size[0] - 16., size[1] - 94.],
                    )?;
                }
                let mut ui = self.ui_frame(paint, window_size, true);
                ui.composition = None;
                ui.node_preview = None;
                if panel == Panel::Composition {
                    ui.composition = Some([
                        8. * self.scale,
                        38. * self.scale,
                        (size[0] - 16.) * self.scale,
                        (size[1] - 94.) * self.scale,
                    ]);
                }
                let path = report.with_extension(format!("{name}.png"));
                self.engine.capture_ui(ui, path.clone())?;
                let pixels = image::open(&path).map_err(|e| e.to_string())?;
                let width = (size[0] * pixels.width() as f32 / window_size[0]).round() as u32;
                let height = (size[1] * pixels.height() as f32 / window_size[1]).round() as u32;
                pixels
                    .crop_imm(0, 0, width, height)
                    .save(&path)
                    .map_err(|e| e.to_string())?;
                output.insert(name.into(), serde_json::json!(path));
            }
            Ok(())
        })();
        if let Err(cause) = &captured {
            log::error!("property acceptance capture failed: {cause}");
        }
        // Always remove the single atomic fixture edit, including after a capture error.
        let restored = self.engine.history(0).and_then(|state| {
            if state["project"] == before["project"] {
                self.state = state;
                let view = panels::composition_view(&self.workspace.root, window_size);
                if view.width() > 0. && view.height() > 0. {
                    self.present_property_capture_scene(
                        window_size,
                        [view.min[0], view.min[1], view.width(), view.height()],
                    )?;
                }
                Ok(())
            } else {
                Err("property capture changed the project".into())
            }
        });
        match (captured, restored) {
            (Err(cause), Err(cleanup)) => {
                Err(format!("{cause}; fixture cleanup failed: {cleanup}"))
            }
            (Err(cause), _) | (_, Err(cause)) => Err(cause),
            _ => Ok(()),
        }
    }

    fn present_property_capture_scene(
        &mut self,
        size: [f32; 2],
        viewport: [f32; 4],
    ) -> Result<(), String> {
        // CaptureUi composites the retained GPU scene. Present the fixture first,
        // and present the restored project during cleanup to avoid stale pixels.
        let mut ui = self.ui_frame(PaintList::default(), size, true);
        ui.composition = Some(viewport.map(|v| v * self.scale));
        ui.node_preview = None;
        let presented = self
            .engine
            .present_async(ui)?
            .recv_timeout(Duration::from_secs(10))
            .map_err(|e| e.to_string())??;
        if presented {
            Ok(())
        } else {
            Err("property capture scene did not present".into())
        }
    }
}
