//! Value curves share the timeline ruler and use the model's actual sampler.
use super::animation_data as data;
use super::types::ScrubRuler;
use super::*;
use motion_core::{Axis, Track};

enum Samples {
    Anchor(Track<[f32; 2]>),
    Scalar(Track<f32>),
    Vector(Track<[f32; 3]>),
}
impl Samples {
    fn fit_bounds(&self,interval:[f64;2],layer_offset:f64,components:&[usize],speed:bool,fps:f64,
        display_scale:f32,size:[f32;2],tolerance:f64)
        ->Result<super::graph_ranges::Bounds,super::graph_adaptive::Failure<motion_core::Error>> {
        use super::graph_adaptive::{self,Failure};
        use super::graph_view::Range;
        let [start,end]=interval;
        let x=Range::fitted(start,end,0.05,1.).ok_or(Failure::InvalidGeometry)?;
        let value=|frame:f64,component:usize|->motion_core::Result<f64> {
            Ok(f64::from((if speed {self.velocity(frame-layer_offset,component,fps)}
                else {self.value(frame-layer_offset,component)?})*display_scale))
        };
        let mut low=f64::INFINITY;let mut high=f64::NEG_INFINITY;
        for component in components.iter().copied() {
            let mut sample=|frame|value(frame,component);
            let bounds=match self {
                Self::Anchor(track)=>graph_adaptive::extrema(super::graph_sampling::knots(track,start,end,layer_offset),&mut sample),
                Self::Scalar(track)=>graph_adaptive::extrema(super::graph_sampling::knots(track,start,end,layer_offset),&mut sample),
                Self::Vector(track)=>graph_adaptive::extrema(super::graph_sampling::knots(track,start,end,layer_offset),&mut sample),
            }?.ok_or(Failure::InvalidGeometry)?;
            low=low.min(bounds[0]);high=high.max(bounds[1]);
        }
        let preliminary=Range::fitted(low,high,0.1,1.).ok_or(Failure::InvalidGeometry)?;
        low=f64::INFINITY;high=f64::NEG_INFINITY;
        for component in components.iter().copied() {
            let points=self.adaptive(start,end,layer_offset,
                [f64::from(size[0])/x.span(),f64::from(size[1])/preliminary.span()],tolerance,
                |frame|value(frame,component))?;
            for (_,value) in points {low=low.min(f64::from(value));high=high.max(f64::from(value));}
        }
        let y=Range::fitted(low,high,0.1,1.).ok_or(Failure::InvalidGeometry)?;
        Ok(super::graph_ranges::Bounds {x,y})
    }
    fn adaptive(
        &self, start: f64, end: f64, layer_offset: f64, scale: [f64;2], tolerance: f64,
        value: impl FnMut(f64)->motion_core::Result<f64>,
    ) -> Result<Vec<(f64,f32)>,super::graph_adaptive::Failure<motion_core::Error>> {
        let points=match self {
            Self::Anchor(track)=>super::graph_adaptive::sample(super::graph_sampling::knots(track,start,end,layer_offset),scale,tolerance,value),
            Self::Scalar(track)=>super::graph_adaptive::sample(super::graph_sampling::knots(track,start,end,layer_offset),scale,tolerance,value),
            Self::Vector(track)=>super::graph_adaptive::sample(super::graph_sampling::knots(track,start,end,layer_offset),scale,tolerance,value),
        }?;
        Ok(points.into_iter().map(|(frame,value)|(frame,value as f32)).collect())
    }
    fn velocity(&self, frame: f64, component: usize, fps: f64) -> f32 {
        match self {
            Self::Anchor(track) => super::graph_sampling::velocity(track,frame,fps,|v|v[component] as f64) as f32,
            Self::Scalar(track) => {
                super::graph_sampling::velocity(track, frame, fps, |v| v as f64) as f32
            }
            Self::Vector(track) => {
                super::graph_sampling::velocity(track, frame, fps, |v| v[component] as f64) as f32
            }
        }
    }

    fn value(&self, frame: f64, component: usize) -> motion_core::Result<f32> {
        Ok(match self {
            Self::Anchor(track) => track.sample(frame)?[component],
            Self::Scalar(track) => track.sample(frame)?,
            Self::Vector(track) => track.sample(frame)?[component],
        })
    }
}

fn unit(property:Property,speed:bool)->&'static str {
    match (property,speed) {
        (Property::Anchor|Property::Position|Property::Target|Property::Radius,false)=>"px",
        (Property::Anchor|Property::Position|Property::Target|Property::Radius,true)=>"px/s",
        (Property::Rotation|Property::Roll|Property::Fov|Property::Azimuth|Property::Elevation,false)=>"°",
        (Property::Rotation|Property::Roll|Property::Fov|Property::Azimuth|Property::Elevation,true)=>"°/s",
        (_,false)=>"%",(_,true)=>"%/s",
    }
}

fn fit_intervals(panels:&Panels,state:&Value,target:&motion_core::KeyframeTarget,source:&Value,offset:i64)
    ->([f64;2],Option<[f64;2]>) {
    let mut all:Option<[f64;2]>=None;let mut selected:Option<[f64;2]>=None;
    let status=panels.selected_keys.status(state);
    let legacy=panels.animation_controls.user_key_in(state,panels.selected);
    for key in source["keys"].as_array().into_iter().flatten() {
        let Some(frame)=key["local_frame"].as_i64().and_then(|frame|frame.checked_add(offset)).or_else(||key["frame"].as_i64())
            .and_then(|frame|u32::try_from(frame).ok()).filter(|frame|f64::from(*frame)<=last_frame(state)) else {continue;};
        let next=f64::from(frame);
        all=Some(all.map_or([next,next],|[low,high]|[low.min(next),high.max(next)]));
        let chosen=match status {
            super::key_selection::Status::Current=>panels.selected_keys.contains(&super::key_selection::Key {target:target.clone(),frame}),
            super::key_selection::Status::Unowned=>panels.timeline.graph_box.has_selection() || legacy.is_some_and(|key| {
                key.frame==frame && *target==(motion_core::KeyframeTarget::Property {object:key.object,property:key.property,axis:key.axis})
            }),
            _=>false,
        };
        if chosen {selected=Some(selected.map_or([next,next],|[low,high]|[low.min(next),high.max(next)]));}
    }
    (all.unwrap_or([0.,last_frame(state)+1.]),selected)
}

fn observe_early_return(panels: &mut Panels, reason: &'static str) {
    panels.timeline.graph_ranges.no_property_plot();
    let active = panels
        .timeline
        .graph_drag
        .as_ref()
        .is_some_and(|d| d.begun())
        || panels.timeline.key_drag.as_ref().is_some_and(|d| d.begun());
    if active {
        let _ = observe!(
            "timeline.graph_early_return",
            Err::<(), _>(reason),
            "selected={:?} property={:?} axis={:?} handle_drag={} key_drag={}",
            panels.selected,
            panels.animation_controls.property,
            panels.animation_controls.axis,
            panels.timeline.graph_drag.is_some(),
            panels.timeline.key_drag.is_some()
        );
    }
}

pub(super) fn render(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    offset: f64,
    ppf: f32,
    out: &mut Vec<Action>,
) {
    if panels.timeline.comparison.enabled {
        let footer=list.styles().metric("field.height",18.).min(area.height());
        let body=Rect::new(area.min[0],area.min[1],area.width(),(area.height()-footer).max(0.));
        let content=super::curve_comparison::toolbar(list, atlas, body, panels, state, input, out);
        if panels.timeline.comparison.enabled {
            panels.timeline.graph_ranges.no_property_plot();
            let neutral=Input::default();
            let graph_input=if panels.timeline.comparison.popup_open(){&neutral}else{input};
            super::curve_comparison::render(list, atlas, content, panels, state, graph_input, offset, ppf, out);
            return;
        }
    }
    if super::effect_timeline::graph::render(
        list, atlas, area, panels, state, input, offset, ppf, out,
    ) {
        panels.timeline.graph_ranges.no_property_plot();
        return;
    }
    let styles = list.styles();
    let colors = styles.colors();

    // A project/selection/channel transition cancels the old transaction once.
    // It must not leave a live drag behind a normal empty-selection projection.
    if panels.animation_controls.track_selection(state, panels.selected)
        == super::animation_controls::TrackSelection::Invalid
    {
        let begun = panels.timeline.graph_drag.take().is_some_and(|drag| drag.begun())
            | panels.timeline.key_drag.take().is_some_and(|drag| drag.begun())
            | panels.timeline.graph_box.cancel();
        if begun {
            out.push(Action::History(4));
        }
    }

    if input.pressed.is_some() {
        if panels
            .timeline
            .key_drag
            .as_ref()
            .is_some_and(|d| !d.begun())
        {
            panels.timeline.key_drag = None;
        }
        if panels
            .timeline
            .graph_drag
            .as_ref()
            .is_some_and(|d| !d.begun())
        {
            panels.timeline.graph_drag = None;
        }
    }
    let mut graph = list.child();
    let fps = state["project"]["fps"].as_f64().unwrap_or(30.);
    paint::rect(&mut graph, area, colors.SUNKEN);
    let Some(object) = panels.selected.filter(|id| *id != 0) else {
        label(
            &mut graph,
            atlas,
            area.shrink(8.),
            panels.locale.t(T::SelectLayer),
            true,
        );
        graph.clip_to(area);
        list.append(graph);
        observe_early_return(panels, "no_selection");
        return;
    };
    let Some(layer) = state["project"]["layers"]
        .as_array()
        .and_then(|ls| ls.iter().find(|l| l["id"].as_u64() == Some(object)))
    else {
        observe_early_return(panels, "missing_layer");
        return;
    };
    if panels.animation_controls.track_selection(state, Some(object))
        == super::animation_controls::TrackSelection::Invalid
    {
        label(&mut graph, atlas, area.shrink(styles.metric("padding.panel", 8.)),
            if panels.locale == Locale::Zh { "请选择属性" } else { "Select a property" }, true);
        graph.clip_to(area);
        list.append(graph);
        observe_early_return(panels, "invalid_property_selection");
        return;
    }
    let property = panels
        .animation_controls
        .property
        .unwrap_or(Property::Position);
    if layer["content"]["kind"].as_str() == Some("audio")
        || !super::timeline::property_visible(panels, state, object, property)
    {
        label(
            &mut graph,
            atlas,
            area.shrink(8.),
            panels.locale.t(T::NoKeys),
            true,
        );
        graph.clip_to(area);
        list.append(graph);
        observe_early_return(panels, "property_hidden_or_audio");
        return;
    }
    let Some(timeline) = data::timeline(state, object) else {
        observe_early_return(panels, "missing_timeline");
        return;
    };
    let root = &timeline["properties"][data::name(property)];
    let separated = root["separated"].as_bool() == Some(true);
    let visible_axes = if property.is_scalar() {
        vec![0]
    } else if property == Property::Anchor {
        vec![0, 1]
    } else if layer["three_d"].as_bool() == Some(true) {
        vec![0, 1, 2]
    } else if property == Property::Rotation {
        vec![2]
    } else {
        vec![0, 1]
    };
    let axis = if separated {
        Some(
            panels
                .animation_controls
                .axis
                .filter(|a| visible_axes.contains(&a.index()))
                .unwrap_or([Axis::X, Axis::Y, Axis::Z][visible_axes[0]]),
        )
    } else {
        None
    };
    panels
        .animation_controls
        .display_track(object, property, axis);
    let title = match property {
        Property::Anchor => T::Anchor,
        Property::Position => T::Position,
        Property::Scale => T::Scale,
        Property::Rotation => T::Rotation,
        Property::Opacity => T::Opacity, Property::Target => T::CameraTarget,
        Property::Roll => T::CameraRoll, Property::Fov => T::CameraFov,
        Property::Radius => T::CameraRadius, Property::Azimuth => T::CameraAzimuth, Property::Elevation => T::CameraElevation,
    };
    let axis = panels.animation_controls.axis;
    let Some(source) = data::channel(state, object, property, axis) else {
        observe_early_return(panels, "missing_channel");
        return;
    };
    let layer_offset = timeline["offset_frame"].as_i64().unwrap_or(0);
    let samples = if property.is_scalar() || axis.is_some() {
        data::track::<f32>(source, layer_offset).map(Samples::Scalar)
    } else if property == Property::Anchor {
        data::track::<[f32; 2]>(source, layer_offset).map(Samples::Anchor)
    } else {
        data::track::<[f32; 3]>(source, layer_offset).map(Samples::Vector)
    };
    let Some(samples) = samples else {
        label(
            &mut graph,
            atlas,
            area.shrink(8.),
            if panels.locale == Locale::Zh {
                "这个通道没有可显示的数据"
            } else {
                "No displayable data in this channel"
            },
            true,
        );
        graph.clip_to(area);
        list.append(graph);
        observe_early_return(panels, "unreadable_track");
        return;
    };
    let components = if axis.is_some() || property.is_scalar() {
        vec![0]
    } else {
        let component = visible_axes
            .iter()
            .copied()
            .find(|c| *c == panels.timeline.graph_component)
            .unwrap_or(visible_axes[0]);
        panels.timeline.graph_component = component;
        vec![component]
    };
    let plot = Rect::new(
        area.min[0],
        area.min[1] + 32.,
        area.width().max(1.),
        (area.height() - 102.).max(1.),
    );
    let end = (offset + plot.width() as f64 / ppf.max(1e-6) as f64).min(last_frame(state));
    let scale = data::display_scale(state,object,property,components[0]);
    let speed_graph = panels.timeline.speed_graph;
    let unit=unit(property,speed_graph);
    let target=motion_core::KeyframeTarget::Property {object,property,axis};
    let identity=super::graph_ranges::Identity {surface:super::graph_ranges::Surface::Timeline,
        target:Some(target.clone()),component:Some(components[0]),speed:speed_graph,unit:unit.into()};
    let sample = |frame: f64, component: usize| -> motion_core::Result<f32> {
        Ok(if speed_graph {
            samples.velocity(frame - layer_offset as f64, component, fps)
        } else {
            samples.value(frame - layer_offset as f64, component)?
        } * scale)
    };
    let points = components
        .iter()
        .map(|component| {
            (0..=256)
                .map(|i| {
                    let frame = offset + (end - offset).max(0.) * i as f64 / 256.;
                    sample(frame, *component).map(|value| (frame, value))
                })
                .collect::<motion_core::Result<Vec<_>>>()
        })
        .collect::<motion_core::Result<Vec<_>>>();
    let Ok(points) = observe!("timeline.graph_samples", points,
        "object={object} property={property:?} axis={axis:?} frame_start={offset} frame_end={end}") else {
        graph.clip_to(area);
        list.append(graph);
        observe_early_return(panels, "sampling_failed");
        return;
    };
    let min = points
        .iter()
        .flatten()
        .map(|p| p.1)
        .fold(f32::INFINITY, f32::min);
    let max = points
        .iter()
        .flatten()
        .map(|p| p.1)
        .fold(f32::NEG_INFINITY, f32::max);
    let margin = ((max - min) * 0.1).max(1.);
    let auto_low = min - margin;
    let auto_high = max + margin;
    let old_zoom = if panels.timeline.graph_y_zoom.is_finite() && panels.timeline.graph_y_zoom > 0. {
        panels.timeline.graph_y_zoom
    } else {
        1.
    };
    use super::graph_ranges::Bounds;
    use super::graph_view::Range;
    let initial_half=(auto_high-auto_low)*0.5/old_zoom;
    let initial_center=(auto_high+auto_low)*0.5+panels.timeline.graph_y_offset;
    let automatic=Range::new(offset,offset+f64::from(plot.width())/f64::from(ppf))
        .zip(Range::new(f64::from(initial_center-initial_half),f64::from(initial_center+initial_half)))
        .map(|(x,y)|Bounds {x,y});
    let Ok(automatic)=observe!("timeline.graph_view_bounds",automatic.ok_or("unrepresentable graph range"),
        "object={object} property={property:?} axis={axis:?} phase=display") else {
        graph.clip_to(area);list.append(graph);observe_early_return(panels,"invalid_view_bounds");return;
    };
    let before_view=panels.timeline.graph_ranges.displayed(state,&identity,automatic);
    if area.contains(input.mouse)
        && input.modifiers.control
        && input.modifiers.alt
        && input.wheel[1].is_finite()
        && input.wheel[1] != 0.
        && !panels.timeline.graph_box.editing()
        && panels.timeline.key_drag.is_none()
        && panels.timeline.graph_drag.is_none()
        && !panels.key_gestures.active()
        && !panels.timeline.scrubbing
        && !panels.timeline.graph_ranges.locked_y(&identity)
    {
        let zoom = (old_zoom * (-input.wheel[1] / 240.).exp()).clamp(0.25, 64.);
        let fraction = (plot.max[1] - input.mouse[1]) / plot.height().max(1.) - 0.5;
        let next_offset=panels.timeline.graph_y_offset+
            fraction * (auto_high - auto_low) * (1. / old_zoom - 1. / zoom);
        if next_offset.is_finite() && panels.timeline.graph_ranges.navigate_y(&identity,before_view.y,
            f64::from(zoom/old_zoom),before_view.y.at(f64::from(fraction+0.5))) {
            panels.timeline.graph_y_offset=next_offset;
            panels.timeline.graph_y_zoom=zoom;
        }
    }
    let zoom = if panels.timeline.graph_y_zoom > 0. {
        panels.timeline.graph_y_zoom
    } else {
        1.
    };
    let half = (auto_high - auto_low) * 0.5 / zoom;
    let center = (auto_high + auto_low) * 0.5 + panels.timeline.graph_y_offset;
    let legacy_y=Range::new(f64::from(center-half),f64::from(center+half)).unwrap_or(before_view.y);
    let displayed=panels.timeline.graph_ranges.displayed(state,&identity,Bounds {x:automatic.x,y:legacy_y});
    let fixed = panels
        .timeline
        .graph_drag
        .as_ref()
        .map(|d| d.bounds())
        .or_else(|| panels.timeline.key_drag.as_ref().map(|d| d.bounds()))
        .or_else(|| panels.key_gestures.drag_bounds())
        .or_else(|| panels.timeline.graph_box.bounds());
    let low = fixed.map_or(displayed.y.low as f32, |b| b[0]);
    let high = fixed.map_or(displayed.y.high as f32, |b| b[1]);
    let adaptive = components.iter().map(|component|samples.adaptive(
        offset,end,layer_offset as f64,
        [f64::from(ppf),f64::from(plot.height())/f64::from(high-low)],
        f64::from(styles.metric("graph.sample.error",0.35)),
        |frame|sample(frame,*component).map(f64::from),
    )).collect::<Result<Vec<_>,_>>();
    let Ok(points)=observe!("timeline.graph_adaptive",adaptive,
        "object={object} property={property:?} axis={axis:?} frame_start={offset} frame_end={end} phase=screen_subdivision") else {
        graph.clip_to(area);list.append(graph);observe_early_return(panels,"adaptive_sampling_failed");return;
    };
    let y_of = |value: f32| plot.max[1] - (value - low) / (high - low) * plot.height();
    for i in 0..5 {
        let value = low + (high - low) * i as f32 / 4.;
        let y = y_of(value);
        paint::line(
            &mut graph,
            vec![[plot.min[0], y], [plot.max[0], y]],
            colors.BORDER.with_alpha(0.35),
        );
        label(
            &mut graph,
            atlas,
            Rect::new(area.min[0] + 4., y - 11., 52., 22.),
            &format!("{value:.1}"),
            true,
        );
    }
    label(
        &mut graph,
        atlas,
        Rect::new(area.min[0] + 8., area.min[1] + 2., 50., 24.),
        unit,
        true,
    );
    let box_plot = super::graph_handles::Plot {
        area: plot,
        offset,
        ppf,
        low,
        high,
        speed: panels.timeline.speed_graph,
        fps,
        scale,
    };
    let box_owned = super::graph_box::interact(
        &styles,
        panels,
        state,
        source,
        object,
        property,
        axis,
        components[0],
        box_plot,
        &points[0],
        input,
        layer["locked"] == true || state["observing"] == true,
        out,
    );
    super::key_selection::ui::plot(panels, plot, low, high);
    super::key_selection::ui::all_channel(panels,state,motion_core::KeyframeTarget::Property {object,property,axis},source);
    if box_owned { super::key_selection::ui::block_background(panels); }
    let neutral_curve = Input::default();
    let curve_input = if box_owned { &neutral_curve } else { input };
    let mut lines = list.child();
    let mut clicked_key = None;
    let selected = panels.animation_controls.user_key_in(state, panels.selected);
    for (index, curve) in points.iter().enumerate() {
        let color = if index == 0 {
            styles.color("curve")
        } else {
            colors.TRACKS[axis.map_or(components[index], |a| a.index()) % colors.TRACKS.len()]
        };
        paint::line_with_width(
            &mut lines,
            curve
                .iter()
                .map(|(frame, value)| [plot.min[0] + (*frame - offset) as f32 * ppf, y_of(*value)])
                .collect(),
            color,
            styles.metric("graph.curve_width", 2.),
        );
        for key in source["keys"].as_array().into_iter().flatten() {
            let Some(frame) = key["frame"]
                .as_i64()
                .filter(|f| *f >= 0 && (*f as f64) >= offset && (*f as f64) <= end)
            else {
                continue;
            };
            let Ok(value) = observe!("timeline.graph_key_sample", sample(frame as f64, components[index]),
                "object={object} property={property:?} axis={axis:?} frame={frame}") else { continue; };
            let point = [
                plot.min[0] + (frame as f64 - offset) as f32 * ppf,
                y_of(value),
            ];
            let chosen = selected.is_some_and(|s| {
                s.object == object
                    && s.property == property
                    && s.axis == axis
                    && s.frame as i64 == frame
            });
            paint::diamond(
                &mut lines,
                point,
                styles.metric("graph.key_radius", 5.) + if chosen { 0.5 } else { 0. },
                colors.KEYFRAME,
            );
            let value_per_pixel=if panels.timeline.speed_graph {0.} else {(high-low)/plot.height()/scale};
            super::key_selection::ui::graph_mark(&mut lines, panels, super::key_selection::Key {
                target: motion_core::KeyframeTarget::Property { object, property, axis }, frame: frame as u32,
            }, point, styles.metric("graph.key_radius", 5.),
                Rect::new(point[0] - styles.metric("graph.selection.hit_radius", 6.) - styles.metric("motion.drag_threshold", 3.),
                    point[1] - styles.metric("graph.selection.hit_radius", 6.) - styles.metric("motion.drag_threshold", 3.),
                    (styles.metric("graph.selection.hit_radius", 6.) + styles.metric("motion.drag_threshold", 3.)) * 2.,
                    (styles.metric("graph.selection.hit_radius", 6.) + styles.metric("motion.drag_threshold", 3.)) * 2.),
                if axis.is_some() || property.is_scalar() {0} else {components[index]},
                value_per_pixel,
                (property==Property::Opacity).then_some([0.,1.]));
            if clicked(
                curve_input,
                Rect::new(point[0] - 9., point[1] - 9., 18., 18.),
            ) {
                clicked_key = Some(frame);
                if layer["locked"] != true && state["observing"] != true && panels.timeline.graph_drag.is_none() {
                    super::graph_keys::begin(
                        panels,
                        state,
                        object,
                        property,
                        axis,
                        components[index],
                        source,
                        frame as u32,
                        point,
                        value,
                        [low, high],
                        curve_input,
                        out,
                    );
                }
            }
        }
    }
    super::graph_handles::draw(
        &mut lines,
        atlas,
        panels,
        source,
        object,
        property,
        axis,
        components[0],
        super::graph_handles::Plot {
            area: plot,
            offset,
            ppf,
            low,
            high,
            speed: panels.timeline.speed_graph,
            fps,
            scale,
        },
        curve_input,
        layer["locked"] == true || state["observing"] == true,
        out,
    );
    super::graph_keys::update(panels, state, plot, ppf, scale, source, curve_input, out);
    let box_selected = if !box_owned && clicked_key.is_none() {
        super::graph_box::select_curve(
            &styles,
            panels,
            state,
            source,
            object,
            property,
            axis,
            components[0],
            box_plot,
            &points[0],
            input,
        )
    } else {
        false
    };
    if box_selected {
        super::key_selection::ui::curve(panels,state,motion_core::KeyframeTarget::Property {object,property,axis},source);
        panels.audio_controls.clear_selection();
        panels.animation_controls.clear_key();
    }
    super::graph_box::draw(&mut lines, panels, state, source, box_plot);
    lines.clip_to(plot);
    graph.append(lines);
    if let Some(frame) = clicked_key {
        panels
            .animation_controls
            .select_key_in(state, object, property, axis, frame);
        out.push(Action::Seek(frame as f64));
    } else if !box_owned
        && !box_selected
        && panels.timeline.graph_drag.is_none()
        && panels.timeline.key_drag.is_none()
        && clicked(input, plot)
    {
        panels.animation_controls.clear_key();
    }
    if plot.contains(input.mouse) {
        let frame = offset + (input.mouse[0] - plot.min[0]) as f64 / ppf.max(0.001) as f64;
        let Ok(value) = observe!("timeline.graph_tooltip_sample", sample(frame, components[0]),
            "object={object} property={property:?} axis={axis:?} frame={frame}") else {
            graph.clip_to(area);
            list.append(graph);
            return;
        };
        let width = 168.;
        let height = 50.;
        let x = if input.mouse[0] + 16. + width > plot.max[0] {
            input.mouse[0] - width - 16.
        } else {
            input.mouse[0] + 16.
        };
        let y = if input.mouse[1] + 16. + height > plot.max[1] {
            input.mouse[1] - height - 16.
        } else {
            input.mouse[1] + 16.
        };
        let card = Rect::new(x.max(plot.min[0]), y.max(plot.min[1]), width, height);
        motion_ui::style::box_style(
            &mut graph,
            card,
            styles.resolve("popup", Default::default()).unwrap(),
        );
        label(
            &mut graph,
            atlas,
            Rect::new(card.min[0] + 8., card.min[1] + 4., 152., 20.),
            panels.locale.t(title),
            true,
        );
        motion_ui::style::text_style(
            &mut graph,
            atlas,
            Rect::new(card.min[0] + 8., card.min[1] + 24., 152., 20.),
            &format!("{value:.1} {unit}"),
            styles.resolve("label.accent", Default::default()).unwrap(),
            Align::Left,
        );
    }
    let mut last_mode=None;
    for (index, speed) in [true, false].into_iter().enumerate() {
        let title = if speed {
            if panels.locale == Locale::Zh {
                "速度图表"
            } else {
                "Speed graph"
            }
        } else if panels.locale == Locale::Zh {
            "数值图表"
        } else {
            "Value graph"
        };
        let rect = Rect::new(
            area.min[0] + 56. + index as f32 * 100.,
            area.min[1] + 2.,
            94.,
            24.,
        );
        last_mode=Some(rect);
        let mut states = motion_ui::style::States::default();
        if panels.timeline.speed_graph == speed {
            states = states.with(motion_ui::style::State::Selected);
        }
        if ui::button_with_states(
            &mut graph,
            atlas,
            rect,
            title,
            input,
            true,
            &styles,
            "button.icon",
            states,
        )
        .clicked
        {
            panels.timeline.speed_graph = speed;
            panels.timeline.graph_y_zoom = 1.;
            panels.timeline.graph_y_offset = 0.;
        }
    }
    if let Some(last_mode)=last_mode {
        let busy=panels.timeline.graph_drag.is_some() || panels.timeline.key_drag.is_some()
            || panels.timeline.graph_box.editing() || panels.key_gestures.active()
            || panels.timeline.horizontal_drag.is_some() || panels.timeline.scrubbing;
        let row=Rect::new(area.min[0],last_mode.min[1],area.width(),last_mode.height());
        if let Some(point)=super::graph_ranges::launch(&mut graph,atlas,row,last_mode.max[0],input,
            panels.timeline.graph_ranges.ready() && !busy) {
            let (all_interval,selection_interval)=fit_intervals(panels,state,&target,source,layer_offset);
            let tolerance=f64::from(styles.metric("graph.sample.error",0.35));
            let started=std::time::Instant::now();
            let all=observe!("timeline.graph_fit_all",samples.fit_bounds(all_interval,layer_offset as f64,
                &components,speed_graph,fps,scale,[plot.width(),plot.height()],tolerance),
                "object={object} property={property:?} axis={axis:?} elapsed_ms={} phase=fit_all",started.elapsed().as_millis()).ok();
            let selection=selection_interval.and_then(|interval| {
                let started=std::time::Instant::now();
                observe!("timeline.graph_fit_selection",samples.fit_bounds(interval,layer_offset as f64,
                    &components,speed_graph,fps,scale,[plot.width(),plot.height()],tolerance),
                    "object={object} property={property:?} axis={axis:?} elapsed_ms={} phase=fit_selection",started.elapsed().as_millis()).ok()
            });
            panels.timeline.graph_ranges.open(point,identity,super::graph_ranges::Options {displayed,all,selection},
                state,&panels.selected_keys,panels.animation_controls.user_key(),panels.selected,panels.focused);
        }
    }
    let step = (plot.width() / ppf / 4.).max(1.);
    for i in 0..=4 {
        let frame = offset + i as f64 * step as f64;
        let x = plot.min[0] + (frame - offset) as f32 * ppf;
        if x <= plot.max[0] {
            if panels.timeline.graph_grid {
                paint::line(
                    &mut graph,
                    vec![[x, plot.min[1]], [x, plot.max[1]]],
                    colors.BORDER.with_alpha(0.3),
                );
            }
            let seconds = frame.max(0.) / fps;
            let whole = seconds.floor() as u32;
            let sub = ((seconds - seconds.floor()) * fps).round() as u32;
            ui::text(
                &mut graph,
                atlas,
                Rect::new(
                    (x - 28.).clamp(area.min[0], (area.max[0] - 56.).max(area.min[0])),
                    plot.max[1] + 16.,
                    56.,
                    22.,
                ),
                &format!("{whole}:{sub:02}"),
                colors.TEXT_MUTED,
                Align::Center,
            );
        }
    }
    let frame = state["frame"].as_f64().unwrap_or(0.);
    let head = plot.min[0] + (frame - offset) as f32 * ppf;
    if head >= plot.min[0] && head <= plot.max[0] {
        paint::rect(
            &mut graph,
            Rect::new(head, area.min[1], 1., plot.max[1] - area.min[1]),
            colors.ACCENT.with_alpha(0.8),
        );
    }
    let ruler = Rect::new(plot.min[0], plot.max[1] + 8., plot.width(), 28.);
    if clicked(input, ruler) {
        panels.timeline.scrubbing = true;
        panels.timeline.scrub_ruler = ScrubRuler::Graph;
        panels.timeline.last_seek = None;
    }
    let owns_scrub =
        panels.timeline.scrubbing && panels.timeline.scrub_ruler == ScrubRuler::Graph;
    if owns_scrub
        && (clicked(input, ruler) || input.dragging() || input.released.is_some())
    {
        let frame = (offset + (input.mouse[0] - plot.min[0]) as f64 / ppf.max(0.001) as f64)
            .round()
            .clamp(0., last_frame(state));
        if panels.timeline.last_seek != Some(frame) {
            out.push(Action::Seek(frame));
            panels.timeline.last_seek = Some(frame);
        }
    }
    if owns_scrub && input.released.is_some() {
        panels.timeline.scrubbing = false;
    }
    // The shared footer puts the authored-value notice in the label gutter
    // and navigation in the time region, leaving the lower ruler unchanged.
    graph.clip_to(area);
    list.append(graph);
}
