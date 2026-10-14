//! Linked animation workspace: a property tree and a graph share one selection.
use super::*;
use motion_core::Axis;

pub(super) fn render(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    let sheet = list.styles();
    let colors = sheet.colors();
    let zh = panels.locale == Locale::Zh;
    let row_height = sheet.metric("row.height", 22.);
    paint::rect(list, area, colors.PANEL);
    let tree_width = 320f32.min(area.width() * 0.45);
    let toolbar = Rect::new(area.min[0], area.min[1], area.width(), 28.);
    paint::rect(list, toolbar, colors.CHROME);
    ui::text(
        list,
        atlas,
        Rect::new(toolbar.min[0] + 8., toolbar.min[1], 94., 28.),
        &timecode(
            state["frame"].as_f64().unwrap_or(0.),
            state["project"]["fps"].as_u64().unwrap_or(30) as u32,
        ),
        colors.ACCENT,
        Align::Left,
    );
    for (i, (speed, title)) in [
        (true, if zh { "速度图表" } else { "Speed graph" }),
        (false, if zh { "数值图表" } else { "Value graph" }),
    ]
    .into_iter()
    .enumerate()
    {
        let rect = Rect::new(
            toolbar.min[0] + 104. + 100. * i as f32,
            toolbar.min[1] + 2.,
            92.,
            24.,
        );
        let mut states = motion_ui::style::States::default();
        if speed == panels.timeline.speed_graph {
            states = states.with(motion_ui::style::State::Selected);
        }
        if ui::button_with_states(
            list,
            atlas,
            rect,
            title,
            input,
            true,
            &sheet,
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
    if !panels.timeline.graph_fitted && panels.animation_controls.user_key().is_none() {
        if let Some(object) = panels.selected {
            let property = panels
                .animation_controls
                .property
                .unwrap_or(Property::Position);
            let axis = panels.animation_controls.axis;
            if let Some(source) = super::animation_data::channel(state, object, property, axis) {
                let current = state["frame"].as_f64().unwrap_or(0.);
                if let Some(frame) = source["keys"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|k| k["frame"].as_i64())
                    .filter(|f| *f >= 0)
                    .min_by(|a, b| {
                        ((*a as f64 - current).abs()).total_cmp(&(*b as f64 - current).abs())
                    })
                {
                    panels
                        .animation_controls
                        .select_key(object, property, axis, frame);
                }
            }
        }
    }
    let mut fit = !panels.timeline.graph_fitted;
    if area.width() > 650. {
        fit |= ui::button_styled(
            list,
            atlas,
            Rect::new(area.max[0] - 284., toolbar.min[1] + 2., 96., 24.),
            if zh {
                "适合所有图表"
            } else {
                "Fit graphs"
            },
            input,
            true,
            &sheet,
            "button.icon",
        )
        .clicked;
        let options = Rect::new(area.max[0] - 182., toolbar.min[1] + 2., 88., 24.);
        let mut states = motion_ui::style::States::default();
        if panels.timeline.graph_grid {
            states = states.with(motion_ui::style::State::Selected);
        }
        if ui::button_with_states(
            list,
            atlas,
            options,
            if zh { "图表网格" } else { "Graph grid" },
            input,
            true,
            &sheet,
            "button.icon",
            states,
        )
        .clicked
        {
            panels.timeline.graph_grid = !panels.timeline.graph_grid;
        }
        if ui::button_styled(
            list,
            atlas,
            Rect::new(area.max[0] - 88., toolbar.min[1] + 2., 84., 24.),
            if zh { "返回时间轴" } else { "Timeline" },
            input,
            true,
            &sheet,
            "button.icon",
        )
        .clicked
        {
            out.extend(super::cancel_interaction(panels));
            panels.timeline.set_graph_mode(false);
            let neutral = Input {
                mouse: input.mouse,
                ..Default::default()
            };
            super::timeline::render(list, atlas, area, panels, state, &neutral, out);
            return;
        }
    }
    let top = area.min[1] + 28.;
    let bottom = area.max[1] - 24.;
    let details_height = if bottom - top > 230. { 96. } else { 0. };
    let tree = Rect::new(area.min[0], top, tree_width, (bottom - top).max(1.));
    let header = Rect::new(tree.min[0], top, tree_width, 24.);
    paint::rect(list, header, colors.CHROME);
    label(
        list,
        atlas,
        Rect::new(tree.min[0] + 8., top, tree_width * 0.6, 24.),
        if zh {
            "图层 / 属性"
        } else {
            "Layer / Property"
        },
        true,
    );
    label(
        list,
        atlas,
        Rect::new(tree.min[0] + tree_width - 100., top, 94., 24.),
        if zh { "当前值" } else { "Value" },
        true,
    );
    let tree_body = Rect::new(
        tree.min[0],
        top + 24.,
        tree_width,
        (tree.height() - 24. - details_height).max(1.),
    );
    let neutral_tree = Input::default();
    let tree_input = if input.pressed.is_none_or(|(_, p)| tree_body.contains(p)) {
        input
    } else {
        &neutral_tree
    };
    let rows = super::timeline::rows(state);
    let mut entries = list.child();
    let mut y = top + 24. - panels.timeline.scroll;
    for row in &rows {
        let expanded = panels.timeline.expanded.contains(&row.id);
        let rect = Rect::new(tree.min[0], y, tree_width, row_height);
        let disclosure = Rect::new(tree.min[0] + 20., y, 14., row_height);
        paint::rect(
            &mut entries,
            Rect::new(tree.min[0] + 12., y + 8., 7., 7.),
            colors.TRACKS[0],
        );
        label(
            &mut entries,
            atlas,
            disclosure,
            if expanded { "⌄" } else { "›" },
            true,
        );
        label(
            &mut entries,
            atlas,
            Rect::new(tree.min[0] + 36., y, tree_width - 44., row_height),
            &row.name,
            false,
        );
        if clicked(tree_input, disclosure) {
            if expanded {
                panels.timeline.expanded.remove(&row.id);
            } else {
                panels.timeline.expanded.insert(row.id);
            }
        } else if clicked(tree_input, rect) {
            panels.selected = Some(row.id);
            panels.timeline.expanded.insert(row.id);
        }
        y += row_height;
        if expanded && row.spatial {
            label(
                &mut entries,
                atlas,
                Rect::new(tree.min[0] + 28., y, tree_width - 36., row_height),
                if zh { "⌄ 变换" } else { "⌄ Transform" },
                true,
            );
            y += row_height;
            let filter = panels
                .timeline
                .filters
                .get(&row.id)
                .copied()
                .unwrap_or(types::PropertyFilter::Animated);
            let mut properties = super::timeline::filtered_properties(state, row.id, filter);
            if properties.is_empty() {
                properties = super::timeline::property_rows(state, row.id);
            }
            for property in properties {
                label(
                    &mut entries,
                    atlas,
                    Rect::new(tree.min[0] + 44., y, tree_width - 52., row_height),
                    panels.locale.t(property.title),
                    true,
                );
                y += row_height;
                for component in &property.axes {
                    let axis = property.axis.map(|a| [Axis::X, Axis::Y, Axis::Z][a]);
                    let selected = panels.selected == Some(row.id)
                        && panels
                            .animation_controls
                            .property
                            .unwrap_or(Property::Position)
                            == property.property
                        && panels.animation_controls.axis == axis
                        && panels.timeline.graph_component == *component;
                    let rect = Rect::new(tree.min[0], y, tree_width, row_height);
                    if selected {
                        paint::rect(&mut entries, rect, colors.ACCENT.with_alpha(0.2));
                    }
                    let caption = if property.property == Property::Opacity {
                        panels.locale.t(property.title).to_string()
                    } else {
                        format!(
                            "{} {}",
                            ["X", "Y", "Z"][*component],
                            panels.locale.t(property.title)
                        )
                    };
                    ui::text(
                        &mut entries,
                        atlas,
                        Rect::new(
                            tree.min[0] + 60.,
                            y,
                            (tree_width - 164.).max(1.),
                            row_height,
                        ),
                        &caption,
                        if selected {
                            colors.ACCENT
                        } else {
                            colors.TEXT_MUTED
                        },
                        Align::Left,
                    );
                    let sample = state["sampledLayers"]
                        .as_array()
                        .and_then(|ls| ls.iter().find(|l| l["id"].as_u64() == Some(row.id)));
                    let value = sample.map_or(0., |s| {
                        if property.property == Property::Opacity {
                            s[property.key].as_f64().unwrap_or(1.) * 100.
                        } else {
                            s[property.key][*component].as_f64().unwrap_or(0.)
                        }
                    });
                    let unit = match property.property {
                        Property::Position => "px",
                        Property::Rotation => "°",
                        _ => "%",
                    };
                    ui::text(
                        &mut entries,
                        atlas,
                        Rect::new(tree.max[0] - 100., y, 92., row_height),
                        &format!("{value:.1} {unit}"),
                        if selected {
                            colors.ACCENT
                        } else {
                            colors.TEXT_MUTED
                        },
                        Align::Right,
                    );
                    if clicked(tree_input, rect) {
                        panels.selected = Some(row.id);
                        panels
                            .animation_controls
                            .choose_track(row.id, property.property, axis);
                        panels.timeline.graph_component = *component;
                        panels.timeline.graph_y_zoom = 1.;
                        panels.timeline.graph_y_offset = 0.;
                    }
                    y += row_height;
                }
            }
        }
    }
    entries.clip_to(tree_body);
    list.append(entries);
    if tree_body.contains(input.mouse) {
        panels.timeline.scroll = (panels.timeline.scroll + input.wheel[1]).clamp(
            0.,
            (y + panels.timeline.scroll - (bottom - details_height)).max(0.),
        );
    }
    if details_height > 0. {
        let dy = bottom - details_height;
        paint::rect(
            list,
            Rect::new(tree.min[0], dy, tree.width(), details_height),
            colors.PANEL,
        );
        label(
            list,
            atlas,
            Rect::new(tree.min[0] + 16., dy, tree.width() - 24., 22.),
            if zh {
                "关键帧插值"
            } else {
                "Keyframe interpolation"
            },
            true,
        );
        if let Some(selected) = panels.animation_controls.user_key() {
            if let Some(source) = super::animation_data::channel(
                state,
                selected.object,
                selected.property,
                selected.axis,
            ) {
                let keys = source["keys"].as_array().map(Vec::as_slice).unwrap_or(&[]);
                if let Some(index) = keys
                    .iter()
                    .position(|k| k["frame"].as_u64() == Some(selected.frame as u64))
                {
                    let current = &keys[index];
                    let incoming = if index > 0 { &keys[index - 1] } else { current };
                    let kind = if current["ease"] == "hold" {
                        if zh {
                            "保持"
                        } else {
                            "Hold"
                        }
                    } else if !current["curve"].is_null() || current["ease"] != "linear" {
                        if zh {
                            "贝塞尔"
                        } else {
                            "Bezier"
                        }
                    } else {
                        if zh {
                            "线性"
                        } else {
                            "Linear"
                        }
                    };
                    let influence = |key: &Value, first: bool| -> Option<f64> {
                        let easing: motion_core::Easing = motion_core::Easing {
                            ease: serde_json::from_value(key["ease"].clone()).ok()?,
                            curve: serde_json::from_value(key["curve"].clone()).ok(),
                        };
                        if let Some(motion_core::Curve {
                            shape:
                                motion_core::CurveShape::Cubic {
                                    control1, control2, ..
                                },
                            ..
                        }) = easing.curve
                        {
                            Some(if first {
                                control1[0] * 100.
                            } else {
                                (1. - control2[0]) * 100.
                            })
                        } else if matches!(
                            easing.ease,
                            motion_core::Ease::In
                                | motion_core::Ease::Out
                                | motion_core::Ease::InOut
                        ) {
                            Some(100. / 3.)
                        } else {
                            None
                        }
                    };
                    let incoming =
                        influence(incoming, false).map_or("—".into(), |v| format!("{v:.1} %"));
                    let outgoing =
                        influence(current, true).map_or("—".into(), |v| format!("{v:.1} %"));
                    for (i, (caption, value)) in [
                        (
                            if zh { "插值类型" } else { "Interpolation" },
                            kind.to_string(),
                        ),
                        (
                            if zh {
                                "入点影响"
                            } else {
                                "Incoming influence"
                            },
                            incoming,
                        ),
                        (
                            if zh {
                                "出点影响"
                            } else {
                                "Outgoing influence"
                            },
                            outgoing,
                        ),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        label(
                            list,
                            atlas,
                            Rect::new(
                                tree.min[0] + 16.,
                                dy + 22. + i as f32 * 20.,
                                tree.width() * 0.55,
                                20.,
                            ),
                            caption,
                            true,
                        );
                        ui::text(
                            list,
                            atlas,
                            Rect::new(tree.max[0] - 102., dy + 22. + i as f32 * 20., 94., 20.),
                            &value,
                            colors.TEXT,
                            Align::Left,
                        );
                    }
                }
            }
        }
    }
    let graph = Rect::new(
        tree.max[0],
        top,
        (area.max[0] - tree.max[0]).max(1.),
        (bottom - top).max(1.),
    );
    let width = (graph.width() - 72.).max(1.);
    let total = last_frame(state) + 1.;
    if fit {
        let source = panels.selected.and_then(|object| {
            super::animation_data::channel(
                state,
                object,
                panels
                    .animation_controls
                    .property
                    .unwrap_or(Property::Position),
                panels.animation_controls.axis,
            )
        });
        let frames: Vec<_> = source
            .into_iter()
            .flat_map(|s| s["keys"].as_array().into_iter().flatten())
            .filter_map(|k| k["frame"].as_f64())
            .collect();
        let first = frames.iter().copied().fold(f64::INFINITY, f64::min);
        let last = frames.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if first.is_finite() && last > first {
            let span = (last - first) * 1.12;
            panels.timeline.zoom = (total / span).clamp(1., 100.) as f32;
            panels.timeline.offset = (first - (last - first) * 0.06) as f32;
        } else {
            panels.timeline.zoom = 1.;
            panels.timeline.offset = 0.;
        }
        panels.timeline.graph_y_zoom = 1.;
        panels.timeline.graph_y_offset = 0.;
        panels.timeline.graph_fitted = true;
    }
    let old_ppf = width / total as f32 * panels.timeline.zoom.max(1.);
    if graph.contains(input.mouse) && input.wheel[1] != 0. && !input.modifiers.control {
        if input.modifiers.shift {
            panels.timeline.offset += input.wheel[1] / old_ppf;
        } else {
            let at = (input.mouse[0] - graph.min[0] - 64.) / old_ppf + panels.timeline.offset;
            panels.timeline.zoom =
                (panels.timeline.zoom.max(1.) * (-input.wheel[1] / 240.).exp()).clamp(1., 100.);
            let ppf = width / total as f32 * panels.timeline.zoom;
            panels.timeline.offset = at - (input.mouse[0] - graph.min[0] - 64.) / ppf;
        }
    }
    let ppf = width / total as f32 * panels.timeline.zoom.max(1.);
    let offset = panels.timeline.offset as f64;
    super::timeline_graph::render(list, atlas, graph, panels, state, input, offset, ppf, out);
    paint::rect(
        list,
        Rect::new(area.min[0], bottom, area.width(), 24.),
        colors.CHROME,
    );
    label(
        list,
        atlas,
        Rect::new(area.min[0] + 8., bottom, (tree_width - 16.).max(1.), 24.),
        if panels.animation_controls.user_key().is_some() {
            if zh {
                "已选择关键帧"
            } else {
                "Keyframe selected"
            }
        } else {
            if zh {
                "选择属性通道编辑曲线"
            } else {
                "Select a property channel"
            }
        },
        true,
    );
    label(
        list,
        atlas,
        Rect::new(tree.max[0] + 8., bottom, (graph.width() - 16.).max(1.), 24.),
        if zh {
            "拖动手柄调整缓动 · 滚轮缩放时间轴"
        } else {
            "Drag handles to adjust easing · Wheel zooms time"
        },
        true,
    );
}
