//! Spatial keys use persistent IDs after their initial frame-key conversion.
use super::*;
use crate::layer_time::LayerClock;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SpatialKeyEdit {
    /// Existing keys are selected in integer composition frames exactly once.
    Enable {
        frames: Vec<u32>,
    },
    Roving {
        keys: Vec<u64>,
        enabled: bool,
    },
    /// The new time is continuous composition time, not a rounded frame index.
    Time {
        key: u64,
        time: f64,
    },
    /// Composition-time translation; the property clock scales and reverses it.
    Move {
        keys: Vec<u64>,
        delta: f64,
    },
    Delete {
        key: u64,
    },
}

fn candidate<T: Tween>(
    source: &Track<T>,
    edit: &SpatialKeyEdit,
    clock: LayerClock,
    offset: i32,
    frames: u32,
    global: bool,
    bound: f32,
) -> Result<Track<T>> {
    let mut track = source.clone();
    match edit {
        SpatialKeyEdit::Enable { frames: selected } => {
            ensure(
                !selected.is_empty() && selected.len() <= 16_385,
                "invalid Roving selection count",
            )?;
            ensure(
                selected.windows(2).all(|keys| keys[0] < keys[1]),
                "Roving source frames must be unique and sorted",
            )?;
            let mut selected = selected
                .iter()
                .map(|frame| {
                    ensure(*frame < frames, "Roving source frame outside composition")?;
                    clock.integer_key_frame(f64::from(*frame), offset)
                })
                .collect::<Result<Vec<_>>>()?;
            // A reverse clock reverses display order, never authored geometry.
            selected.sort_unstable();
            track.enable_roving(&selected)?;
        }
        _ => {
            let animation = track
                .spatial_animation
                .as_mut()
                .ok_or_else(|| Error::Invalid("spatial key storage is not enabled".into()))?;
            match edit {
                SpatialKeyEdit::Roving { keys, enabled } => animation.set_roving(keys, *enabled)?,
                SpatialKeyEdit::Time { key, time } => {
                    let mut local = clock.local_frame(*time, offset)?;
                    // Inverting a projected continuous key can lose an ULP.
                    // Recover only this explicit ID's exact projection, never
                    // a neighbouring key, epsilon match or rounded frame.
                    if let Some((_, existing)) =
                        animation.prepared().keys().find(|(id, _)| id == key)
                    {
                        if clock.projected_composition_frame(existing, offset)? == *time {
                            local = existing;
                        }
                    }
                    animation.set_fixed_time(*key, local)?
                }
                SpatialKeyEdit::Move { keys, delta } => {
                    animation.move_keys(keys, *delta * clock.rate())?
                }
                SpatialKeyEdit::Delete { key } => animation.delete_key(*key)?,
                SpatialKeyEdit::Enable { .. } => {
                    return Err(Error::Invalid(
                        "spatial conversion dispatch mismatch".into(),
                    ))
                }
            }
        }
    }
    if global {
        track.validate(frames)?;
    } else {
        track.validate_local()?;
    }
    track.validate_bound(bound)?;
    Ok(track)
}

pub fn edit(project: &mut Project, target: &KeyframeTarget, edit: &SpatialKeyEdit) -> Result<()> {
    let result = (|| -> Result<()> {
        target.editable(project)?;
        let KeyframeTarget::Property {
            object,
            property,
            axis: None,
        } = target
        else {
            return Err(Error::Invalid(
                "Roving requires an explicit coupled spatial property".into(),
            ));
        };
        ensure(
            matches!(
                property,
                Property::Position | Property::Anchor | Property::Target
            ),
            "Roving requires a position, anchor or camera target",
        )?;
        let (clock, offset) = if *object == 0 {
            (LayerClock::default(), 0)
        } else {
            let layer = project
                .layers
                .iter()
                .find(|layer| layer.id == *object)
                .ok_or(Error::Missing(*object))?;
            (
                layer
                    .time_mapping
                    .as_ref()
                    .map_or_else(LayerClock::default, |mapping| mapping.clock),
                layer.timeline.map_or(0, |timeline| timeline.offset_frame),
            )
        };
        // Solve and validate before borrowing the write channel. Rejections
        // preserve model callers as well as the core's transaction snapshot.
        match read(project, target)? {
            ReadChannel::Vec2(track) => {
                let next = candidate(
                    track,
                    edit,
                    clock,
                    offset,
                    project.frames,
                    *object == 0,
                    1000.,
                )?;
                match write(project, target)? {
                    WriteChannel::Vec2(track) => *track = next,
                    _ => {
                        return Err(Error::Invalid(
                            "spatial channel changed after preparation".into(),
                        ))
                    }
                }
            }
            ReadChannel::Vec3(track) => {
                let next = candidate(
                    track,
                    edit,
                    clock,
                    offset,
                    project.frames,
                    *object == 0,
                    10_000_000.,
                )?;
                match write(project, target)? {
                    WriteChannel::Vec3(track) => *track = next,
                    _ => {
                        return Err(Error::Invalid(
                            "spatial channel changed after preparation".into(),
                        ))
                    }
                }
            }
            _ => {
                return Err(Error::Invalid(
                    "Roving requires a 2D or 3D spatial property".into(),
                ))
            }
        }
        Ok(())
    })();
    let context = match edit {
        SpatialKeyEdit::Enable { frames } => format!(
            "phase=enable frames={:?} count={}",
            frames.iter().take(8).collect::<Vec<_>>(),
            frames.len()
        ),
        SpatialKeyEdit::Roving { keys, enabled } => format!(
            "phase=roving enabled={enabled} keys={:?} count={}",
            keys.iter().take(8).collect::<Vec<_>>(),
            keys.len()
        ),
        SpatialKeyEdit::Time { key, time } => format!("phase=time key={key} time={time}"),
        SpatialKeyEdit::Move { keys, delta } => format!(
            "phase=move delta={delta} keys={:?} count={}",
            keys.iter().take(8).collect::<Vec<_>>(),
            keys.len()
        ),
        SpatialKeyEdit::Delete { key } => format!("phase=delete key={key}"),
    };
    result.map_err(|source| Error::KeyframeEdit {
        operation: "spatial_keyframes",
        object: target.object(),
        channel: format!(
            "{} {context}",
            format!("{target:?}").chars().take(192).collect::<String>()
        ),
        source: Box::new(source),
    })
}
