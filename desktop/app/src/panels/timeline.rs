use super::*;
use super::{toolbar::icon_button, types::Tool};

pub(super) struct Row {
    pub(super) id: u64,
    pub(super) name: String,
    pub(super) visible: bool,
    pub(super) locked: bool,
    pub(super) clip: [i32; 2],
    pub(super) range: [f64; 2],
    pub(super) keys: Vec<f64>,
}

pub(super) struct PropertyRow {
    pub property: Property,
    key: &'static str,
    title: T,
    pub axis: Option<usize>,
    pub axes: Vec<usize>,
    pub keys: Vec<f64>,
}
fn property_rows(state: &Value, object: u64) -> Vec<PropertyRow> {
    let layer = state["project"]["layers"]
        .as_array()
        .and_then(|ls| ls.iter().find(|l| l["id"].as_u64() == Some(object)));
    let three_d = layer.is_some_and(|l| l["three_d"].as_bool() == Some(true));
    if layer.is_some_and(|l| l["content"]["kind"].as_str() == Some("audio")) {
        return vec![];
    }
    let timeline = state["timeline_layers"]
        .as_array()
        .and_then(|ts| ts.iter().find(|t| t["object"].as_u64() == Some(object)));
    let mut rows = vec![];
    for (property, key, title, axes) in [
        (
            Property::Position,
            "position",
            T::Position,
            if three_d { vec![0, 1, 2] } else { vec![0, 1] },
        ),
        (
            Property::Scale,
            "scale",
            T::Scale,
            if three_d { vec![0, 1, 2] } else { vec![0, 1] },
        ),
        (
            Property::Rotation,
            "rotation",
            T::Rotation,
            if three_d { vec![0, 1, 2] } else { vec![2] },
        ),
        (Property::Opacity, "opacity", T::Opacity, vec![0]),
    ] {
        let track = timeline
            .map(|t| &t["properties"][key])
            .unwrap_or(&Value::Null);
        let separated = track["separated"].as_bool().unwrap_or(false);
        let selections = if separated {
            axes.iter().map(|a| Some(*a)).collect()
        } else {
            vec![None]
        };
        for axis in selections {
            let source = axis
                .map(|a| &track["axes"][["x", "y", "z"][a]])
                .unwrap_or(track);
            let keys = source["keys"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|k| k["frame"].as_f64())
                .collect();
            rows.push(PropertyRow {
                property,
                key,
                title,
                axis,
                axes: axis.map(|a| vec![a]).unwrap_or_else(|| axes.clone()),
                keys,
            });
        }
    }
    rows
}
pub(super) fn rows(state: &Value) -> Vec<Row> {
    let total = state["project"]["frames"].as_u64().unwrap_or(1) as u32;
    let mut rows: Vec<_> = state["project"]["layers"]
        .as_array()
        .into_iter()
        .flatten()
        .rev()
        .map(|l| {
            let id = l["id"].as_u64().unwrap_or(0);
            let t = state["timeline_layers"]
                .as_array()
                .and_then(|ts| ts.iter().find(|t| t["object"].as_u64() == Some(id)));
            let mut keys = Vec::new();
            if let Some(t) = t {
                for property in ["position", "rotation", "scale", "opacity"] {
                    let track = &t["properties"][property];
                    for k in track["keys"].as_array().into_iter().flatten() {
                        if let Some(f) = k["frame"].as_f64() {
                            keys.push(f);
                        }
                    }
                    for axis in ["x", "y", "z"] {
                        for k in track["axes"][axis]["keys"].as_array().into_iter().flatten() {
                            if let Some(f) = k["frame"].as_f64() {
                                keys.push(f);
                            }
                        }
                    }
                }
            }
            keys.sort_by(|a, b| a.total_cmp(b));
            keys.dedup();
            Row {
                id,
                name: l["name"].as_str().unwrap_or("Layer").into(),
                visible: l["visible"].as_bool().unwrap_or(true),
                locked: l["locked"].as_bool().unwrap_or(false),
                clip: [
                    t.and_then(|t| t["in_frame"].as_i64()).unwrap_or(0) as i32,
                    t.and_then(|t| t["out_frame"].as_i64())
                        .unwrap_or(i64::from(total)) as i32,
                ],
                range: [
                    t.and_then(|t| {
                        t["subframe_range"]["in_frame"]
                            .as_f64()
                            .or_else(|| t["in_frame"].as_f64())
                    })
                    .unwrap_or(0.),
                    t.and_then(|t| {
                        t["subframe_range"]["out_frame"]
                            .as_f64()
                            .or_else(|| t["out_frame"].as_f64())
                    })
                    .unwrap_or(total as f64),
                ],
                keys,
            }
        })
        .collect();
    if state["project"]["camera"]["created"]
        .as_bool()
        .unwrap_or(false)
    {
        rows.insert(
            0,
            Row {
                id: 0,
                name: "Camera".into(),
                visible: true,
                locked: true,
                clip: [0, total as i32],
                range: [0., total as f64],
                keys: vec![],
            },
        );
    }
    rows
}
pub(super) fn filtered_properties(
    state: &Value,
    object: u64,
    filter: types::PropertyFilter,
) -> Vec<PropertyRow> {
    property_rows(state, object)
        .into_iter()
        .filter(|row| filter.admits(row.property, !row.keys.is_empty()))
        .collect()
}
pub(super) fn property_visible(
    panels: &Panels,
    state: &Value,
    object: u64,
    property: Property,
) -> bool {
    let animated = property_rows(state, object)
        .iter()
        .any(|r| r.property == property && !r.keys.is_empty());
    panels
        .timeline
        .filters
        .get(&object)
        .copied()
        .unwrap_or_default()
        .admits(property, animated)
}
pub(super) fn render(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    if panels.focused == Some(Panel::Timeline) {
        super::properties::process_field(panels, state, input, out);
    }
    let context_input = input;
    let neutral = Input::default();
    let input = if panels.timeline.timeline_context_menu.is_some() {
        &neutral
    } else {
        input
    };
    paint::rect(list, area, palette::PANEL);
    let total = last_frame(state) + 1.0;
    let graph_mode = panels.timeline.graph_mode;
    let mut rows = rows(state);
    rows.retain(|row| {
        row.name
            .to_lowercase()
            .contains(&panels.timeline.timeline_query.to_lowercase())
    });
    let gutter = GUTTER.min(area.width() * 0.4);
    let track = Rect::new(
        area.min[0] + gutter,
        area.min[1],
        (area.width() - gutter - 8.0).max(1.0),
        area.height(),
    );
    let ruler = Rect::new(track.min[0], area.min[1] + 36.0, track.width(), 28.0);
    if area.contains(input.mouse) {
        if input.modifiers.control {
            panels.timeline.zoom =
                (panels.timeline.zoom.max(1.0) * (input.wheel[1] * -0.005).exp()).clamp(1.0, 100.0);
        } else if input.modifiers.shift {
            panels.timeline.offset = (panels.timeline.offset + input.wheel[1] as f32).max(0.0);
        } else if !(graph_mode && input.mouse[0] >= track.min[0]) {
            panels.timeline.scroll = (panels.timeline.scroll + input.wheel[1]).clamp(
                0.0,
                (rows
                    .iter()
                    .map(|row| {
                        if panels.timeline.expanded.contains(&row.id) {
                            2 + filtered_properties(
                                state,
                                row.id,
                                panels
                                    .timeline
                                    .filters
                                    .get(&row.id)
                                    .copied()
                                    .unwrap_or_default(),
                            )
                            .len()
                        } else {
                            1
                        }
                    })
                    .sum::<usize>() as f32
                    * metrics::ROW
                    - (area.height() - 64.0))
                    .max(0.0),
            );
        }
    }
    panels.timeline.offset = panels
        .timeline
        .offset
        .min((total as f32 - total as f32 / panels.timeline.zoom.max(1.0)).max(0.0));
    let ppf = track.width() / total as f32 * panels.timeline.zoom.max(1.0);
    let offset = panels.timeline.offset;
    let x_of = |f: f64| track.min[0] + (f as f32 - offset) * ppf;
    let frame_of = |x: f32| {
        ((x - track.min[0]) / ppf + offset)
            .round()
            .clamp(0.0, (total - 1.0) as f32) as f64
    };
    paint::rect(
        list,
        Rect::new(area.min[0], area.min[1], area.width(), 64.0),
        palette::CHROME,
    );
    motion_ui::style::text_style(
        list,
        atlas,
        Rect::new(area.min[0] + 10.0, area.min[1], gutter - 20.0, 30.0),
        &timecode(
            state["frame"].as_f64().unwrap_or(0.0),
            state["project"]["fps"].as_u64().unwrap_or(30) as u32,
        ),
        motion_ui::style::ResolvedStyle {
            foreground: palette::ACCENT,
            ..motion_ui::style::default_stylesheet()
                .resolve("label.title", Default::default())
                .unwrap()
        },
        Align::Left,
    );
    let locale = panels.locale;
    if gutter >= 370. {
        if ui::button(
            list,
            atlas,
            Rect::new(area.min[0] + gutter - 98., area.min[1] + 5., 90., 25.),
            if panels.timeline.graph_mode {
                if locale == Locale::Zh {
                    "时间轴"
                } else {
                    "Timeline"
                }
            } else {
                if locale == Locale::Zh {
                    "曲线"
                } else {
                    "Graph"
                }
            },
            input,
            true,
        )
        .clicked
        {
            panels.timeline.graph_mode = !panels.timeline.graph_mode;
        }
    }
    let search = Rect::new(
        track.min[0] + 12.,
        area.min[1] + 5.,
        (track.width() - 176.).min(250.).max(1.),
        25.,
    );
    paint::rect(list, search, palette::SUNKEN);
    paint::stroke(
        list,
        search,
        if panels.timeline.timeline_search {
            palette::ACCENT
        } else {
            palette::BORDER
        },
        1.,
    );
    if clicked(input, search) {
        panels.timeline.timeline_search = true;
    } else if input.pressed.is_some() {
        panels.timeline.timeline_search = false;
    }
    if panels.timeline.timeline_search {
        panels.timeline.timeline_query.push_str(&input.text);
        if input.keys.iter().any(|(k, _)| *k == Key::Backspace) {
            panels.timeline.timeline_query.pop();
        }
    }
    label(
        list,
        atlas,
        search.shrink(5.),
        if panels.timeline.timeline_query.is_empty() {
            if locale == Locale::Zh {
                "搜索图层…"
            } else {
                "Search layers…"
            }
        } else {
            &panels.timeline.timeline_query
        },
        true,
    );
    let zoom_x = track.max[0] - 146.;
    if ui::button(
        list,
        atlas,
        Rect::new(zoom_x, area.min[1] + 5., 25., 25.),
        "−",
        input,
        true,
    )
    .clicked
    {
        panels.timeline.zoom = (panels.timeline.zoom.max(1.) / 1.25).max(1.);
    }
    label(
        list,
        atlas,
        Rect::new(zoom_x + 29., area.min[1] + 5., 58., 25.),
        &format!("{:.0}%", panels.timeline.zoom.max(1.) * 100.),
        true,
    );
    if ui::button(
        list,
        atlas,
        Rect::new(zoom_x + 86., area.min[1] + 5., 25., 25.),
        "+",
        input,
        true,
    )
    .clicked
    {
        panels.timeline.zoom = (panels.timeline.zoom.max(1.) * 1.25).min(100.);
    }
    if ui::button(
        list,
        atlas,
        Rect::new(zoom_x + 117., area.min[1] + 5., 26., 25.),
        "↔",
        input,
        true,
    )
    .clicked
    {
        panels.timeline.zoom = 1.;
        panels.timeline.offset = 0.;
    }
    for (i, (tool, action)) in [
        (Tool::First, Action::Seek(0.0)),
        (
            if panels.playing {
                Tool::Pause
            } else {
                Tool::Play
            },
            Action::TogglePlay,
        ),
        (Tool::Last, Action::Seek(last_frame(state))),
    ]
    .into_iter()
    .enumerate()
    {
        icon_button(
            list,
            Rect::new(
                area.min[0] + 146.0 + i as f32 * 38.0,
                area.min[1] + 2.0,
                32.0,
                28.0,
            ),
            tool,
            input,
            true,
            action,
            out,
        );
    }
    label(
        list,
        atlas,
        Rect::new(
            area.min[0] + 90.0,
            ruler.min[1],
            (gutter - 212.0).max(1.0),
            28.0,
        ),
        locale.t(T::SourceName),
        true,
    );
    label(
        list,
        atlas,
        Rect::new(area.min[0] + gutter - 112.0, ruler.min[1], 104.0, 28.0),
        locale.t(T::ParentLink),
        true,
    );
    ui::icon(
        list,
        Rect::new(area.min[0] + 10.0, ruler.min[1] + 3.0, 16.0, 24.0),
        Icon::Eye,
    );
    ui::icon(
        list,
        Rect::new(area.min[0] + 30.0, ruler.min[1] + 3.0, 16.0, 24.0),
        Icon::Lock,
    );
    let fps = state["project"]["fps"].as_u64().unwrap_or(30).max(1) as f32;
    let step = (60.0 / (ppf * fps)).ceil().max(1.0) * fps;
    let mut f = (panels.timeline.offset / step).ceil() * step;
    while x_of(f as f64) < track.max[0] {
        let x = x_of(f as f64);
        paint::rect(
            list,
            Rect::new(x, ruler.max[1] - 6.0, 1.0, 6.0),
            palette::TEXT_MUTED,
        );
        label(
            list,
            atlas,
            Rect::new(x + 3.0, ruler.min[1], 52.0, 22.0),
            &format!("{:.0}s", f / fps),
            true,
        );
        f += step;
    }
    if clicked(input, ruler) {
        panels.timeline.scrubbing = true;
        out.push(Action::Seek(frame_of(input.mouse[0])));
    }
    if panels.timeline.scrubbing && input.dragging() {
        out.push(Action::Seek(frame_of(input.mouse[0])));
    }
    if input.released.is_some() {
        panels.timeline.scrubbing = false;
    }
    let body = Rect::new(
        area.min[0],
        ruler.max[1],
        area.width(),
        (area.max[1] - ruler.max[1]).max(0.0),
    );
    let mut row_offset = 0;
    for (index, row) in rows.iter().enumerate() {
        let expandable = row.id != 0
            && state["project"]["layers"]
                .as_array()
                .and_then(|ls| ls.iter().find(|l| l["id"].as_u64() == Some(row.id)))
                .is_some_and(|l| l["content"]["kind"].as_str() != Some("audio"));
        let expanded = expandable && panels.timeline.expanded.contains(&row.id);
        let properties = if expanded {
            filtered_properties(
                state,
                row.id,
                panels
                    .timeline
                    .filters
                    .get(&row.id)
                    .copied()
                    .unwrap_or_default(),
            )
        } else {
            vec![]
        };
        let y = body.min[1] + row_offset as f32 * metrics::ROW - panels.timeline.scroll;
        row_offset += 1 + if expanded { 1 + properties.len() } else { 0 };
        if expanded {
            let group = Rect::new(
                area.min[0] + 80.,
                y + metrics::ROW,
                gutter - 86.,
                metrics::ROW,
            );
            if group.min[1] >= body.min[1] && group.max[1] <= body.max[1] {
                label(list, atlas, group, locale.t(T::Transform), true);
            }
            let sample = state["sampledLayers"]
                .as_array()
                .and_then(|ls| ls.iter().find(|l| l["id"].as_u64() == Some(row.id)));
            for (i, property) in properties.iter().enumerate() {
                let py = y + (i + 2) as f32 * metrics::ROW;
                if py < body.min[1] || py + metrics::ROW > body.max[1] {
                    continue;
                }
                paint::rect(
                    list,
                    Rect::new(area.min[0], py, gutter, metrics::ROW),
                    palette::PANEL,
                );
                let axis = property.axis.map(|a| {
                    [
                        motion_core::Axis::X,
                        motion_core::Axis::Y,
                        motion_core::Axis::Z,
                    ][a]
                });
                let watch = Rect::new(area.min[0] + 78., py, 14., metrics::ROW);
                ui::icon(list, watch, Icon::Stopwatch);
                if clicked(input, watch) && !row.locked {
                    out.push(Action::Edit(Command::Animate {
                        object: row.id,
                        property: property.property,
                        axis,
                        frame: state["frame"].as_f64().unwrap_or(0.).floor() as u32,
                        enabled: property.keys.is_empty(),
                    }));
                }
                let text = property
                    .axis
                    .map(|a| format!("{} {}", locale.t(property.title), ["X", "Y", "Z"][a]))
                    .unwrap_or_else(|| locale.t(property.title).into());
                let caption = Rect::new(
                    area.min[0] + 98.,
                    py,
                    (gutter * 0.5 - 98.).max(1.),
                    metrics::ROW,
                );
                label(list, atlas, caption, &text, true);
                if clicked(input, caption) {
                    panels.selected = Some(row.id);
                    panels
                        .animation_controls
                        .choose_track(row.id, property.property, axis);
                    panels.animation_controls.clear_key();
                }
                for (j, component) in property.axes.iter().enumerate() {
                    let value = if property.property == Property::Opacity {
                        sample
                            .map(|s| s[property.key].as_f64().unwrap_or(1.) * 100.)
                            .unwrap_or(100.)
                    } else {
                        sample
                            .map(|s| s[property.key][*component].as_f64().unwrap_or(0.))
                            .unwrap_or(0.)
                    };
                    let cell_width = (gutter * 0.44 / property.axes.len() as f32).min(64.);
                    let field = Rect::new(
                        area.min[0] + gutter * 0.52 + j as f32 * cell_width,
                        py,
                        (cell_width - 2.).max(1.),
                        metrics::ROW,
                    );
                    if clicked(input, field) {
                        panels.selected = Some(row.id);
                    }
                    super::properties::draw_value(
                        list,
                        atlas,
                        field,
                        panels,
                        input,
                        row.id,
                        property.property,
                        *component,
                        value,
                        row.locked,
                        true,
                    );
                }
                for frame in property.keys.iter().filter(|_| !graph_mode) {
                    let x = x_of(*frame);
                    if x >= track.min[0] + 4. && x < track.max[0] - 4. {
                        let current =
                            (frame - state["frame"].as_f64().unwrap_or(0.)).abs() < 0.0001;
                        paint::diamond(
                            list,
                            [x, py + metrics::ROW * 0.5],
                            4.,
                            if current {
                                palette::KEYFRAME
                            } else {
                                palette::ACCENT
                            },
                        );
                        if clicked(input, Rect::new(x - 5., py, 10., metrics::ROW)) {
                            panels.selected = Some(row.id);
                            panels.animation_controls.select_key(
                                row.id,
                                property.property,
                                axis,
                                *frame as i64,
                            );
                            out.push(Action::Seek(frame.clamp(0., total - 1.)));
                        }
                    }
                }
            }
        }
        if y < body.min[1] || y + metrics::ROW > body.max[1] {
            continue;
        }
        let name = Rect::new(
            area.min[0] + 112.0,
            y,
            (gutter - 320.0).max(1.0),
            metrics::ROW,
        );
        let selected = panels.selected == Some(row.id);
        let expand = Rect::new(area.min[0] + 75., y, 14., metrics::ROW);
        if expandable {
            let x = expand.min[0] + 3.;
            let cy = y + metrics::ROW * 0.5;
            paint::line(
                list,
                if expanded {
                    vec![[x, cy - 2.], [x + 3., cy + 2.], [x + 6., cy - 2.]]
                } else {
                    vec![[x, cy - 3.], [x + 4., cy], [x, cy + 3.]]
                },
                palette::TEXT_MUTED,
            );
            if clicked(input, expand) {
                if expanded {
                    panels.timeline.expanded.remove(&row.id);
                } else {
                    panels.timeline.expanded.insert(row.id);
                }
            }
        }
        paint::rect(
            list,
            Rect::new(area.min[0], y, gutter, metrics::ROW),
            if selected {
                palette::ACCENT.with_alpha(0.18)
            } else {
                palette::PANEL
            },
        );
        label(list, atlas, name, &row.name, false);
        paint::rect(
            list,
            Rect::new(area.min[0] + 66.0, y + 7.0, 6.0, 12.0),
            palette::TRACKS[index % palette::TRACKS.len()],
        );
        label(
            list,
            atlas,
            Rect::new(area.min[0] + 50.0, y, 14.0, metrics::ROW),
            &(index + 1).to_string(),
            true,
        );
        let layer = state["project"]["layers"]
            .as_array()
            .and_then(|ls| ls.iter().find(|l| l["id"].as_u64() == Some(row.id)));
        let icon = match layer.map(|l| l["content"]["kind"].as_str().unwrap_or("")) {
            Some("text") => motion_ui::icons::Icon::Text,
            Some("image") => motion_ui::icons::Icon::Image,
            Some("video") => motion_ui::icons::Icon::Video,
            Some("audio") => motion_ui::icons::Icon::Audio,
            _ if row.id == 0 => motion_ui::icons::Icon::Camera,
            _ => motion_ui::icons::Icon::Shape,
        };
        motion_ui::icons::draw(
            list,
            Rect::new(area.min[0] + 91., y, 18., metrics::ROW),
            icon,
            18.,
            palette::TEXT_MUTED,
        );
        label(
            list,
            atlas,
            Rect::new(area.min[0] + gutter - 196., y, 76., metrics::ROW),
            layer
                .and_then(|l| l["blend"].as_str())
                .unwrap_or(if locale == Locale::Zh {
                    "正常"
                } else {
                    "Normal"
                }),
            true,
        );
        let parent = layer
            .and_then(|l| l["parent"]["object"].as_u64())
            .and_then(|id| {
                state["project"]["layers"]
                    .as_array()?
                    .iter()
                    .find(|l| l["id"].as_u64() == Some(id))
            })
            .and_then(|l| l["name"].as_str())
            .unwrap_or(locale.t(T::None));
        label(
            list,
            atlas,
            Rect::new(area.min[0] + gutter - 110.0, y, 102.0, metrics::ROW),
            parent,
            true,
        );
        let eye = Rect::new(area.min[0] + 10.0, y, 16.0, metrics::ROW);
        let lock = Rect::new(area.min[0] + 28.0, y, 16.0, metrics::ROW);
        ui::icon(
            list,
            eye,
            if row.visible { Icon::Eye } else { Icon::EyeOff },
        );
        ui::icon(
            list,
            lock,
            if row.locked { Icon::Lock } else { Icon::Unlock },
        );
        if row.id != 0 && clicked(input, eye) {
            out.push(Action::Edit(Command::Flags {
                object: row.id,
                visible: !row.visible,
                locked: row.locked,
            }));
        }
        if row.id != 0 && clicked(input, lock) {
            out.push(Action::Edit(Command::Flags {
                object: row.id,
                visible: row.visible,
                locked: !row.locked,
            }));
        }
        if clicked(input, name) {
            panels.selected = Some(row.id);
            panels.properties.field = None;
        }
        if graph_mode {
            continue;
        }
        let left = x_of(row.range[0]).max(track.min[0]);
        let right = x_of(row.range[1]).min(track.max[0]);
        let bar = Rect::new(left, y + 5.0, (right - left).max(0.0), metrics::ROW - 10.0);
        paint::rect(
            list,
            bar,
            palette::TRACKS[index % palette::TRACKS.len()].with_alpha(if row.visible {
                0.7
            } else {
                0.2
            }),
        );
        if selected {
            paint::stroke(list, bar, palette::ACCENT, 1.0);
        }
        for (head, x, visible) in [
            (true, left, x_of(row.range[0]) >= track.min[0]),
            (false, right, x_of(row.range[1]) <= track.max[0]),
        ] {
            let handle = Rect::new(x - 4.0, y + 4.0, 8.0, metrics::ROW - 8.0);
            if selected && visible && !row.locked {
                paint::rect(list, handle, palette::ACCENT);
                if clicked(input, handle) {
                    panels.timeline.trimming = Some((
                        row.id,
                        row.range[0],
                        row.range[1],
                        head,
                        if head { row.range[0] } else { row.range[1] },
                    ));
                    out.push(Action::History(2));
                }
            }
        }
        if clicked(input, bar) && panels.timeline.trimming.is_none() {
            panels.selected = Some(row.id);
            if !row.locked {
                panels.timeline.moving = Some((
                    row.id,
                    input.mouse[0],
                    row.clip[0],
                    row.clip[1] - row.clip[0],
                    row.clip[0],
                ));
                out.push(Action::History(2));
            }
        }
        for f in row.keys.iter().filter(|_| !expanded) {
            let x = x_of(*f);
            if x >= track.min[0] + 4.0 && x < track.max[0] - 4.0 {
                paint::diamond(list, [x, y + metrics::ROW * 0.5], 4.0, palette::KEYFRAME);
            }
        }
    }
    if graph_mode {
        super::timeline_graph::render(
            list,
            atlas,
            Rect::new(track.min[0], body.min[1], track.width(), body.height()),
            panels,
            state,
            input,
            offset as f64,
            ppf,
            out,
        );
    }
    if let Some((object, start, end, head, last)) = panels.timeline.trimming.as_mut() {
        let position = ((input.mouse[0] - track.min[0]) / ppf + offset) as f64;
        let position = if input.modifiers.alt {
            position
        } else {
            position.round()
        };
        let position = position.clamp(
            -(motion_core::MAX_FRAMES as f64),
            motion_core::MAX_FRAMES as f64,
        );
        let next = if *head {
            position.min(*end - 0.000001)
        } else {
            position.max(*start + 0.000001)
        };
        if next != *last && (input.dragging() || input.released.is_some()) {
            out.push(Action::Edit(Command::SetLayerRange {
                object: *object,
                in_frame: if *head { next } else { *start },
                out_frame: if *head { *end } else { next },
            }));
            *last = next;
        }
        if input.released.is_some() {
            out.push(Action::History(3));
            panels.timeline.trimming = None;
        }
    }
    if let Some((object, start, in_frame, duration, last)) = panels.timeline.moving.as_mut() {
        let next = (*in_frame as f32 + (input.mouse[0] - *start) / ppf)
            .round()
            .clamp(
                -(motion_core::MAX_FRAMES as f32),
                motion_core::MAX_FRAMES as f32 - *duration as f32,
            ) as i32;
        if next != *last && (input.dragging() || input.released.is_some()) {
            out.push(Action::Edit(Command::MoveLayerClip {
                object: *object,
                in_frame: next,
            }));
            *last = next;
        }
        if input.released.is_some() {
            out.push(Action::History(3));
            panels.timeline.moving = None;
        }
    }
    let x = x_of(state["frame"].as_f64().unwrap_or(0.0));
    if x >= track.min[0] && x < track.max[0] {
        paint::rect(
            list,
            Rect::new(x, ruler.min[1], 1.0, area.height()),
            palette::ACCENT,
        );
        paint::diamond(list, [x, ruler.min[1] + 6.0], 6.0, palette::ACCENT);
    }

    // Timeline right-click context menu for creating layers
    if right_clicked(input, body) {
        panels.timeline.timeline_context_menu = Some(input.mouse);
    }

    // Render timeline context menu if open
    if let Some(menu_pos) = panels.timeline.timeline_context_menu {
        render_timeline_context_menu(
            list,
            atlas,
            area,
            menu_pos,
            state,
            context_input,
            locale,
            out,
            &mut panels.timeline.timeline_context_menu,
        );
    }
}

// Render timeline context menu for layer creation
fn render_timeline_context_menu(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    pos: [f32; 2],
    state: &Value,
    input: &Input,
    locale: Locale,
    out: &mut Vec<Action>,
    menu_state: &mut Option<[f32; 2]>,
) {
    let menu_items = vec![
        (locale.t(T::NewSolid), Action::AddSolid),
        (locale.t(T::NewText), Action::NewText),
        (locale.t(T::NewNull), Action::AddNull),
        (locale.t(T::NewAdjustment), Action::AddAdjustment),
        (locale.t(T::NewCamera), Action::Edit(Command::CreateCamera)),
        (
            locale.t(T::NewRectangle),
            Action::AddShape(motion_core::vector::ShapeKind::Rectangle),
        ),
        (
            locale.t(T::NewEllipse),
            Action::AddShape(motion_core::vector::ShapeKind::Ellipse),
        ),
    ];

    let item_height = 28.0;
    let menu_width = 200.0f32.min(area.width().max(1.));
    let menu_height = menu_items.len() as f32 * item_height + 8.0;

    // Position menu near click, but keep it on screen
    let menu_x = pos[0].clamp(area.min[0], (area.max[0] - menu_width).max(area.min[0]));
    let menu_y = pos[1].clamp(area.min[1], (area.max[1] - menu_height).max(area.min[1]));

    let menu_rect = Rect::new(menu_x, menu_y, menu_width, menu_height);
    list.glyphs.retain(|g| {
        g.position[0] + g.size[0] <= menu_rect.min[0]
            || g.position[0] >= menu_rect.max[0]
            || g.position[1] + g.size[1] <= menu_rect.min[1]
            || g.position[1] >= menu_rect.max[1]
    });

    // Menu background
    paint::rect(list, menu_rect, palette::CHROME);
    paint::stroke(list, menu_rect, palette::BORDER, 1.0);

    // Check if camera already exists for camera item
    let camera_exists = state["project"]["camera"]["created"]
        .as_bool()
        .unwrap_or(false);

    // Render menu items
    for (i, (text, action)) in menu_items.into_iter().enumerate() {
        let item_rect = Rect::new(
            menu_x + 4.0,
            menu_y + 4.0 + i as f32 * item_height,
            menu_width - 8.0,
            item_height,
        );

        // Disable camera item if camera already exists
        let enabled = !matches!(&action, Action::Edit(Command::CreateCamera)) || !camera_exists;

        let hovered = enabled && item_rect.contains(input.mouse);

        if hovered {
            paint::rect(list, item_rect, palette::ACCENT.with_alpha(0.15));
        }

        label(list, atlas, item_rect.shrink(8.0), text, !enabled);

        if hovered && clicked(input, item_rect) {
            out.push(action);
            *menu_state = None;
        }
    }

    // Close menu on click outside or escape
    if input.pressed.is_some() && !menu_rect.contains(input.mouse) {
        *menu_state = None;
    }
    if input.keys.iter().any(|(k, _)| *k == Key::Escape) {
        *menu_state = None;
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    fn fixture() -> Value {
        serde_json::json!({"frame":21,"project":{"frames":120,"fps":30,"layers":[{"id":7,"name":"Layer","visible":true,"three_d":true}]},"sampledLayers":[{"id":7,"position":[10,20,30],"rotation":[0,0,0],"scale":[100,100,100],"opacity":1}],"timeline_layers":[{"object":7,"properties":{"position":{"separated":true,"axes":{"x":{"keys":[{"frame":0},{"frame":60}]},"y":{"keys":[{"frame":15},{"frame":90}]},"z":{"keys":[]}}}}}]})
    }
    #[test]
    fn separated_rows_use_their_own_key_times_and_static_axis_stays_empty() {
        let rows = property_rows(&fixture(), 7);
        assert_eq!(rows[0].keys, vec![0., 60.]);
        assert_eq!(rows[1].keys, vec![15., 90.]);
        assert!(rows[2].keys.is_empty());
        assert_eq!(rows[0].axis, Some(0));
        assert_eq!(rows[1].axis, Some(1));
        assert_eq!(rows[2].axis, Some(2));
    }
    #[test]
    fn timeline_numeric_input_uses_the_shared_component_command() {
        let state = fixture();
        let mut panels = Panels {
            selected: Some(7),
            focused: Some(Panel::Timeline),
            ..Default::default()
        };
        panels.timeline.expanded.insert(7);
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let area = Rect::new(0., 0., 1200., 600.);
        let point = [247., 124.];
        let mut actions = vec![];
        for input in [
            Input {
                mouse: point,
                pressed: Some((MouseButton::Left, point)),
                ..Default::default()
            },
            Input {
                mouse: point,
                released: Some((MouseButton::Left, point)),
                ..Default::default()
            },
            Input {
                mouse: point,
                text: "32.5".into(),
                ..Default::default()
            },
            Input {
                mouse: point,
                keys: vec![(Key::Enter, Default::default())],
                ..Default::default()
            },
        ] {
            render(
                &mut PaintList::default(),
                &mut atlas,
                area,
                &mut panels,
                &state,
                &input,
                &mut actions,
            );
        }
        assert!(
            matches!(actions.as_slice(),[Action::Edit(Command::SetComponent{object:7,property:Property::Position,axis:motion_core::Axis::X,frame:21,value})] if *value==32.5)
        );
    }
}
