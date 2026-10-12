use super::*;

pub(super) fn label(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    text: &str,
    muted: bool,
) {
    let styles = list.styles();

    motion_ui::style::text_style(
        list,
        atlas,
        area,
        text,
        styles
            .as_ref()
            .resolve(
                if muted {
                    "label.small.muted"
                } else {
                    "label.small"
                },
                Default::default(),
            )
            .unwrap(),
        Align::Left,
    );
}
pub(super) fn clicked(input: &Input, area: Rect) -> bool {
    matches!(input.pressed, Some((MouseButton::Left, p)) if area.contains(p))
}
pub(super) fn right_clicked(input: &Input, area: Rect) -> bool {
    matches!(input.pressed, Some((MouseButton::Right, p)) if area.contains(p))
}
pub(super) fn button(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    text: &str,
    input: &Input,
    enabled: bool,
    action: Action,
    out: &mut Vec<Action>,
) {
    if ui::button(list, atlas, area, text, input, enabled).clicked {
        out.push(action);
    }
}
pub fn timecode(frame: f64, fps: u32) -> String {
    let frame = frame.max(0.0).floor() as u64;
    let fps = u64::from(fps.max(1));
    let seconds = frame / fps;
    format!(
        "{:02}:{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60,
        frame % fps
    )
}
pub(super) fn last_frame(state: &Value) -> f64 {
    state["project"]["frames"].as_f64().unwrap_or(1.0).max(1.0) - 1.0
}
