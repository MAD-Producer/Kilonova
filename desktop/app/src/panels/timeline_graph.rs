//! Value curves share the timeline ruler and use the model's actual sampler.
use super::animation_data as data;
use super::*;
use motion_core::{Axis, Track};

enum Samples {
    Scalar(Track<f32>),
    Vector(Track<[f32; 3]>),
}
impl Samples {
    fn value(&self, frame: f64, component: usize) -> f32 {
        match self {
            Self::Scalar(track) => track.sample(frame),
            Self::Vector(track) => track.sample(frame)[component],
        }
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
    let mut graph = PaintList::default();
    paint::rect(&mut graph, area, palette::SUNKEN);
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
        return;
    };
    let Some(layer) = state["project"]["layers"]
        .as_array()
        .and_then(|ls| ls.iter().find(|l| l["id"].as_u64() == Some(object)))
    else {
        return;
    };
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
        return;
    }
    let Some(timeline) = data::timeline(state, object) else {
        return;
    };
    let root = &timeline["properties"][data::name(property)];
    let separated = root["separated"].as_bool() == Some(true);
    let visible_axes = if property == Property::Opacity {
        vec![0]
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
        .choose_track(object, property, axis);
    let title = match property {
        Property::Position => T::Position,
        Property::Scale => T::Scale,
        Property::Rotation => T::Rotation,
        _ => T::Opacity,
    };
    label(
        &mut graph,
        atlas,
        Rect::new(
            area.min[0] + 8.,
            area.min[1] + 2.,
            (area.width() - 140.).max(1.),
            25.,
        ),
        &format!(
            "{} · {}",
            panels.locale.t(title),
            if panels.locale == Locale::Zh {
                "值曲线"
            } else {
                "Value graph"
            }
        ),
        false,
    );
    if separated {
        for index in &visible_axes {
            let button = Rect::new(
                area.max[0] - 112. + *index as f32 * 32.,
                area.min[1] + 3.,
                28.,
                22.,
            );
            if ui::button(
                &mut graph,
                atlas,
                button,
                ["X", "Y", "Z"][*index],
                input,
                true,
            )
            .clicked
            {
                panels.animation_controls.choose_track(
                    object,
                    property,
                    Some([Axis::X, Axis::Y, Axis::Z][*index]),
                );
            }
        }
    }
    let axis = panels.animation_controls.axis;
    let Some(source) = data::channel(state, object, property, axis) else {
        return;
    };
    let layer_offset = timeline["offset_frame"].as_i64().unwrap_or(0);
    let samples = if property == Property::Opacity || axis.is_some() {
        data::track::<f32>(source, layer_offset).map(Samples::Scalar)
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
        return;
    };
    let components = if axis.is_some() || property == Property::Opacity {
        vec![0]
    } else {
        visible_axes
    };
    let plot = Rect::new(
        area.min[0],
        area.min[1] + 30.,
        area.width(),
        (area.height() - 56.).max(1.),
    );
    let end = (offset + area.width() as f64 / ppf.max(1e-6) as f64).min(last_frame(state));
    let scale = if property == Property::Opacity {
        100.
    } else {
        1.
    };
    let points: Vec<Vec<(f64, f32)>> = components
        .iter()
        .map(|component| {
            (0..=256)
                .map(|i| {
                    let frame = offset + (end - offset).max(0.) * i as f64 / 256.;
                    (
                        frame,
                        samples.value(frame - layer_offset as f64, *component) * scale,
                    )
                })
                .collect()
        })
        .collect();
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
    let low = min - margin;
    let high = max + margin;
    let y_of = |value: f32| plot.max[1] - (value - low) / (high - low) * plot.height();
    for i in 0..5 {
        let value = low + (high - low) * i as f32 / 4.;
        let y = y_of(value);
        paint::line(
            &mut graph,
            vec![[plot.min[0], y], [plot.max[0], y]],
            palette::BORDER.with_alpha(0.35),
        );
        label(
            &mut graph,
            atlas,
            Rect::new(plot.min[0] + 4., y - 12., 52., 22.),
            &format!("{value:.1}"),
            true,
        );
    }
    let mut lines = PaintList::default();
    let mut clicked_key = None;
    let selected = panels.animation_controls.user_key();
    for (index, curve) in points.iter().enumerate() {
        let color =
            palette::TRACKS[axis.map_or(components[index], |a| a.index()) % palette::TRACKS.len()];
        paint::line(
            &mut lines,
            curve
                .iter()
                .map(|(frame, value)| [area.min[0] + (*frame - offset) as f32 * ppf, y_of(*value)])
                .collect(),
            color,
        );
        for key in source["keys"].as_array().into_iter().flatten() {
            let Some(frame) = key["frame"]
                .as_i64()
                .filter(|f| *f >= 0 && (*f as f64) >= offset && (*f as f64) <= end)
            else {
                continue;
            };
            let value =
                samples.value(frame as f64 - layer_offset as f64, components[index]) * scale;
            let point = [
                area.min[0] + (frame as f64 - offset) as f32 * ppf,
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
                if chosen { 5. } else { 4. },
                if chosen { palette::KEYFRAME } else { color },
            );
            if clicked(input, Rect::new(point[0] - 6., point[1] - 6., 12., 12.)) {
                clicked_key = Some(frame);
            }
        }
    }
    lines.clip_to(plot);
    graph.append(lines);
    if let Some(frame) = clicked_key {
        panels
            .animation_controls
            .select_key(object, property, axis, frame);
        out.push(Action::Seek(frame as f64));
    } else if clicked(input, plot) {
        panels.animation_controls.clear_key();
    }
    label(
        &mut graph,
        atlas,
        Rect::new(area.min[0] + 8., area.max[1] - 24., area.width() - 16., 22.),
        if panels.locale == Locale::Zh {
            "单击关键帧选择 · F9 平滑 · Shift+F3 返回时间轴"
        } else {
            "Click a key to select · F9 smooth · Shift+F3 timeline"
        },
        true,
    );
    graph.clip_to(area);
    list.append(graph);
}
