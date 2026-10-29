//! Timeline view navigation only: no engine edits or history entries.
use super::*;
use std::time::Instant;

pub(super) struct Geometry {
    pub lane: Rect,
    pub thumb: Rect,
    max_offset: f32,
    edge: f32,
}
impl Geometry {
    pub fn new(
        track: Rect,
        height: f32,
        total: f32,
        view: &types::TimelineState,
        styles: &motion_ui::style::StyleSheet,
    ) -> Self {
        let lane = Rect::new(track.min[0], track.max[1] - height, track.width(), height);
        let zoom = view.zoom.max(1.);
        let max_offset = (total - total / zoom).max(0.);
        let width = (lane.width() / zoom)
            .max(styles.metric("timeline.scrollbar.thumb.min_width", 24.))
            .min(lane.width());
        let fraction = if max_offset > 0. {
            view.offset / max_offset
        } else {
            0.
        };
        let thumb = Rect::new(
            lane.min[0] + (lane.width() - width) * fraction.clamp(0., 1.),
            lane.min[1],
            width,
            height,
        );
        Self {
            lane,
            thumb,
            max_offset,
            edge: styles.metric("icon.slot", 20.),
        }
    }
}

pub(super) fn update(
    g: &Geometry,
    view: &mut types::TimelineState,
    input: &Input,
    total: f32,
    busy: bool,
) {
    update_at(g, view, input, total, busy, Instant::now());
}
fn update_at(
    g: &Geometry,
    view: &mut types::TimelineState,
    input: &Input,
    total: f32,
    busy: bool,
    now: Instant,
) {
    if view.navigation_paint_only {
        return;
    }
    if view.graph_mode && view.graph_ranges.locked_x() {
        view.horizontal_drag=None;view.edge_pan_at=None;return;
    }
    if input.focus_lost || input.keys.iter().any(|(key, _)| *key == Key::Escape) {
        view.horizontal_drag = None;
        view.edge_pan_at = None;
        return;
    }
    if !busy && clicked(input, g.lane) && g.max_offset > 0. {
        // Keep the grabbed point under the pointer; clicking the trough centers it.
        view.horizontal_drag = Some(if g.thumb.contains(input.mouse) {
            input.mouse[0] - g.thumb.min[0]
        } else {
            g.thumb.width() * 0.5
        });
    }
    if let Some(grab) = view.horizontal_drag {
        let travel = g.lane.width() - g.thumb.width();
        if travel > 0. && (input.dragging() || input.pressed.is_some() || input.released.is_some())
        {
            view.offset =
                ((input.mouse[0] - g.lane.min[0] - grab) / travel).clamp(0., 1.) * g.max_offset;
        }
        if input.released.is_some() {
            view.horizontal_drag = None;
        }
        view.edge_pan_at = None;
        return;
    }
    // Absolute-pointer gestures can follow the view. Key and handle drags have
    // separate relative-coordinate contracts and are deliberately not changed.
    let active = (view.scrubbing || view.moving.is_some() || view.trimming.is_some())
        && input.dragging()
        && !input.focus_lost;
    if !active {
        // Acknowledgement-only paints carry neutral input. They must not stop
        // the gesture that still belongs to the real window's held pointer.
        if input.released.is_some()
            || (!view.scrubbing && view.moving.is_none() && view.trimming.is_none())
        {
            view.edge_pan_at = None;
        }
        return;
    }

    let direction = if input.mouse[0] < g.lane.min[0] + g.edge {
        -((g.lane.min[0] + g.edge - input.mouse[0]) / g.edge).clamp(0., 1.)
    } else if input.mouse[0] > g.lane.max[0] - g.edge {
        ((input.mouse[0] - g.lane.max[0] + g.edge) / g.edge).clamp(0., 1.)
    } else {
        0.
    };
    if direction == 0. {
        view.edge_pan_at = None;
        return;
    }
    let dt = view.edge_pan_at.replace(now).map_or(0., |before| {
        now.saturating_duration_since(before)
            .as_secs_f32()
            .min(0.05)
    });
    let before = view.offset;
    view.offset = (before + direction * total / view.zoom.max(1.) * dt).clamp(0., g.max_offset);
    if let Some(moving) = view.moving.as_mut() {
        // Preserve the initial composition-time anchor as the viewport pans.
        moving.start_mouse -= (view.offset - before) * g.lane.width() / total * view.zoom.max(1.);
    }
}

pub(super) fn paint(
    list: &mut PaintList,
    track: Rect,
    height: f32,
    total: f32,
    view: &types::TimelineState,
    input: &Input,
) {
    let styles = list.styles();
    let g = Geometry::new(track, height, total, view, &styles);
    paint::rect(list, g.lane, styles.color("panel"));
    paint::rect(
        list,
        Rect::new(
            g.lane.min[0],
            g.lane.min[1],
            g.lane.width(),
            styles.metric("border", 1.),
        ),
        styles.color("border"),
    );
    let color = if g.max_offset <= 0. {
        "border"
    } else if view.horizontal_drag.is_some() {
        "focus"
    } else if g.thumb.contains(input.mouse) {
        "muted"
    } else {
        "disabled"
    };
    // The slim visible pill retains the full footer's pointer target.
    let inset = styles.metric("timeline.scrollbar.inset", 4.);
    let thumb_height = styles
        .metric("timeline.scrollbar.thumb.height", 6.)
        .min(height);
    paint::box_rect(
        list,
        Rect::new(
            g.thumb.min[0] + inset,
            g.thumb.min[1] + (height - thumb_height) * 0.5,
            (g.thumb.width() - inset * 2.).max(0.),
            thumb_height,
        ),
        styles.color(color),
        styles.color("transparent"),
        0.,
        styles.metric("timeline.scrollbar.thumb.radius", 3.),
    );
}

#[cfg(test)]
mod tests;
