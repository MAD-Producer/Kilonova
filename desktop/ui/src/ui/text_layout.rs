use super::*;

/// Paint one complete shaped line. Callers provide scrolling and clipping;
/// syntax colors are attached to source clusters without reshaping fragments.
pub(crate) fn positioned_text(
    list: &mut PaintList,
    atlas: &mut crate::text::TextAtlas,
    area: Rect,
    layout: &crate::text::layout::LineLayout,
    origin: f32,
    font_size: f32,
    clip: bool,
    mut color: impl FnMut(usize) -> Color,
) {
    if !font_size.is_finite()
        || font_size <= 0.
        || !atlas.font_size().is_finite()
        || atlas.font_size() <= 0.
    {
        atlas.report_layout_failure(
            "invalid_size",
            format_args!("size={font_size} base_size={}", atlas.font_size()),
        );
        return;
    }
    let scale = font_size / atlas.font_size();
    let baseline =
        area.min[1] + (area.height() + (atlas.ascent * 2. - atlas.line_height) * scale) * 0.5;
    for positioned in &layout.glyphs {
        let Some(glyph) = atlas.layout_bitmap(positioned, font_size) else {
            continue;
        };
        let width = (glyph.uv[2] - glyph.uv[0]) * atlas.config().width as f32 / atlas.pixel_ratio();
        let height =
            (glyph.uv[3] - glyph.uv[1]) * atlas.config().height as f32 / atlas.pixel_ratio();
        if width > 0. && height > 0. {
            let glyph = paint::Glyph {
                uv: glyph.uv,
                atlas_size: Some([atlas.config().width, atlas.config().height]),
                position: [
                    ((origin + positioned.position[0] * font_size) * atlas.pixel_ratio()).round()
                        / atlas.pixel_ratio()
                        + glyph.offset[0],
                    ((baseline - positioned.position[1] * font_size) * atlas.pixel_ratio()).round()
                        / atlas.pixel_ratio()
                        + glyph.offset[1],
                ],
                size: [width, height],
                color: color(positioned.cluster),
            };
            if let Some(glyph) = if clip {
                glyph.clipped(area)
            } else {
                Some(glyph)
            } {
                list.push_glyph(glyph);
            }
        }
    }
}
