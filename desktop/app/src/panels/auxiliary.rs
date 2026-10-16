//! Small independent panels backed by the shared project snapshot and commands.
use super::*;
use motion_ui::{
    style::{box_style, text_style, State as WidgetState, States},
    widgets::{Checkbox, NumberOptions},
};
use std::collections::BTreeMap;

#[derive(Default)]
pub(crate) struct State {
    pub scroll: BTreeMap<u32, f32>,
    pub expression: String,
    pub expression_object: Option<u64>,
    pub expression_property: usize,
    color_popup: Option<ColorPopup>,
    color_field: Option<(usize, numeric::Field)>,
}
#[derive(Clone)]
struct ColorPopup {
    object: u64,
    anchor: Rect,
    opened: bool,
}
impl State {
    pub fn editing(&self) -> bool {
        self.color_popup.is_some()
    }
    pub fn cancel(&mut self) -> bool {
        let begun = self
            .color_field
            .take()
            .is_some_and(|(_, mut f)| f.cancel());
        self.color_popup = None;
        begun
    }
}

pub(super) fn selected<'a>(panels: &Panels, state: &'a Value) -> Option<&'a Value> {
    state["project"]["layers"]
        .as_array()?
        .iter()
        .find(|l| l["id"].as_u64() == panels.selected)
}
pub(super) fn body_y(
    panel: Panel,
    area: Rect,
    height: f32,
    panels: &mut Panels,
    input: &Input,
) -> f32 {
    let scroll = panels.auxiliary.scroll.entry(panel as u32).or_default();
    if area.contains(input.mouse) {
        *scroll = (*scroll + input.wheel[1]).clamp(0., (height - area.height()).max(0.));
    }
    area.min[1] + 8. - *scroll
}
pub(super) fn line(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    y: &mut f32,
    text: &str,
    muted: bool,
) {
    label(
        list,
        atlas,
        Rect::new(area.min[0] + 10., *y, (area.width() - 20.).max(0.), 24.),
        text,
        muted,
    );
    *y += 26.;
}
pub(super) fn localized(locale: Locale, zh: &'static str, en: &'static str) -> &'static str {
    if locale == Locale::Zh {
        zh
    } else {
        en
    }
}
/// Muted label followed by a brighter value inside one cell.
fn label_value(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    rect: Rect,
    label: &str,
    value: &str,
    accent: bool,
) {
    let colors = list.styles().colors();
    ui::text(list, atlas, rect, label, colors.TEXT_MUTED, Align::Left);
    let offset = atlas.measure(label) + 4.;
    ui::text(
        list,
        atlas,
        Rect::new(
            rect.min[0] + offset,
            rect.min[1],
            (rect.width() - offset).max(0.),
            rect.height(),
        ),
        value,
        if accent { colors.ACCENT } else { colors.TEXT },
        Align::Left,
    );
}
/// Design-mock field shell for controls whose backend is not wired yet.
fn placeholder_field(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    rect: Rect,
    value: &str,
    dropdown: bool,
) {
    let styles = list.styles();
    let states = States::default().with(WidgetState::Disabled);
    let style = styles.resolve("field", states).unwrap();
    box_style(list, rect, style);
    let text_area = if dropdown {
        Rect::new(
            rect.min[0],
            rect.min[1],
            (rect.width() - 24.).max(0.),
            rect.height(),
        )
    } else {
        rect
    };
    text_style(list, atlas, text_area.shrink(style.padding), value, style, Align::Left);
    if dropdown {
        motion_ui::icons::draw(
            list,
            Rect::new(rect.max[0] - 24., rect.min[1], 22., rect.height()),
            motion_ui::icons::Icon::ChevronDown,
            14.,
            style.foreground,
        );
    }
}
pub(super) fn render(
    panel: Panel,
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    let styles = list.styles();
    let colors = styles.colors();

    paint::rect(list, area, colors.PANEL);
    let locale = panels.locale;
    let layer = selected(panels, state).cloned();
    let mut y = body_y(panel, area, 500., panels, input);
    match panel {
        Panel::Info => {
            // Design-mock hero grid: probe channels, cursor, selection, timecode.
            // Channel and cursor values stay placeholders until the backend probe lands.
            let left = area.min[0] + 10.;
            let width = (area.width() - 20.).max(0.);
            let quarter = width * 0.25;
            let right = left + width * 0.62;
            for (row, pairs) in [
                [("R", "—"), ("G", "—"), ("X", "—")],
                [("B", "—"), ("A", "—"), ("Y", "—")],
            ]
            .into_iter()
            .enumerate()
            {
                let line_y = y + row as f32 * 26.;
                for (i, (tag, value)) in pairs.into_iter().enumerate() {
                    let x = if i < 2 {
                        left + quarter * i as f32
                    } else {
                        right
                    };
                    label_value(
                        list,
                        atlas,
                        Rect::new(x, line_y, quarter.max(48.), 24.),
                        &format!("{tag}:"),
                        value,
                        false,
                    );
                }
            }
            y += 56.;
            ui::text(
                list,
                atlas,
                Rect::new(left, y, width * 0.55, 24.),
                layer
                    .as_ref()
                    .and_then(|l| l["name"].as_str())
                    .unwrap_or("—"),
                colors.TEXT,
                Align::Left,
            );
            ui::text(
                list,
                atlas,
                Rect::new(left + width * 0.55, y, width * 0.45, 24.),
                &timecode(
                    state["frame"].as_f64().unwrap_or(0.),
                    state["project"]["fps"].as_u64().unwrap_or(30) as u32,
                ),
                colors.TEXT,
                Align::Right,
            );
            y += 32.;
            line(
                list,
                atlas,
                area,
                &mut y,
                &format!(
                    "{} × {}  ·  {} fps",
                    state["project"]["width"], state["project"]["height"], state["project"]["fps"]
                ),
                true,
            );
            line(
                list,
                atlas,
                area,
                &mut y,
                &format!(
                    "{}: {}",
                    localized(locale, "工程版本", "Revision"),
                    state["revision"]
                ),
                true,
            );
            let performance = &state["desktop"]["uiPerformance"];
            if performance.is_object() {
                let renders = performance["compositionRenders"].as_u64().unwrap_or(0);
                let reuses = performance["compositionReuses"].as_u64().unwrap_or(0);
                let total = renders + reuses;
                let reuse_ratio = if total > 0 {
                    reuses as f64 / total as f64 * 100.
                } else {
                    0.
                };
                line(
                    list,
                    atlas,
                    area,
                    &mut y,
                    &format!(
                        "{}: {}  ·  {} {:.0}%",
                        localized(locale, "合成渲染", "Comp renders"),
                        renders,
                        localized(locale, "纹理复用", "reused"),
                        reuse_ratio,
                    ),
                    true,
                );
                let present_us = performance["lastPresentUs"].as_f64().unwrap_or(0.);
                line(
                    list,
                    atlas,
                    area,
                    &mut y,
                    &format!(
                        "{}: {:.1} ms",
                        localized(locale, "上次呈现耗时", "Last present"),
                        present_us / 1000.,
                    ),
                    true,
                );
            }
        }
        Panel::Preview => {
            let transport = [
                (types::Tool::First, Action::Seek(0.)),
                (
                    if panels.playing {
                        types::Tool::Pause
                    } else {
                        types::Tool::Play
                    },
                    Action::TogglePlay,
                ),
                (types::Tool::Last, Action::Seek(last_frame(state))),
            ];
            let start = area.min[0] + (area.width() - (36. * 3. + 6. * 2.)).max(0.) * 0.5;
            for (i, (tool, action)) in transport.into_iter().enumerate() {
                toolbar::icon_button(
                    list,
                    Rect::new(start + i as f32 * 42., y, 36., 26.),
                    tool,
                    input,
                    true,
                    action,
                    out,
                );
            }
            y += 38.;
            label(
                list,
                atlas,
                Rect::new(area.min[0] + 10., y, 56., 24.),
                locale.t(T::PlayShortcut),
                true,
            );
            placeholder_field(
                list,
                atlas,
                Rect::new(
                    area.min[0] + 66.,
                    y,
                    (area.max[0] - 10. - (area.min[0] + 66.)).max(0.),
                    24.,
                ),
                localized(locale, "空格键", "Space"),
                true,
            );
            y += 32.;
            Checkbox::draw(
                list,
                atlas,
                Rect::new(area.min[0] + 10., y, 66., 22.),
                locale.t(T::PreviewVideo),
                true,
                input,
                false,
                false,
                None,
            );
            Checkbox::draw(
                list,
                atlas,
                Rect::new(area.min[0] + 82., y, 66., 22.),
                locale.t(T::PreviewAudio),
                true,
                input,
                false,
                false,
                None,
            );
            label(
                list,
                atlas,
                Rect::new(area.min[0] + 154., y, (area.width() - 164.).max(0.), 22.),
                locale.t(T::CacheThenPlay),
                true,
            );
            y += 30.;
            let (quality, next) = match state["preview"]["mode"].as_str().unwrap_or("auto") {
                "high" => (T::Full, 2),
                "balanced" => (T::Smooth, 3),
                "economy" => (T::Economy, 0),
                _ => (T::Auto, 1),
            };
            button(
                list,
                atlas,
                Rect::new(area.min[0] + 10., y, area.width() - 20., 24.),
                locale.t(quality),
                input,
                true,
                Action::PreviewMode(next),
                out,
            );
        }
        Panel::Character => {
            if let Some(l) = layer.as_ref().filter(|l| l["content"]["kind"] == "text") {
                let width = (area.width() - 20.).max(0.);
                let font = l["content"]["font"].as_str().unwrap_or("");
                let name = state["project"]["fonts"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|f| f["id"] == font)
                    .and_then(|f| f["name"].as_str())
                    .unwrap_or(font);
                placeholder_field(list, atlas, Rect::new(area.min[0] + 10., y, width, 26.), name, true);
                y += 32.;
                let half = (width - 6.) * 0.5;
                placeholder_field(
                    list,
                    atlas,
                    Rect::new(area.min[0] + 10., y, half, 26.),
                    "—",
                    true,
                );
                placeholder_field(
                    list,
                    atlas,
                    Rect::new(area.min[0] + 16. + half, y, half, 26.),
                    "— px",
                    false,
                );
                y += 32.;
                label(
                    list,
                    atlas,
                    Rect::new(area.min[0] + 10., y, width * 0.4, 22.),
                    locale.t(T::TrackingLeading),
                    true,
                );
                ui::text(
                    list,
                    atlas,
                    Rect::new(area.min[0] + 10. + width * 0.4, y, width * 0.6, 22.),
                    "—  /  — px",
                    colors.TEXT_MUTED,
                    Align::Left,
                );
                y += 26.;
                label(
                    list,
                    atlas,
                    Rect::new(area.min[0] + 10., y, width * 0.4, 22.),
                    locale.t(T::FillStroke),
                    true,
                );
                let object = l["id"].as_u64().unwrap_or(0);
                let mut color = [0.; 4];
                for (i, v) in l["content"]["color"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(4)
                    .enumerate()
                {
                    color[i] = v.as_f64().unwrap_or(0.) as f32;
                }
                let swatch = Rect::new(area.min[0] + 10. + width * 0.4, y + 2., 18., 18.);
                let unlocked = !l["locked"].as_bool().unwrap_or(false);
                color_well::swatch(list, swatch, &color, input, unlocked);
                let caption_rect = Rect::new(
                    swatch.max[0] + 6.,
                    y,
                    (width * 0.28).max(0.),
                    22.,
                );
                ui::text(
                    list,
                    atlas,
                    caption_rect,
                    &color_well::caption(&color),
                    colors.ACCENT,
                    Align::Left,
                );
                ui::text(
                    list,
                    atlas,
                    Rect::new(caption_rect.max[0] + 4., y, width, 22.),
                    &format!("/  {}", localized(locale, "无", "None")),
                    colors.TEXT_MUTED,
                    Align::Left,
                );
                if unlocked && (clicked(input, swatch) || clicked(input, caption_rect)) {
                    if panels.auxiliary.cancel() {
                        out.push(Action::History(4));
                    }
                    panels.auxiliary.color_popup = Some(ColorPopup {
                        object,
                        anchor: swatch,
                        opened: true,
                    });
                }
                y += 30.;
                for (i, icon) in [
                    motion_ui::icons::Icon::AlignLeft,
                    motion_ui::icons::Icon::AlignCenter,
                    motion_ui::icons::Icon::AlignRight,
                ]
                .into_iter()
                .enumerate()
                {
                    ui::button_icon(
                        list,
                        Rect::new(area.min[0] + 10. + i as f32 * 34., y, 28., 22.),
                        icon,
                        input,
                        false,
                    );
                }
                ui::text(
                    list,
                    atlas,
                    Rect::new(area.min[0] + 10., y, width - 0., 22.),
                    locale.t(T::Reset),
                    colors.ACCENT.with_alpha(0.45),
                    Align::Right,
                );
                y += 28.;
                line(
                    list,
                    atlas,
                    area,
                    &mut y,
                    l["content"]["text"].as_str().unwrap_or(""),
                    true,
                );
            } else {
                line(
                    list,
                    atlas,
                    area,
                    &mut y,
                    localized(locale, "选择文字图层", "Select a text layer"),
                    true,
                );
            }
            button(
                list,
                atlas,
                Rect::new(area.min[0] + 10., y, area.width() - 20., 26.),
                locale.t(T::NewText),
                input,
                true,
                Action::NewText,
                out,
            );
        }
        Panel::Paragraph => {
            if let Some(l) = layer.as_ref().filter(|l| l["content"]["kind"] == "text") {
                line(
                    list,
                    atlas,
                    area,
                    &mut y,
                    l["content"]["text"].as_str().unwrap_or(""),
                    false,
                );
                line(
                    list,
                    atlas,
                    area,
                    &mut y,
                    &format!("{} × {}", l["size"][0], l["size"][1]),
                    true,
                );
            } else {
                line(
                    list,
                    atlas,
                    area,
                    &mut y,
                    localized(locale, "选择文字图层", "Select a text layer"),
                    true,
                );
            }
        }
        Panel::Media => {
            button(
                list,
                atlas,
                Rect::new(area.min[0] + 10., y, area.width() - 20., 26.),
                locale.t(T::ImportMedia),
                input,
                true,
                Action::ImportMedia,
                out,
            );
            y += 32.;
            for key in ["assets", "video_assets", "audio_assets", "fonts"] {
                for asset in state["project"][key].as_array().into_iter().flatten() {
                    line(
                        list,
                        atlas,
                        area,
                        &mut y,
                        asset["name"]
                            .as_str()
                            .or(asset["path"].as_str())
                            .unwrap_or(key),
                        false,
                    );
                }
            }
        }
        Panel::RenderQueue => {
            line(
                list,
                atlas,
                area,
                &mut y,
                panels.message.as_deref().unwrap_or(locale.t(T::Ready)),
                true,
            );
            for (text, action, enabled) in [
                (T::ExportPng, Action::ExportPng, true),
                (
                    T::ExportMovie,
                    Action::ExportMovie,
                    cfg!(feature = "ffmpeg"),
                ),
                (T::ExportPackage, Action::Pack, true),
                (T::CancelExport, Action::CancelExport, true),
            ] {
                button(
                    list,
                    atlas,
                    Rect::new(area.min[0] + 10., y, area.width() - 20., 26.),
                    locale.t(text),
                    input,
                    enabled,
                    action,
                    out,
                );
                y += 32.;
            }
        }
        _ => {}
    }
    if panel == Panel::Character {
        color_overlay(list, atlas, area, panels, &layer, input, out);
    }
}

/// The character fill popup: four drag-scrub channels writing Command::SetColor.
fn color_overlay(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    layer: &Option<Value>,
    input: &Input,
    out: &mut Vec<Action>,
) {
    let Some(popup) = panels.auxiliary.color_popup.clone() else {
        return;
    };
    let current = layer
        .as_ref()
        .filter(|l| l["id"].as_u64() == Some(popup.object) && l["content"]["kind"] == "text")
        .map(|l| {
            let mut c = [0.; 4];
            for (i, v) in l["content"]["color"]
                .as_array()
                .into_iter()
                .flatten()
                .take(4)
                .enumerate()
            {
                c[i] = v.as_f64().unwrap_or(0.) as f32;
            }
            c
        });
    let Some(mut color) = current else {
        panels.auxiliary.color_popup = None;
        return;
    };
    let styles = list.styles();
    let pad = styles.metric("padding.panel", 8.);
    let row = styles.metric("row.height", 22.);
    let slot = styles.metric("icon.slot", 20.);
    let width = (styles.metric("label.column", 72.) + styles.metric("value.min_width", 64.)
        + pad * 2.)
    .max(styles.metric("dropdown.menu_min_width", 160.))
    .min(area.width());
    let height = (row * 5. + pad * 2.).min(area.height());
    let rect = Rect::new(
        popup.anchor.min[0]
            .min(area.max[0] - width)
            .max(area.min[0]),
        popup.anchor.max[1]
            .min(area.max[1] - height)
            .max(area.min[1]),
        width,
        height,
    );
    if !popup.opened
        && input
            .pressed
            .is_some_and(|(_, point)| !rect.contains(point))
    {
        if panels.auxiliary.cancel() {
            out.push(Action::History(4));
        }
        return;
    }
    if let Some(p) = &mut panels.auxiliary.color_popup {
        p.opened = false;
    }
    let neutral = Input::default();
    let input = if popup.opened { &neutral } else { input };
    let mut paint = list.child();
    box_style(
        &mut paint,
        rect,
        styles.resolve("popup", States::default()).unwrap(),
    );
    let title = Rect::new(
        rect.min[0] + pad,
        rect.min[1] + pad,
        (width - pad * 2. - slot).max(0.),
        row,
    );
    label(&mut paint, atlas, title, "RGBA", false);
    if ui::button_icon(
        &mut paint,
        Rect::new(rect.max[0] - pad - slot, title.min[1], slot, row),
        motion_ui::icons::Icon::Close,
        input,
        true,
    )
    .clicked
    {
        if panels.auxiliary.cancel() {
            out.push(Action::History(4));
        }
        return;
    }
    let locale = panels.locale;
    for (axis, name) in (if locale == Locale::Zh {
        ["R 红", "G 绿", "B 蓝", "A 不透明度"]
    } else {
        ["R Red", "G Green", "B Blue", "A Opacity"]
    })
    .iter()
    .enumerate()
    {
        let y = title.max[1] + axis as f32 * row;
        let start = rect.min[0] + pad + styles.metric("label.column", 72.);
        label(
            &mut paint,
            atlas,
            Rect::new(
                rect.min[0] + pad,
                y,
                (start - rect.min[0] - pad).max(0.),
                row,
            ),
            name,
            true,
        );
        let number = Rect::new(start, y, (rect.max[0] - pad - start).max(0.), row);
        let active = panels
            .auxiliary
            .color_field
            .as_ref()
            .is_some_and(|(a, _)| *a == axis);
        if clicked(input, number) && !active {
            if panels
                .auxiliary
                .color_field
                .take()
                .is_some_and(|(_, mut f)| f.cancel())
            {
                out.push(Action::History(4));
            }
            panels.auxiliary.color_field = Some((axis, numeric::Field::default()));
        }
        if let Some((_, field)) = panels
            .auxiliary
            .color_field
            .as_mut()
            .filter(|(a, _)| *a == axis)
        {
            let events = field.draw(
                &mut paint,
                atlas,
                number,
                f64::from(color[axis]),
                NumberOptions {
                    min: 0.,
                    max: 1.,
                    speed: 0.005,
                    decimals: 3,
                },
                input,
            );
            if let Some(note) = field.readout() {
                panels.drag_readout = Some((input.mouse, note));
            }
            let object = popup.object;
            if numeric::apply(events, out, |v| {
                color[axis] = v;
                Command::SetColor { object, value: color }
            }) {
                panels.auxiliary.color_field = None;
            }
        } else {
            numeric::passive(&mut paint, atlas, number, f64::from(color[axis]), input, true);
        }
    }
    paint.clip_to(rect);
    list.append(paint);
}
