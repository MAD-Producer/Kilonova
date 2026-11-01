//! Explicit property/axis selection; all timing edits use shared core commands.
use super::types::KeySelection;
use crate::{
    i18n::{Locale, Text as T},
    panels::Action,
};
use motion_core::{Axis, Command, Curve, CurveShape, CurveSpace, Ease, Easing, Property};
use motion_ui::{
    input::{Input, Key, MouseButton, Rect},
    paint::{self, PaintList},
    text::TextAtlas,
    ui::{self, Align},
};
use serde_json::Value;
mod selection;
mod time;
#[cfg(test)]
mod mapped_tests;
pub(in crate::panels) mod mini;
pub(super) use selection::{highlight_row, SelectedTrack, TrackSelection};

#[derive(Default)]
pub struct Controls {
    pub expanded: bool,
    pub property: Option<Property>,
    pub axis: Option<Axis>,
    selected_key: Option<i64>,
    key_context: Option<(u64, Property, Option<Axis>)>,
    key_explicit: bool,
    key_local: bool,
    local_selection: Option<Option<super::key_selection::LocalKey>>,
    drag_time: Option<time::Context>,
    drag_key: Option<(u64, Property, Option<Axis>, i64)>,
    track_explicit: bool,
    track_scope: Option<(String, String, Option<u64>)>,
    row_probe: Option<selection::RowProbe>,
    selection_intent: bool,
    pub(super) clipboard_busy: bool,
    #[cfg(test)]
    clipboard_hits: Option<[Rect; 2]>,
    dragging: Option<(usize, Easing, super::graph_ranges::Bounds)>,
    mini: mini::View,
}
fn key(property: Property) -> &'static str {
    match property {
        Property::Anchor => "anchor",
        Property::Rotation => "rotation",
        Property::Scale => "scale",
        Property::Opacity => "opacity",
        Property::Target => "target", Property::Roll => "roll", Property::Fov => "fov",
        Property::Radius => "radius", Property::Azimuth => "azimuth", Property::Elevation => "elevation",
        Property::Position => "position",
    }
}
fn pressed(input: &Input, rect: Rect) -> bool {
    input
        .pressed
        .is_some_and(|(b, p)| b == MouseButton::Left && rect.contains(p))
}
fn button(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    input: &Input,
    rect: Rect,
    label: &str,
    enabled: bool,
) -> bool {
    let styles = list.styles();
    let colors = styles.colors();

    paint::rect(
        list,
        rect,
        if enabled && rect.contains(input.mouse) {
            colors.SUNKEN
        } else {
            colors.CHROME
        },
    );
    ui::text(
        list,
        atlas,
        rect.shrink(3.),
        label,
        if enabled {
            colors.TEXT
        } else {
            colors.TEXT_MUTED.with_alpha(0.4)
        },
        Align::Center,
    );
    enabled && pressed(input, rect)
}
impl Controls {
    pub(in crate::panels) fn mini_owns_wheel(&self, input: &Input) -> bool {
        self.mini.owns_wheel(input)
    }
    pub(in crate::panels) fn mini_context(&self) -> Option<(motion_core::KeyframeTarget, i64)> {
        let (object, property, axis) = self.key_context?;
        if self.property != Some(property) || self.axis != axis { return None; }
        Some((motion_core::KeyframeTarget::Property { object, property, axis }, self.selected_key?))
    }
    #[cfg(test)]
    pub(crate) fn clipboard_test_rects(&self) -> Option<[Rect; 2]> { self.clipboard_hits }
    #[cfg(test)]
    pub(in crate::panels) fn mini_test_geometry(&self) -> Option<(Rect, Rect, super::graph_ranges::Bounds)> {
        self.mini.geometry
    }
    #[cfg(test)]
    pub(in crate::panels) fn mini_test_readout(&self) -> Option<&(String, Vec<String>)> {
        self.mini.last_readout.as_ref()
    }
    pub(crate) fn choose_track(&mut self, object: u64, property: Property, axis: Option<Axis>) {
        self.selection_intent = true;
        self.track_explicit = true;
        self.track_scope = None;
        self.display_track_inner(object, property, axis);
    }
    fn display_track_inner(&mut self, object: u64, property: Property, axis: Option<Axis>) {
        let context = (object, property, axis);
        if self.key_context != Some(context) {
            self.selected_key = None;
            self.key_explicit = false;
            self.key_local = false;
        }
        self.key_context = Some(context);
        self.property = Some(property);
        self.axis = axis;
        self.expanded = true;
    }
    pub(crate) fn select_key(
        &mut self,
        object: u64,
        property: Property,
        axis: Option<Axis>,
        frame: i64,
    ) {
        self.choose_track(object, property, axis);
        self.selected_key = Some(frame);
        self.key_explicit = true;
        self.key_local = false;
    }
    pub(crate) fn user_key(&self) -> Option<KeySelection> {
        if !self.key_explicit || self.key_local {
            return None;
        }
        let (object, property, axis) = self.key_context?;
        if self.property != Some(property) || self.axis != axis {
            return None;
        }
        Some(KeySelection {
            object,
            property,
            axis,
            frame: self.selected_key?.try_into().ok()?,
        })
    }
    pub(crate) fn clear_key(&mut self) {
        self.key_local = false;
        self.selected_key = None;
        self.key_explicit = false;
    }
    pub fn cancel(&mut self) -> bool {
        self.drag_time = None;
        self.drag_key = None;
        self.dragging.take().is_some()
    }
    pub fn editing(&self) -> bool {
        self.dragging.is_some()
    }
    pub fn render(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: &mut f32,
        layer: &Value,
        sample: &Value,
        timeline: &Value,
        state: &Value,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
        ranges: Option<mini::Context<'_>>,
    ) {
        self.local_selection = None;
        #[cfg(test)]
        { self.clipboard_hits = None; }
        let cancelled = self.editing() && (input.focus_lost
            || !input.composing && !input.ime_consumed && input.key_presses().any(|(key,modifiers,repeat)|
                key==Key::Escape && !repeat && !modifiers.control && !modifiers.alt)) && self.cancel();
        if cancelled { out.push(Action::History(4)); }
        let neutral = Input::default();
        let input = if cancelled { &neutral } else { input };
        let styles = list.styles();
        let colors = styles.colors();

        let object = layer["id"].as_u64().unwrap();
        let locked = layer["locked"] == true || state["observing"] == true;
        let timing = observe!("curve.controls.clock", time::Context::read(state, object),
            "object={object} phase=clock generation={:?} revision={:?}",
            state["document_generation"].as_u64(), state["revision"].as_u64()).ok();
        if self.editing() && (timing.is_none() || self.drag_time != timing) && self.cancel() {
            out.push(Action::History(4));
            return;
        }
        let Some(timing) = timing else { return; };
        let frame = timing.composition as u32;
        let start = area.min[0] + 16.;
        let width = (area.width() - 32.).max(1.);
        if button(
            list,
            atlas,
            input,
            Rect::new(start, *y, width, 26.),
            locale.t(T::KeyframeCurves),
            true,
        ) {
            self.expanded = !self.expanded;
        }
        *y += 30.;
        if !self.expanded {
            return;
        }
        for (i, (property, label)) in [
            (Property::Position, T::Position),
            (Property::Rotation, T::Rotation),
            (Property::Scale, T::Scale),
            (Property::Opacity, T::Opacity),
        ].into_iter().enumerate() {
            if button(
                list,
                atlas,
                input,
                Rect::new(start + i as f32 * width / 4., *y, width / 4. - 2., 24.),
                locale.t(label),
                !locked && self.dragging.is_none(),
            ) {
                let axis = super::timeline::property_rows(state, object).iter()
                    .find(|row| row.property == property).and_then(|row| row.axis)
                    .map(|index| [Axis::X, Axis::Y, Axis::Z][index]);
                self.choose_track_in(state, object, property, axis);
                self.clear_key();
            }
        }
        *y += 28.;
        if self.track_selection(state, Some(object)) == TrackSelection::Invalid {
            if self.cancel() {
                out.push(Action::History(4));
            }
            ui::text(list, atlas, Rect::new(start, *y, width, styles.metric("row.height", 22.)),
                if locale == Locale::Zh { "请选择属性" } else { "Select a property" },
                colors.TEXT_MUTED, Align::Left);
            return;
        }
        let property = self.property.unwrap_or(Property::Position);
        let track = &timeline["properties"][key(property)];
        let separated = track["separated"].as_bool().unwrap_or(false);
        if separated {
            if self.axis.is_none() {
                self.axis = Some(Axis::X);
            }
            for (i, axis) in [Axis::X, Axis::Y, Axis::Z].into_iter().enumerate() {
                if button(
                    list,
                    atlas,
                    input,
                    Rect::new(start + i as f32 * width / 3., *y, width / 3. - 2., 24.),
                    ["X", "Y", "Z"][i],
                    !locked && self.dragging.is_none(),
                ) {
                    self.choose_track_in(state, object, property, Some(axis));
                    self.clear_key();
                }
            }
        } else {
            self.axis = None;
            if !matches!(property,Property::Opacity | Property::Anchor)
                && button(
                    list,
                    atlas,
                    input,
                    Rect::new(start, *y, width, 24.),
                    locale.t(T::SeparateAxes),
                    !locked,
                )
            {
                out.push(Action::Edit(Command::SeparateDimensions {
                    object,
                    property,
                }));
            }
        }
        *y += 28.;
        let track = if let Some(axis) = self.axis {
            &track["axes"][match axis {
                Axis::X => "x",
                Axis::Y => "y",
                Axis::Z => "z",
            }]
        } else {
            track
        };
        let Some(keys) = observe!("curve.controls.keys", timing.keys(track),
            "object={object} property={property:?} axis={:?} phase=keys", self.axis).ok() else {
            if self.cancel() { out.push(Action::History(4)); }
            return;
        };
        if !timing.mapped && self.key_local && self.key_explicit {
            // Retire the authored identity instead of aliasing a composition key.
            self.selected_key = None;
        }
        if timing.mapped && self.key_explicit && !self.key_local {
            self.selected_key = self.selected_key.and_then(|frame|
                keys.iter().find(|key| key.composition == frame as f64).map(|key| key.frame));
            self.key_local = true;
        }
        self.display_track(object, property, self.axis);
        let key_valid = !self.key_explicit || self.selected_key.is_some_and(|frame|
            keys.iter().any(|key| key.frame == frame));
        let retargeted = self.drag_key.is_some_and(|key|
            Some(key) != self.selected_key.map(|frame| (object, property, self.axis, frame)));
        if (locked || !key_valid || retargeted) && self.cancel() {
            out.push(Action::History(4));
            return;
        }
        let animated = !keys.is_empty();
        if button(
            list,
            atlas,
            input,
            Rect::new(start, *y, width / 2. - 2., 24.),
            if animated {
                locale.t(T::DisableAnimation)
            } else {
                locale.t(T::EnableAnimation)
            },
            !locked && (animated || !timing.mapped || timing.integer().is_some()),
        ) {
            out.push(Action::Edit(super::property_authoring::at_playhead(state, Command::Animate {
                object,
                property,
                axis: self.axis,
                frame,
                enabled: !animated,
            })));
        }
        if button(
            list,
            atlas,
            input,
            Rect::new(start + width / 2., *y, width / 2., 24.),
            locale.t(T::AddKey),
            !locked && (!timing.mapped || timing.integer().is_some()),
        ) {
            let command = if matches!(property, Property::Opacity | Property::Roll | Property::Fov | Property::Radius | Property::Azimuth | Property::Elevation) {
                Command::SetScalar {
                    object,
                    property,
                    frame,
                    value: sample[key(property)].as_f64().unwrap_or(if property == Property::Opacity {1.} else {0.}) as f32,
                }
            } else if let Some(axis) = self.axis {
                Command::SetComponent {
                    object,
                    property,
                    axis,
                    frame,
                    value: sample[key(property)][match axis {
                        Axis::X => 0,
                        Axis::Y => 1,
                        Axis::Z => 2,
                    }]
                    .as_f64()
                    .unwrap_or(0.) as f32,
                }
            } else {
                Command::SetVector {
                    object,
                    property,
                    frame,
                    value: std::array::from_fn(|i| {
                        sample[key(property)][i].as_f64().unwrap_or(0.) as f32
                    }),
                }
            };
            out.push(Action::EditBatch(vec![
                super::property_authoring::at_playhead(state, Command::Animate {
                    object,
                    property,
                    axis: self.axis,
                    frame,
                    enabled: true,
                }),
                super::property_authoring::at_playhead(state, command),
            ]));
            self.select_display_key(state, object, property, self.axis,
                if timing.mapped { i64::from(timing.integer().expect("enabled exact key")) } else { frame as i64 }, timing.mapped);
        }
        *y += 30.;
        let selected = self
            .selected_key
            .and_then(|f| keys.iter().position(|key| key.frame == f))
            .unwrap_or_else(|| {
                keys.iter()
                    .rposition(|key| key.frame as f64 <= timing.local)
                    .unwrap_or(0)
            });
        if keys.is_empty() {
            ui::text(
                list,
                atlas,
                Rect::new(start, *y, width, 26.),
                locale.t(T::NoKeys),
                colors.TEXT_MUTED,
                Align::Left,
            );
            *y += 30.;
            return;
        }
        let keyframe = keys[selected].value;
        let selected_frame = keys[selected].frame;
        if !self.key_explicit {
            self.selected_key = Some(selected_frame);
            self.key_local = timing.mapped;
        }
        let previous = (selected.saturating_sub(1), selected > 0);
        let next = ((selected + 1).min(keys.len() - 1), selected + 1 < keys.len());
        let (previous, next) = if timing.mapped && timing.clock.direction() == motion_core::layer_time::TimeDirection::Reverse {
            (next, previous)
        } else { (previous, next) };
        for (i, (label, index, enabled)) in [
            ("‹", previous.0, previous.1), ("›", next.0, next.1),
        ]
        .into_iter()
        .enumerate()
        {
            if button(
                list,
                atlas,
                input,
                Rect::new(start + i as f32 * 32., *y, 28., 24.),
                label,
                enabled && self.dragging.is_none(),
            ) {
                let next = &keys[index];
                self.select_display_key(state, object, property, self.axis, next.frame, timing.mapped);
                if next.composition >= 0. && state["project"]["frames"].as_f64()
                    .is_none_or(|frames| next.composition < frames) {
                    out.push(Action::Seek(next.composition));
                }
            }
        }
        ui::text(
            list,
            atlas,
            Rect::new(start + 70., *y, width - 70., 24.),
            &format!("{}: {}", locale.t(T::Keyframe),
                if timing.mapped { keys[selected].composition.to_string() } else { selected_frame.to_string() }),
            colors.ACCENT,
            Align::Left,
        );
        *y += 28.;
        if button(
            list,
            atlas,
            input,
            Rect::new(start, *y, width / 2. - 2., 24.),
            locale.t(T::KeyToPlayhead),
            !locked && (timing.mapped || selected_frame >= 0) && (!timing.mapped || timing.integer().is_some()) && self.dragging.is_none() && key_valid,
        ) {
            let destination = if timing.mapped { i64::from(timing.integer().expect("enabled exact key")) } else { frame as i64 };
            if let Some(delta) = destination.checked_sub(selected_frame).and_then(|d| i32::try_from(d).ok()) {
                out.push(Action::Edit(if timing.mapped {
                    Command::LocalKeyframes { target: motion_core::KeyframeTarget::Property { object, property, axis: self.axis },
                        edit: motion_core::LocalKeyEdit::Translate { frames: vec![selected_frame as i32], delta_frame: delta, copy: false, value_offset: None } }
                } else { Command::MoveKey { object, property, axis: self.axis, from: selected_frame as u32, to: frame } }));
                self.select_display_key(state, object, property, self.axis, destination, timing.mapped);
            } else {
                let _ = observe!("curve.controls.move_delta", Err::<(),_>("local move exceeds signed frame range"),
                    "object={object} property={property:?} from={selected_frame} to={destination}");
            }
        }
        if button(
            list,
            atlas,
            input,
            Rect::new(start + width / 2., *y, width / 2., 24.),
            locale.t(T::DeleteKey),
            !locked && (timing.mapped || selected_frame >= 0) && self.dragging.is_none() && key_valid,
        ) {
            out.push(Action::Edit(if timing.mapped {
                Command::LocalKeyframes { target: motion_core::KeyframeTarget::Property { object, property, axis: self.axis },
                    edit: motion_core::LocalKeyEdit::Delete { frames: vec![selected_frame as i32] } }
            } else { Command::DeleteKey { object, property, axis: self.axis, frame: selected_frame as u32 } }));
            self.clear_key();
            if timing.mapped { self.local_selection = Some(None); }
        }
        *y += 30.;
        let mut easing = super::graph_handles::observed_easing(
            keyframe,
            object,
            property,
            selected_frame as f64,
        );
        let editable = !locked && (timing.mapped || selected_frame >= 0) && selected + 1 < keys.len() && key_valid;
        let choices = [
            (locale.t(T::LinearCurve), Easing::default()),
            (
                locale.t(T::QuadraticCurve),
                Easing {
                    ease: Ease::Linear,
                    curve: Some(Curve {
                        space: CurveSpace::Progress,
                        shape: CurveShape::Quadratic {
                            control: [0.5, 0.0],
                            start: 0.,
                            end: 1.,
                        },
                    }),
                },
            ),
            (
                locale.t(T::BezierCurve),
                Easing {
                    ease: Ease::Linear,
                    curve: Some(Curve {
                        space: CurveSpace::Progress,
                        shape: CurveShape::Cubic {
                            control1: [0.25, 0.1],
                            control2: [0.75, 0.9],
                            start: 0.,
                            end: 1.,
                        },
                    }),
                },
            ),
            (
                locale.t(T::ElasticCurve),
                Easing {
                    ease: Ease::Linear,
                    curve: Some(Curve {
                        space: CurveSpace::Progress,
                        shape: CurveShape::Elastic {
                            oscillations: 2.,
                            damping: 6.,
                        },
                    }),
                },
            ),
        ];
        let mut changed = false;
        for (i, (name, value)) in choices.into_iter().enumerate() {
            if button(
                list,
                atlas,
                input,
                Rect::new(start + i as f32 * width / 4., *y, width / 4. - 2., 24.),
                name,
                editable && self.dragging.is_none(),
            ) {
                easing = value;
                changed = true;
            }
        }
        *y += 28.;
        let velocity = easing
            .curve
            .is_some_and(|c| c.space == CurveSpace::Velocity);
        if button(
            list,
            atlas,
            input,
            Rect::new(start, *y, width / 2. - 2., 24.),
            if velocity {
                locale.t(T::VelocityCurve)
            } else {
                locale.t(T::ProgressCurve)
            },
            editable && easing.curve.is_some() && self.dragging.is_none(),
        ) {
            if let Some(curve) = &mut easing.curve {
                curve.space = if velocity {
                    CurveSpace::Progress
                } else {
                    CurveSpace::Velocity
                };
                changed = true;
            }
        }
        let copy_rect = Rect::new(start + width / 2., *y,
            width / 4. - styles.metric("graph.control.gap", 2.), styles.metric("graph.control.height", 24.));
        let paste_rect = Rect::new(start + width * 0.75, *y, width / 4., styles.metric("graph.control.height", 24.));
        #[cfg(test)]
        { self.clipboard_hits = Some([copy_rect, paste_rect]); }
        let clipboard_frame = if timing.mapped { None } else { u32::try_from(selected_frame).ok() };
        let clipboard_local = timing.mapped.then_some(selected_frame).and_then(|frame| i32::try_from(frame).ok());
        let clipboard_selections = || {
            let target = motion_core::KeyframeTarget::Property { object, property, axis: self.axis };
            let selections = clipboard_frame.map(|frame| motion_core::KeyframeSelection { target: target.clone(), frames: vec![frame] }).into_iter().collect();
            let local_selections = clipboard_local.map(|frame| motion_core::LocalKeyframeSelection { target, frames: vec![frame] }).into_iter().collect();
            (selections, local_selections)
        };
        if button(
            list,
            atlas,
            input,
            copy_rect,
            locale.t(T::CopyCurve),
            (clipboard_frame.is_some() || clipboard_local.is_some()) && selected + 1 < keys.len() && key_valid
                && !self.clipboard_busy && self.dragging.is_none() && !input.focus_lost && !input.composing && !input.ime_consumed,
        ) {
            out.push(Action::CurveClipboard {
                intent: crate::layer_clipboard::controller::Intent::Copy,
                source: crate::layer_clipboard::KeyframeSource::of(state),
                selections: clipboard_selections().0,
                local_selections: clipboard_selections().1,
            });
        }
        if button(
            list,
            atlas,
            input,
            paste_rect,
            locale.t(T::PasteCurve),
            editable && (clipboard_frame.is_some() || clipboard_local.is_some()) && !self.clipboard_busy && self.dragging.is_none() && !input.focus_lost && !input.composing && !input.ime_consumed,
        ) {
            out.push(Action::CurveClipboard {
                intent: crate::layer_clipboard::controller::Intent::Paste { in_place: true },
                source: crate::layer_clipboard::KeyframeSource::of(state),
                selections: clipboard_selections().0,
                local_selections: clipboard_selections().1,
            });
        }
        *y += 30.;
        *y += super::graph_notice::draw(list, atlas,
            Rect::new(start, *y, width, styles.metric("row.height", 22.)), locale,
            super::graph_notice::has_negative(track));
        if let Some((_, value, _)) = self.dragging {
            easing = value;
        }
        let legacy = self.user_key();
        let (graph, bounds) = self.mini.draw(list,atlas,
            Rect::new(start,*y,width,styles.metric("graph.mini.height",130.)),
            state,input,locale,easing,
            motion_core::KeyframeTarget::Property {object,property,axis:self.axis},
            selected_frame,keys.get(selected+1).map(|key|key.frame),
            legacy,self.dragging.map(|(_,_,bounds)|bounds),ranges);
        let point = |p: [f64; 2]| {
            [
                graph.min[0] + bounds.x.fraction(p[0]) as f32 * graph.width(),
                graph.max[1] - bounds.y.fraction(p[1]) as f32 * graph.height(),
            ]
        };
        let mut handles = list.child();
        if let Some(curve) = easing.curve {
            let controls = match curve.shape {
                CurveShape::Cubic {
                    control1, control2, ..
                } => vec![control1, control2],
                CurveShape::Quadratic { control, .. } => vec![control],
                _ => vec![],
            };
            for (i, p) in controls.into_iter().enumerate() {
                let p = point(p);
                if !graph.contains(p) { continue; }
                paint::diamond(&mut handles, p, styles.metric("graph.key_radius",5.), colors.KEYFRAME);
                let hit = styles.metric("graph.mini.handle.hit_radius",10.);
                if editable && pressed(input, Rect::new(p[0]-hit,p[1]-hit,hit*2.,hit*2.)) {
                    self.select_display_key(state, object, property, self.axis, selected_frame, timing.mapped);
                    self.dragging = Some((i, easing, bounds));
                    self.drag_time = Some(timing);
                    self.drag_key = Some((object, property, self.axis, selected_frame));
                    out.push(Action::History(2));
                }
            }
        }
        handles.clip_to(graph);
        list.append(handles);
        let ended = self.dragging.is_some() && input.released.is_some();
        if let Some((index, mut current, bounds)) = self.dragging {
            if input.dragging() || input.released.is_some() {
                let p = [
                    bounds.x.at(f64::from((input.mouse[0]-graph.min[0])/graph.width())).clamp(0.,1.),
                    bounds.y.at(f64::from((graph.max[1]-input.mouse[1])/graph.height())).clamp(-8.,8.),
                ];
                if let Some(curve) = &mut current.curve {
                    match &mut curve.shape {
                        CurveShape::Cubic {
                            control1, control2, ..
                        } => {
                            if index == 0 {
                                *control1 = p
                            } else {
                                *control2 = p
                            }
                        }
                        CurveShape::Quadratic { control, .. } => *control = p,
                        _ => {}
                    }
                }
                if observe!("curve.controls.handle_value", current.validate(),
                    "object={object} property={property:?} axis={:?} local={selected_frame} phase=handle",self.axis).is_ok() {
                    easing = current;
                    self.dragging = Some((index, current, bounds));
                    changed = true;
                }
                if input.released.is_some() {
                    self.dragging = None;
                    self.drag_time = None;
                    self.drag_key = None;
                }
            }
        }
        if changed && editable && observe!("curve.controls.easing", easing.validate(),
            "object={object} property={property:?} axis={:?} local={selected_frame} phase=commit",self.axis).is_ok() {
            self.select_display_key(state, object, property, self.axis, selected_frame, timing.mapped);
            out.push(Action::Edit(timing.curve(
                object, property, self.axis, selected_frame, easing)));
        }
        if ended {
            out.push(Action::History(3));
        }
        *y += styles.metric("row.height",22.)+graph.height()+styles.metric("padding.panel",8.);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editing_a_separated_interval_emits_only_the_captured_axis_and_composition_time() {
        let state = serde_json::json!({"frame":21});
        let layer = serde_json::json!({"id":7,"locked":false});
        let sample = serde_json::json!({"position":[0,0,0]});
        let timeline = serde_json::json!({"properties":{"position":{"separated":true,"axes":{"x":{"keys":[{"frame":0,"ease":"linear"},{"frame":60,"ease":"linear"}]},"y":{"keys":[{"frame":15},{"frame":90}]},"z":{"keys":[]}}}}});
        let mut controls = Controls {
            expanded: true,
            axis: Some(Axis::X),
            ..Default::default()
        };
        let input = Input {
            pressed: Some((MouseButton::Left, [235., 182.])),
            ..Default::default()
        };
        let mut atlas = TextAtlas::from_system(18.).unwrap();
        let mut list = PaintList::default();
        let mut out = vec![];
        let mut y = 0.;
        controls.render(
            &mut list,
            &mut atlas,
            Rect::new(0., 0., 360., 600.),
            &mut y,
            &layer,
            &sample,
            &timeline,
            &state,
            &input,
            Locale::En,
            &mut out,
            None,
        );
        let command = out
            .into_iter()
            .find_map(|a| {
                if let Action::Edit(c) = a {
                    Some(c)
                } else {
                    None
                }
            })
            .unwrap();
        assert!(matches!(
            command,
            Command::Curve {
                object: 7,
                property: Property::Position,
                axis: Some(Axis::X),
                frame: 0,
                ..
            }
        ));
    }
}
