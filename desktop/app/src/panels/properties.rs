use super::*;

pub(super) fn render(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    let styles = list.styles();
    let colors = styles.colors();
    let neutral = Input::default();
    let mask_modal = panels.path_controls.inspector_menu_in(Panel::Properties);
    let parent_modal = panels.parent_controls.open;
    let original = input;
    let mask_input = if parent_modal { &neutral } else { original };
    let input = if mask_modal || parent_modal {
        &neutral
    } else {
        input
    };
    if panels.focused != Some(Panel::Timeline) {
        process_field(panels, state, input, out);
    }
    paint::rect(list, area, colors.PANEL);
    let row = styles.metric("row.height", 22.);
    let pad = styles.metric("padding.panel", 8.);
    let input = Input {
        pressed: input.pressed.filter(|(_, p)| area.contains(*p)),
        ..input.clone()
    };
    let input = &input;
    if area.contains(input.mouse) && !panels.parent_controls.open {
        panels.properties.properties_scroll =
            (panels.properties.properties_scroll + input.wheel[1]).clamp(
                0.,
                (panels.properties.content_height - area.height()).max(0.),
            );
    }
    let mut y = area.min[1] + pad - panels.properties.properties_scroll;
    if panels.selected == Some(0) && state["project"]["camera"]["created"] == true {
        panels
            .camera_controls
            .render(list, atlas, area, &mut y, state, input, panels.locale, out);
        panels.properties.content_height =
            y - area.min[1] + panels.properties.properties_scroll + pad;
        return;
    }
    let Some(object) = panels.selected.filter(|id| *id != 0) else {
        label(
            list,
            atlas,
            area.shrink(pad),
            panels.locale.t(T::SelectLayer),
            true,
        );
        return;
    };
    let Some(layer) = state["project"]["layers"]
        .as_array()
        .and_then(|ls| ls.iter().find(|l| l["id"].as_u64() == Some(object)))
    else {
        return;
    };
    let Some(sample) = state["sampledLayers"]
        .as_array()
        .and_then(|ls| ls.iter().find(|l| l["id"].as_u64() == Some(object)))
    else {
        return;
    };
    let locale = panels.locale;
    let locked = layer["locked"].as_bool().unwrap_or(false);
    let three_d = layer["three_d"].as_bool().unwrap_or(false);
    let context = Rect::new(area.min[0] + pad, y, (area.width() - pad * 2.).max(0.), row);
    paint::rect(
        list,
        Rect::new(context.min[0], y + 7., 6., 8.),
        colors.TRACKS[0],
    );
    label(
        list,
        atlas,
        Rect::new(
            context.min[0] + 12.,
            y,
            (context.width() - 70.).max(0.),
            row,
        ),
        layer["name"].as_str().unwrap_or(locale.t(T::Layer)),
        true,
    );
    if input.key_presses().any(|(k, _, _)| k == Key::Tab) {
        panels.properties.checkbox_focused = true;
    }
    if input.pressed.is_some() {
        panels.properties.checkbox_focused = false;
    }
    if motion_ui::widgets::Checkbox::draw(
        list,
        atlas,
        Rect::new(context.max[0] - 48., y, 48., row),
        "3D",
        three_d,
        input,
        !locked,
        panels.properties.checkbox_focused,
        None,
    )
    .clicked
    {
        out.push(Action::Edit(Command::SetLayer3d {
            object,
            enabled: !three_d,
        }));
    }
    y += row;
    let group = Rect::new(area.min[0] + pad, y, (area.width() - pad * 2.).max(0.), row);
    label(
        list,
        atlas,
        group,
        &format!(
            "{} {}",
            if panels.properties.transform_collapsed {
                "›"
            } else {
                "⌄"
            },
            locale.t(T::Transform)
        ),
        true,
    );
    if clicked(input, group) {
        panels.properties.transform_collapsed = !panels.properties.transform_collapsed;
        if !panels.properties.transform_collapsed {
            panels.timeline.reopen_properties(object);
        }
        panels.properties.field = None;
    }
    y += row;
    if !panels.properties.transform_collapsed {
        let mut rows = vec![
            (
                Property::Position,
                "position",
                locale.t(T::Position),
                vec![0],
            ),
            (
                Property::Position,
                "position",
                locale.t(T::Position),
                vec![1],
            ),
        ];
        if three_d {
            rows.push((
                Property::Position,
                "position",
                locale.t(T::Position),
                vec![2],
            ));
        }
        rows.push((
            Property::Scale,
            "scale",
            locale.t(T::Scale),
            if three_d { vec![0, 1, 2] } else { vec![0, 1] },
        ));
        if three_d {
            for axis in [0, 1, 2] {
                rows.push((
                    Property::Rotation,
                    "rotation",
                    locale.t(T::Rotation),
                    vec![axis],
                ));
            }
        } else {
            rows.push((
                Property::Rotation,
                "rotation",
                locale.t(T::Rotation),
                vec![2],
            ));
        }
        rows.push((Property::Opacity, "opacity", locale.t(T::Opacity), vec![0]));
        for (property, key, name, axes) in rows {
            if !super::timeline::property_visible(panels, state, object, property) {
                continue;
            }
            let suffix = if property == Property::Position {
                format!(" {}", ["X", "Y", "Z"][axes[0]])
            } else if three_d && property == Property::Rotation {
                format!(" {}", ["X", "Y", "Z"][axes[0]])
            } else {
                String::new()
            };
            let columns =
                motion_ui::widgets::PropertyColumns::new(&styles, area, y, 64. * axes.len() as f32);
            label(
                list,
                atlas,
                Rect::new(area.min[0] + pad, y, columns.label.width() + 20., row),
                &format!("{name}{suffix}"),
                true,
            );
            let count = axes.len();
            let width = (columns.value.width() / count as f32)
                .max(1.)
                .min(styles.metric("value.min_width", 64.));
            for (index, axis) in axes.into_iter().enumerate() {
                let value = if property == Property::Opacity {
                    sample[key].as_f64().unwrap_or(1.) * 100.
                } else {
                    sample[key][axis].as_f64().unwrap_or(0.)
                };
                let r = Rect::new(
                    columns.value.min[0] + index as f32 * width,
                    columns.value.min[1],
                    (width - 2.).max(0.),
                    columns.value.height(),
                );
                draw_value(
                    list, atlas, r, panels, state, input, object, property, axis, value, locked,
                    false, out,
                );
            }
            paint::rect(
                list,
                Rect::new(area.min[0], y + columns.height - 0.5, area.width(), 0.5),
                colors.BORDER.with_alpha(0.18),
            );
            y += columns.height;
        }
    }
    let mut mask_list = list.child();
    if !matches!(
        layer["content"]["kind"].as_str(),
        Some("audio" | "null" | "adjustment")
    ) {
        if let Some(typed) = panels.path_controls.inspector_layer(state, layer) {
            panels.path_controls.masks_in_properties(
                &mut mask_list,
                atlas,
                area,
                &mut y,
                &typed,
                state,
                mask_input,
                locale,
                out,
            );
            if let Some((rect, dragging)) = panels
                .path_controls
                .numeric_cursor()
                .filter(|_| panels.path_controls.numeric_owner() == Some(Panel::Properties))
            {
                if dragging || rect.contains(input.mouse) {
                    panels.cursor = Some(motion_ui::widgets::numeric_behavior::cursor(
                        input, dragging,
                    ));
                }
            }
        }
    }
    label(
        list,
        atlas,
        Rect::new(area.min[0] + pad, y, area.width() - pad * 2., row),
        if locale == Locale::Zh {
            "⌄ 图层关系"
        } else {
            "⌄ Layer relationships"
        },
        true,
    );
    y += row;
    panels.parent_controls.render(
        list,
        atlas,
        area,
        &mut y,
        object,
        layer,
        state,
        if mask_modal { &neutral } else { original },
        locale,
        out,
    );
    if layer["content"]["kind"] == "vector" {
        if let Some(layer) = panels.path_controls.inspector_layer(state, layer) {
            panels.path_controls.groups.render(
                list,
                atlas,
                area,
                &mut y,
                &layer,
                state,
                input,
                locale,
                Panel::Properties,
                out,
            );
            if let Some((rect, dragging)) = panels.path_controls.groups.cursor {
                if dragging || rect.contains(input.mouse) {
                    panels.cursor = Some(motion_ui::widgets::numeric_behavior::cursor(
                        input, dragging,
                    ));
                }
            }
        }
    } else if panels.path_controls.groups.cancel() {
        out.push(Action::History(4));
    }
    panels.properties.content_height = y - area.min[1] + panels.properties.properties_scroll + pad;
    list.append(mask_list);
}

pub(super) fn draw_value(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    r: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    object: u64,
    property: Property,
    axis: usize,
    value: f64,
    locked: bool,
    timeline: bool,
    out: &mut Vec<Action>,
) {
    let styles = list.styles();
    let colors = styles.colors();

    let key = (
        object,
        100 + property as u64 * 8 + axis as u64 + if timeline { 64 } else { 0 },
    );
    if !locked
        && (r.contains(input.mouse)
            || panels
                .properties
                .field
                .as_ref()
                .is_some_and(|f| f.scrubbing.as_ref().is_some_and(|s| s.begun)))
    {
        panels.cursor = Some(motion_ui::widgets::numeric_behavior::cursor(
            input,
            panels
                .properties
                .field
                .as_ref()
                .is_some_and(|f| f.scrubbing.as_ref().is_some_and(|s| s.begun)),
        ));
    }
    let hover = panels.motion.value(
        key,
        0.,
        if !locked && r.contains(input.mouse) {
            1.
        } else {
            0.
        },
        styles.as_ref().metric("motion.hover_ms", 90.),
        false,
    );
    let changed = panels.motion.changed(
        (key.0, key.1 + 200),
        value,
        styles.as_ref().metric("motion.value_ms", 140.),
    );
    paint::rect(
        list,
        r,
        colors.ACCENT.with_alpha((hover * 0.07).max(changed * 0.14)),
    );
    let active = panels.properties.field.as_ref().is_some_and(|f| {
        f.object == object
            && f.property == property
            && f.axis == axis
            && (panels.focused == Some(Panel::Timeline)) == timeline
    });
    let mut states = motion_ui::style::States::default();
    if active
        && panels
            .properties
            .field
            .as_ref()
            .is_some_and(|f| f.scrubbing.is_none())
    {
        states = states.with(motion_ui::style::State::Focused);
    }
    if !locked && r.contains(input.mouse) {
        states = states.with(motion_ui::style::State::Hovered);
    }
    if locked {
        states = states.with(motion_ui::style::State::Disabled);
    }
    let style = styles.as_ref().resolve("field.number", states).unwrap();
    motion_ui::style::box_style(list, r, style);
    let text = if active {
        panels.properties.field.as_ref().unwrap().text.clone()
    } else {
        format!(
            "{value:.2}{}",
            if matches!(property, Property::Scale | Property::Opacity) {
                "%"
            } else {
                ""
            }
        )
    };
    if active
        && panels
            .properties
            .field
            .as_ref()
            .is_some_and(|f| f.scrubbing.as_ref().is_some_and(|s| s.begun))
    {
        let delta = panels
            .properties
            .field
            .as_ref()
            .and_then(|f| f.scrubbing.as_ref())
            .map_or(0., |s| s.last_value - s.start_value);
        panels.drag_readout = Some((input.mouse, format!("{}  {:+.2}", text, delta)));
    }
    motion_ui::style::text_style(
        list,
        atlas,
        r.shrink(style.padding),
        &text,
        style,
        Align::Left,
    );
    if active {
        panels.properties.field.as_mut().unwrap().rect = r;
    }
    if clicked(input, r) && !locked && !active && !panels.properties.consumed_press {
        panels.properties.field = Some(Field {
            object,
            property,
            axis,
            text: (value as f32).to_string(),
            replace: false,
            rect: r,
            scrubbing: Some(ScrubState {
                start_mouse: input.pressed.unwrap().1[0],
                start_value: value as f32,
                last_value: value as f32,
                begun: false,
            }),
        });
        if input.released.is_some() {
            process_field(panels, state, input, out);
        }
    }
}

fn numeric_command(field: &Field, value: f32, state: &Value) -> Option<Command> {
    let frame = state["frame"].as_f64().unwrap_or(0.).floor() as u32;
    if field.property == Property::Opacity {
        return Some(Command::SetScalar {
            object: field.object,
            property: field.property,
            frame,
            value: (value / 100.).clamp(0., 1.),
        });
    }
    let key = match field.property {
        Property::Position => "position",
        Property::Rotation => "rotation",
        _ => "scale",
    };
    let separated = state["timeline_layers"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|t| {
            t["object"].as_u64() == Some(field.object)
                && t["properties"][key]["separated"].as_bool() == Some(true)
        });
    if separated {
        return Some(Command::SetComponent {
            object: field.object,
            property: field.property,
            axis: [
                motion_core::Axis::X,
                motion_core::Axis::Y,
                motion_core::Axis::Z,
            ][field.axis],
            frame,
            value,
        });
    }
    let sample = state["sampledLayers"]
        .as_array()?
        .iter()
        .find(|s| s["id"].as_u64() == Some(field.object))?;
    let mut xyz = std::array::from_fn(|i| sample[key][i].as_f64().unwrap_or(0.) as f32);
    xyz[field.axis] = value;
    Some(Command::SetVector {
        object: field.object,
        property: field.property,
        frame,
        value: xyz,
    })
}

pub(super) fn process_field(
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    panels.properties.consumed_press = false;
    let Some(mut field) = panels.properties.field.take() else {
        return;
    };
    if input.focus_lost || input.keys.iter().any(|(key, _)| *key == Key::Escape) {
        if field.scrubbing.as_ref().is_some_and(|s| s.begun) {
            out.push(Action::History(4));
        }
        return;
    }
    if field.scrubbing.is_none() && !input.composing && !input.ime_consumed {
        if let Some((MouseButton::Left, point)) = input
            .pressed
            .filter(|(_, point)| field.rect.contains(*point))
        {
            let value = field
                .text
                .trim()
                .parse::<f32>()
                .ok()
                .filter(|value| value.is_finite())
                .unwrap_or_else(|| {
                    let sample = state["sampledLayers"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|sample| sample["id"].as_u64() == Some(field.object));
                    sample.map_or(0., |sample| match field.property {
                        Property::Opacity => sample["opacity"].as_f64().unwrap_or(1.) as f32 * 100.,
                        Property::Position => {
                            sample["position"][field.axis].as_f64().unwrap_or(0.) as f32
                        }
                        Property::Rotation => {
                            sample["rotation"][field.axis].as_f64().unwrap_or(0.) as f32
                        }
                        _ => sample["scale"][field.axis].as_f64().unwrap_or(1.) as f32,
                    })
                });
            field.scrubbing = Some(ScrubState {
                start_mouse: point[0],
                start_value: value,
                last_value: value,
                begun: false,
            });
        }
    }
    let outside = input.pressed.is_some_and(|(_, p)| !field.rect.contains(p));
    let released = input
        .released
        .is_some_and(|(button, _)| button == MouseButton::Left);
    if let Some(scrub) = field.scrubbing.as_mut() {
        let delta = input.mouse[0] - scrub.start_mouse;
        if (input.dragging() || released)
            && (motion_ui::widgets::numeric_behavior::started(
                delta,
                input.pointer_travel,
                panels.styles.as_ref().map_or_else(
                    || {
                        motion_ui::style::default_stylesheet_arc()
                            .metric("motion.drag_threshold", 3.)
                    },
                    |s| s.metric("motion.drag_threshold", 3.),
                ),
            ) || scrub.begun)
        {
            if !scrub.begun {
                scrub.begun = true;
                out.push(Action::History(2));
            }
            let step = motion_ui::widgets::numeric_behavior::step(input) as f32;
            let mut value = scrub.start_value + delta * 0.5 * step;
            if field.property == Property::Opacity {
                value = value.clamp(0., 100.);
            }
            if value.is_finite() && value != scrub.last_value {
                scrub.last_value = value;
                field.text = format!("{value:.2}");
                if let Some(command) = numeric_command(&field, value, state) {
                    out.push(Action::Edit(command));
                }
            }
        }
        if released || outside {
            if field.scrubbing.as_ref().is_some_and(|s| s.begun) {
                panels.properties.consumed_press =
                    input.pressed.is_some_and(|(_, p)| field.rect.contains(p));
                out.push(Action::History(3));
                return;
            }
            field.scrubbing = None;
            field.replace = true;
        }
    }
    if field.scrubbing.is_none() {
        if !input.text.is_empty() {
            if field.replace {
                field.text.clear();
                field.replace = false;
            }
            field.text.push_str(&input.text);
        }
        if input.keys.iter().any(|(key, _)| *key == Key::Backspace) {
            if field.replace {
                field.text.clear();
                field.replace = false;
            } else {
                field.text.pop();
            }
        }
        if outside || input.keys.iter().any(|(key, _)| *key == Key::Enter) {
            if field.replace {
                return;
            }
            if field.text.trim().is_empty() {
                return;
            }
            match field.text.parse::<f32>() {
                Ok(value) if value.is_finite() => {
                    if let Some(command) = numeric_command(&field, value, state) {
                        out.push(Action::Edit(command));
                    }
                    return;
                }
                _ => panels.message = Some(panels.locale.t(T::EnterFinite).into()),
            }
        }
    }
    panels.properties.field = Some(field);
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> Value {
        serde_json::json!({"frame":21,"project":{"layers":[{"id":7}]},
            "sampledLayers":[{"id":7,"position":[10,20,0],"rotation":[0,0,0],"scale":[100,100,100],"opacity":1}],
            "timeline_layers":[{"object":7,"properties":{"position":{"separated":true}}}]})
    }
    fn panels() -> Panels {
        Panels {
            selected: Some(7),
            properties: types::PropertiesState {
                field: Some(Field {
                    object: 7,
                    property: Property::Position,
                    axis: 0,
                    text: "10.00".into(),
                    replace: false,
                    rect: Rect::new(90., 0., 120., 30.),
                    scrubbing: Some(ScrubState {
                        start_mouse: 100.,
                        start_value: 10.,
                        last_value: 10.,
                        begun: false,
                    }),
                }),
                ..Default::default()
            },
            ..Default::default()
        }
    }
    fn drag(x: f32) -> Input {
        Input {
            mouse: [x, 15.],
            held: Some(MouseButton::Left),
            ..Default::default()
        }
    }
    fn release(x: f32) -> Input {
        Input {
            mouse: [x, 15.],
            released: Some((MouseButton::Left, [x, 15.])),
            ..Default::default()
        }
    }
    #[test]
    fn stationary_held_pointer_does_not_resubmit_the_numeric_edit() {
        let mut panels = panels();
        let mut first = vec![];
        process_field(&mut panels, &state(), &drag(120.), &mut first);
        assert_eq!(first.len(), 2);
        let mut next = vec![];
        process_field(&mut panels, &state(), &drag(120.), &mut next);
        assert!(
            next.is_empty(),
            "same pointer submitted work and prevented presentation"
        );
        process_field(&mut panels, &state(), &release(120.), &mut next);
        assert!(matches!(next.as_slice(), [Action::History(3)]));
    }
    #[test]
    fn stylesheet_threshold_and_grabbing_cursor_preserve_single_transaction() {
        for threshold in [0., 12.] {
            let mut panels = panels();
            panels.set_stylesheet(motion_ui::style::default_stylesheet_arc().with_overrides(format!("{{\"tokens\":{{\"sizes\":{{\"motion.drag_threshold\":{threshold}}}}}}}").as_bytes()).unwrap());
            let mut actions = vec![];
            process_field(&mut panels, &state(), &drag(105.), &mut actions);
            assert_eq!(
                actions
                    .iter()
                    .filter(|a| matches!(a, Action::History(2)))
                    .count(),
                usize::from(threshold == 0.)
            );
            process_field(&mut panels, &state(), &drag(120.), &mut actions);
            let mut atlas = TextAtlas::from_system(12.).unwrap();
            draw_value(
                &mut PaintList::default(),
                &mut atlas,
                Rect::new(90., 0., 120., 30.),
                &mut panels,
                &state(),
                &drag(120.),
                7,
                Property::Position,
                0,
                20.,
                false,
                false,
                &mut vec![],
            );
            assert_eq!(panels.cursor, Some(motion_ui::cursor::Cursor::Grabbing));
            process_field(&mut panels, &state(), &release(120.), &mut actions);
            assert_eq!(
                actions
                    .iter()
                    .filter(|a| matches!(a, Action::History(2)))
                    .count(),
                1
            );
            assert_eq!(
                actions
                    .iter()
                    .filter(|a| matches!(a, Action::History(3)))
                    .count(),
                1
            );
        }
    }
    #[test]
    fn coalesced_return_drag_closes_without_reopening_input() {
        let mut panels = panels();
        let mut actions = vec![];
        let mut input = release(100.);
        input.pressed = Some((MouseButton::Left, [100., 15.]));
        input.pointer_travel = 40.;
        process_field(&mut panels, &state(), &input, &mut actions);
        assert!(panels.properties.field.is_none());
        assert!(panels.properties.consumed_press);
        assert!(matches!(actions.first(), Some(Action::History(2))));
        assert!(matches!(actions.last(), Some(Action::History(3))));
    }
    #[test]
    fn scrub_returning_through_origin_starts_once_and_closes_transaction() {
        let mut panels = panels();
        let mut actions = vec![];
        for x in [120., 100., 140.] {
            process_field(&mut panels, &state(), &drag(x), &mut actions);
        }
        process_field(&mut panels, &state(), &release(140.), &mut actions);
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, Action::History(2)))
                .count(),
            1
        );
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, Action::History(3)))
                .count(),
            1
        );
        assert!(panels.properties.field.is_none());
        // Feed actual UI commands to the core: one undo restores both value and axes.
        let mut project = motion_core::Project::new(1280, 720, 30, 120).unwrap();
        project.layers.push(motion_core::Layer::solid(
            7,
            "Layer",
            [100., 100.],
            [10., 20., 0.],
            [1.; 4],
        ));
        let mut editor = motion_core::Engine::new(project).unwrap();
        editor
            .apply(Command::SeparateDimensions {
                object: 7,
                property: Property::Position,
            })
            .unwrap();
        let before = editor.project().clone();
        for action in actions {
            match action {
                Action::History(2) => editor.begin_gesture().unwrap(),
                Action::History(3) => editor.end_gesture(true).unwrap(),
                Action::Edit(command) => editor.apply(command).unwrap(),
                _ => {}
            }
        }
        assert_ne!(editor.project(), &before);
        editor.undo().unwrap();
        assert_eq!(editor.project(), &before);
        editor.begin_gesture().unwrap();
        editor.end_gesture(false).unwrap();
    }
    #[test]
    fn click_keeps_value_and_typing_replaces_only_after_input() {
        let mut panels = panels();
        let mut actions = vec![];
        process_field(&mut panels, &state(), &release(100.), &mut actions);
        assert_eq!(panels.properties.field.as_ref().unwrap().text, "10.00");
        assert!(actions.is_empty());
        process_field(
            &mut panels,
            &state(),
            &Input {
                text: "32.5".into(),
                ..Default::default()
            },
            &mut actions,
        );
        assert_eq!(panels.properties.field.as_ref().unwrap().text, "32.5");
        process_field(
            &mut panels,
            &state(),
            &Input {
                mouse: [250., 15.],
                pressed: Some((MouseButton::Left, [250., 15.])),
                ..Default::default()
            },
            &mut actions,
        );
        assert!(panels.properties.field.is_none());
        assert!(
            matches!(&actions[0],Action::Edit(Command::SetComponent {axis:motion_core::Axis::X,frame:21,value,..}) if *value==32.5)
        );
    }
    #[test]
    fn empty_numeric_input_changes_nothing_and_escape_cancels_scrub() {
        let mut panels = panels();
        let mut actions = vec![];
        process_field(&mut panels, &state(), &release(100.), &mut actions);
        process_field(
            &mut panels,
            &state(),
            &Input {
                keys: vec![(Key::Backspace, Default::default())],
                ..Default::default()
            },
            &mut actions,
        );
        process_field(
            &mut panels,
            &state(),
            &Input {
                keys: vec![(Key::Enter, Default::default())],
                ..Default::default()
            },
            &mut actions,
        );
        assert!(actions.is_empty());
        assert!(panels.properties.field.is_none());
        panels = super::tests::panels();
        process_field(&mut panels, &state(), &drag(130.), &mut actions);
        let cancelled = cancel_interaction(&mut panels);
        assert!(matches!(cancelled.as_slice(), [Action::History(4)]));
        assert!(panels.properties.field.is_none());
        assert!(cancel_interaction(&mut panels).is_empty());
    }
}
