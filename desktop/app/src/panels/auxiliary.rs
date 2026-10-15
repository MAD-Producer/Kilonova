//! Small independent panels backed by the shared project snapshot and commands.
use super::*;
use std::collections::BTreeMap;

#[derive(Default)]
pub(crate) struct State {
    pub scroll: BTreeMap<u32, f32>,
    pub expression: String,
    pub expression_object: Option<u64>,
    pub expression_property: usize,
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
            line(
                list,
                atlas,
                area,
                &mut y,
                &timecode(
                    state["frame"].as_f64().unwrap_or(0.),
                    state["project"]["fps"].as_u64().unwrap_or(30) as u32,
                ),
                false,
            );
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
            if let Some(layer) = &layer {
                line(
                    list,
                    atlas,
                    area,
                    &mut y,
                    layer["name"].as_str().unwrap_or("Layer"),
                    false,
                );
            }
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
            for (i, (tool, action)) in [
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
            ]
            .into_iter()
            .enumerate()
            {
                toolbar::icon_button(
                    list,
                    Rect::new(area.min[0] + 10. + i as f32 * 42., y, 36., 26.),
                    tool,
                    input,
                    true,
                    action,
                    out,
                );
            }
            y += 32.;
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
        Panel::Character | Panel::Paragraph => {
            if let Some(l) = layer.as_ref().filter(|l| l["content"]["kind"] == "text") {
                line(
                    list,
                    atlas,
                    area,
                    &mut y,
                    l["content"]["text"].as_str().unwrap_or(""),
                    false,
                );
                let font = l["content"]["font"].as_str().unwrap_or("");
                if let Some(f) = state["project"]["fonts"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|f| f["id"] == font)
                {
                    line(
                        list,
                        atlas,
                        area,
                        &mut y,
                        f["name"].as_str().unwrap_or(font),
                        true,
                    );
                }
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
}
