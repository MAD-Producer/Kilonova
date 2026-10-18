//! Both graph surfaces display authored values, never evaluated expressions/constraints.
use super::*;

pub(super) fn draw(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    locale: Locale,
    negative: bool,
) -> f32 {
    let styles = list.styles();
    let row = styles.metric("row.height", 22.);
    let text = if negative {
        auxiliary::localized(
            locale,
            "原始曲线（无表达式/约束）· 零帧前只读",
            "Raw · no expressions/constraints · before 0: read-only",
        )
    } else {
        auxiliary::localized(
            locale,
            "原始曲线 · 不含表达式/约束",
            "Raw curve · excludes expressions/constraints",
        )
    };
    let mut content = list.child();
    motion_ui::style::text_style(
        &mut content,
        atlas,
        Rect::new(area.min[0], area.min[1], area.width(), row),
        text,
        styles.resolve("label.muted", Default::default()).unwrap(),
        Align::Left,
    );
    content.clip_to(area);
    list.append(content);
    row
}

pub(super) fn has_negative(source: &Value) -> bool {
    source["keys"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|k| k["frame"].as_f64().is_some_and(|f| f < 0.))
}
