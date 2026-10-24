use super::*;

#[derive(Default)]
pub(crate) struct AnchorControls {
    field: Option<AnchorField>,
}
struct AnchorField {
    object: u64,
    axis: usize,
    rect: Rect,
    number: super::numeric::Field,
}
pub(super) fn anchor_command(state: &Value, object: u64, anchor: [f32;2]) -> Command {
    let animated = state["project"]["layers"].as_array().into_iter().flatten()
        .find(|l|l["id"].as_u64()==Some(object))
        .is_some_and(|l|l["transform"]["anchor"]["keys"].as_array().is_some_and(|k|!k.is_empty()));
    if animated {
        Command::AnchorAt { object, anchor, frame: state["frame"].as_f64().unwrap_or(0.).floor() as u32 }
    } else { Command::Anchor { object, anchor } }
}
impl AnchorControls {
    pub fn editing(&self) -> bool {
        self.field.is_some()
    }
    pub fn cancel(&mut self) -> bool {
        self.field
            .take()
            .is_some_and(|mut field| field.number.cancel())
    }
    fn options(styles: &motion_ui::style::StyleSheet) -> motion_ui::widgets::NumberOptions {
        motion_ui::widgets::NumberOptions {
            min: -1e9,
            max: 1e9,
            speed: styles.metric("property.anchor.step", 0.5) as f64,
            decimals: 2,
        }
    }
    fn values(layer: &Value) -> ([f32; 2], [f32; 2]) {
        Self::values_with_anchor(layer,&layer["transform"]["anchor"])
    }
    fn values_with_anchor(layer: &Value, anchor: &Value) -> ([f32;2],[f32;2]) {
        (
            std::array::from_fn(|axis| {
                observe!(
                    "properties.anchor",
                    anchor[axis]
                        .as_f64()
                        .ok_or("missing or nonnumeric anchor"),
                    "object={} axis={axis}",
                    layer["id"]
                )
                .unwrap_or(0.5) as f32
            }),
            std::array::from_fn(|axis| {
                let size = layer["size"][axis].as_f64();
                let _ = observe!(
                    "properties.anchor_size",
                    size.filter(|v| v.is_finite() && *v >= 1.)
                        .ok_or("missing, nonfinite or clamped size"),
                    "object={} axis={axis} size={size:?}",
                    layer["id"]
                );
                size.unwrap_or(1.).max(1.) as f32
            }),
        )
    }
    fn values_at(state: &Value, layer: &Value) -> ([f32;2],[f32;2]) {
        // The host's sampled anchor uses the same composition/local clock as its
        // model matrices. Legacy fixtures and static anchors retain their array.
        if layer["transform"]["anchor"].is_object() {
            let sampled = state["sampledLayers"].as_array().into_iter().flatten()
                .find(|l| l["id"]==layer["id"]).map(|l|&l["anchor"]);
            if let Some(anchor) = observe!("properties.anchor_sample",sampled.ok_or("animated layer anchor sample missing"),"object={}",layer["id"]).ok() {
                return Self::values_with_anchor(layer,anchor);
            } else { return Self::values_with_anchor(layer,&layer["transform"]["anchor"]["value"]); }
        }
        Self::values(layer)
    }
    pub fn process(
        &mut self,
        styles: &motion_ui::style::StyleSheet,
        state: &Value,
        input: &Input,
        out: &mut Vec<Action>,
    ) {
        let Some(object) = self.field.as_ref().map(|field| field.object) else {
            return;
        };
        let Some(layer) = state["project"]["layers"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|layer| layer["id"].as_u64() == Some(object))
        else {
            if self.cancel() {
                out.push(Action::History(4));
            }
            return;
        };
        if layer["locked"] == true || state["observing"] == true {
            if self.cancel() {
                out.push(Action::History(4));
            }
            return;
        }
        self.process_layer(styles, layer, state, input, out);
    }
    fn process_layer(
        &mut self,
        styles: &motion_ui::style::StyleSheet,
        layer: &Value,
        state: &Value,
        input: &Input,
        out: &mut Vec<Action>,
    ) {
        let Some(mut field) = self.field.take() else {
            return;
        };
        let (anchor, size) = Self::values_at(state,layer);
        field.number.editor.drag_threshold = Some(styles.metric("motion.drag_threshold", 3.));
        let events = field.number.editor.update(
            f64::from(anchor[field.axis] * size[field.axis]),
            Self::options(styles),
            input,
            field.rect,
        );
        let finished = super::numeric::apply(events, out, |value| {
            let mut next = anchor;
            next[field.axis] = value / size[field.axis];
            anchor_command(state,field.object,next)
        });
        if !finished {
            self.field = Some(field);
        }
    }
    pub(super) fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: f32,
        host: Panel,
        layer: &Value,
        state: &Value,
        input: &Input,
        locked: bool,
        out: &mut Vec<Action>,
    ) {
        let object = layer["id"].as_u64().unwrap_or(0);
        let styles = list.styles();
        let (anchor, size) = Self::values_at(state,layer);
        for axis in 0..2 {
            let rect = if host == Panel::Timeline {
                super::timeline::property_layout(
                    &styles,
                    area,
                    y,
                    styles.metric("row.height", 22.),
                    axis,
                    2,
                )
                .number
            } else {
                inspector_layout(&styles, area, y, axis, 2).number
            };
            let active = self
                .field
                .as_ref()
                .is_some_and(|field| field.object == object && field.axis == axis);
            if clicked(input, rect) && !locked && !active {
                if self.cancel() {
                    out.push(Action::History(4));
                }
                self.field = Some(AnchorField {
                    object,
                    axis,
                    rect,
                    number: Default::default(),
                });
                self.process_layer(&styles, layer, state, input, out);
            }
            let value = f64::from(anchor[axis] * size[axis]);
            if let Some(field) = self
                .field
                .as_mut()
                .filter(|field| field.object == object && field.axis == axis)
            {
                field.rect = rect;
                field
                    .number
                    .editor
                    .draw(list, atlas, rect, value, Self::options(&styles));
            } else {
                super::numeric::passive_with_precision(list, atlas, rect, value, input, !locked, 2);
            }
        }
    }
}

pub(super) fn inspector_layout(
    styles: &motion_ui::style::StyleSheet,
    area: Rect,
    y: f32,
    axis: usize,
    axes: usize,
) -> super::timeline::PropertyLayout {
    let pad = styles.metric("padding.panel", 8.);
    let indent = styles.metric("tree.indent", 14.);
    let slot = styles.metric("icon.slot", 20.);
    let row = styles.metric("row.height", 22.);
    let field = styles.metric("field.height", 18.).min(row);
    let left = area.min[0] + pad + super::key_navigation::width(styles) + indent;
    let caption = left + slot;
    let value = (caption + styles.metric("label.column", 72.) + pad)
        .min(area.max[0] - pad - axes.max(1) as f32 * styles.metric("value.min_width", 64.))
        .max(caption + styles.metric("label.column", 72.) * 0.5 + pad);
    let width = ((area.max[0] - pad - value).max(0.) / axes.max(1) as f32)
        .min(styles.metric("value.min_width", 64.));
    super::timeline::PropertyLayout {
        watch: Rect::new(left, y, slot, row),
        caption: Rect::new(caption, y, (value - pad - caption).max(0.), row),
        number: Rect::new(
            value + axis as f32 * width,
            y + (row - field) * 0.5,
            (width - styles.metric("border", 1.) * 2.).max(0.),
            field,
        ),
    }
}

fn group(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    y: f32,
    title: &str,
    open: bool,
    input: &Input,
) -> bool {
    let styles = list.styles();
    let pad = styles.metric("padding.panel", 8.);
    let slot = styles.metric("icon.slot", 20.);
    let row = styles.metric("row.height", 22.);
    let rect = Rect::new(area.min[0] + pad, y, (area.width() - pad * 2.).max(0.), row);
    motion_ui::style::box_style(
        list,
        rect,
        styles
            .resolve("effect.header", motion_ui::style::States::default())
            .unwrap(),
    );
    motion_ui::icons::draw(
        list,
        Rect::new(rect.min[0], y, slot, row),
        if open {
            motion_ui::icons::Icon::ChevronDown
        } else {
            motion_ui::icons::Icon::ChevronRight
        },
        styles.metric("icon.small.size", 14.),
        styles.color("muted"),
    );
    label(
        list,
        atlas,
        Rect::new(rect.min[0] + slot, y, (rect.width() - slot).max(0.), row),
        title,
        false,
    );
    clicked(input, rect)
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
    let styles = list.styles();
    let colors = styles.colors();
    let neutral = Input::default();
    let mask_modal = panels.path_controls.inspector_menu_in(Panel::Properties);
    let parent_modal = panels.parent_controls.open;
    let effect_modal = panels.properties.effects.editing();
    let original = input;
    let input = if mask_modal || parent_modal || effect_modal {
        &neutral
    } else {
        input
    };
    if panels.focused != Some(Panel::Timeline) {
        process_field(panels, state, input, out);
    }
    panels.properties.anchor.process(&styles, state, input, out);
    paint::rect(list, area, colors.PANEL);
    let row = styles.metric("row.height", 22.);
    let pad = styles.metric("padding.panel", 8.);
    let indent = styles.metric("tree.indent", 14.);
    let slot = styles.metric("icon.slot", 20.);
    let filtered = Input {
        pressed: input.pressed.filter(|(_, point)| area.contains(*point)),
        ..input.clone()
    };
    let input = &filtered;
    if input.pressed.is_some() {
        panels.properties.effects.clear_key_channel();
    }
    if area.contains(input.mouse) {
        panels.properties.properties_scroll += input.wheel[1];
    }
    panels.properties.properties_scroll = panels.properties.properties_scroll.clamp(
        0.,
        (panels.properties.content_height - area.height()).max(0.),
    );
    let mut y = area.min[1] + pad - panels.properties.properties_scroll;
    if let Some(object) = panels.selected.filter(|id| {
        let camera = super::camera_controls::camera(state, *id);
        !camera.is_null() && camera["created"].as_bool().unwrap_or(true)
    }) {
        panels.camera_controls.filter = panels.timeline.filters.get(&object).copied().unwrap_or_default();
        let navigation = panels
            .camera_controls
            .render(list, atlas, area, &mut y, state, object, input, panels.locale,
                &mut panels.animation_controls, &mut panels.timeline.graph_component, out);
        if let Some((property, axis, choice)) = navigation {
            super::key_navigation::apply_property_choice(panels, state, object, property, axis, choice, out);
        }
        if panels.camera_controls.cursor {
            panels.cursor = Some(motion_ui::widgets::numeric_behavior::cursor(
                input, panels.camera_controls.field_dragging(),
            ));
        }
        if let Some(note) = &panels.camera_controls.drag_readout {
            panels.drag_readout = Some(note.clone());
        }
        panels.properties.content_height =
            y - area.min[1] + panels.properties.properties_scroll + pad;
        return;
    }
    if panels.camera_controls.cancel() {
        out.push(Action::History(4));
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
    let Some(layer) = state["project"]["layers"].as_array().and_then(|layers| {
        layers
            .iter()
            .find(|layer| layer["id"].as_u64() == Some(object))
    }) else {
        return;
    };
    let sample = state["sampledLayers"].as_array().and_then(|layers| {
        layers
            .iter()
            .find(|layer| layer["id"].as_u64() == Some(object))
    });
    let Some(sample) = observe!(
        "properties.sample",
        sample.ok_or("selected layer has no sampled data"),
        "object={object} revision={}",
        state["revision"]
    )
    .ok() else {
        return;
    };
    let locale = panels.locale;
    let locked = layer["locked"] == true || state["observing"] == true;
    label(
        list,
        atlas,
        Rect::new(area.min[0] + pad, y, (area.width() - pad * 2.).max(0.), row),
        &format!("<{}>", layer["name"].as_str().unwrap_or(locale.t(T::Layer))),
        false,
    );
    y += row;
    if !matches!(
        layer["content"]["kind"].as_str(),
        Some("audio" | "null" | "adjustment")
    ) {
        if let Some(typed) = panels.path_controls.inspector_layer(state, layer) {
            panels.path_controls.masks_in_properties(
                list,
                atlas,
                area,
                &mut y,
                &typed,
                state,
                if parent_modal
                    || effect_modal
                    || panels.path_controls.groups.menu_in(Panel::Properties)
                {
                    &neutral
                } else {
                    original
                },
                locale,
                panels.timeline.filters.get(&object).copied().unwrap_or_default(),
                out,
            );
        }
    }
    if group(
        list,
        atlas,
        area,
        y,
        locale.t(T::Transform),
        !panels.properties.transform_collapsed,
        input,
    ) {
        out.extend(super::cancel_interaction(panels));
        panels.properties.transform_collapsed = !panels.properties.transform_collapsed;
        if !panels.properties.transform_collapsed {
            panels.timeline.reopen_properties(object);
        }
    }
    y += row;
    if !panels.properties.transform_collapsed {
        let rows = super::timeline::filtered_properties(state, object,
            panels.timeline.filters.get(&object).copied().unwrap_or_default());
        let mut selection = panels.animation_controls.track_selection(state, panels.selected);
        for track in rows {
            let property = track.property;
            let axis = track.axis.map(|axis| [motion_core::Axis::X, motion_core::Axis::Y, motion_core::Axis::Z][axis]);
            let key = track.key;
            let name = locale.t(track.title);
            let axes = &track.axes;
            let fields = inspector_layout(&styles, area, y, 0, axes.len());
            panels.animation_controls.record_row(
                super::animation_controls::SelectedTrack {object, property, axis}, fields.caption,
                axes.iter().enumerate().map(|(index, component)| (*component,
                    inspector_layout(&styles, area, y, index, axes.len()).number)),
            );
            let component = axes.iter().enumerate().find_map(|(index, component)| {
                clicked(input, inspector_layout(&styles, area, y, index, axes.len()).number)
                    .then_some(*component)
            });
            if clicked(input, fields.caption) || component.is_some() {
                panels.select_only(Some(object));
                panels.animation_controls.choose_track_in(state, object, property, axis);
                if let Some(component) = component {
                    panels.timeline.graph_component = component;
                } else {
                    panels.animation_controls.clear_key();
                }
                selection = panels.animation_controls.track_selection(state, panels.selected);
            }
            super::animation_controls::highlight_row(
                list, Rect::new(area.min[0], y, area.width(), row),
                selection == super::animation_controls::TrackSelection::Current(
                    super::animation_controls::SelectedTrack { object, property, axis }),
            );
            super::key_navigation::property_row(
                list, Rect::new(area.min[0], y, area.width(), row), panels, state, input,
                object, property, axis, &track.keys, out,
            );
            super::timeline::property_branch(list, fields.watch);
            let animated = !track.keys.is_empty();
            super::timeline::property_icon(
                list,
                fields.watch,
                motion_ui::icons::Icon::Stopwatch,
                animated,
                !locked,
            );
            let expression_target = motion_core::ExpressionTarget::Property {
                object,
                property,
                axis: track.axis.map(|a| {
                    [
                        motion_core::Axis::X,
                        motion_core::Axis::Y,
                        motion_core::Axis::Z,
                    ][a]
                }),
            };
            if property != Property::Anchor { panels.expressions.context(
                expression_target.clone(),
                Rect::new(
                    fields.watch.min[0],
                    y,
                    (area.max[0] - fields.watch.min[0]).max(0.),
                    row,
                ),
                input,
                Panel::Properties,
            ); }
            if clicked(input, fields.watch) && !locked && input.modifiers.alt && property != Property::Anchor {
                panels
                    .expressions
                    .open(state, expression_target.clone(), Panel::Properties);
            } else if clicked(input, fields.watch) && !locked {
                out.push(Action::Edit(Command::Animate {
                    object,
                    property,
                    axis: track.axis.map(|a| {
                        [
                            motion_core::Axis::X,
                            motion_core::Axis::Y,
                            motion_core::Axis::Z,
                        ][a]
                    }),
                    frame: state["frame"].as_f64().unwrap_or(0.).floor() as u32,
                    enabled: !animated,
                }));
            }
            let caption = track.axis.map_or_else(
                || name.into(),
                |axis| format!("{name} {}", ["X", "Y", "Z"][axis]),
            );
            label(list, atlas, fields.caption, &caption, true);
            if property == Property::Anchor {
                panels.properties.anchor.draw(list,atlas,area,y,Panel::Properties,layer,state,input,locked,out);
            } else {
            for (index, axis) in axes.iter().copied().enumerate() {
                let value = if property == Property::Opacity {
                    sample[key].as_f64().unwrap_or(1.) * 100.
                } else {
                    sample[key][axis].as_f64().unwrap_or(0.)
                };
                let rect = inspector_layout(&styles, area, y, index, axes.len()).number;
                draw_value(
                    list, atlas, rect, panels, state, input, object, property, axis, value, locked,
                    false, out,
                );
            }
            }
            y += row;
            if panels
                .expressions
                .visible(state, &expression_target, Panel::Properties)
            {
                let split = (area.min[0] + area.width() * 0.4).max(fields.caption.min[0]);
                panels.expressions.row(
                    list,
                    atlas,
                    Rect::new(
                        fields.caption.min[0],
                        y,
                        (split - fields.caption.min[0]).max(0.),
                        row,
                    ),
                    Rect::new(split, y, (area.max[0] - split - pad).max(0.), row),
                    state,
                    expression_target,
                    input,
                    locale,
                    !locked,
                    out,
                );
                y += row;
            }
        }
    }
    let child = Rect::new(
        area.min[0] + indent,
        area.min[1],
        (area.width() - indent).max(0.),
        area.height(),
    );
    panels.parent_controls.render(
        list,
        atlas,
        area,
        &mut y,
        object,
        layer,
        state,
        if mask_modal || effect_modal {
            &neutral
        } else {
            original
        },
        locale,
        out,
    );
    if layer["content"]["kind"] == "vector" {
        if let Some(layer) = panels.path_controls.inspector_layer(state, layer) {
            panels.path_controls.groups.property_filter = panels.timeline.filters.get(&object).copied().unwrap_or_default();
            panels.path_controls.groups.render(
                list,
                atlas,
                area,
                &mut y,
                &layer,
                state,
                if panels.path_controls.groups.menu_in(Panel::Properties) {
                    original
                } else {
                    input
                },
                locale,
                Panel::Properties,
                out,
            );
            panels.path_controls.groups.finish_paint(
                list,
                atlas,
                area,
                state,
                if parent_modal || effect_modal {
                    &neutral
                } else {
                    original
                },
                locale,
                Panel::Properties,
                out,
            );
        }
    }
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
    if let Some((rect, dragging)) = panels.path_controls.groups.cursor {
        if dragging || rect.contains(input.mouse) {
            panels.cursor = Some(motion_ui::widgets::numeric_behavior::cursor(
                input, dragging,
            ));
        }
    }
    if group(
        list,
        atlas,
        area,
        y,
        if locale == Locale::Zh {
            "效果"
        } else {
            "Effects"
        },
        !panels.properties.effects_collapsed,
        input,
    ) {
        out.extend(super::cancel_interaction(panels));
        panels.properties.effects_collapsed = !panels.properties.effects_collapsed;
    }
    y += row;
    if !panels.properties.effects_collapsed {
        panels.properties.effects.property_filter = panels.timeline.filters.get(&object).copied().unwrap_or_default();
        panels.properties.effects.catalogue = panels.effect_controls.catalogue.clone();
        panels.properties.effects.importing_package = panels.effect_controls.importing_package;
        panels.properties.effects.favorites = panels.effect_controls.favorites.clone();
        panels.properties.effects.render(
            list,
            atlas,
            child,
            &mut y,
            layer,
            state,
            if mask_modal || parent_modal {
                &neutral
            } else {
                original
            },
            locale,
            out,
        );
        if let Some(note) = &panels.properties.effects.drag_readout {
            panels.drag_readout = Some(note.clone());
        }
        if panels.properties.effects.cursor {
            panels.cursor = Some(motion_ui::widgets::numeric_behavior::cursor(
                original,
                panels.properties.effects.drag_readout.is_some(),
            ));
        }
    }
    panels.properties.content_height = y - area.min[1] + panels.properties.properties_scroll + pad;
    panels.path_controls.append_inspector_overlays(list);
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
    let draft = panels.properties.field.as_ref()
        .filter(|field| active && field.scrubbing.is_none() && field.editor.active());
    if draft.is_none() {
        motion_ui::style::box_style(list, r, style);
    }
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
    if let Some(field) = draft {
        field.editor.draw_styled(list, atlas, r, &styles, "field.number", states);
    } else {
        motion_ui::style::text_style(
            list,
            atlas,
            r.shrink(style.padding),
            &text,
            style,
            if states.contains(motion_ui::style::State::Focused) {
                Align::Left
            } else {
                Align::Right
            },
        );
    }
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
            editor: Default::default(),
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
    if matches!(field.property, Property::Roll | Property::Fov | Property::Radius | Property::Azimuth | Property::Elevation) {
        return Some(Command::SetScalar { object: field.object, property: field.property, frame, value });
    }
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
        Property::Target => "target",
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
    if input.focus_lost || input.key_presses().any(|(key, _, repeat)| key == Key::Escape && !repeat) {
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
                    log::warn!("property scrub fallback object={} property={:?} axis={} reason=invalid_numeric_text", field.object, field.property, field.axis);
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
                        Property::Scale => sample["scale"][field.axis].as_f64().unwrap_or(1.) as f32,
                        Property::Target => sample["target"][field.axis].as_f64().unwrap_or(0.) as f32,
                        property => sample[match property {
                            Property::Roll => "roll", Property::Fov => "fov", Property::Radius => "radius",
                            Property::Azimuth => "azimuth", _ => "elevation",
                        }].as_f64().unwrap_or(0.) as f32,
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
            if field.property == Property::Opacity { value = value.clamp(0., 100.); }
            if value.is_finite() && value != scrub.last_value {
                scrub.last_value = value;
                field.text = format!("{value:.2}");
                if let Some(command) = observe!(
                    "properties.scrub_command",
                    numeric_command(&field, value, state).ok_or("no command after scrub advanced"),
                    "object={} property={:?} axis={} phase=scrubbing value={value}",
                    field.object,
                    field.property,
                    field.axis
                )
                .ok()
                {
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
            field.editor.cancel();
        }
    }
    if field.scrubbing.is_none() {
        let event = field.editor.input_value(&mut field.text, &mut field.replace, input);
        if outside || matches!(event, Some(motion_ui::widgets::TextEvent::Commit(_))) {
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
    mod clipboard;
    #[test]
    fn anchor_and_scrub_fallback_evidence_preserves_values_and_empty_gestures() {
        const CHILD: &str = "MOTION_PROPERTY_FALLBACK_OBSERVATION_CHILD";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = std::path::PathBuf::from(root);
            let _logger = crate::logging::init(root.clone()).unwrap();
            let healthy =
                serde_json::json!({"id":7,"transform":{"anchor":[0.5,0.5]},"size":[100,200]});
            assert_eq!(AnchorControls::values(&healthy), ([0.5; 2], [100., 200.]));
            assert!(std::fs::read_to_string(root.join("motion-studio.log"))
                .unwrap()
                .is_empty());
            let broken =
                serde_json::json!({"id":7,"transform":{"anchor":["bad",0.5]},"size":[0,200]});
            for _ in 0..120 {
                assert_eq!(AnchorControls::values(&broken), ([0.5; 2], [1., 200.]));
                AnchorControls::values(&healthy);
            }
            let mut p = panels();
            let field = p.properties.field.as_mut().unwrap();
            field.scrubbing = None;
            field.text = "private invalid input".into();
            let mut actions = Vec::new();
            process_field(
                &mut p,
                &state(),
                &Input {
                    mouse: [100., 15.],
                    pressed: Some((MouseButton::Left, [100., 15.])),
                    ..Default::default()
                },
                &mut actions,
            );
            assert_eq!(
                p.properties
                    .field
                    .as_ref()
                    .unwrap()
                    .scrubbing
                    .as_ref()
                    .unwrap()
                    .start_value,
                10.
            );
            assert!(actions.is_empty());
            let mut missing = state();
            missing["sampledLayers"] = serde_json::json!([]);
            missing["timeline_layers"] = serde_json::json!([]);
            for _ in 0..120 {
                let mut p = panels();
                let mut actions = Vec::new();
                process_field(&mut p, &missing, &drag(120.), &mut actions);
                assert!(matches!(actions.as_slice(), [Action::History(2)]));
                let scrub = p
                    .properties
                    .field
                    .as_ref()
                    .unwrap()
                    .scrubbing
                    .as_ref()
                    .unwrap();
                assert!(scrub.begun);
                assert_eq!(scrub.last_value, 20.);
            }
            let text = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
            for site in [
                "properties.anchor",
                "properties.anchor_size",
                "properties.scrub_command",
            ] {
                assert_eq!(
                    text.lines()
                        .filter(|line| line.contains(&format!("site={site} ")))
                        .count(),
                    2,
                    "{text}"
                );
            }
            assert!(
                text.contains("reason=invalid_numeric_text")
                    && text.contains("object=7 property=Position axis=0")
            );
            assert!(!text.contains("private invalid input"));
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "panels::properties::tests::anchor_and_scrub_fallback_evidence_preserves_values_and_empty_gestures"])
            .env_remove("MOTION_LOG").env(CHILD,temp.path()).output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stderr)
            .contains("persistent panel degradation site=properties.scrub_command"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("panel degradation"));
    }
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
                    editor: Default::default(),
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

#[cfg(test)]
mod tree_tests;
