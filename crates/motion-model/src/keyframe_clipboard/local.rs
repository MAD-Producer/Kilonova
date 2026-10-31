//! Authored-frame addressing for keys whose composition projection is fractional.
//! Frames and deltas are signed layer-local integers, never composition rounding.
//! Continuous spatial keys retain their separate stable-ID protocol.
use super::*;
use crate::{CurveTrack, Ease, Easing};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalValueOffset {
    pub component: usize,
    pub delta: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LocalKeyEdit {
    Translate {
        frames: Vec<i32>,
        delta_frame: i32,
        copy: bool,
        value_offset: Option<LocalValueOffset>,
    },
    Interpolation {
        frames: Vec<i32>,
        ease: Ease,
    },
    /// Each selected local key starts an authored interval.
    Curve {
        frames: Vec<i32>,
        easing: Easing,
    },
    /// Sides refer to increasing LOCAL time; reversed views must swap UI sides.
    Ease {
        frames: Vec<i32>,
        incoming: bool,
        outgoing: bool,
    },
    Automatic {
        frames: Vec<i32>,
        enabled: bool,
    },
    Delete {
        frames: Vec<i32>,
    },
}
impl LocalKeyEdit {
    fn frames(&self) -> &[i32] {
        match self {
            Self::Translate { frames, .. }
            | Self::Interpolation { frames, .. }
            | Self::Curve { frames, .. }
            | Self::Ease { frames, .. }
            | Self::Automatic { frames, .. }
            | Self::Delete { frames } => frames,
        }
    }
    fn operation(&self) -> &'static str {
        match self {
            Self::Translate { .. } => "local_translate",
            Self::Interpolation { .. } => "local_interpolation",
            Self::Curve { .. } => "local_curve",
            Self::Ease { .. } => "local_ease",
            Self::Automatic { .. } => "local_automatic",
            Self::Delete { .. } => "local_delete",
        }
    }
}

enum Plan {
    Scalar(Track<f32>),
    Vec2(Track<[f32; 2]>),
    Vec3(Track<[f32; 3]>),
    Vec4(Track<[f32; 4]>),
    Geometry(Track<[f32; 6]>),
    ColorCurve(CurveTrack),
}

/// Build and validate one candidate channel before installing it. Call through
/// the core transaction for final project-domain checks and multi-channel undo.
pub fn edit(project: &mut Project, target: &KeyframeTarget, edit: &LocalKeyEdit) -> Result<()> {
    let result = (|| {
        target.editable(project)?;
        target.signature(project)?;
        target.address()?;
        ensure(
            crate::serialized_bytes(edit) <= MAX_KEYFRAME_CLIPBOARD_BYTES,
            "local keyframe edit exceeds metadata limit",
        )?;
        let frames = edit.frames();
        ensure(
            !frames.is_empty() && frames.len() <= crate::MAX_FRAMES as usize,
            "local keyframe selection is empty or exceeds limit",
        )?;
        ensure(
            frames.windows(2).all(|pair| pair[0] < pair[1]),
            "local keyframe frames must be sorted and unique",
        )?;
        if let LocalKeyEdit::Ease {
            incoming, outgoing, ..
        } = edit
        {
            ensure(
                *incoming || *outgoing,
                "choose at least one keyframe easing side",
            )?;
        }
        if let KeyframeTarget::Effect {
            object,
            effect,
            param,
        } = target
        {
            let parameter = project
                .layers
                .iter()
                .find(|l| l.id == *object)
                .and_then(|l| l.effects.iter().find(|e| e.id == *effect))
                .and_then(|e| e.params.get(param))
                .ok_or_else(|| Error::Invalid("local effect parameter missing".into()))?;
            if parameter.kind.discrete() {
                ensure(
                    matches!(
                        edit,
                        LocalKeyEdit::Translate { .. }
                            | LocalKeyEdit::Delete { .. }
                            | LocalKeyEdit::Interpolation {
                                ease: Ease::Hold,
                                ..
                            }
                    ),
                    "boolean and enum parameters support hold interpolation only",
                )?;
            }
        }
        let layer = if target.object() == 0 {
            None
        } else {
            Some(
                project
                    .layers
                    .iter()
                    .find(|layer| layer.id == target.object())
                    .ok_or(Error::Missing(target.object()))?,
            )
        };
        let destination = if let LocalKeyEdit::Translate { delta_frame, .. } = edit {
            frames
                .iter()
                .map(|frame| {
                    let local = frame.checked_add(*delta_frame).ok_or_else(|| {
                        Error::Invalid("local keyframe destination overflow".into())
                    })?;
                    let composition = layer.map_or(Ok(f64::from(local)), |layer| {
                        layer.property_composition_frame(f64::from(local))
                    })?;
                    ensure(
                        (0. ..f64::from(project.frames)).contains(&composition),
                        "keyframe destination outside composition",
                    )?;
                    Ok(local)
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            Vec::new()
        };
        let value = if let LocalKeyEdit::Translate {
            value_offset: Some(value),
            ..
        } = edit
        {
            ensure(
                value.delta.is_finite(),
                "keyframe value offset must be finite",
            )?;
            Some(KeyframeValueOffset {
                target: target.clone(),
                component: value.component,
                delta: value.delta,
            })
        } else {
            None
        };
        macro_rules! plan {
            ($track:expr, $variant:ident) => {
                Plan::$variant(edit_track($track, edit, &destination, value.as_ref())?)
            };
        }
        let plan = match read(project, target)? {
            ReadChannel::Scalar(t) => plan!(t, Scalar),
            ReadChannel::Vec2(t) => plan!(t, Vec2),
            ReadChannel::Vec3(t) => plan!(t, Vec3),
            ReadChannel::Vec4(t) => plan!(t, Vec4),
            ReadChannel::Geometry(t) => {
                ensure(
                    !matches!(edit, LocalKeyEdit::Automatic { .. }),
                    "path geometry does not support automatic temporal Bezier",
                )?;
                plan!(t, Geometry)
            }
            ReadChannel::ColorCurve(t) => {
                Plan::ColorCurve(edit_color(t, edit, &destination, value.as_ref())?)
            }
        };
        match (write(project, target)?, plan) {
            (WriteChannel::Scalar(t), Plan::Scalar(next)) => *t = next,
            (WriteChannel::Vec2(t), Plan::Vec2(next)) => *t = next,
            (WriteChannel::Vec3(t), Plan::Vec3(next)) => *t = next,
            (WriteChannel::Vec4(t), Plan::Vec4(next)) => *t = next,
            (WriteChannel::Geometry(t), Plan::Geometry(next)) => *t = next,
            (WriteChannel::ColorCurve(t), Plan::ColorCurve(next)) => *t = next,
            _ => {
                return Err(Error::Invalid(
                    "local keyframe channel changed after preflight".into(),
                ))
            }
        }
        Ok(())
    })();
    result.map_err(|source| target.failure(edit.operation(), source))
}

fn translate<T: transform::NumericOffset>(
    keys: &mut Vec<Keyframe<T>>,
    edit: &LocalKeyEdit,
    destination: &[i32],
    value: Option<&KeyframeValueOffset>,
) -> Result<()> {
    let LocalKeyEdit::Translate { frames, copy, .. } = edit else {
        return Err(Error::Invalid("local translate operation missing".into()));
    };
    let moved = transform::plan(keys, frames, destination, *copy, value)?;
    if !copy {
        keys.retain(|k| frames.binary_search(&k.frame).is_err());
    }
    keys.extend(moved);
    keys.sort_by_key(|k| k.frame);
    Ok(())
}

fn edit_track<T: Tween + transform::NumericOffset>(
    track: &Track<T>,
    edit: &LocalKeyEdit,
    destination: &[i32],
    value: Option<&KeyframeValueOffset>,
) -> Result<Track<T>> {
    ensure(
        track.axes.is_none(),
        "choose a separated keyframe axis explicitly",
    )?;
    ensure(
        track.spatial_animation.is_none(),
        "continuous spatial keys require their stable-ID protocol",
    )?;
    interpolation::check_keys(&track.keys, edit.frames())?;
    let mut next = track.clone();
    match edit {
        LocalKeyEdit::Translate { .. } => translate(&mut next.keys, edit, destination, value)?,
        LocalKeyEdit::Interpolation { frames, ease } => {
            for frame in frames {
                next.set_ease(*frame, *ease)?;
            }
        }
        LocalKeyEdit::Curve { frames, easing } => {
            for frame in frames {
                next.set_curve(*frame, *easing)?;
            }
        }
        LocalKeyEdit::Ease {
            frames,
            incoming,
            outgoing,
        } => {
            let intervals =
                easing::intervals(&track.keys, frames, *incoming, *outgoing, |index| {
                    track.effective_easing(index)
                })?;
            for (frame, easing, start, end) in intervals {
                next.set_curve_sides(frame, easing, start, end)?;
            }
        }
        LocalKeyEdit::Automatic { frames, enabled } => {
            for frame in frames {
                next.set_auto_bezier(*frame, *enabled)?;
            }
        }
        LocalKeyEdit::Delete { frames } => {
            for frame in frames {
                next.delete_key(*frame)?;
            }
        }
    }
    next.validate_local()?;
    Ok(next)
}

fn edit_color(
    track: &CurveTrack,
    edit: &LocalKeyEdit,
    destination: &[i32],
    value: Option<&KeyframeValueOffset>,
) -> Result<CurveTrack> {
    interpolation::check_keys(&track.keys, edit.frames())?;
    let mut next = track.clone();
    match edit {
        LocalKeyEdit::Translate { .. } => translate(&mut next.keys, edit, destination, value)?,
        LocalKeyEdit::Interpolation { frames, ease } => {
            for key in &mut next.keys {
                if frames.binary_search(&key.frame).is_ok() {
                    key.ease = *ease;
                    key.curve = None;
                }
            }
        }
        LocalKeyEdit::Curve { frames, easing } => {
            easing.validate()?;
            for frame in frames {
                let index = next
                    .keys
                    .binary_search_by_key(frame, |k| k.frame)
                    .map_err(|_| Error::Invalid("local color curve key missing".into()))?;
                ensure(
                    index + 1 < next.keys.len(),
                    "curve needs two adjacent keyframes",
                )?;
                next.keys[index].ease = easing.ease;
                next.keys[index].curve = easing.curve;
            }
        }
        LocalKeyEdit::Ease {
            frames,
            incoming,
            outgoing,
        } => {
            for (frame, easing, _, _) in
                easing::intervals(&track.keys, frames, *incoming, *outgoing, |index| {
                    track.effective_easing(index)
                })?
            {
                let key = next
                    .keys
                    .iter_mut()
                    .find(|key| key.frame == frame)
                    .ok_or_else(|| Error::Invalid("local color curve interval missing".into()))?;
                key.ease = easing.ease;
                key.curve = easing.curve;
            }
        }
        LocalKeyEdit::Automatic { .. } => {
            return Err(Error::Invalid(
                "color transfer curves do not support automatic temporal Bezier".into(),
            ))
        }
        LocalKeyEdit::Delete { frames } => {
            // Match ordinary tracks: the final deleted key supplies static value.
            if frames.len() == next.keys.len() {
                next.value = next
                    .keys
                    .last()
                    .ok_or_else(|| Error::Invalid("color curve keys missing".into()))?
                    .value
                    .clone();
            }
            next.keys
                .retain(|key| frames.binary_search(&key.frame).is_err());
        }
    }
    next.validate(crate::MAX_FRAMES)?;
    Ok(next)
}
