//! Camera fields use the same property clock as their sampled FOV and pose.
use super::*;

pub(super) fn availability(
    state: &Value,
    object: u64,
) -> motion_core::Result<(Option<super::super::property_authoring::NumericTime>, bool)> {
    let time = super::super::property_authoring::NumericTime::read(state, object)?;
    let integer = if super::super::property_authoring::precise(state, object) {
        let (clock, offset) = super::super::property_authoring::clock(state, object)?;
        // Fractional local time is a normal unavailable integer-key insertion.
        clock
            .integer_key_frame(state["frame"].as_f64().unwrap_or(f64::NAN), offset)
            .is_ok()
    } else {
        super::super::key_navigation::editable_frame(
            state["frame"].as_f64().unwrap_or(f64::NAN),
            super::super::last_frame(state),
        )
        .is_some()
    };
    Ok((time, integer))
}

pub(super) fn can_set(source: &Value, integer: bool) -> bool {
    integer
        || source["spatial_animation"].is_object()
        || (!source["keys"]
            .as_array()
            .is_some_and(|keys| !keys.is_empty())
            && !["x", "y", "z"].iter().any(|axis| {
                source["axes"][axis]["keys"]
                    .as_array()
                    .is_some_and(|keys| !keys.is_empty())
            }))
}
