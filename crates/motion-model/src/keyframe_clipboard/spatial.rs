//! Spatial keys use persistent IDs after their initial frame-key conversion.
use super::*;

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
    /// Composition-time translation; offsets cancel and identities stay stable.
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
            let selected = selected
                .iter()
                .map(|frame| {
                    ensure(*frame < frames, "Roving source frame outside composition")?;
                    i32::try_from(i64::from(*frame) - i64::from(offset))
                        .map_err(|_| Error::Invalid("Roving source local time overflow".into()))
                })
                .collect::<Result<Vec<_>>>()?;
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
                    animation.set_fixed_time(*key, *time - f64::from(offset))?
                }
                SpatialKeyEdit::Move { keys, delta } => animation.move_keys(keys, *delta)?,
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
        let offset = if *object == 0 {
            0
        } else {
            project
                .layers
                .iter()
                .find(|layer| layer.id == *object)
                .ok_or(Error::Missing(*object))?
                .timeline
                .map_or(0, |timeline| timeline.offset_frame)
        };
        // Solve and validate before borrowing the write channel. Rejections
        // preserve model callers as well as the core's transaction snapshot.
        match read(project, target)? {
            ReadChannel::Vec2(track) => {
                let next = candidate(track, edit, offset, project.frames, *object == 0, 1000.)?;
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
