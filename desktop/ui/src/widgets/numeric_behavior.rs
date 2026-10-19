//! Shared scrubbing policy; preserve maximum gesture travel at low frame rates.
use crate::cursor::Cursor;

pub fn started(delta: f32, travel: f32, threshold: f32) -> bool {
    let threshold = if threshold.is_finite() {
        threshold.max(0.)
    } else {
        3.
    };
    delta.abs().max(travel) >= threshold
}
pub fn step(input: &impl crate::InputSource) -> f64 {
    let view = input.input_view();
    let input = &view;
    if input.modifiers.shift {
        0.1
    } else if input.modifiers.control {
        10.
    } else {
        1.
    }
}
pub fn cursor(input: &impl crate::InputSource, dragging: bool) -> Cursor {
    let view = input.input_view();
    let input = &view;
    if dragging {
        Cursor::Grabbing
    } else if input.modifiers.shift {
        Cursor::ScrubFine
    } else if input.modifiers.control {
        Cursor::ScrubCoarse
    } else {
        Cursor::ResizeEW
    }
}
