//! Native captures exercise the same hit testing and engine transactions as input.
use crate::*;
use motion_ui::{
    dock::Panel,
    paint::{Pattern, Shape},
    MouseButton,
};

impl Shell {
    pub(super) fn capture_timeline_key_drag_screens(
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
            .ok_or("timing fixture ID exhausted")?;
        let mut layer = Layer::solid(
            object,
            if self.workspace.locale == i18n::Locale::Zh {
                "关键帧拖动"
            } else {
                "Keyframe timing"
            },
            [100.; 2],
            [100.; 3],
            [1.; 4],
        );
        for frame in [20, 70] {
            layer
                .transform
                .position
                .upsert(frame, [frame as f32; 3], motion_core::Ease::Linear)
                .map_err(|e| e.to_string())?;
        }
        for frame in [20, 90] {
            layer
                .transform
                .opacity
                .upsert(frame, 0.5, motion_core::Ease::Hold)
                .map_err(|e| e.to_string())?;
        }
        let mut state = self.engine.edit(
            serde_json::to_string(&Command::Add {
                layer: layer.clone(),
            })
            .map_err(|e| e.to_string())?,
        )?;
        let mut undos = 1;
        let mut active = false;
        let result = (|| {
            // Add normalizes the layer interval; compare against the accepted
            // model, not the pre-command fixture with an implicit interval.
            let layer: Layer = serde_json::from_value(
                state["project"]["layers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|l| l["id"] == object)
                    .ok_or("accepted timing fixture missing")?
                    .clone(),
            )
            .map_err(|e| e.to_string())?;
            for expanded in [true, false] {
                let mut panels = panels::Panels {
                    styles: self.panels.styles.clone(),
                    locale: self.workspace.locale,
                    focused: Some(Panel::Timeline),
                    ..Default::default()
                };
                panels.select_only(Some(object));
                panels.timeline.timeline_query = layer.name.clone();
                if expanded {
                    panels.timeline.expanded.insert(object);
                }
                panels.animation_controls.begin_row_probe(
                    object,
                    motion_core::Property::Position,
                    None,
                );
                let size = [900., 520.];
                let draw = |atlas: &mut TextAtlas,
                            panels: &mut panels::Panels,
                            state: &Value,
                            input: &Input| {
                    let mut paint = PaintList::default();
                    paint.stylesheet = panels.styles.clone();
                    let actions = if expanded {
                        panels::chrome(
                            &mut paint,
                            atlas,
                            size,
                            &dock::Node::dock(Panel::Timeline.dock()),
                            panels,
                            state,
                            input,
                        )
                    } else {
                        panels::floating_chrome(
                            &mut paint,
                            atlas,
                            Panel::Timeline,
                            size,
                            panels,
                            state,
                            input,
                        )
                    };
                    (paint, actions)
                };
                let (paint, actions) =
                    draw(&mut self.atlas, &mut panels, &state, &Input::default());
                if !actions.is_empty() {
                    return Err(format!("timing initial paint emitted actions: {actions:?}"));
                }
                let track = panels
                    .timeline
                    .zoom_track
                    .ok_or("timing fixture track missing")?;
                let total = state["project"]["frames"]
                    .as_f64()
                    .ok_or("timing fixture frames missing")? as f32;
                let ppf = track.width() / total;
                let x = track.min[0] + 20. * ppf;
                let y = if expanded {
                    let row = panels
                        .animation_controls
                        .probed_row(None)
                        .ok_or("timing property row invisible")?;
                    (row.min[1] + row.max[1]) * 0.5
                } else {
                    paint
                        .shapes
                        .iter()
                        .filter_map(|shape| match shape {
                            Shape::Procedural {
                                area,
                                kind: Pattern::Diamond,
                                ..
                            } if ((area.min[0] + area.max[0]) * 0.5 - x).abs() < 0.01 => {
                                Some((area.min[1] + area.max[1]) * 0.5)
                            }
                            _ => None,
                        })
                        .max_by(f32::total_cmp)
                        .ok_or("timing summary diamond invisible")?
                };
                let origin = [x, y];
                let end = [x + 15. * ppf, y + 30.];
                for after in [false, true] {
                    if after {
                        for input in [
                            Input {
                                mouse: origin,
                                pressed: Some((MouseButton::Left, origin)),
                                held: Some(MouseButton::Left),
                                ..Default::default()
                            },
                            Input {
                                mouse: [size[0] - 1., size[1] - 1.],
                                released: Some((MouseButton::Left, end)),
                                ..Default::default()
                            },
                        ] {
                            let (_, actions) = draw(&mut self.atlas, &mut panels, &state, &input);
                            if input.released.is_some()
                                && actions
                                    .iter()
                                    .filter(|a| matches!(a, panels::Action::EditBatch(_)))
                                    .count()
                                    != 1
                            {
                                return Err(format!(
                                    "timing release must emit exactly one key batch: {actions:?}"
                                ));
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
                                    panels::Action::EditBatch(commands) => {
                                        state = self.engine.edit(
                                            serde_json::to_string(&commands)
                                                .map_err(|e| e.to_string())?,
                                        )?;
                                    }
                                    panels::Action::Seek(_) => {}
                                    other => {
                                        return Err(format!(
                                            "unexpected timing capture action: {other:?}"
                                        ))
                                    }
                                }
                            }
                        }
                        let moved: Layer = serde_json::from_value(
                            state["project"]["layers"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .find(|l| l["id"] == object)
                                .ok_or("timing fixture disappeared")?
                                .clone(),
                        )
                        .map_err(|e| e.to_string())?;
                        if moved
                            .transform
                            .position
                            .keys
                            .first()
                            .is_none_or(|key| key.frame != 35 || key.value != [20.; 3])
                            || moved
                                .transform
                                .opacity
                                .keys
                                .first()
                                .is_none_or(|key| key.frame != if expanded { 20 } else { 35 })
                            || moved.timeline != layer.timeline
                        {
                            return Err(
                                "timing capture changed values, layer range or wrong key time"
                                    .into(),
                            );
                        }
                    }
                    let (paint, actions) =
                        draw(&mut self.atlas, &mut panels, &state, &Input::default());
                    if !actions.is_empty() {
                        return Err(format!("timing passive paint emitted actions: {actions:?}"));
                    }
                    let mut ui = self.ui_frame(paint, window_size, true);
                    // This isolated capture overrides shell geometry; use the single-view frame protocol.
                    ui.views = None;
                    ui.composition = None;
                    ui.composition_image = None;
                    ui.node_preview = None;
                    let name = format!(
                        "timeline-key-drag-{}-{}",
                        if expanded { "property" } else { "summary" },
                        if after { "after" } else { "before" }
                    );
                    let path = report.with_extension(format!("{name}.png"));
                    self.capture_presented_ui(ui, path.clone())?;
                    let pixels = image::open(&path).map_err(|e| e.to_string())?;
                    let width = (size[0] * pixels.width() as f32 / window_size[0]).round() as u32;
                    let height = (size[1] * pixels.height() as f32 / window_size[1]).round() as u32;
                    pixels
                        .crop_imm(0, 0, width, height)
                        .save(&path)
                        .map_err(|e| e.to_string())?;
                    output.insert(name, serde_json::json!(path));
                }
                state = self.engine.history(0)?;
                undos -= 1;
                let restored = state["project"]["layers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|l| l["id"] == object)
                    .ok_or("timing undo removed fixture")?;
                if serde_json::from_value::<Layer>(restored.clone()).map_err(|e| e.to_string())?
                    != layer
                {
                    return Err("timing undo failed to restore layer".into());
                }
            }
            Ok(())
        })();
        let mut cleanup = vec![];
        if active {
            if let Err(e) = self.engine.history(4) {
                cleanup.push(e);
            }
        }
        for _ in 0..undos {
            if let Err(e) = self.engine.history(0) {
                cleanup.push(e);
                break;
            }
        }
        match self.engine.state() {
            Ok(state) if state["project"] == before["project"] => {}
            Ok(_) => cleanup.push("timing capture did not restore project".into()),
            Err(e) => cleanup.push(e),
        }
        match (result, cleanup.is_empty()) {
            (Err(primary), false) => Err(format!(
                "{primary}; timing capture cleanup: {}",
                cleanup.join("; ")
            )),
            (Err(primary), true) => Err(primary),
            (Ok(()), false) => Err(format!("timing capture cleanup: {}", cleanup.join("; "))),
            (Ok(()), true) => Ok(()),
        }
    }
}
