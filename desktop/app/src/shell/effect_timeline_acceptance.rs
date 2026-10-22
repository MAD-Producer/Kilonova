//! Native GPU screenshots of independently authored node-effect tracks.
use crate::*;
use motion_ui::{
    dock::Panel,
    paint::{Pattern, Shape},
    MouseButton,
};

impl Shell {
    pub(super) fn capture_effect_timeline_screens(
        &mut self,
        window_size: [f32; 2],
        report: &std::path::Path,
        output: &mut serde_json::Map<String, Value>,
    ) -> Result<(), String> {
        let graph = motion_graph::builtin_examples()
            .iter()
            .find(|e| e.id == "soft_sphere_motion")
            .ok_or("sphere fixture missing")?
            .load()
            .map_err(|e| e.to_string())?;
        let compiled =
            motion_graph::compile(&graph, &Default::default()).map_err(|e| e.to_string())?;
        let bytes = motion_graph::compiled_package_bytes(&compiled.manifest, &compiled.source)
            .map_err(|e| e.to_string())?;
        let package =
            motion_effects::EffectPackage::from_bytes(bytes).map_err(|e| e.to_string())?;
        let catalogue =
            serde_json::json!({"packages":[{"hash":package.hash,"manifest":package.manifest}]});
        let mut project =
            motion_core::Project::new(640, 360, 30, 180).map_err(|e| e.to_string())?;
        let mut layer = Layer::solid(
            7,
            if self.workspace.locale == i18n::Locale::Zh {
                "节点效果动画"
            } else {
                "Node effect animation"
            },
            [100.; 2],
            [100., 100., 0.],
            [1.; 4],
        );
        let mut effect = motion_core::EffectInstance::new(
            1,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            &package.manifest.effects[0],
            [100.; 2],
        );
        let speed = effect
            .params
            .get_mut("speed")
            .ok_or("speed fixture missing")?;
        speed
            .track
            .upsert(20, [0.25, 0., 0., 0.], motion_core::Ease::Linear)
            .map_err(|e| e.to_string())?;
        speed
            .track
            .upsert(100, [1.5, 0., 0., 0.], motion_core::Ease::Linear)
            .map_err(|e| e.to_string())?;
        layer.effects.push(effect);
        project.layers.push(layer);
        project.rebuild_plugin_dependencies();
        let mut engine = motion_core::Engine::new(project).map_err(|e| e.to_string())?;
        let original = engine.project().clone();
        for floating in [false, true] {
            let size = if floating {
                [900., 760.]
            } else {
                [1200., 760.]
            };
            let mut panels = panels::Panels {
                styles: self.panels.styles.clone(),
                locale: self.workspace.locale,
                focused: Some(Panel::Timeline),
                ..Default::default()
            };
            panels.select_only(Some(7));
            panels.effect_controls.catalogue = catalogue.clone().into();
            panels.timeline.expanded.insert(7);
            panels.timeline.collapsed_transforms.insert(7);
            panels.timeline.expanded_effects.insert(7);
            panels.effect_timeline.begin_probe(1, "speed");
            let state = |engine: &motion_core::Engine| serde_json::json!({"root":"effect-timeline-native-fixture","composition":motion_core::composition::MAIN_COMPOSITION,"revision":engine.revision(),"frame":20,"project":engine.project(),"timeline_layers":engine.project().timeline_layers(20.).unwrap()});
            let draw = |atlas: &mut TextAtlas,
                        panels: &mut panels::Panels,
                        engine: &mut motion_core::Engine,
                        input: &Input|
             -> Result<PaintList, String> {
                let mut list = PaintList::default();
                list.stylesheet = panels.styles.clone();
                let state = state(engine);
                let actions = if floating {
                    panels::floating_chrome(
                        &mut list,
                        atlas,
                        Panel::Timeline,
                        size,
                        panels,
                        &state,
                        input,
                    )
                } else {
                    panels::chrome(
                        &mut list,
                        atlas,
                        size,
                        &dock::Node::dock(Panel::Timeline.dock()),
                        panels,
                        &state,
                        input,
                    )
                };
                for action in actions {
                    match action {
                        panels::Action::Edit(c) => engine.apply(c).map_err(|e| e.to_string())?,
                        panels::Action::EditBatch(cs) => {
                            engine.apply_batch(cs).map_err(|e| e.to_string())?;
                        }
                        panels::Action::History(2) => {
                            engine.begin_gesture().map_err(|e| e.to_string())?
                        }
                        panels::Action::History(3) => {
                            engine.end_gesture(true).map_err(|e| e.to_string())?
                        }
                        panels::Action::History(4) => {
                            engine.end_gesture(false).map_err(|e| e.to_string())?
                        }
                        panels::Action::Seek(_) => {}
                        other => {
                            return Err(format!("effect capture unexpected action: {other:?}"))
                        }
                    }
                }
                Ok(list)
            };
            draw(&mut self.atlas, &mut panels, &mut engine, &Input::default())?;
            let caption = panels
                .effect_timeline
                .probe
                .rect()
                .ok_or("effect track caption not visible")?;
            let track = panels
                .timeline
                .zoom_track
                .ok_or("effect timeline time lane missing")?;
            let point = [
                track.min[0] + 20. * track.width() / 180.,
                (caption.min[1] + caption.max[1]) * 0.5,
            ];
            draw(
                &mut self.atlas,
                &mut panels,
                &mut engine,
                &Input {
                    mouse: point,
                    pressed: Some((MouseButton::Left, point)),
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
            )?;
            draw(
                &mut self.atlas,
                &mut panels,
                &mut engine,
                &Input {
                    mouse: point,
                    released: Some((MouseButton::Left, point)),
                    ..Default::default()
                },
            )?;
            draw(
                &mut self.atlas,
                &mut panels,
                &mut engine,
                &Input {
                    keys: vec![(motion_ui::Key::F9, Default::default())],
                    ..Default::default()
                },
            )?;
            if engine.project().layers[0].effects[0].params["speed"]
                .track
                .keys[0]
                .curve
                .is_none()
            {
                return Err("effect capture F9 did not modify real easing".into());
            }
            let surface = if floating { "floating" } else { "main" };
            for mode in [false, true] {
                if mode {
                    panels.timeline.set_graph_mode(true);
                }
                let paint = draw(&mut self.atlas, &mut panels, &mut engine, &Input::default())?;
                let track = panels
                    .timeline
                    .zoom_track
                    .ok_or("effect graph lane missing")?;
                let x = track.min[0] + 20. * track.width() / 180.;
                let point = if mode {
                    paint
                        .shapes
                        .iter()
                        .filter_map(|s| match s {
                            Shape::Procedural {
                                area,
                                kind: Pattern::Diamond,
                                ..
                            } if ((area.min[0] + area.max[0]) * 0.5 - x).abs() < 0.1
                                && area.min[1] > caption.min[1] - size[1] =>
                            {
                                Some([
                                    (area.min[0] + area.max[0]) * 0.5,
                                    (area.min[1] + area.max[1]) * 0.5,
                                ])
                            }
                            _ => None,
                        })
                        .max_by(|a, b| a[1].total_cmp(&b[1]))
                        .ok_or("effect graph key not painted")?
                } else {
                    point
                };
                for menu in [false, true] {
                    let input = if menu {
                        Input {
                            mouse: point,
                            pressed: Some((MouseButton::Right, point)),
                            ..Default::default()
                        }
                    } else {
                        Input::default()
                    };
                    let paint = draw(&mut self.atlas, &mut panels, &mut engine, &input)?;
                    let mut frame = self.ui_frame(paint, window_size, true);
                    frame.composition = None;
                    frame.composition_image = None;
                    frame.node_preview = None;
                    let name = format!(
                        "effect-timeline-{surface}-{}{}",
                        if mode { "graph" } else { "parameters" },
                        if menu { "-menu" } else { "" }
                    );
                    let path = report.with_extension(format!("{name}.png"));
                    self.engine.capture_ui(frame, path.clone())?;
                    let pixels = image::open(&path).map_err(|e| e.to_string())?;
                    let width = (size[0] * pixels.width() as f32 / window_size[0]).round() as u32;
                    let height = (size[1] * pixels.height() as f32 / window_size[1]).round() as u32;
                    pixels
                        .crop_imm(0, 0, width, height)
                        .save(&path)
                        .map_err(|e| e.to_string())?;
                    output.insert(name, serde_json::json!(path));
                }
                draw(
                    &mut self.atlas,
                    &mut panels,
                    &mut engine,
                    &Input {
                        keys: vec![(motion_ui::Key::Escape, Default::default())],
                        ..Default::default()
                    },
                )?;
            }
            engine.undo().map_err(|e| e.to_string())?;
            if engine.project() != &original {
                return Err("effect capture undo did not restore authored project".into());
            }
        }
        Ok(())
    }
}
