//! Horizontal timeline zoom; view state never enters the project history.
use super::*;
use crate::shortcuts::Command as Shortcut;

pub(super) fn zoom_at(view: &mut types::TimelineState, total: f32, next: f32, fraction: f32) {
    if !total.is_finite() || total < 1. || !next.is_finite() || !fraction.is_finite() {
        return;
    }
    let limit = total.max(100.);
    let old = if view.zoom.is_finite() {
        view.zoom.clamp(1., limit)
    } else {
        1.
    };
    let offset = if view.offset.is_finite() {
        view.offset.clamp(0., total - total / old)
    } else {
        0.
    };
    let fraction = fraction.clamp(0., 1.);
    let anchor = offset + total / old * fraction;
    view.zoom = next.clamp(1., limit);
    view.offset =
        (anchor - total / view.zoom * fraction).clamp(0., (total - total / view.zoom).max(0.));
}

pub(super) fn shortcut(
    command: Shortcut,
    input: &Input,
    panels: &mut Panels,
    state: &Value,
) -> bool {
    if !matches!(
        command,
        Shortcut::ZoomIn | Shortcut::ZoomOut | Shortcut::ToggleFrameView
    ) || panels.focused != Some(Panel::Timeline)
    {
        return false;
    }
    if panels.timeline.horizontal_drag.is_some()
        || panels.timeline.scrubbing
        || panels.timeline.reorder.is_some()
        || panels.timeline.moving.is_some()
        || panels.timeline.trimming.is_some()
    {
        return true;
    }
    let total = (last_frame(state) + 1.) as f32;
    let frame = (state["frame"].as_f64().unwrap_or(0.) as f32).clamp(0., total - 1.);
    let view = &mut panels.timeline;
    if command == Shortcut::ToggleFrameView {
        if view.zoom >= total {
            view.zoom = 1.;
            view.offset = 0.;
        } else {
            view.zoom = total;
            view.offset = frame.floor();
        }
    } else {
        let fraction = view
            .zoom_track
            .filter(|r| r.contains(input.mouse))
            .map_or_else(
                || ((frame - view.offset) * view.zoom.max(1.) / total).clamp(0., 1.),
                |r| ((input.mouse[0] - r.min[0]) / r.width()).clamp(0., 1.),
            );
        let next = view.zoom.max(1.)
            * if command == Shortcut::ZoomIn {
                1.25
            } else {
                1. / 1.25
            };
        zoom_at(view, total, next, fraction);
    }
    true
}

#[cfg(test)]
mod tests;
