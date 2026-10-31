//! Real dock controls and engine edits, captured through the native GPU renderer.
use crate::*;
use motion_core::Property;
use motion_ui::{
    dock::{Edge as DockEdge, Node, Panel},
    MouseButton,
};

impl Shell {
    pub(super) fn capture_alignment_screens(
        &mut self,
        window: [f32; 2],
        report: &std::path::Path,
        output: &mut serde_json::Map<String, Value>,
    ) -> Result<(), String> {
        let before = self.engine.state()?;
        let id = before["project"]["layers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|l| l["id"].as_u64())
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("alignment fixture ID exhausted")?;
        let second = id.checked_add(1).ok_or("alignment fixture ID exhausted")?;
        let third = second
            .checked_add(1)
            .ok_or("distribution fixture ID exhausted")?;
        let width = before["project"]["width"]
            .as_f64()
            .ok_or("alignment fixture width missing")? as f32;
        let height = before["project"]["height"]
            .as_f64()
            .ok_or("alignment fixture height missing")? as f32;
        self.engine.history(2)?;
        let captured = (|| {
            let mut a = motion_core::Layer::solid(
                id,
                "Boundary A",
                [width * 0.22, height * 0.18],
                [width * 0.35, height * 0.35, 0.],
                [0.25, 0.55, 0.9, 1.],
            );
            a.transform.rotation.value[2] = 25.;
            a.transform.anchor.value = [0.2, 0.8];
            let b = motion_core::Layer::solid(
                second,
                "Boundary B",
                [width * 0.12, height * 0.28],
                [width * 0.7, height * 0.65, 0.],
                [0.9, 0.55, 0.25, 1.],
            );
            let mut state = self.engine.edit(
                serde_json::to_string(&vec![Command::Add { layer: a }, Command::Add { layer: b }])
                    .map_err(|e| e.to_string())?,
            )?;
            let size = [900., 560.];
            let layout = Node::Split {
                edge: DockEdge::Left,
                split: 0.4,
                first: Box::new(Node::dock(Panel::Align.dock())),
                second: Box::new(Node::dock(Panel::Composition.dock())),
            };
            let mut panels = panels::Panels {
                locale: self.workspace.locale,
                styles: self.panels.styles.clone(),
                focused: Some(Panel::Align),
                ..Default::default()
            };
            let mut discarded = PaintList::default();
            discarded.stylesheet = panels.styles.clone();
            let actions = panels::chrome(
                &mut discarded,
                &mut self.atlas,
                size,
                &layout,
                &mut panels,
                &state,
                &Input::default(),
            );
            if !actions.is_empty() {
                return Err("alignment warmup emitted actions".into());
            }
            panels.selected = Some(second);
            panels.selected_layers = vec![id, second];
            let area = panels::frames(&layout, size)
                .into_iter()
                .find(|f| f.panel == Panel::Align)
                .ok_or("alignment panel missing")?
                .content;
            let styles = discarded.styles();
            let pad = styles.metric("padding.panel", 8.);
            let row = styles.metric("row.height", 22.);
            let gap = styles.metric("auxiliary.gap", 6.);
            let button = styles.metric("auxiliary.control.height", 26.);
            for name in [
                "align-composition",
                "align-target-menu",
                "align-selection-before",
                "align-selection-after",
                "distribute-before",
                "distribute-horizontal-centers",
                "distribute-horizontal-gaps",
                "distribute-vertical-centers",
                "distribute-vertical-gaps",
            ] {
                if name == "distribute-before" {
                    let c = motion_core::Layer::solid(
                        third,
                        "Boundary C",
                        [width * 0.08, height * 0.12],
                        [width * 0.48, height * 0.78, 0.],
                        [0.3, 0.8, 0.5, 1.],
                    );
                    let mut commands = vec![
                        Command::SetVector {
                            object: id,
                            property: Property::Position,
                            frame: state["frame"].as_f64().unwrap_or(0.).floor() as u32,
                            value: [width * 0.35, height * 0.35, 0.],
                        },
                        Command::SetVector {
                            object: second,
                            property: Property::Position,
                            frame: state["frame"].as_f64().unwrap_or(0.).floor() as u32,
                            value: [width * 0.7, height * 0.65, 0.],
                        },
                        Command::Add { layer: c },
                    ];
                    for layer in before["project"]["layers"]
                        .as_array()
                        .ok_or("distribution fixture layers missing")?
                    {
                        commands.push(Command::Flags {
                            object: layer["id"]
                                .as_u64()
                                .ok_or("distribution fixture layer ID missing")?,
                            visible: false,
                            locked: layer["locked"]
                                .as_bool()
                                .ok_or("distribution fixture lock state missing")?,
                        });
                    }
                    state = self
                        .engine
                        .edit(serde_json::to_string(&commands).map_err(|e| e.to_string())?)?;
                    panels.selected = Some(third);
                    panels.selected_layers = vec![id, second, third];
                }
                let distribution = match name {
                    "distribute-horizontal-centers" => Some((
                        0,
                        0,
                        motion_host::ops::alignment::Distribution::HorizontalCenters,
                    )),
                    "distribute-horizontal-gaps" => Some((
                        1,
                        0,
                        motion_host::ops::alignment::Distribution::HorizontalGaps,
                    )),
                    "distribute-vertical-centers" => Some((
                        0,
                        1,
                        motion_host::ops::alignment::Distribution::VerticalCenters,
                    )),
                    "distribute-vertical-gaps" => Some((
                        1,
                        1,
                        motion_host::ops::alignment::Distribution::VerticalGaps,
                    )),
                    _ => None,
                };
                if let Some((column, line, expected)) = distribution {
                    let w = (area.width() - pad * 2. - gap) / 2.;
                    let p = [
                        area.min[0] + pad + column as f32 * (w + gap) + w * 0.5,
                        area.min[1]
                            + pad
                            + row * 6.
                            + button * 2.
                            + gap * 7.
                            + line as f32 * (button + gap)
                            + button * 0.5,
                    ];
                    let actions = panels::chrome(
                        &mut discarded,
                        &mut self.atlas,
                        size,
                        &layout,
                        &mut panels,
                        &state,
                        &Input {
                            mouse: p,
                            pressed: Some((MouseButton::Left, p)),
                            ..Default::default()
                        },
                    );
                    let [panels::Action::DistributeLayers {
                        objects,
                        distribution,
                    }] = actions.as_slice()
                    else {
                        return Err(format!(
                            "distribution click did not dispatch once: {actions:?}"
                        ));
                    };
                    if *distribution != expected {
                        return Err("distribution control dispatched a different operation".into());
                    }
                    state = self
                        .engine
                        .distribute_layers_async(objects.clone(), *distribution)?
                        .recv_timeout(std::time::Duration::from_secs(30))
                        .map_err(|e| e.to_string())??;
                }
                let input = if name == "align-target-menu" {
                    let p = [area.max[0] - pad - gap, area.min[1] + pad + row * 0.5];
                    Input {
                        mouse: p,
                        pressed: Some((MouseButton::Left, p)),
                        ..Default::default()
                    }
                } else {
                    Input::default()
                };
                if name == "align-selection-before" {
                    for key in [Key::Down, Key::Enter] {
                        let actions = panels::chrome(
                            &mut discarded,
                            &mut self.atlas,
                            size,
                            &layout,
                            &mut panels,
                            &state,
                            &Input {
                                keys: vec![(key, Default::default())],
                                ..Default::default()
                            },
                        );
                        if !actions.is_empty() {
                            return Err("alignment target selection emitted actions".into());
                        }
                    }
                }
                if name == "align-selection-after" {
                    let p = [
                        area.min[0] + pad + (area.width() - pad * 2. - gap * 2.) / 6.,
                        area.min[1] + pad + (row + gap) * 2. + button * 0.5,
                    ];
                    let actions = panels::chrome(
                        &mut discarded,
                        &mut self.atlas,
                        size,
                        &layout,
                        &mut panels,
                        &state,
                        &Input {
                            mouse: p,
                            pressed: Some((MouseButton::Left, p)),
                            ..Default::default()
                        },
                    );
                    let [panels::Action::AlignLayers {
                        objects,
                        edge,
                        target,
                    }] = actions.as_slice()
                    else {
                        return Err(format!(
                            "alignment click did not dispatch once: {actions:?}"
                        ));
                    };
                    if *target != motion_host::ops::alignment::Target::Selection {
                        return Err("alignment target did not change through dropdown".into());
                    }
                    state = self
                        .engine
                        .align_layers_async(objects.clone(), *edge, *target)?
                        .recv_timeout(std::time::Duration::from_secs(30))
                        .map_err(|e| e.to_string())??;
                }
                let mut paint = PaintList::default();
                paint.stylesheet = panels.styles.clone();
                let actions = panels::chrome(
                    &mut paint,
                    &mut self.atlas,
                    size,
                    &layout,
                    &mut panels,
                    &state,
                    &input,
                );
                if !actions.is_empty() {
                    return Err("alignment capture emitted actions".into());
                }
                let view = panels::composition_view_styled(&layout, size, &styles);
                self.present_property_capture_scene(
                    window,
                    [view.min[0], view.min[1], view.width(), view.height()],
                )?;
                let mut ui = self.ui_frame(paint, window, true);
                // This isolated capture overrides shell geometry; use the single-view frame protocol.
                ui.views = None;
                ui.composition = Some([
                    view.min[0] * self.scale,
                    view.min[1] * self.scale,
                    view.width() * self.scale,
                    view.height() * self.scale,
                ]);
                ui.composition_image = self.viewer_image(view, self.scale);
                ui.node_preview = None;
                let path = report.with_extension(format!("{name}.png"));
                self.capture_presented_ui(ui, path.clone())?;
                let pixels = image::open(&path).map_err(|e| e.to_string())?;
                let crop = [
                    (size[0] * pixels.width() as f32 / window[0]).round() as u32,
                    (size[1] * pixels.height() as f32 / window[1]).round() as u32,
                ];
                pixels
                    .crop_imm(0, 0, crop[0], crop[1])
                    .save(&path)
                    .map_err(|e| e.to_string())?;
                output.insert(name.into(), serde_json::json!(path));
            }
            Ok(())
        })();
        let restored = self.engine.history(4).and_then(|after| {
            self.state = after.clone();
            let styles = self
                .panels
                .styles
                .clone()
                .unwrap_or_else(motion_ui::style::default_stylesheet_arc);
            let view =
                panels::composition_view_styled(self.workspace.display_root(), window, &styles);
            self.present_property_capture_scene(
                window,
                [view.min[0], view.min[1], view.width(), view.height()],
            )?;
            Ok(after)
        });
        if let Ok(after) = &restored {
            self.state = after.clone();
        }
        match (captured, restored) {
            (Err(primary), Err(cleanup)) => Err(format!(
                "{primary}; alignment fixture rollback also failed: {cleanup}"
            )),
            (Err(primary), Ok(_)) => Err(primary),
            (Ok(()), Err(cleanup)) => Err(cleanup),
            (Ok(()), Ok(after)) => {
                for key in ["project", "canUndo", "canRedo"] {
                    if before[key] != after[key] {
                        return Err(format!("alignment capture changed {key}"));
                    }
                }
                Ok(())
            }
        }
    }
}
