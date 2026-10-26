//! One toolbar row at every width. Popups render after timeline navigation and
//! own the original input while all underlying timeline content is suspended.
use super::*;

fn units(c: &Controls) -> Vec<data::Unit> {
    c.entries
        .iter()
        .filter(|e| e.visible)
        .filter_map(|e| e.data.as_ref().ok())
        .map(|d| d.unit.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}
fn advance(c: &mut Controls, input: &Input) {
    if input.focus_lost {
        c.bar_focus = None;
        c.channels_open = false;
        c.mode.open = false;
    }
    for (key, modifiers) in &input.keys {
        if *key == Key::Tab && !c.mode.open {
            if let Some(focus) = c.bar_focus {
                let first = if c.channels_open { 3 } else { 0 };
                let end = if c.channels_open {
                    5 + c.entries.len()
                } else {
                    3
                };
                let count = end - first;
                c.bar_focus = Some(
                    first
                        + (focus.saturating_sub(first)
                            + if modifiers.shift { count - 1 } else { 1 })
                            % count,
                );
            }
        }
        if *key == Key::Escape {
            c.channels_open = false;
            c.mode.open = false;
            c.bar_focus = None;
        }
    }
}
fn key(c: &Controls, index: usize, input: &Input) -> bool {
    c.bar_focus == Some(index)
        && input
            .keys
            .iter()
            .any(|(k, _)| matches!(k, Key::Enter | Key::Space))
}
pub(in crate::panels) fn toolbar(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) -> Rect {
    let styles = list.styles();
    let row = styles.metric("row.height", 22.);
    let gap = styles.metric("padding", 5.);
    let pad = styles.metric("padding.panel", 8.);
    let width = styles.metric("graph.mode.width", 100.);
    let toggle = styles.metric("graph.comparison.toggle.width", 72.);
    let mut c = std::mem::take(&mut panels.timeline.comparison);
    c.sync(state, panels.locale);
    c.channels_opened = false;
    let neutral = (input.composing || input.ime_consumed || input.focus_lost).then(|| Input {
        keys: vec![],
        ..input.clone()
    });
    let input = neutral.as_ref().unwrap_or(input);
    advance(&mut c, input);
    let mode_width = (area.width() - toggle - width - gap * 2. - pad * 2.)
        .max(0.)
        .min(width * 2.);
    let check = Rect::new(area.min[0] + pad, area.min[1], toggle, row);
    let mode = Rect::new(check.max[0] + gap, area.min[1], mode_width, row);
    let channels = Rect::new(mode.max[0] + gap, area.min[1], width, row);
    c.anchors = Some([check, mode, channels]);
    if input.pressed.is_some_and(|(_, p)| mode.contains(p)) {
        out.extend(super::super::cancel_interaction(panels));
        c.bar_focus = Some(1);
        super::super::key_selection::ui::block_background(panels);
    }
    let bottom = (area.min[1] + row + gap).min(area.max[1]);
    let content = Rect::new(
        area.min[0],
        bottom,
        area.width(),
        (area.max[1] - bottom).max(0.),
    );
    let count = units(&c).len();
    c.compact_groups =
        content.height() < styles.metric("graph.comparison.group.min_height", 120.) * count as f32;
    c.single_group = c.unit.is_some() || c.compact_groups;
    if Checkbox::draw(
        list,
        atlas,
        check,
        words(panels.locale, "比较", "Compare"),
        c.enabled,
        input,
        true,
        c.bar_focus == Some(0),
        None,
    )
    .clicked
        || key(&c, 0, input)
    {
        out.extend(super::super::cancel_interaction(panels));
        c.enabled = !c.enabled;
        c.bar_focus = Some(0);
        super::super::key_selection::ui::block_background(panels);
    }
    let title = format!(
        "{} ({})",
        words(panels.locale, "通道", "Channels"),
        c.entries.len()
    );
    let states = if c.bar_focus == Some(2) || c.channels_open {
        motion_ui::style::States::default().with(motion_ui::style::State::Focused)
    } else {
        Default::default()
    };
    if ui::button_with_states(
        list, atlas, channels, &title, input, true, &styles, "button", states,
    )
    .clicked
        || key(&c, 2, input)
    {
        out.extend(super::super::cancel_interaction(panels));
        c.channels_open = !c.channels_open;
        c.channels_opened = c.channels_open;
        c.mode.open = false;
        c.bar_focus = Some(if c.channels_open { 3 } else { 2 });
        super::super::key_selection::ui::block_background(panels);
    }
    if input
        .pressed
        .is_some_and(|(_, p)| !check.contains(p) && !mode.contains(p) && !channels.contains(p))
        && !c.popup_open()
    {
        c.bar_focus = None;
    }
    paint::line(
        list,
        vec![[area.min[0], bottom], [area.max[0], bottom]],
        styles.color("border"),
    );
    panels.timeline.comparison = c;
    content
}

pub(in crate::panels) fn overlay(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    bounds: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    if !panels.timeline.graph_mode || !panels.timeline.comparison.enabled {
        return;
    }
    let styles = list.styles();
    let mut c = std::mem::take(&mut panels.timeline.comparison);
    let Some([_, mode, anchor]) = c.anchors else {
        panels.timeline.comparison = c;
        return;
    };
    let neutral = (input.composing || input.ime_consumed || input.focus_lost).then(|| Input {
        keys: vec![],
        ..input.clone()
    });
    let input = neutral.as_ref().unwrap_or(input);
    // Toolbar receives neutral input while a popup is already open.
    if c.popup_open() {
        advance(&mut c, input);
    }
    let units = units(&c);
    let selected_unit = c
        .unit
        .as_ref()
        .filter(|u| units.contains(u))
        .cloned()
        .or_else(|| c.single_group.then(|| units.first().cloned()).flatten());
    let all = !c.compact_groups || units.is_empty();
    let choices: Vec<_> = [false, true]
        .into_iter()
        .flat_map(|speed| {
            let options = if all { vec![None] } else { vec![] };
            options
                .into_iter()
                .chain(units.iter().cloned().map(Some))
                .map(move |unit| (speed, unit))
        })
        .collect();
    let labels: Vec<_> = choices
        .iter()
        .map(|(speed, unit)| {
            format!(
                "{} · {}",
                words(
                    panels.locale,
                    if *speed { "速度" } else { "数值" },
                    if *speed { "Speed" } else { "Value" }
                ),
                unit.as_ref().map_or_else(
                    || words(panels.locale, "全部单位", "All units").to_string(),
                    |u| u.title(*speed)
                )
            )
        })
        .collect();
    let items: Vec<_> = labels
        .iter()
        .map(|label| DropdownItem {
            label,
            enabled: true,
        })
        .collect();
    let selected_index = choices
        .iter()
        .position(|(speed, unit)| *speed == panels.timeline.speed_graph && *unit == selected_unit)
        .unwrap_or(0);
    let was_open = c.mode.open;
    let quiet = Input::default();
    if let Some(index) = c.mode.draw(
        list,
        atlas,
        mode,
        bounds,
        &items,
        selected_index,
        if c.channels_open { &quiet } else { input },
        true,
        c.bar_focus == Some(1),
    ) {
        out.extend(super::super::cancel_interaction(panels));
        (panels.timeline.speed_graph, c.unit) = choices[index].clone();
        c.groups = ScrollArea::default();
        c.bar_focus = Some(1);
        super::super::key_selection::ui::block_background(panels);
    }
    if c.mode.open && !was_open {
        out.extend(super::super::cancel_interaction(panels));
        c.bar_focus = Some(1);
        super::super::key_selection::ui::block_background(panels);
    }
    if c.channels_open {
        let opening = Input::default();
        let input = if c.channels_opened { &opening } else { input };
        let row = styles.metric("row.height", 22.);
        let pad = styles.metric("padding.panel", 8.);
        let gap = styles.metric("padding", 5.);
        let width = styles
            .metric("graph.comparison.popup.width", 360.)
            .min(bounds.width());
        let desired = (2.
            + (c.entries.len() as f32).min(styles.metric("graph.comparison.popup.rows", 8.)))
            * row
            + pad * 2.
            + gap;
        let below = (bounds.max[1] - anchor.max[1]).max(0.);
        let above = (anchor.min[1] - bounds.min[1]).max(0.);
        let down = below >= desired || below >= above;
        let height = desired.min(if down { below } else { above });
        let popup = Rect::new(
            anchor.max[0].min(bounds.max[0]).max(bounds.min[0] + width) - width,
            if down {
                anchor.max[1]
            } else {
                anchor.min[1] - height
            },
            width,
            height,
        );
        if input.pressed.is_some_and(|(_, p)| anchor.contains(p)) {
            c.channels_open = false;
            c.bar_focus = Some(2);
        } else if input
            .pressed
            .is_some_and(|(_, p)| !popup.contains(p) && !anchor.contains(p))
        {
            c.channels_open = false;
            c.bar_focus = None;
        } else {
            let mut layer = list.child();
            if let Ok(style) = observe!(
                "curve.comparison.popup_style",
                styles
                    .resolve("popup", Default::default())
                    .ok_or("missing popup style"),
                "operation=channels"
            ) {
                motion_ui::style::box_style(&mut layer, popup, style);
            }
            let action_area = popup.shrink(pad);
            for (i, title) in [
                words(panels.locale, "加入所选", "Add selected"),
                words(panels.locale, "清空", "Clear"),
            ]
            .into_iter()
            .enumerate()
            {
                let rect = Rect::new(
                    action_area.min[0] + i as f32 * (action_area.width() + gap) * 0.5,
                    action_area.min[1],
                    (action_area.width() - gap) * 0.5,
                    row,
                );
                let enabled = if i == 0 {
                    scope(state).is_some()
                } else {
                    !c.entries.is_empty()
                };
                let states = if c.bar_focus == Some(i + 3) {
                    motion_ui::style::States::default().with(motion_ui::style::State::Focused)
                } else {
                    Default::default()
                };
                if enabled
                    && (ui::button_with_states(
                        &mut layer, atlas, rect, title, input, enabled, &styles, "button", states,
                    )
                    .clicked
                        || key(&c, i + 3, input))
                {
                    out.extend(super::super::cancel_interaction(panels));
                    c.bar_focus = Some(i + 3);
                    super::super::key_selection::ui::block_background(panels);
                    if i == 0 {
                        let selected = selected(panels, state);
                        if let Err(error) = c.add(
                            state,
                            panels.locale,
                            selected.as_ref().map_or(&[], |s| s.targets.as_slice()),
                        ) {
                            panels.message = Some(format!(
                                "{}: {error}",
                                words(
                                    panels.locale,
                                    "无法加入比较，请选择数值属性或关键帧",
                                    "Cannot add to comparison; select numeric properties or keys"
                                )
                            ));
                        }
                    } else {
                        c.entries.clear();
                        c.groups = ScrollArea::default();
                        c.unit = None;
                    }
                }
            }
            let legend = Rect::new(
                action_area.min[0],
                action_area.min[1] + row + gap,
                action_area.width(),
                (action_area.height() - row - gap).max(0.),
            );
            let scroll = c.legend.begin_view(
                &mut layer,
                legend,
                c.entries.len() as f32 * row,
                input,
                false,
            );
            let mut checks = list.child();
            for (index, entry) in c.entries.iter_mut().enumerate() {
                let rect = Rect::new(
                    scroll.content.min[0],
                    scroll.content.min[1] + index as f32 * row,
                    scroll.viewport.width(),
                    row,
                );
                if rect.max[1] <= scroll.viewport.min[1] || rect.min[1] >= scroll.viewport.max[1] {
                    continue;
                }
                let slot = styles.metric("icon.slot", 20.);
                paint::rect(
                    &mut checks,
                    Rect::new(
                        rect.min[0],
                        rect.min[1] + row * 0.5,
                        slot - gap,
                        styles.metric("graph.curve_width", 2.),
                    ),
                    view::color(&styles, index),
                );
                let check = Rect::new(
                    rect.min[0] + slot,
                    rect.min[1],
                    (rect.width() - slot).max(0.),
                    row,
                );
                #[cfg(test)]
                c.legend_hits.push((index, check));
                let title = match &entry.data {
                    Ok(data) => format!(
                        "{} · {}",
                        entry.label,
                        data.unit.title(panels.timeline.speed_graph)
                    ),
                    Err(_) => format!(
                        "{} · {}",
                        entry.label,
                        words(panels.locale, "通道不可用", "Unavailable channel")
                    ),
                };
                if Checkbox::draw(
                    &mut checks,
                    atlas,
                    check,
                    &title,
                    entry.visible,
                    &scroll.input,
                    true,
                    c.bar_focus == Some(index + 5),
                    None,
                )
                .clicked
                {
                    entry.visible = !entry.visible;
                    c.bar_focus = Some(index + 5);
                    out.extend(super::super::cancel_interaction(panels));
                    super::super::key_selection::ui::block_background(panels);
                }
            }
            checks.clip_to(scroll.viewport);
            layer.append(checks);
            layer.clip_to(bounds);
            list.append(layer);
        }
    }
    panels.timeline.comparison = c;
}
