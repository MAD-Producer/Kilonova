//! Read-only status, separate from the existing message/error channel.
use super::*;

pub(super) fn reserve(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    locale: Locale,
    preparing: bool,
) -> Rect {
    if !preparing {
        return area;
    }
    let text = if locale == Locale::Zh {
        "正在准备预览…"
    } else {
        "Preparing preview…"
    };
    let styles = list.styles();
    let style = styles
        .resolve("label.small.muted", Default::default())
        .unwrap();
    let width = (atlas.measure(text) / atlas.font_size() * style.font_size).min(area.width());
    let right = Rect::new(area.max[0] - width, area.min[1], width, area.height());
    motion_ui::style::text_style(list, atlas, right, text, style, Align::Right);
    Rect::new(
        area.min[0],
        area.min[1],
        (area.width() - width - styles.resolve("panel", Default::default()).unwrap().padding)
            .max(0.),
        area.height(),
    )
}
