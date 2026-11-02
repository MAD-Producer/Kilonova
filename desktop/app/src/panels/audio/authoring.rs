//! Audio envelopes share the exact property clock; gain and balance are global.
use super::*;
use crate::panels::property_authoring as property;
use motion_core::{Error, KeyframeTarget};

pub(super) fn numeric_time(
    state: &Value,
    object: u64,
    control: Control,
) -> motion_core::Result<Option<property::NumericTime>> {
    if control == Control::Gain {
        return Ok(None);
    }
    property::NumericTime::read(state, object)
}

pub(super) fn times(
    state: &Value,
    object: u64,
    track: &Track<f32>,
) -> motion_core::Result<Vec<f64>> {
    navigation(state, object, track).map(|(times, _)| times)
}

pub(super) fn navigation(
    state: &Value,
    object: u64,
    track: &Track<f32>,
) -> motion_core::Result<(Vec<f64>, Option<i32>)> {
    let (clock, offset) = property::clock(state, object)?;
    let at = state["frame"]
        .as_f64()
        .filter(|at| at.is_finite())
        .ok_or_else(|| Error::Invalid("invalid audio authoring playhead".into()))?;
    clock.key_edit_time(at, offset)?;
    let times = track
        .keys
        .iter()
        .map(|key| clock.projected_composition_frame(f64::from(key.frame), offset))
        .collect::<motion_core::Result<Vec<_>>>()?;
    let key = (0. ..=last_frame(state))
        .contains(&at)
        .then(|| clock.integer_key_frame(at, offset).ok())
        .flatten();
    Ok((times, key))
}

/// Fractional local time is a normal unavailable key insertion, not a fault.
pub(super) fn local_key(state: &Value, object: u64, at: f64) -> motion_core::Result<Option<i32>> {
    let (clock, offset) = property::clock(state, object)?;
    if !at.is_finite() || !(0. ..=last_frame(state)).contains(&at) {
        return Ok(None);
    }
    Ok(clock.integer_key_frame(at, offset).ok())
}

pub(super) fn select(
    panels: &mut Panels,
    state: &Value,
    object: u64,
    channel: AudioChannel,
    composition: Option<f64>,
) -> motion_core::Result<()> {
    let frame = composition
        .map(|at| {
            local_key(state, object, at)?.ok_or_else(|| {
                Error::Invalid("audio navigation key has no exact local identity".into())
            })
        })
        .transpose()?;
    panels.selected_keys.apply_addressed(
        state,
        frame.into_iter().map(|frame| {
            super::super::key_selection::LocalKey {
                target: KeyframeTarget::Audio { object, channel },
                frame,
            }
            .into()
        }),
        super::super::key_selection::Mode::Replace,
    )?;
    panels.audio_controls.selection = None;
    Ok(())
}
