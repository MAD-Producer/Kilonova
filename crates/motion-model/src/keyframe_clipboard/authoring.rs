//! Exact composition-time property authoring. Integer tracks reject a fractional
//! LOCAL key time; static values and continuous spatial keys do not need rounding.
use super::*;
use crate::{layer_time::LayerClock, CurveObject, CurveTrack, Ease};
mod anchor;
pub use anchor::{compensate_anchor, compensate_anchor_with_keys, AnchorKeyBinding};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TrackValue {
    Scalar(f32),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    Geometry([f32; 6]),
    ColorCurve(Box<CurveObject>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TrackEdit {
    Set {
        value: TrackValue,
    },
    /// A gesture keeps this identity even when Roving redistributes its time.
    SetSpatialKey {
        key: u64,
        value: TrackValue,
    },
    Animate {
        enabled: bool,
    },
    ToggleKey,
}

struct EditTime {
    composition: f64,
    local: f64,
    clock: LayerClock,
    offset: i32,
}
impl EditTime {
    fn read(project: &Project, object: u64, frame: f64) -> Result<Self> {
        ensure(
            frame.is_finite() && (0. ..f64::from(project.frames)).contains(&frame),
            "edit frame outside the composition",
        )?;
        let (clock, offset) = if object == 0 {
            (LayerClock::default(), 0)
        } else {
            let layer = project
                .layers
                .iter()
                .find(|layer| layer.id == object)
                .ok_or(Error::Missing(object))?;
            (
                layer
                    .time_mapping
                    .as_ref()
                    .map_or_else(LayerClock::default, |mapping| mapping.clock),
                layer.clip(project.frames).offset_frame,
            )
        };
        Ok(Self {
            composition: frame,
            local: clock.key_edit_time(frame, offset)?,
            clock,
            offset,
        })
    }
    fn integer(&self) -> Result<i32> {
        self.clock.integer_key_frame(self.composition, self.offset)
    }
    fn spatial_local<T: Tween>(&self, source: &Track<T>, edit: &TrackEdit) -> Result<f64> {
        if !matches!(edit, TrackEdit::Set { .. } | TrackEdit::ToggleKey) {
            return Ok(self.local);
        }
        let Some(spatial) = &source.spatial_animation else {
            return Ok(self.local);
        };
        let mut exact = None;
        for (_, local) in spatial.prepared().keys() {
            if self.clock.projected_composition_frame(local, self.offset)? == self.composition {
                ensure(
                    exact.is_none(),
                    "coincident spatial keys require an explicit identity",
                )?;
                exact = Some(local);
            }
        }
        Ok(exact.unwrap_or(self.local))
    }
}

fn edit_track<T: Tween>(
    source: &Track<T>,
    time: &EditTime,
    edit: &TrackEdit,
    value: Option<T>,
    discrete: bool,
) -> Result<Track<T>> {
    let local = time.spatial_local(source, edit)?;
    let mut next = source.clone();
    match edit {
        TrackEdit::SetSpatialKey { key, .. } => {
            let value =
                value.ok_or_else(|| Error::Invalid("property value type mismatch".into()))?;
            ensure(value.finite(), "non-finite property value")?;
            next.spatial_animation
                .as_mut()
                .ok_or_else(|| Error::Invalid("spatial key storage is not enabled".into()))?
                .set_value(*key, value)?;
        }
        TrackEdit::Set { .. } => {
            let value =
                value.ok_or_else(|| Error::Invalid("property value type mismatch".into()))?;
            ensure(value.finite(), "non-finite property value")?;
            if let Some(spatial) = &mut next.spatial_animation {
                spatial.set_at_time(local, value)?;
            } else if !next.is_animated() {
                if next.axes.is_some() {
                    // Static separated channels own their scalar values. Frame
                    // zero only reaches the existing setter; it creates no keys.
                    next.set_at(0, value)?;
                } else {
                    next.value = value;
                }
            } else {
                next.set_at(time.integer()?, value)?;
            }
        }
        TrackEdit::Animate { enabled: false } => {
            let value = source.sample(local)?;
            next.value = value;
            next.keys.clear();
            next.spatial_animation = None;
        }
        TrackEdit::Animate { enabled: true } => {
            if !source.is_animated() {
                next.upsert(time.integer()?, source.sample(local)?, Ease::Linear)?;
            }
        }
        TrackEdit::ToggleKey => {
            if let Some(spatial) = &mut next.spatial_animation {
                let matches: Vec<_> = spatial
                    .prepared()
                    .keys()
                    .filter(|(_, time)| *time == local)
                    .map(|(id, _)| id)
                    .collect();
                ensure(
                    matches.len() <= 1,
                    "coincident spatial keys require an explicit identity",
                )?;
                if let Some(id) = matches.first() {
                    spatial.delete_key(*id)?;
                } else {
                    spatial.set_at_time(local, source.sample(local)?)?;
                }
            } else {
                let frame = time.integer()?;
                if next.keys.iter().any(|key| key.frame == frame) {
                    next.delete_key(frame)?;
                } else {
                    next.upsert(frame, source.sample(local)?, Ease::Linear)?;
                }
            }
        }
    }
    if discrete {
        for key in &mut next.keys {
            key.ease = Ease::Hold;
            key.curve = None;
        }
    }
    Ok(next)
}

fn frozen(source: &CurveTrack, local: f64) -> Result<CurveObject> {
    if source.keys.is_empty() {
        return Ok(source.value.clone());
    }
    let upper = source
        .keys
        .partition_point(|key| f64::from(key.frame) <= local);
    let mut value = source.keys[upper.saturating_sub(1)].value.clone();
    value.sampled_lut = Some(source.sampled_curve(local)?.high_precision_lut());
    value.channel_luts = std::array::from_fn(|_| None);
    value.validate()?;
    Ok(value)
}

fn edit_curve(source: &CurveTrack, time: &EditTime, edit: &TrackEdit) -> Result<CurveTrack> {
    let local = time.local;
    let mut next = source.clone();
    let value = match edit {
        TrackEdit::Set {
            value: TrackValue::ColorCurve(value),
        } => {
            value.validate()?;
            if next.keys.is_empty() {
                next.value = *value.clone();
                return Ok(next);
            }
            *value.clone()
        }
        TrackEdit::Set { .. } | TrackEdit::SetSpatialKey { .. } => {
            return Err(Error::Invalid("property value type mismatch".into()))
        }
        TrackEdit::Animate { enabled: false } => {
            next.value = frozen(source, local)?;
            next.keys.clear();
            return Ok(next);
        }
        TrackEdit::Animate { enabled: true } if !next.keys.is_empty() => return Ok(next),
        TrackEdit::Animate { enabled: true } | TrackEdit::ToggleKey => frozen(source, local)?,
    };
    let frame = time.integer()?;
    match next.keys.binary_search_by_key(&frame, |key| key.frame) {
        Ok(index) if matches!(edit, TrackEdit::ToggleKey) => {
            next.keys.remove(index);
        }
        Ok(index) => next.keys[index].value = value,
        Err(index) => next.keys.insert(
            index,
            Keyframe {
                frame,
                value,
                ease: Ease::Linear,
                curve: None,
                spatial: None,
                auto_bezier: false,
            },
        ),
    }
    Ok(next)
}

/// Core transactions retain project-domain validation, gesture ownership and
/// atomic rollback. Candidate construction here never modifies the live channel.
pub fn edit(
    project: &mut Project,
    target: &KeyframeTarget,
    frame: f64,
    edit: &TrackEdit,
) -> Result<()> {
    let result = (|| {
        target.editable(project)?;
        target.address()?;
        if let TrackEdit::SetSpatialKey { key, .. } = edit {
            ensure(*key > 0, "invalid spatial key identity")?;
            ensure(
                matches!(
                    target,
                    KeyframeTarget::Property {
                        property: Property::Position | Property::Anchor | Property::Target,
                        axis: None,
                        ..
                    }
                ),
                "spatial value edit requires a coupled spatial property",
            )?;
        }
        let time = EditTime::read(project, target.object(), frame)?;
        ensure(
            crate::serialized_bytes(edit) <= MAX_KEYFRAME_CLIPBOARD_BYTES,
            "property edit exceeds metadata limit",
        )?;
        let object = target.object();
        if let KeyframeTarget::Audio { channel, .. } = target {
            let layer = project
                .layers
                .iter()
                .find(|layer| layer.id == object)
                .ok_or(Error::Missing(object))?;
            ensure(
                matches!(
                    layer.content,
                    Content::Audio { .. } | Content::Video { .. } | Content::Composition { .. }
                ),
                "audio levels require an audio, video or composition layer",
            )?;
            // An absent envelope means unity, just as in AudioLevelsAction.
            // Construct it off-project so invalid edits cannot publish defaults.
            let mut next = layer.audio_levels.clone().unwrap_or_default();
            let track = match channel {
                crate::AudioChannel::Left => &mut next.left,
                crate::AudioChannel::Right => &mut next.right,
            };
            let value = if let TrackEdit::Set {
                value: TrackValue::Scalar(value),
            } = edit
            {
                Some(*value)
            } else {
                None
            };
            *track = edit_track(track, &time, edit, value, false)?;
            next.validate()?;
            project.layer_mut(object)?.audio_levels = Some(next);
            return Ok(());
        }
        // Clipboard channels require animation capability. Static-only effect
        // parameters still accept value edits through this authoring command.
        if let KeyframeTarget::Effect { effect, param, .. } = target {
            let frames = project.frames;
            let layer = project.layer_mut(object)?;
            ensure(
                !matches!(layer.content, Content::Null | Content::Camera { .. }),
                "null and camera objects do not have image effects",
            )?;
            let parameter = layer
                .effects
                .iter_mut()
                .find(|instance| instance.id == *effect)
                .and_then(|instance| instance.params.get_mut(param))
                .ok_or_else(|| Error::Invalid("effect parameter missing".into()))?;
            ensure(parameter.implemented, "effect parameter is not implemented")?;
            ensure(
                parameter.animatable || matches!(edit, TrackEdit::Set { .. }),
                "effect parameter is not animatable",
            )?;
            let mut next = parameter.clone();
            if let Some(curve) = &parameter.curve {
                next.curve = Some(edit_curve(curve, &time, edit)?);
            } else {
                let value = if let TrackEdit::Set {
                    value: TrackValue::Vec4(value),
                } = edit
                {
                    Some(*value)
                } else {
                    None
                };
                next.track = edit_track(
                    &parameter.track,
                    &time,
                    edit,
                    value,
                    parameter.kind.discrete(),
                )?;
            }
            next.validate(frames)?;
            *parameter = next;
            return Ok(());
        }
        macro_rules! apply {
            ($source:expr, $variant:ident) => {{
                let value = if let TrackEdit::Set {
                    value: TrackValue::$variant(value),
                }
                | TrackEdit::SetSpatialKey {
                    value: TrackValue::$variant(value),
                    ..
                } = edit
                {
                    Some(*value)
                } else {
                    None
                };
                edit_track($source, &time, edit, value, false)?
            }};
        }
        // Separate read and write borrows keep all fallible work in preflight.
        match read(project, target)? {
            ReadChannel::Scalar(source) => {
                let next = apply!(source, Scalar);
                match write(project, target)? {
                    WriteChannel::Scalar(dest) => *dest = next,
                    _ => {
                        return Err(Error::Invalid(
                            "property channel changed after preflight".into(),
                        ))
                    }
                }
            }
            ReadChannel::Vec2(source) => {
                let next = apply!(source, Vec2);
                match write(project, target)? {
                    WriteChannel::Vec2(dest) => *dest = next,
                    _ => {
                        return Err(Error::Invalid(
                            "property channel changed after preflight".into(),
                        ))
                    }
                }
            }
            ReadChannel::Vec3(source) => {
                let next = apply!(source, Vec3);
                match write(project, target)? {
                    WriteChannel::Vec3(dest) => *dest = next,
                    _ => {
                        return Err(Error::Invalid(
                            "property channel changed after preflight".into(),
                        ))
                    }
                }
            }
            ReadChannel::Vec4(source) => {
                let next = apply!(source, Vec4);
                match write(project, target)? {
                    WriteChannel::Vec4(dest) => *dest = next,
                    _ => {
                        return Err(Error::Invalid(
                            "property channel changed after preflight".into(),
                        ))
                    }
                }
            }
            ReadChannel::Geometry(source) => {
                let next = apply!(source, Geometry);
                match write(project, target)? {
                    WriteChannel::Geometry(dest) => *dest = next,
                    _ => {
                        return Err(Error::Invalid(
                            "property channel changed after preflight".into(),
                        ))
                    }
                }
            }
            ReadChannel::ColorCurve(_) => {
                return Err(Error::Invalid(
                    "color curve requires an effect target".into(),
                ))
            }
        }
        Ok(())
    })();
    result.map_err(|source| target.failure("track_at", source))
}
