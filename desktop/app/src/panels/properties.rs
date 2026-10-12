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
    if panels.focused != Some(Panel::Timeline) {
        process_field(panels, state, input, out);
    }
    paint::rect(list, area, palette::PANEL);
    let filtered = Input {
        pressed: input.pressed.filter(|(_, p)| area.contains(*p)),
        ..input.clone()
    };
    let input = &filtered;
    if panels.selected == Some(0)
        && state["project"]["camera"]["created"]
            .as_bool()
            .unwrap_or(false)
    {
        if area.contains(input.mouse) && !panels.parent_controls.open {
            panels.properties.properties_scroll = (panels.properties.properties_scroll
                + input.wheel[1])
                .clamp(0., (800. - area.height()).max(0.));
        }
        let mut y = area.min[1] + 8. - panels.properties.properties_scroll;
        panels.parent_controls.render(
            list,
            atlas,
            area,
            &mut y,
            0,
            &state["project"]["camera"],
            state,
            input,
            panels.locale,
            out,
        );
        panels
            .camera_controls
            .render(list, atlas, area, &mut y, state, input, panels.locale, out);
        if panels.camera_controls.cursor {panels.cursor=Some(motion_ui::cursor::Cursor::ResizeEW);}
        if let Some(readout) = &panels.camera_controls.drag_readout {panels.drag_readout=Some(readout.clone());}
        return;
    }
    let Some(object) = panels.selected.filter(|id| *id != 0) else {
        label(
            list,
            atlas,
            area.shrink(10.0),
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
    let timeline = state["timeline_layers"]
        .as_array()
        .and_then(|ls| ls.iter().find(|l| l["object"].as_u64() == Some(object)));
    let content_height = if three_d { 620.0 } else { 490.0 };
    if area.contains(input.mouse) && !panels.parent_controls.open {
        panels.properties.properties_scroll = (panels.properties.properties_scroll
            + input.wheel[1])
            .clamp(0.0, (content_height - area.height()).max(0.0));
    }
    let mut y = area.min[1] + 8.0 - panels.properties.properties_scroll;
    let heading = Rect::new(area.min[0], y, area.width(), 32.0);
    paint::rect(list, heading, palette::CHROME.with_alpha(0.55));
    label(
        list,
        atlas,
        heading.shrink(10.0),
        layer["name"].as_str().unwrap_or(locale.t(T::Layer)),
        false,
    );
    y += 40.0;
    panels.parent_controls.render(
        list, atlas, area, &mut y, object, layer, state, input, locale, out,
    );
    let check = Rect::new(area.max[0] - 58.0, y + 7.0, 14.0, 14.0);
    paint::stroke(list, check, palette::TEXT_MUTED, 1.0);
    if three_d {
        paint::rect(list, check.shrink(3.0), palette::ACCENT);
    }
    label(
        list,
        atlas,
        Rect::new(area.min[0] + 20.0, y, area.width() - 82.0, 30.0),
        locale.t(T::Enable3D),
        false,
    );
    if clicked(
        input,
        Rect::new(area.min[0] + 12.0, y, area.width() - 24.0, 30.0),
    ) && !locked
    {
        out.push(Action::Edit(Command::SetLayer3d {
            object,
            enabled: !three_d,
        }));
    }
    y += 42.0;
    paint::rect(
        list,
        Rect::new(area.min[0] + 12.0, y, area.width() - 24.0, 1.0),
        palette::BORDER,
    );
    y += 4.0;
    let group = Rect::new(area.min[0] + 10.0, y, area.width() - 20.0, 30.0);
    label(
        list,
        atlas,
        group,
        &format!(
            "{}  {}",
            if panels.properties.transform_collapsed {
                "›"
            } else {
                "⌄"
            },
            locale.t(T::Transform)
        ),
        false,
    );
    if clicked(input, group) {
        panels.properties.transform_collapsed = !panels.properties.transform_collapsed;
        panels.properties.field = None;
    }
    y += 34.0;
    let frame = state["frame"].as_f64().unwrap_or(0.0).floor() as u32;
    if !panels.properties.transform_collapsed {
        for (name, property, key, axes) in [
            (
                locale.t(T::Position),
                Property::Position,
                "position",
                if three_d { vec![0, 1, 2] } else { vec![0, 1] },
            ),
            (
                locale.t(T::Rotation),
                Property::Rotation,
                "rotation",
                if three_d { vec![0, 1, 2] } else { vec![2] },
            ),
            (
                locale.t(T::Scale),
                Property::Scale,
                "scale",
                if three_d { vec![0, 1, 2] } else { vec![0, 1] },
            ),
            (locale.t(T::Opacity), Property::Opacity, "opacity", vec![0]),
        ] {
            if !super::timeline::property_visible(panels, state, object, property) {
                continue;
            }
            let track = timeline.map(|l| &l["properties"][key]);
            let separated = track.is_some_and(|t| t["separated"].as_bool().unwrap_or(false));
            let animated = track.is_some_and(|t| !t["keys"].as_array().is_none_or(Vec::is_empty));
            let animate = Rect::new(area.min[0] + 14.0, y + 2.0, 14.0, 24.0);
            ui::icon(list, animate, Icon::Stopwatch);
            if animated {
                paint::rect(
                    list,
                    Rect::new(animate.min[0], y + 22.0, 12.0, 2.0),
                    palette::ACCENT,
                );
            }
            if clicked(input, animate) && !locked && !separated {
                out.push(Action::Edit(Command::Animate {
                    object,
                    property,
                    axis: None,
                    frame,
                    enabled: !animated,
                }));
            }
            label(
                list,
                atlas,
                Rect::new(area.min[0] + 38.0, y, area.width() * 0.40, 28.0),
                name,
                true,
            );
            for axis in axes {
                let value = if property == Property::Opacity {
                    sample[key].as_f64().unwrap_or(1.0) * 100.0
                } else {
                    sample[key][axis].as_f64().unwrap_or(0.0)
                };
                let r = Rect::new(
                    area.min[0] + area.width() * 0.59,
                    y,
                    (area.width() * 0.38 - 8.0).max(1.0),
                    28.0,
                );
                if property != Property::Opacity {
                    label(
                        list,
                        atlas,
                        Rect::new(r.min[0] - 20.0, y, 18.0, 28.0),
                        ["X", "Y", "Z"][axis],
                        true,
                    );
                }
                draw_value(
                    list, atlas, r, panels, state, input, object, property, axis, value, locked,
                    false, out,
                );
                y += 28.0;
            }
            y += 6.0;
        }
    }
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
    let key = (
        object,
        100 + property as u64 * 8 + axis as u64 + if timeline { 64 } else { 0 },
    );
    if !locked && (r.contains(input.mouse)||panels.properties.field.as_ref().is_some_and(|f|f.scrubbing.as_ref().is_some_and(|s|s.begun))) {
        panels.cursor = Some(motion_ui::cursor::Cursor::ResizeEW);
    }
    let hover = panels.motion.value(
        key,
        0.,
        if !locked && r.contains(input.mouse) {
            1.
        } else {
            0.
        },
        motion_ui::style::default_stylesheet().metric("motion.hover_ms", 90.),
        false,
    );
    let changed = panels.motion.changed(
        (key.0, key.1 + 200),
        value,
        motion_ui::style::default_stylesheet().metric("motion.value_ms", 140.),
    );
    paint::rect(
        list,
        r,
        palette::ACCENT.with_alpha((hover * 0.07).max(changed * 0.14)),
    );
    let active = panels.properties.field.as_ref().is_some_and(|f| {
        f.object == object
            && f.property == property
            && f.axis == axis
            && (panels.focused == Some(Panel::Timeline)) == timeline
    });
    let mut states=motion_ui::style::States::default();
    if active&&panels.properties.field.as_ref().is_some_and(|f|f.scrubbing.is_none()) {states=states.with(motion_ui::style::State::Focused);}
    if !locked&&r.contains(input.mouse){states=states.with(motion_ui::style::State::Hovered);}
    if locked {states=states.with(motion_ui::style::State::Disabled);}
    let style=motion_ui::style::default_stylesheet().resolve("field.number",states).unwrap();
    motion_ui::style::box_style(list,r,style);
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
    if active&&panels.properties.field.as_ref().is_some_and(|f|f.scrubbing.as_ref().is_some_and(|s|s.begun)) {
        let delta=panels.properties.field.as_ref().and_then(|f|f.scrubbing.as_ref()).map_or(0.,|s|s.last_value-s.start_value);
        panels.drag_readout=Some((input.mouse,format!("{}  {:+.2}",text,delta)));
    }
    ui::text(list, atlas, r, &text, palette::ACCENT, Align::Left);
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
    let outside = input.pressed.is_some_and(|(_, p)| !field.rect.contains(p));
    let released = input
        .released
        .is_some_and(|(button, _)| button == MouseButton::Left);
    if let Some(scrub) = field.scrubbing.as_mut() {
        let delta = input.mouse[0] - scrub.start_mouse;
        if (input.dragging() || released)
            && (delta.abs().max(input.pointer_travel) >= 3. || scrub.begun)
        {
            if !scrub.begun {
                scrub.begun = true;
                out.push(Action::History(2));
            }
            let step = if input.modifiers.shift {
                0.1
            } else if input.modifiers.control {
                10.
            } else {
                1.
            };
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
