//! Edit existing key metadata through the same typed channel addresses as transfer.
use super::*;

pub(super) fn failure(
    operation: &'static str,
    selection: &KeyframeSelection,
    source: Error,
) -> Error {
    let target = &selection.target;
    Error::KeyframeEdit {
        operation,
        object: target.object(),
        channel: format!(
            "{} frames={:?} count={}",
            format!("{target:?}").chars().take(192).collect::<String>(),
            selection.frames.iter().take(8).collect::<Vec<_>>(),
            selection.frames.len()
        ),
        source: Box::new(source),
    }
}

/// All addresses and frames are checked before modifying any key. Values,
/// authored times, spatial tangents and unselected channels are preserved.
pub fn set_interpolation(
    project: &mut Project,
    selections: &[KeyframeSelection],
    ease: crate::Ease,
) -> Result<()> {
    let local_frames = preflight(project, selections, "interpolation")?;
    for (selected, frames) in selections.iter().zip(local_frames) {
        let result = (|| -> Result<()> {
            macro_rules! edit {
                ($track:expr) => {
                    for frame in &frames {
                        $track.set_ease(*frame, ease)?;
                    }
                };
            }
            match write(project, &selected.target)? {
                WriteChannel::Scalar(t) => edit!(t),
                WriteChannel::Vec2(t) => edit!(t),
                WriteChannel::Vec3(t) => edit!(t),
                WriteChannel::Vec4(t) => edit!(t),
                WriteChannel::Geometry(t) => edit!(t),
                WriteChannel::ColorCurve(t) => {
                    for frame in frames {
                        let index = t
                            .keys
                            .binary_search_by_key(&frame, |key| key.frame)
                            .map_err(|_| {
                                Error::Invalid("keyframe not found after preflight".into())
                            })?;
                        t.keys[index].ease = ease;
                        t.keys[index].curve = None;
                    }
                }
            }
            Ok(())
        })();
        result.map_err(|cause| failure("interpolation", selected, cause))?;
    }
    Ok(())
}

/// Validate the complete selection before callers mutate any channel. Reuse
/// identical alias, lock, separated-axis, offset and metadata rules for edits.
pub(super) fn preflight(
    project: &Project,
    selections: &[KeyframeSelection],
    operation: &'static str,
) -> Result<Vec<Vec<i32>>> {
    ensure(!selections.is_empty(), "no keyframes selected")?;
    ensure(
        crate::serialized_bytes(&selections) <= MAX_KEYFRAME_CLIPBOARD_BYTES,
        "keyframe edit selection exceeds metadata limit",
    )?;
    let mut seen = BTreeSet::new();
    let mut local_frames = Vec::with_capacity(selections.len());
    for selected in selections {
        let local = (|| -> Result<Vec<i32>> {
            selected.target.editable(project)?;
            selected.target.signature(project)?;
            ensure(
                seen.insert(selected.target.address()?),
                "duplicate keyframe channel",
            )?;
            ensure(
                !selected.frames.is_empty(),
                "no keyframes selected in channel",
            )?;
            ensure(
                selected.frames.windows(2).all(|f| f[0] < f[1]),
                "keyframe frames must be sorted and unique",
            )?;
            let frames = selected
                .frames
                .iter()
                .map(|&f| project.edit_frame(selected.target.object(), f))
                .collect::<Result<Vec<_>>>()?;
            macro_rules! check {
                ($track:expr) => {{
                    ensure(
                        $track.axes.is_none(),
                        "choose a separated keyframe axis explicitly",
                    )?;
                    check_keys(&$track.keys, &frames)?;
                }};
            }
            match read(project, &selected.target)? {
                ReadChannel::Scalar(t) => check!(t),
                ReadChannel::Vec2(t) => check!(t),
                ReadChannel::Vec3(t) => check!(t),
                ReadChannel::Vec4(t) => check!(t),
                ReadChannel::Geometry(t) => check!(t),
                ReadChannel::ColorCurve(t) => check_keys(&t.keys, &frames)?,
            }
            Ok(frames)
        })()
        .map_err(|cause| failure(operation, selected, cause))?;
        local_frames.push(local);
    }
    Ok(local_frames)
}

pub(super) fn check_keys<T>(keys: &[Keyframe<T>], frames: &[i32]) -> Result<()> {
    ensure(
        frames
            .iter()
            .all(|f| keys.binary_search_by_key(f, |key| key.frame).is_ok()),
        "selected keyframe does not exist",
    )
}
