//! Small independent panels backed by the shared project snapshot and commands.
use super::*;
use motion_ui::{
    style::{box_style, text_style, States},
    widgets::{Checkbox, Dropdown, DropdownItem, NumberOptions},
};
use std::collections::BTreeMap;
pub(crate) mod text_style;
pub(crate) mod render_queue;

#[derive(Default)]
pub(crate) struct State {
    pub(crate) audio: super::audio::monitor::Controls,
    pub(crate) text_style: text_style::State,
    pub(crate) render_queue: render_queue::Controls,
    pub(super) align: super::align::Controls,
    pub scroll: BTreeMap<u32, f32>,
    color_popup: Option<ColorPopup>,
    preview_quality: Dropdown,
    preview_options: [Dropdown; 3],
    export_scope: Dropdown,
    #[cfg(test)]
    preview_rect: Option<Rect>,
    #[cfg(test)]
    preview_fields: [Option<Rect>; 3],
    #[cfg(test)]
    preview_loop: Option<Rect>,
    #[cfg(test)]
    movie_scope_rect: Option<Rect>,
    #[cfg(test)]
    play_rect: Option<Rect>,
    color_field: Option<(usize, numeric::Field)>,
}
#[derive(Clone)]
struct ColorPopup {
    object: u64,
    anchor: Rect,
    opened: bool,
}
impl State {
    pub(super) fn text_menu(&mut self, request: &mut super::text_menu::Request<'_>) {
        self.audio.text_menu(request);
        self.text_style.text_menu(request);
        self.render_queue.text_menu(request);
        if let Some((_, field)) = &mut self.color_field { request.visit(&mut field.editor.text); }
    }
    pub fn editing(&self) -> bool {
        self.owner().is_some()
    }
    pub fn owner(&self) -> Option<Panel> {
        if self.audio.editing() { return Some(Panel::Audio); }
        if let Some(owner) = self.text_style.owner() { return Some(owner); }
        if self.render_queue.editing() { return Some(Panel::RenderQueue); }
        if self.color_popup.is_some() {
            Some(Panel::Character)
        } else if self.preview_quality.open || self.preview_options.iter().any(|d| d.open) {
            Some(Panel::Preview)
        } else if self.export_scope.open {
            Some(Panel::RenderQueue)
        } else if self.align.target.open {
            Some(Panel::Align)
        } else {
            None
        }
    }
    pub fn cancel(&mut self) -> bool {
        let audio = self.audio.cancel();
        self.text_style.cancel_field();
        self.render_queue.cancel_field();
        let begun = self.color_field.take().is_some_and(|(_, mut f)| f.cancel());
        self.color_popup = None;
        self.preview_quality.open = false;
        for dropdown in &mut self.preview_options { dropdown.open = false; }
        self.export_scope.open = false;
        self.align.target.open = false;
        begun || audio
    }
}

#[cfg(test)]
mod selection_tests;

pub(super) fn selected<'a>(panels: &Panels, state: &'a Value) -> Option<&'a Value> {
    let object = panels.selected?;
    state["project"]["layers"]
        .as_array()?
        .iter()
        .find(|l| l["id"].as_u64() == Some(object))
}
pub(super) fn body_y(
    styles: &motion_ui::style::StyleSheet,
    panel: Panel,
    area: Rect,
    height: f32,
    panels: &mut Panels,
    input: &Input,
) -> f32 {
    let pad = styles.metric("padding.panel", 8.);
    let scroll = panels.auxiliary.scroll.entry(panel as u32).or_default();
    if area.contains(input.mouse) {
        *scroll += input.wheel[1];
    }
    *scroll = scroll.clamp(0., (height - area.height()).max(0.));
    area.min[1] + pad - *scroll
}
pub(super) fn line(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    y: &mut f32,
    text: &str,
    muted: bool,
) {
    let styles = list.styles();
    let pad = styles.metric("padding.panel", 8.);
    let row = styles.metric("row.height", 22.);
    label(
        list,
        atlas,
        Rect::new(
            area.min[0] + pad,
            *y,
            (area.width() - pad * 2.).max(0.),
            row,
        ),
        text,
        muted,
    );
    *y += styles
        .metric("auxiliary.control.height", 26.)
        .max(styles.metric("row.height", 22.));
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
    let offset = atlas.measure(label) + list.styles().metric("padding", 5.);
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
    let pad = styles.metric("padding.panel", 8.);
    let gap = styles.metric("auxiliary.gap", 6.);
    let row = styles.metric("row.height", 22.);
    let control = styles
        .metric("auxiliary.control.height", 26.)
        .max(styles.metric("row.height", 22.));
    let original_input = input;
    let neutral = Input::default();
    let input = if panels.auxiliary.editing() {
        &neutral
    } else {
        input
    };

    paint::rect(list, area, colors.PANEL);
    if panel == Panel::RenderQueue {
        let input = if panels.auxiliary.owner().is_some_and(|owner|owner != Panel::RenderQueue) { &neutral } else { original_input };
        render_queue::render(list,atlas,area,panels,state,input,out);
        return;
    }
    if matches!(panel, Panel::Character | Panel::Paragraph) {
        let input = if panels.auxiliary.color_popup.is_some() { &neutral } else { original_input };
        text_style::render(list, atlas, area, panels, state, input, panel, out);
        if panel == Panel::Character { color_overlay(list, atlas, area, panels, &selected(panels, state).cloned(), original_input, out); }
        return;
    }
    let locale = panels.locale;
    let layer = selected(panels, state).cloned();
    let height = match panel {
        Panel::Preview => pad * 2. + control * 5. + gap * 6. + row * 4.,
        Panel::Info => {
            pad * 2.
                + control
                    * (if state["desktop"]["uiPerformance"].is_object() {
                        7.
                    } else {
                        5.
                    })
                + gap * 2.
        }
        _ => styles.metric("auxiliary.content.height", 500.),
    };
    let mut y = if panel == Panel::Media { area.min[1] + pad }
        else { body_y(&styles, panel, area, height, panels, input) };
    match panel {
        Panel::Info => {
            // Design-mock hero grid: probe channels, cursor, selection, timecode.
            // Channel and cursor values stay placeholders until the backend probe lands.
            let left = area.min[0] + pad;
            let width = (area.width() - pad * 2.).max(0.);
            let quarter = width * 0.25;
            let right = left + width * 0.62;
            for (index, pairs) in [
                [("R", "—"), ("G", "—"), ("X", "—")],
                [("B", "—"), ("A", "—"), ("Y", "—")],
            ]
            .into_iter()
            .enumerate()
            {
                let line_y = y + index as f32 * control;
                for (i, (tag, value)) in pairs.into_iter().enumerate() {
                    let x = if i < 2 {
                        left + quarter * i as f32
                    } else {
                        right
                    };
                    label_value(
                        list,
                        atlas,
                        Rect::new(x, line_y, quarter, row),
                        &format!("{tag}:"),
                        value,
                        false,
                    );
                }
            }
            y += control * 2. + gap;
            ui::text(
                list,
                atlas,
                Rect::new(left, y, width * 0.55, row),
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
                Rect::new(left + width * 0.55, y, width * 0.45, row),
                &timecode(
                    state["frame"].as_f64().unwrap_or(0.),
                    state["project"]["fps"].as_u64().unwrap_or(30) as u32,
                ),
                colors.TEXT,
                Align::Right,
            );
            y += control + gap;
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
            let transport_width = styles.metric("auxiliary.transport.width", 36.);
            let start =
                area.min[0] + (area.width() - (transport_width * 3. + gap * 2.)).max(0.) * 0.5;
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
                #[cfg(test)]
                if i == 1 {
                    panels.auxiliary.play_rect = Some(Rect::new(
                        start + i as f32 * (transport_width + gap),
                        y,
                        transport_width,
                        control,
                    ));
                }
                toolbar::icon_button(
                    list,
                    Rect::new(
                        start + i as f32 * (transport_width + gap),
                        y,
                        transport_width,
                        control,
                    ),
                    tool,
                    input,
                    true,
                    action,
                    out,
                );
            }
            y += control + gap * 2.;
            preview_options(list, atlas, area, panels, state, original_input, input, y, out);
        }
        Panel::Media => {
            button(
                list,
                atlas,
                Rect::new(area.min[0] + pad, y, area.width() - pad * 2., control),
                locale.t(T::ImportMedia),
                input,
                true,
                Action::ImportMedia,
                out,
            );
            y += control + gap;
            let rows = Rect::new(area.min[0] + pad, y, (area.width() - pad * 2.).max(0.), (area.max[1] - y - pad).max(0.));
            super::media_assets::render_list(list, atlas, rows, panels, state, input, Panel::Media, "");
        }
        _ => {}
    }
    if panel == Panel::Character {
        color_overlay(list, atlas, area, panels, &layer, original_input, out);
    }
}

fn preview_options(
    list: &mut PaintList, atlas: &mut TextAtlas, area: Rect, panels: &mut Panels,
    state: &Value, original: &Input, input: &Input, y: f32, out: &mut Vec<Action>,
) {
    use crate::preview_playback::{Skip, Start};
    use motion_core::FrameScope;
    let styles = list.styles();
    let pad = styles.metric("padding.panel", 8.);
    let gap = styles.metric("auxiliary.gap", 6.);
    let control = styles.metric("auxiliary.control.height", 26.).max(styles.metric("row.height", 22.));
    let row = styles.metric("row.height", 22.);
    let width = (area.width() - pad * 2.).max(0.);
    let caption = styles.metric("label.column", 72.).min(width * 0.36);
    let locale = panels.locale;
    let t = |zh, en| localized(locale, zh, en);
    let options = panels.preview_playback;
    let titles = [t("范围", "Range"), t("开始播放", "Start from"), t("跳帧", "Skip"), t("分辨率", "Resolution")];
    let labels = [
        vec![t("整段合成", "Full composition"), t("工作范围", "Work area")],
        vec![t("当前时间", "Current time"), t("范围起点", "Range start")],
        vec![t("不跳帧", "No skipping"), "1", "2", "3", "4", "5"],
        vec![locale.t(T::Auto), locale.t(T::Full), locale.t(T::Smooth), locale.t(T::Economy)],
    ];
    let selected = [usize::from(options.scope == FrameScope::WorkArea), usize::from(options.start == Start::RangeStart), options.skip as usize,
        match state["preview"]["mode"].as_str().unwrap_or("auto") { "high" => 1, "balanced" => 2, "economy" => 3, _ => 0 }];
    let rects: [Rect;4] = std::array::from_fn(|i| Rect::new(area.min[0] + pad + caption + gap,
        y + i as f32 * (control + gap), (width - caption - gap).max(0.), control));
    for i in 0..4 {
        label(list, atlas, Rect::new(area.min[0]+pad, rects[i].min[1], caption, control), titles[i], true);
    }
    let below = y + 4. * (control + gap);
    #[cfg(test)] { panels.auxiliary.preview_loop = Some(Rect::new(area.min[0]+pad,below,width,row)); }
    if Checkbox::draw(list, atlas, Rect::new(area.min[0]+pad,below,width,row), t("循环播放", "Loop playback"), options.looping, input, true, false, None).clicked {
        out.push(Action::PreviewPlayback(crate::preview_playback::Options { looping: !options.looping, ..options }));
    }
    let width_px = state["preview"]["memoryAdaptation"]["actualWidth"].as_u64().or_else(|| state["preview"]["width"].as_u64());
    let height_px = state["preview"]["memoryAdaptation"]["actualHeight"].as_u64().or_else(|| state["preview"]["height"].as_u64());
    let dimensions = match (width_px,height_px) { (Some(w),Some(h)) => format!("{}: {w} × {h}",t("实际像素", "Actual pixels")), _ => t("等待首帧", "Waiting for first frame").into() };
    label(list,atlas,Rect::new(area.min[0]+pad,below+row,width,row),&dimensions,true);
    let video = &state["preview"]["video"];
    let frames = video["cacheFrames"].as_u64().unwrap_or(0);
    let bytes = video["cacheBytes"].as_u64().unwrap_or(0);
    let pending = video["decodeInFlight"].as_array().map_or(0,Vec::len);
    let status = format!("{}: {frames} {} · {:.1} MiB",t("视频缓存", "Video cache"),t("帧", "frames"),bytes as f64/(1024.*1024.));
    label(list,atlas,Rect::new(area.min[0]+pad,below+row*2.,width,row),&status,true);
    let status = if pending > 0 { format!("{}: {pending}", t("正在解码", "Decoding")) }
        else { t("按需解码 · 未预缓存整个范围", "On-demand decode · range not pre-cached").into() };
    label(list,atlas,Rect::new(area.min[0]+pad,below+row*3.,width,row),&status,true);
    #[cfg(test)] {
        panels.auxiliary.preview_rect = Some(rects[3]);
        panels.auxiliary.preview_fields = [Some(rects[0]),Some(rects[1]),Some(rects[2])];
    }
    // The active popup paints after its siblings and is the only input owner.
    let active = (0..4).find(|&i| if i == 3 {panels.auxiliary.preview_quality.open} else {panels.auxiliary.preview_options[i].open});
    let mut order = [0,1,2,3]; order.sort_by_key(|i| Some(*i) == active);
    let neutral = Input::default();
    for i in order {
        let items: Vec<_> = labels[i].iter().map(|label| DropdownItem { label, enabled:true }).collect();
        let dropdown = if i == 3 { &mut panels.auxiliary.preview_quality } else { &mut panels.auxiliary.preview_options[i] };
        let key_input = if active.is_none() || active == Some(i) { original } else { &neutral };
        if let Some(index) = dropdown.draw(list,atlas,rects[i],area,&items,selected[i],key_input,true,false) {
            if i == 3 {out.push(Action::PreviewMode(index as i32)); continue;}
            let mut next = options;
            match i {
                0 => next.scope = if index == 0 {FrameScope::Composition} else {FrameScope::WorkArea},
                1 => next.start = if index == 0 {Start::Current} else {Start::RangeStart},
                _ => next.skip = [Skip::None,Skip::One,Skip::Two,Skip::Three,Skip::Four,Skip::Five][index],
            }
            out.push(Action::PreviewPlayback(next));
        }
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
    let width =
        (styles.metric("label.column", 72.) + styles.metric("value.min_width", 64.) + pad * 2.)
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
                Command::SetColor {
                    object,
                    value: color,
                }
            }) {
                panels.auxiliary.color_field = None;
            }
        } else {
            numeric::passive(
                &mut paint,
                atlas,
                number,
                f64::from(color[axis]),
                input,
                true,
            );
        }
    }
    paint.clip_to(rect);
    list.append(paint);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loop_checkbox_and_movie_scope_dropdown_dispatch_real_values_in_both_hosts() {
        for locale in [Locale::En,Locale::Zh] { for floating in [false,true] {
            let mut p = Panels::default(); p.locale = locale;
            let mut atlas = TextAtlas::from_system(12.).unwrap();
            let state = serde_json::json!({"root":"preview","composition":"main","frame":0,"preview":{"mode":"auto"},"project":{"frames":120,"fps":30,"layers":[]}});
            let mut draw = |panel,p: &mut Panels,i: &Input| if floating {
                super::super::floating_chrome(&mut PaintList::default(),&mut atlas,panel,[300.,420.],p,&state,i)
            } else {
                super::super::chrome(&mut PaintList::default(),&mut atlas,[300.,420.],&dock::Node::dock(panel.dock()),p,&state,i)
            };
            let click = |r:Rect| {let point=[(r.min[0]+r.max[0])/2.,(r.min[1]+r.max[1])/2.];Input {mouse:point,pressed:Some((MouseButton::Left,point)),..Default::default()}};
            draw(Panel::Preview,&mut p,&Input::default());
            let rect = p.auxiliary.preview_loop.unwrap();
            assert!(matches!(draw(Panel::Preview,&mut p,&click(rect)).as_slice(),[Action::PreviewPlayback(options)] if !options.looping));
            draw(Panel::RenderQueue,&mut p,&Input::default());
            let rect = p.auxiliary.movie_scope_rect.unwrap();
            assert!(draw(Panel::RenderQueue,&mut p,&click(rect)).is_empty());
            assert_eq!(p.auxiliary.owner(),Some(Panel::RenderQueue));
            draw(Panel::RenderQueue,&mut p,&Input {keys:vec![(Key::Down,Default::default())],..Default::default()});
            assert!(matches!(draw(Panel::RenderQueue,&mut p,&Input {keys:vec![(Key::Enter,Default::default())],..Default::default()}).as_slice(),[Action::MovieExportScope(motion_core::FrameScope::WorkArea)]));
            assert!(!p.auxiliary.editing());
        }}
    }
    #[test]
    fn preview_range_start_and_skip_select_explicit_values_in_both_hosts_and_locales() {
        for locale in [Locale::En,Locale::Zh] { for floating in [false,true] {
            let mut panels = Panels::default(); panels.locale = locale;
            let mut atlas = TextAtlas::from_system(12.).unwrap();
            let state = serde_json::json!({"root":"preview","composition":"main","frame":0,"preview":{"mode":"auto"},"project":{"frames":120,"fps":30,"layers":[]}});
            let mut draw = |p: &mut Panels,i: &Input| if floating {
                super::super::floating_chrome(&mut PaintList::default(),&mut atlas,Panel::Preview,[300.,420.],p,&state,i)
            } else {
                super::super::chrome(&mut PaintList::default(),&mut atlas,[300.,420.],&dock::Node::dock(Panel::Preview.dock()),p,&state,i)
            };
            for field in 0..3 {
                draw(&mut panels,&Input::default());
                let rect = panels.auxiliary.preview_fields[field].unwrap();
                let point = [(rect.min[0]+rect.max[0])/2.,(rect.min[1]+rect.max[1])/2.];
                assert!(draw(&mut panels,&Input {mouse:point,pressed:Some((MouseButton::Left,point)),..Default::default()}).is_empty());
                assert_eq!(panels.auxiliary.owner(),Some(Panel::Preview));
                assert!(draw(&mut panels,&Input {keys:vec![(Key::Down,Default::default())],..Default::default()}).is_empty());
                let actions = draw(&mut panels,&Input {keys:vec![(Key::Enter,Default::default())],..Default::default()});
                let [Action::PreviewPlayback(options)] = actions.as_slice() else {panic!("{actions:?}");};
                panels.preview_playback = *options;
                assert!(!panels.auxiliary.editing());
            }
            assert_eq!(panels.preview_playback.scope,motion_core::FrameScope::WorkArea);
            assert_eq!(panels.preview_playback.start,crate::preview_playback::Start::RangeStart);
            assert_eq!(panels.preview_playback.skip,crate::preview_playback::Skip::One);
        }}
    }
    #[test]
    fn content_shrink_clamps_scroll_even_when_pointer_is_outside() {
        let mut panels = Panels::default();
        let styles =
            motion_ui::style::StyleSheet::from_json(include_bytes!("../../../ui/styles/dark.json"))
                .unwrap();
        let area = Rect::new(100., 100., 220., 360.);
        panels
            .auxiliary
            .scroll
            .insert(Panel::EffectControls as u32, 560.);
        let input = Input::default();
        let y = body_y(
            &styles,
            Panel::EffectControls,
            area,
            500.,
            &mut panels,
            &input,
        );
        assert_eq!(y, area.min[1] + styles.metric("padding.panel", 8.) - 140.);
        let y = body_y(
            &styles,
            Panel::EffectControls,
            area,
            200.,
            &mut panels,
            &input,
        );
        assert_eq!(y, area.min[1] + styles.metric("padding.panel", 8.));
    }
    #[test]
    fn quality_dropdown_selects_explicitly_and_blocks_transport_behind_the_popup() {
        for floating in [false, true] {
            let mut panels = Panels::default();
            let mut atlas = TextAtlas::from_system(12.).unwrap();
            let state = serde_json::json!({"root":"preview","composition":"main","frame":0,"preview":{"mode":"auto"},"project":{"frames":120,"fps":30,"layers":[]}});
            let mut draw = |panels: &mut Panels, input: &Input| {
                if floating {
                    super::super::floating_chrome(
                        &mut PaintList::default(),
                        &mut atlas,
                        Panel::Preview,
                        [220., 360.],
                        panels,
                        &state,
                        input,
                    )
                } else {
                    super::super::chrome(
                        &mut PaintList::default(),
                        &mut atlas,
                        [220., 360.],
                        &motion_ui::dock::Node::dock(Panel::Preview.dock()),
                        panels,
                        &state,
                        input,
                    )
                }
            };
            draw(&mut panels, &Input::default());
            let field = panels.auxiliary.preview_rect.unwrap();
            let click = |rect: Rect| {
                let point = [
                    rect.min[0] + rect.width() * 0.5,
                    rect.min[1] + rect.height() * 0.5,
                ];
                Input {
                    mouse: point,
                    pressed: Some((motion_ui::input::MouseButton::Left, point)),
                    ..Default::default()
                }
            };
            assert!(draw(&mut panels, &click(field)).is_empty());
            assert_eq!(panels.auxiliary.owner(), Some(Panel::Preview));
            assert!(draw(
                &mut panels,
                &Input {
                    keys: vec![(Key::Down, Default::default())],
                    ..Default::default()
                }
            )
            .is_empty());
            let actions = draw(
                &mut panels,
                &Input {
                    keys: vec![(Key::Enter, Default::default())],
                    ..Default::default()
                },
            );
            assert!(actions
                .iter()
                .any(|action| matches!(action, Action::PreviewMode(1))));
            assert!(!panels.auxiliary.editing());
            assert!(draw(&mut panels, &click(field)).is_empty());
            let play = panels.auxiliary.play_rect.unwrap();
            assert!(
                draw(&mut panels, &click(play)).is_empty(),
                "outside click closes the popup without playing behind it"
            );
            assert!(!panels.auxiliary.editing());
        }
    }
}
