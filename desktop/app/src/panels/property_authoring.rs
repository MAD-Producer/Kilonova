//! Adapt explicit playhead edits without changing legacy selected-key commands.
use motion_core::{Command, EffectAction, Error, KeyframeTarget, Property, TrackEdit, TrackValue};
use serde_json::Value;

/// A numeric draft belongs to the time and document at pointer-down. Revision
/// deliberately is not included: the draft's own preview changes it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NumericTime {
    root: String,
    composition: String,
    generation: u64,
    clock: motion_core::layer_time::LayerClock,
    offset: i32,
    playhead: f64,
}
impl NumericTime {
    pub(super) fn read(state: &Value, object: u64) -> motion_core::Result<Option<Self>> {
        if !precise(state, object) {
            return Ok(None);
        }
        let identity = |value: Option<&str>| {
            value
                .map(str::to_owned)
                .ok_or_else(|| Error::Invalid("invalid numeric authoring document identity".into()))
        };
        let root = identity(state["root"].as_str())?;
        let composition = identity(
            state["composition"]
                .as_str()
                .or_else(|| state["project"]["composition_id"].as_str()),
        )?;
        let generation = state["document_generation"].as_u64().ok_or_else(|| {
            Error::Invalid("invalid numeric authoring document generation".into())
        })?;
        let playhead = state["frame"]
            .as_f64()
            .filter(|frame| frame.is_finite())
            .ok_or_else(|| Error::Invalid("invalid numeric authoring playhead".into()))?;
        let (clock, offset) = clock(state, object)?;
        // Also reject invalid mapped time before a gesture is begun.
        clock.key_edit_time(playhead, offset)?;
        Ok(Some(Self {
            root,
            composition,
            generation,
            clock,
            offset,
            playhead,
        }))
    }
    pub(super) fn current(
        captured: &Option<Self>,
        state: &Value,
        object: u64,
    ) -> motion_core::Result<bool> {
        Ok(captured == &Self::read(state, object)?)
    }
}

pub(super) fn precise(state: &Value, object: u64) -> bool {
    state["frame"]
        .as_f64()
        .is_none_or(|frame| !frame.is_finite())
        || super::keyframe_clipboard::layer(state, object)
            .is_some_and(|layer| !layer["time_mapping"].is_null())
}

pub(super) fn clock(
    state: &Value,
    object: u64,
) -> motion_core::Result<(motion_core::layer_time::LayerClock, i32)> {
    let raw = if let Some(layer) = super::keyframe_clipboard::layer(state, object) {
        layer
            .get("timeline")
            .and_then(|clip| clip.get("offset_frame"))
    } else {
        super::animation_data::timeline(state, object).and_then(|row| row.get("offset_frame"))
    };
    let offset = match raw {
        None | Some(Value::Null) => 0,
        Some(raw) => raw
            .as_i64()
            .and_then(|value| i32::try_from(value).ok())
            .ok_or_else(|| Error::Invalid("invalid property timeline offset".into()))?,
    };
    // Partial legacy views omit the authored layer and cannot carry a clock.
    let clock = if super::keyframe_clipboard::layer(state, object).is_none() {
        Default::default()
    } else {
        super::key_selection::property_clock(state, object)?
    };
    Ok((clock, offset))
}

pub(super) fn local(state: &Value, object: u64, frame: f64) -> motion_core::Result<f64> {
    let (clock, offset) = clock(state, object)?;
    clock.key_edit_time(frame, offset)
}

/// Only call for commands created from the current playhead. Selected keys and
/// graph handles already carry their authored identities and must not use this.
pub(super) fn at_playhead(state: &Value, command: Command) -> Command {
    let property = |object, property, axis| KeyframeTarget::Property {
        object,
        property,
        axis,
    };
    let (target, edit) = match &command {
        Command::SetScalar {
            object,
            property: name,
            value,
            ..
        } => (
            property(*object, *name, None),
            TrackEdit::Set {
                value: TrackValue::Scalar(*value),
            },
        ),
        Command::SetComponent {
            object,
            property: name,
            axis,
            value,
            ..
        } => (
            property(*object, *name, Some(*axis)),
            TrackEdit::Set {
                value: TrackValue::Scalar(*value),
            },
        ),
        Command::SetVector {
            object,
            property: name,
            value,
            ..
        } => {
            // Preserve core's rejection of an invalid anchor third component.
            if *name == Property::Anchor && value[2] != 0. {
                return command;
            }
            let value = if *name == Property::Anchor {
                TrackValue::Vec2([value[0], value[1]])
            } else {
                TrackValue::Vec3(*value)
            };
            (property(*object, *name, None), TrackEdit::Set { value })
        }
        Command::Animate {
            object,
            property: name,
            axis,
            enabled,
            ..
        } => (
            property(*object, *name, *axis),
            TrackEdit::Animate { enabled: *enabled },
        ),
        Command::AudioLevels { object, action } => {
            let (channel, edit) = match action {
                motion_core::AudioLevelsAction::Set {
                    channel,
                    value,
                    animated: None,
                    ..
                } => (
                    *channel,
                    TrackEdit::Set {
                        value: TrackValue::Scalar(*value),
                    },
                ),
                motion_core::AudioLevelsAction::Animate {
                    channel, enabled, ..
                } => (*channel, TrackEdit::Animate { enabled: *enabled }),
                _ => return command,
            };
            (
                KeyframeTarget::Audio {
                    object: *object,
                    channel,
                },
                edit,
            )
        }
        Command::Mask { object, action } => {
            use motion_core::masks::{MaskAction, MaskProperty};
            let (mask, property, edit) = match action {
                MaskAction::Set {
                    mask,
                    property,
                    value,
                    animated: None,
                    ..
                } => {
                    let value = match (property, value.as_slice()) {
                        (MaskProperty::Opacity | MaskProperty::Expansion, [value]) => {
                            TrackValue::Scalar(*value)
                        }
                        (MaskProperty::Feather, [x, y]) => TrackValue::Vec2([*x, *y]),
                        (MaskProperty::Node(_), [x, y, a, b, c, d]) => {
                            TrackValue::Geometry([*x, *y, *a, *b, *c, *d])
                        }
                        _ => return command,
                    };
                    (*mask, property.clone(), TrackEdit::Set { value })
                }
                MaskAction::Animate {
                    mask,
                    property,
                    enabled,
                    ..
                } => (
                    *mask,
                    property.clone(),
                    TrackEdit::Animate { enabled: *enabled },
                ),
                _ => return command,
            };
            (
                KeyframeTarget::Mask {
                    object: *object,
                    mask,
                    property,
                },
                edit,
            )
        }
        Command::Vector {
            object,
            action:
                motion_core::vector::VectorAction::SetGroupParameter {
                    item,
                    parameter,
                    value,
                    animated,
                    ..
                },
        } => {
            use motion_core::vector::groups::ParameterValue;
            let edit = match animated {
                Some(enabled) => TrackEdit::Animate { enabled: *enabled },
                None => TrackEdit::Set {
                    value: match value {
                        ParameterValue::Scalar(v) => TrackValue::Scalar(*v),
                        ParameterValue::Vector(v) => TrackValue::Vec2(*v),
                        ParameterValue::Color(v) => TrackValue::Vec4(*v),
                        ParameterValue::Geometry(v) => TrackValue::Geometry(*v),
                    },
                },
            };
            (
                KeyframeTarget::Vector {
                    object: *object,
                    item: Some(*item),
                    parameter: parameter.clone(),
                },
                edit,
            )
        }
        Command::Effect { object, action } => {
            let (effect, param, edit) = match action {
                EffectAction::Set {
                    effect,
                    param,
                    value,
                    ..
                } => (
                    *effect,
                    param,
                    TrackEdit::Set {
                        value: TrackValue::Vec4(*value),
                    },
                ),
                EffectAction::SetCurveObject {
                    effect,
                    param,
                    value,
                    ..
                } => (
                    *effect,
                    param,
                    TrackEdit::Set {
                        value: TrackValue::ColorCurve(Box::new(value.clone())),
                    },
                ),
                EffectAction::Animate {
                    effect,
                    param,
                    enabled,
                    ..
                } => (*effect, param, TrackEdit::Animate { enabled: *enabled }),
                _ => return command,
            };
            (
                KeyframeTarget::Effect {
                    object: *object,
                    effect,
                    param: param.clone(),
                },
                edit,
            )
        }
        _ => return command,
    };
    if !precise(state, target.object()) {
        return command;
    }
    // An invalid playhead remains invalid for the core/serialization error path;
    // never substitute a healthy-looking rounded frame.
    Command::TrackAt {
        target,
        frame: state["frame"].as_f64().unwrap_or(f64::NAN),
        edit,
    }
}
