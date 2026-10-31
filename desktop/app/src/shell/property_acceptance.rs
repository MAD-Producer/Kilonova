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
mod modified;

#[cfg(test)]
#[test]
fn typed_capture_fixtures_paint_clickable_properties_without_media_io() {
    for locale in [i18n::Locale::En,i18n::Locale::Zh] {
        for (audio,panel) in [(false,Panel::Properties),(false,Panel::Timeline),(true,Panel::Timeline)] {
            let state = Shell::typed_selection_capture_state(
                &serde_json::json!({"root":"capture-fixture","composition":"comp-main","revision":1,"frame":40}),7,audio).unwrap();
            let mut panels = panels::Panels {locale,focused:Some(panel),..Default::default()};
            panels.select_only(Some(7));
            panels.properties.transform_collapsed=true;
            panels.timeline.expanded.insert(7);
            panels.timeline.collapsed_transforms.insert(7);
            if audio {panels.audio_controls.selection_probe.start();} else {
                panels.path_controls.begin_shape_probe();
                panels.path_controls.reveal_vector_item(&state,7,2);
                panels.timeline.vector_open.extend([(7,0,0),(7,1,0),(7,2,2)]);
            }
            let mut atlas=TextAtlas::from_system(18.).unwrap();
            let size=if panel==Panel::Properties {[360.,560.]} else {[900.,520.]};
            let actions=panels::floating_chrome(&mut PaintList::default(),&mut atlas,panel,size,
                &mut panels,&state,&Input::default());
            assert!(actions.is_empty(),"{actions:?}");
            let hit=if audio {panels.audio_controls.selection_probe.rect()}
                else {panels.path_controls.shape_probe_rect()}.expect("visible typed property caption");
            let point=[(hit.min[0]+hit.max[0])*0.5,(hit.min[1]+hit.max[1])*0.5];
            let actions=panels::floating_chrome(&mut PaintList::default(),&mut atlas,panel,size,
                &mut panels,&state,&Input {mouse:point,pressed:Some((MouseButton::Left,point)),..Default::default()});
            assert!(actions.is_empty(),"{actions:?}");
            assert!(panels.animation_controls.foreign_track_selected());
        }
    }
}

impl Shell {
    fn typed_selection_capture_state(state: &Value, object: u64, audio: bool) -> Result<Value, String> {
        let mut project = motion_core::Project::new(640, 360, 30, 120).map_err(|e| e.to_string())?;
        let mut layer = Layer::solid(object, "Selection fixture", [100.;2], [100.,100.,0.], [1.;4]);
        if audio {
            layer.size = [0.;2];
            layer.transform = motion_core::Transform::new([0.;3]);
            project.audio_assets.push(motion_core::AudioAsset {id:1,path:"assets/selection-fixture.wav".into(),
                mime:"audio/wav".into(),bytes:768044,track:0,sample_rate:48000,channels:2,
                sample_frames:192000,duration_us:4_000_000});
            layer.content = motion_core::Content::Audio {audio:motion_core::AudioClip::new(1)};
        } else {
            use motion_core::vector::{VectorContent, VectorSource, FillRule, groups::{VectorGroup,GroupItem,Composite}};
            layer.content = motion_core::Content::Vector {vector:VectorContent {
                source:VectorSource::Group {group:Box::new(VectorGroup {id:1,name:"Group".into(),
                    transform:Default::default(),items:vec![GroupItem::Fill {id:2,name:"Fill".into(),
                        color:Track::constant([0.4,0.6,0.9,1.]),fill_rule:FillRule::NonZero,composite:Composite::Below}]})},
                fill:None,fill_rule:FillRule::NonZero,stroke:None,trim:None}};
        }
        project.layers.push(layer);
        // Validate authored data, then render its view snapshot only. This visual
        // fixture performs no media IO and does not claim audio decoding coverage.
        let engine = motion_core::Engine::new(project).map_err(|e| e.to_string())?;
        let mut view = state.clone();
        view["project"] = serde_json::to_value(engine.project()).map_err(|e| e.to_string())?;
        view["timeline_layers"] = serde_json::to_value(engine.project().timeline_layers(40.).unwrap()).map_err(|e| e.to_string())?;
        view["sampledLayers"] = engine.project().layers.iter().map(|l| -> motion_core::Result<_> {
            let local = l.local_frame(40.);
            Ok(serde_json::json!({"id":l.id,"position":l.transform.position.sample(local)?,
                "rotation":l.transform.rotation.sample(local)?,"scale":l.transform.scale.sample(local)?,
                "anchor":l.transform.anchor.sample(local)?,"opacity":l.transform.opacity.sample(local)?,
                "active":l.active(40.,engine.project().frames),"three_d":l.three_d}))
        }).collect::<motion_core::Result<Vec<_>>>().map_err(|e| e.to_string())?.into();
        view["revision"] = (state["revision"].as_u64().unwrap_or(0).saturating_add(1)).into();
        Ok(view)
    }

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
                tangents: Default::default(), id: 1,
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
                ("selection-property-main", Panel::Properties, [360., 560.]),
                ("selection-property-float", Panel::Properties, [360., 560.]),
                ("selection-property-narrow", Panel::Properties, [220., 560.]),
                ("selection-property-number", Panel::Properties, [360., 560.]),
                ("selection-anchor-main", Panel::Properties, [360., 560.]),
                ("selection-timeline-main", Panel::Timeline, [900., 520.]),
                ("selection-timeline-float", Panel::Timeline, [900., 520.]),
                ("selection-timeline-graph", Panel::Timeline, [900., 520.]),
                ("typed-selection-mask-property", Panel::Properties, [360., 560.]),
                ("typed-selection-mask-timeline", Panel::Timeline, [900., 520.]),
                ("typed-selection-shape-property", Panel::Properties, [360., 560.]),
                ("typed-selection-shape-timeline", Panel::Timeline, [900., 520.]),
                ("typed-selection-audio-timeline", Panel::Timeline, [900., 520.]),
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
                ("layer-matte-menu", Panel::Timeline, [900., 520.]),
                ("layer-matte-menu-narrow", Panel::Timeline, [720., 520.]),
                ("timeline-columns-main-menu", Panel::Timeline, [900., 520.]),
                ("diagnostics-main-menu", Panel::Timeline, [900., 520.]),
                ("mask-mode-menu", Panel::Properties, [360., 560.]),
                ("mask-mode-menu-narrow", Panel::Properties, [220., 560.]),
                ("graph-raw-readonly", Panel::Timeline, [900., 520.]),
                ("mask-handles-selected", Panel::Composition, [900., 520.]),
            ] {
                let state = if name.starts_with("typed-selection-shape") || name.starts_with("typed-selection-audio") {
                    Self::typed_selection_capture_state(&state, object, name.contains("audio"))?
                } else {state.clone()};
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
                if name.starts_with("selection-") {
                    let anchor = name == "selection-anchor-main";
                    panels.animation_controls.begin_row_probe(object,
                        if anchor { Property::Anchor } else { Property::Position },
                        if anchor { None } else { Some(Axis::X) });
                    panels.timeline.set_graph_mode(name.ends_with("-graph"));
                }
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
                if name.starts_with("typed-selection-") {
                    panels.properties.transform_collapsed = true;
                    panels.timeline.collapsed_transforms.insert(object);
                    if name.contains("mask") {
                        panels.path_controls.masks_open = true;
                        panels.path_controls.selection_probe.start();
                    } else if name.contains("shape") {
                        panels.path_controls.begin_shape_probe();
                        panels.path_controls.reveal_vector_item(&state, object, 2);
                        panels.timeline.vector_open.extend([(object,0,0),(object,1,0),(object,2,2)]);
                    } else {
                        panels.audio_controls.selection_probe.start();
                    }
                }
                let mut style_context = PaintList::default();
                style_context.stylesheet = panels.styles.clone();
                let styles = style_context.styles();
                let row = styles.metric("row.height", 22.);
                let pad = styles.metric("padding.panel", 8.);
                let mut draw =
                    |panels: &mut panels::Panels, input: &Input| -> Result<PaintList, String> {
                        let mut paint = PaintList::default();
                        paint.stylesheet = panels.styles.clone();
                        let actions = if name.ends_with("-main-menu") || name == "property-key-navigation-main" || name.starts_with("selection-") && name.ends_with("-main") {
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
                if name.starts_with("typed-selection-") {
                    let hit = if name.contains("mask") {panels.path_controls.selection_probe.rect()}
                        else if name.contains("shape") {panels.path_controls.shape_probe_rect()}
                        else {panels.audio_controls.selection_probe.rect()}
                        .ok_or_else(|| format!("{name} did not paint a visible typed property"))?;
                    let point = [(hit.min[0]+hit.max[0])*0.5,(hit.min[1]+hit.max[1])*0.5];
                    draw(&mut panels, &Input {mouse:point,pressed:Some((MouseButton::Left,point)),..Default::default()})?;
                    if !panels.animation_controls.foreign_track_selected() {
                        return Err(format!("{name} click did not capture typed property ownership"));
                    }
                } else if name.starts_with("selection-") {
                    let anchor = name == "selection-anchor-main";
                    let component = name.ends_with("-number").then_some(0);
                    // Read actual painted hit geometry, including mask rows and
                    // clipping. Scroll normally if styled rows move it offscreen.
                    for _ in 0..4 {
                        if panels.animation_controls.probed_row(component).is_some() { break; }
                        draw(&mut panels, &Input {mouse:[size[0] * 0.25, size[1] * 0.5],
                            wheel:[0., row * 3.], ..Default::default()})?;
                    }
                    let hit = panels.animation_controls.probed_row(component)
                        .ok_or_else(|| format!("{name} rendered property target is not visible"))?;
                    let point = [(hit.min[0] + hit.max[0]) * 0.5, (hit.min[1] + hit.max[1]) * 0.5];
                    draw(&mut panels, &Input {mouse:point, pressed:Some((MouseButton::Left, point)), ..Default::default()})?;
                    if panels.animation_controls.property != Some(if anchor { Property::Anchor } else { Property::Position })
                        || panels.animation_controls.axis != if anchor { None } else { Some(Axis::X) }
                    {
                        return Err(format!("{name} actual click did not select the expected property/axis"));
                    }
                    if name.ends_with("-number") {
                        draw(&mut panels, &Input {keys:vec![(Key::Escape, Modifiers::default())], ..Default::default()})?;
                    }
                } else if name.starts_with("property-merge-") {
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
                } else if name == "timeline-columns-main-menu" {
                    panels.menu = Some(6);
                } else if name == "diagnostics-main-menu" {
                    panels.menu = Some(8);
                } else if name.starts_with("layer-") {
                    panels
                        .layer_actions
                        .open(panels.selected, [100., 400.], &state);
                    draw(&mut panels, &Input::default())?;
                    if name.starts_with("layer-matte-menu") {
                        for _ in 0..3 {
                            draw(&mut panels, &Input {keys:vec![(Key::Down,Modifiers::default())],..Default::default()})?;
                        }
                    }
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
                // This isolated capture overrides shell geometry; use the single-view frame protocol.
                ui.views = None;
                ui.composition = None;
                ui.node_preview = None;
                if panel == Panel::Composition {
                    ui.views = None;
                    ui.composition = Some([
                        8. * self.scale,
                        38. * self.scale,
                        (size[0] - 16.) * self.scale,
                        (size[1] - 94.) * self.scale,
                    ]);
                }
                let path = report.with_extension(format!("{name}.png"));
                self.capture_presented_ui(ui, path.clone())?;
                let pixels = image::open(&path).map_err(|e| e.to_string())?;
                let width = (size[0] * pixels.width() as f32 / window_size[0]).round() as u32;
                let height = (size[1] * pixels.height() as f32 / window_size[1]).round() as u32;
                pixels
                    .crop_imm(0, 0, width, height)
                    .save(&path)
                    .map_err(|e| e.to_string())?;
                output.insert(name.into(), serde_json::json!(path));
            }
            self.capture_observation_three_d_screens(window_size, report, output, object)?;
            self.capture_shape_resize_screens(window_size, report, output)?;
            self.capture_effect_selection_screens(window_size, report, output, object)?;
            self.capture_timeline_key_drag_screens(window_size, report, output)?;
            self.capture_effect_timeline_screens(window_size, report, output)?;
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

    fn capture_observation_three_d_screens(
        &mut self,
        window_size: [f32; 2],
        report: &std::path::Path,
        output: &mut serde_json::Map<String, Value>,
        object: u64,
    ) -> Result<(), String> {
        fn draw(atlas: &mut TextAtlas, panels: &mut panels::Panels, state: &Value, input: &Input)
            -> (PaintList, Vec<panels::Action>) {
            let mut paint = PaintList::default();
            paint.stylesheet = panels.styles.clone();
            let actions = panels::floating_chrome(&mut paint, atlas, Panel::Timeline,
                [900., 520.], panels, state, input);
            (paint, actions)
        }
        let before = self.engine.state()?;
        let original_view = if before["observing"] != true {0} else {match before["observationView"].as_str() {
            Some("top")=>2, Some("side")=>3, _=>1,
        }};
        let mut changed = false;
        let captured = (|| {
            let mut state = self.engine.view(2)?;
            let mut panels = panels::Panels {locale:self.workspace.locale,styles:self.panels.styles.clone(),
                focused:Some(Panel::Timeline),..Default::default()};
            panels.select_only(Some(object));
            panels.timeline.three_d_probe.start();
            for enabled in [false,true] {
                let (_, actions) = draw(&mut self.atlas, &mut panels, &state, &Input::default());
                if !actions.is_empty() {return Err(format!("3D observation idle emitted actions: {actions:?}"));}
                if enabled {
                    let hit=panels.timeline.three_d_probe.rect().ok_or("3D observation switch not visible")?;
                    let point=[(hit.min[0]+hit.max[0])*0.5,(hit.min[1]+hit.max[1])*0.5];
                    let (_,actions)=draw(&mut self.atlas,&mut panels,&state,
                        &Input {mouse:point,pressed:Some((MouseButton::Left,point)),..Default::default()});
                    let [panels::Action::Edit(command @ Command::SetLayer3d {object:target,enabled:true})]=actions.as_slice() else {
                        return Err(format!("3D observation click emitted unexpected actions: {actions:?}"));
                    };
                    if *target!=object {return Err("3D observation click targeted another layer".into());}
                    state=self.engine.edit(serde_json::to_string(command).map_err(|e|e.to_string())?)?;
                    changed=true;
                }
                let layer=state["project"]["layers"].as_array().and_then(|layers|
                    layers.iter().find(|layer|layer["id"].as_u64()==Some(object))).ok_or("3D capture layer missing")?;
                if layer["three_d"]!=enabled || state["observing"]!=true || state["observationView"]!="top" {
                    return Err("3D observation click changed the view or did not update the model".into());
                }
                let (paint,actions)=draw(&mut self.atlas,&mut panels,&state,&Input::default());
                if !actions.is_empty() {return Err(format!("3D observation repaint emitted actions: {actions:?}"));}
                let name=if enabled {"timeline-3d-observation-on"} else {"timeline-3d-observation-off"};
                let path=report.with_extension(format!("{name}.png"));
                let mut ui=self.ui_frame(paint,window_size,true);
                ui.composition=None;
                ui.node_preview=None;
                self.capture_presented_ui(ui,path.clone())?;
                let pixels=image::open(&path).map_err(|e|e.to_string())?;
                let width=(900.*pixels.width() as f32/window_size[0]).round() as u32;
                let height=(520.*pixels.height() as f32/window_size[1]).round() as u32;
                pixels.crop_imm(0,0,width,height).save(&path).map_err(|e|e.to_string())?;
                output.insert(name.into(),serde_json::json!(path));
            }
            Ok(())
        })();
        // Restore both authored history and the independent preview camera even
        // when capture fails. The enclosing fixture still owns one undo entry.
        let undo = if changed {self.engine.history(0).map(|_|())} else {Ok(())};
        let view = self.engine.view(original_view).and_then(|state| {
            if state["project"]==before["project"] {Ok(())}
            else {Err("3D observation capture did not restore the project".into())}
        });
        let cleanup=match (undo,view) {
            (Err(a),Err(b))=>Err(format!("{a}; view restore failed: {b}")),
            (Err(a),_)|(_,Err(a))=>Err(a),_=>Ok(()),
        };
        match (captured,cleanup) {
            (Err(a),Err(b))=>Err(format!("{a}; 3D capture cleanup failed: {b}")),
            (Err(a),_)|(_,Err(a))=>Err(a),_=>Ok(()),
        }
    }

    pub(super) fn present_property_capture_scene(
        &mut self,
        size: [f32; 2],
        viewport: [f32; 4],
    ) -> Result<(), String> {
        // CaptureUi composites the retained GPU scene. Present the fixture first,
        // and present the restored project during cleanup to avoid stale pixels.
        let mut ui = self.ui_frame(PaintList::default(), size, true);
        ui.views = None;
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
