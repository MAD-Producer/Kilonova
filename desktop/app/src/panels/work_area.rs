//! One composition range, with pointer preview and one atomic edit on release.
//! Cancelling, losing focus or changing the authored snapshot never writes a range.
use super::*;
use motion_core::WorkArea;

#[derive(Clone)]
pub(crate) struct Drag {
    root: String,
    composition: String,
    revision: u64,
    original: WorkArea,
    current: WorkArea,
    edge: Edge,
    press_frame: f64,
    ppf: f32,
    offset: f32,
}
#[derive(Clone, Copy)]
enum Edge {
    In,
    Out,
    Range,
}

fn range(state: &Value) -> Option<WorkArea> {
    match observe!(
        "timeline.work_area",
        serde_json::from_value(state["project"]["work_area"].clone()),
        "revision={}",
        state["revision"]
    ) {
        Ok(area) => Some(area),
        Err(_) => None,
    }
}
fn command(area: WorkArea, revision: u64) -> Action {
    Action::EditChecked {
        command: Command::SetWorkArea {
            in_frame: area.in_frame,
            out_frame: area.out_frame,
        },
        revision,
    }
}

pub(super) fn shortcut(
    command_id: crate::shortcuts::Command,
    state: &Value,
    frame: &mut f64,
    out: &mut Vec<Action>,
) -> bool {
    use crate::shortcuts::Command as S;
    if !matches!(
        command_id,
        S::WorkAreaIn | S::WorkAreaOut | S::GoToWorkAreaIn | S::GoToWorkAreaOut
    ) {
        return false;
    }
    let Some(mut area) = range(state) else {
        return false;
    };
    match command_id {
        S::GoToWorkAreaIn => {
            *frame = f64::from(area.in_frame);
            out.push(Action::Seek(*frame));
        }
        S::GoToWorkAreaOut => {
            *frame = f64::from(area.out_frame - 1);
            out.push(Action::Seek(*frame));
        }
        S::WorkAreaIn | S::WorkAreaOut if state["observing"] != true => {
            if command_id == S::WorkAreaIn {
                area.in_frame = frame.floor().max(0.) as u32;
            } else {
                area.out_frame = (frame.floor().max(0.) as u32).saturating_add(1);
            }
            // The engine rejects crossed bounds through its ordinary visible error channel.
            out.push(command(area, state["revision"].as_u64().unwrap_or(0)));
        }
        _ => {}
    }
    true
}

pub(super) fn render(
    list: &mut PaintList,
    band: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    ppf: f32,
    offset: f32,
    out: &mut Vec<Action>,
) -> bool {
    let Some(area) = range(state) else {
        panels.timeline.work_area_drag = None;
        return false;
    };
    let Ok(total) = observe!(
        "timeline.work_area_frames",
        state["project"]["frames"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or("missing or invalid composition frame count"),
        "revision={}",
        state["revision"]
    ) else {
        panels.timeline.work_area_drag = None;
        return false;
    };
    if observe!(
        "timeline.work_area_bounds",
        area.validate(total),
        "frames={total}"
    )
    .is_err()
        || !ppf.is_finite()
        || ppf <= 0.
    {
        panels.timeline.work_area_drag = None;
        return false;
    }
    let styles = list.styles();
    let colors = styles.colors();
    let x = |frame: u32| band.min[0] + (frame as f32 - offset) * ppf;
    let frame = |point: [f32; 2]| f64::from((point[0] - band.min[0]) / ppf + offset);
    let cancelled = input.focus_lost || input.keys.iter().any(|(k, _)| *k == Key::Escape);
    let stale = panels.timeline.work_area_drag.as_ref().is_some_and(|d| {
        d.root != state["root"].as_str().unwrap_or("")
            || d.composition != state["composition"].as_str().unwrap_or("")
            || d.revision != state["revision"].as_u64().unwrap_or(0)
            || d.ppf != ppf
            || d.offset != offset
            || d.original != area
            || state["observing"] == true
    });
    if cancelled || stale {
        panels.timeline.work_area_drag = None;
    }
    let mut owned = panels.timeline.work_area_drag.is_some();
    if !cancelled && !owned && !panels.timeline.scrubbing && state["observing"] != true {
        if let Some((MouseButton::Left, point)) = input.pressed.filter(|(_, p)| band.contains(*p)) {
            let hit = styles.metric("timeline.work_area.handle.hit", 10.);
            let left = (point[0] - x(area.in_frame)).abs();
            let right = (point[0] - x(area.out_frame)).abs();
            let edge = if left <= hit && left <= right {
                Some(Edge::In)
            } else if right <= hit {
                Some(Edge::Out)
            } else if point[0] > x(area.in_frame) && point[0] < x(area.out_frame) {
                Some(Edge::Range)
            } else {
                None
            };
            if let Some(edge) = edge {
                panels.timeline.work_area_drag = Some(Drag {
                    root: state["root"].as_str().unwrap_or("").into(),
                    composition: state["composition"].as_str().unwrap_or("").into(),
                    revision: state["revision"].as_u64().unwrap_or(0),
                    original: area,
                    current: area,
                    edge,
                    press_frame: frame(point),
                    ppf,
                    offset,
                });
                owned = true;
            }
        }
    }
    let release = input
        .released
        .filter(|(button, _)| *button == MouseButton::Left);
    let mut shown = area;
    if let Some(mut drag) = panels.timeline.work_area_drag.take() {
        if input.dragging() || release.is_some() {
            let delta =
                (frame(release.map_or(input.mouse, |(_, p)| p)) - drag.press_frame).round() as i64;
            let original = drag.original;
            drag.current = match drag.edge {
                Edge::In => WorkArea {
                    in_frame: (i64::from(original.in_frame) + delta)
                        .clamp(0, i64::from(original.out_frame) - 1)
                        as u32,
                    ..original
                },
                Edge::Out => WorkArea {
                    out_frame: (i64::from(original.out_frame) + delta)
                        .clamp(i64::from(original.in_frame) + 1, i64::from(total))
                        as u32,
                    ..original
                },
                Edge::Range => {
                    let shift = delta.clamp(
                        -i64::from(original.in_frame),
                        i64::from(total - original.out_frame),
                    );
                    WorkArea {
                        in_frame: (i64::from(original.in_frame) + shift) as u32,
                        out_frame: (i64::from(original.out_frame) + shift) as u32,
                    }
                }
            };
        }
        shown = drag.current;
        if release.is_some() {
            if shown != area {
                out.push(command(shown, drag.revision));
            }
        } else {
            panels.timeline.work_area_drag = Some(drag);
        }
    }
    let inset = styles.metric("timeline.work_area.inset", 4.);
    let handle = styles
        .metric("timeline.work_area.handle.width", 5.)
        .min(band.width());
    let line = styles.metric("timeline.work_area.line.height", 2.);
    let left = x(shown.in_frame).clamp(band.min[0], band.max[0]);
    let right = x(shown.out_frame).clamp(band.min[0], band.max[0]);
    if right > left {
        paint::rect(
            list,
            Rect::new(
                left,
                band.min[1] + inset,
                right - left,
                band.height() - inset * 2.,
            ),
            styles.color("hover"),
        );
        paint::rect(
            list,
            Rect::new(left, band.max[1] - inset - line, right - left, line),
            colors.ACCENT,
        );
        for edge in [shown.in_frame, shown.out_frame] {
            let xx = x(edge);
            if xx >= band.min[0] && xx <= band.max[0] {
                paint::rect(
                    list,
                    Rect::new(
                        (xx - handle / 2.).clamp(band.min[0], band.max[0] - handle),
                        band.min[1] + inset,
                        handle,
                        band.height() - inset * 2.,
                    ),
                    colors.ACCENT,
                );
            }
        }
    }
    #[cfg(test)]
    {
        panels.timeline.work_area_band = Some(band);
    }
    owned
}

#[cfg(test)]
mod tests;
