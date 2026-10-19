//! Shared result rows and actual port signatures for both node search surfaces.
use super::*;
use crate::node_editor::search::Entry;

pub(super) fn menu_area(
    bounds: Rect,
    position: [f32; 2],
    styles: &motion_ui::style::StyleSheet,
) -> Option<Rect> {
    let pad = styles.metric("padding.panel", 8.);
    let width = styles
        .metric("node.add_menu.width", 512.)
        .min(bounds.width() - pad * 2.);
    let height = styles
        .metric("node.add_menu.height", 440.)
        .min(bounds.height() - pad * 2.);
    if width <= 0. || height <= 0. {
        return None;
    }
    Some(Rect::new(
        position[0].clamp(bounds.min[0] + pad, bounds.max[0] - width - pad),
        position[1].clamp(bounds.min[1] + pad, bounds.max[1] - height - pad),
        width,
        height,
    ))
}

pub(super) fn reveal(scroll: f32, index: usize, row: f32, height: f32, max: f32) -> f32 {
    reveal_offset(scroll, index as f32 * row, row, height, max)
}
pub(super) fn reveal_offset(scroll: f32, top: f32, row: f32, height: f32, max: f32) -> f32 {
    scroll.min(top).max(top + row - height).clamp(0., max)
}

pub(super) fn result_row(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    row: Rect,
    entry: &Entry,
    language: Language,
    selected: bool,
    hovered: bool,
) {
    let styles = list.styles();
    let pad = styles.metric("padding", 5.);
    let line = styles.metric("row.height", 22.).min(row.height() * 0.5);
    let mut paint = list.child();
    if selected || hovered {
        paint::rect(
            &mut paint,
            row,
            styles.color(if selected { "selected" } else { "hover" }),
        );
    }
    let text = Rect::new(
        row.min[0] + pad,
        row.min[1],
        (row.width() - pad * 2.).max(0.),
        line,
    );
    ui::text_sized(
        &mut paint,
        atlas,
        text,
        entry.title(language),
        styles.colors().TEXT,
        Align::Left,
        styles.metric("font.body", 12.),
    );
    ui::text_sized(
        &mut paint,
        atlas,
        Rect::new(text.min[0], text.max[1], text.width(), line),
        entry.description(language),
        styles.colors().TEXT_MUTED,
        Align::Left,
        styles.metric("font.small", 11.),
    );
    paint.clip_to(row);
    list.append(paint);
}

fn wrap(atlas: &mut TextAtlas, text: &str, font: f32, width: f32, limit: usize) -> Vec<String> {
    if limit == 0 || width <= 0. {
        return Vec::new();
    }
    let mut measure = |s: &str| atlas.measure(s) * font / atlas.font_size();
    let mut lines = Vec::new();
    let mut line = String::new();
    for c in text.chars() {
        if !line.is_empty() && measure(&format!("{line}{c}")) > width {
            if lines.len() + 1 == limit {
                while !line.is_empty() && measure(&format!("{line}…")) > width {
                    line.pop();
                }
                line.push('…');
                lines.push(line);
                return lines;
            }
            lines.push(std::mem::take(&mut line));
        }
        line.push(c);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

pub(super) fn details(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    entry: Option<&Entry>,
    language: Language,
    keyboard: bool,
) {
    let styles = list.styles();
    let pad = styles.metric("padding", 5.);
    let font = styles.metric("font.small", 11.);
    let line = font + pad;
    let inside = area.shrink(pad);
    let mut paint = list.child();
    paint::rect(&mut paint, area, styles.colors().SUNKEN);
    if let Some(entry) = entry {
        let count = ((inside.height() - line) / line).floor().max(0.) as usize;
        let (inputs, outputs) = entry
            .signature(language)
            .split_once('→')
            .unwrap_or(("", entry.signature(language)));
        let mut lines = wrap(
            atlas,
            &format!("{}: {}", language.text("输入", "In"), inputs.trim()),
            font,
            inside.width(),
            count.saturating_sub(1),
        );
        lines.extend(wrap(
            atlas,
            &format!("{}: {}", language.text("输出", "Out"), outputs.trim()),
            font,
            inside.width(),
            usize::from(count > 0),
        ));
        for (i, text) in lines.iter().enumerate() {
            ui::text_sized(
                &mut paint,
                atlas,
                Rect::new(
                    inside.min[0],
                    inside.min[1] + i as f32 * line,
                    inside.width(),
                    line,
                ),
                text,
                styles.colors().TEXT,
                Align::Left,
                font,
            );
        }
    }
    let hint = if keyboard {
        language.text(
            "↑↓ 选择 · Enter 添加 · Esc 退出",
            "↑↓ Select · Enter Add · Esc Exit",
        )
    } else {
        language.text("悬停查看输入和输出", "Hover to inspect ports")
    };
    let hint = if keyboard && atlas.measure(hint) * font / atlas.font_size() > inside.width() {
        "↑↓ · Enter · Esc"
    } else {
        hint
    };
    ui::text_sized(
        &mut paint,
        atlas,
        Rect::new(inside.min[0], inside.max[1] - line, inside.width(), line),
        hint,
        styles.colors().TEXT_MUTED,
        Align::Left,
        font,
    );
    paint.clip_to(area);
    list.append(paint);
}
