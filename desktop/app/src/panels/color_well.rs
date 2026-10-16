//! Shared color well painting for property rows that carry an RGBA value.
use motion_ui::{
    input::{Input, Rect},
    paint::{self, PaintList},
    style::{box_style, State, States},
    Color,
};

/// Bordered well with a checkerboard under the authored color.
pub fn swatch(list: &mut PaintList, rect: Rect, values: &[f32], input: &Input, enabled: bool) {
    let styles = list.styles();
    let mut states = States::default();
    if enabled && rect.contains(input.mouse) {
        states = states.with(State::Hovered);
    }
    if !enabled {
        states = states.with(State::Disabled);
    }
    let style = styles.resolve("color_well", states).unwrap();
    box_style(list, rect, style);
    let inset = rect.shrink(style.padding + style.border_width);
    paint::checkerboard(
        list,
        inset,
        styles.color("checker.light"),
        styles.color("checker.dark"),
        styles.metric("padding", 5.),
    );
    // This is authored color data, rather than an editor theme color.
    paint::rect(
        list,
        inset,
        Color([values[0], values[1], values[2], values[3]]),
    );
}

/// Compact `#RRGGBB · nn%` caption matching the vector color row.
pub fn caption(values: &[f32]) -> String {
    let byte = |axis: usize| (values[axis].clamp(0., 1.) * 255.).round() as u8;
    format!(
        "#{:02X}{:02X}{:02X} · {:.0}%",
        byte(0),
        byte(1),
        byte(2),
        values[3] * 100.
    )
}
