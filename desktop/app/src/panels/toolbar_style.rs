//! Selection paint controls share the native numeric, dropdown and history contracts.
mod model;
use super::{color_well, types::Action};
use crate::i18n::Locale;
use model::{Kind, Paint, Selection, Shared};
use motion_ui::{
    input::{Input, Key, MouseButton, Rect},
    paint::PaintList,
    style::{box_style, text_style, State, States, StyleSheet},
    text::TextAtlas,
    ui::{self, Align},
    widgets::{Dropdown, DropdownItem, NumberEditor, NumberEvent, NumberOptions},
};
use serde_json::Value;
use std::sync::Arc;

struct Session {
    selection: Arc<Selection>,
    paint: Paint,
    begun: bool,
}
#[derive(PartialEq)]
struct Context {
    root: String,
    composition: String,
    revision: u64,
    frame: f64,
    ids: Vec<u64>,
}
struct Popup {
    kind: Kind,
    anchor: Rect,
    opened: bool,
    fields: [NumberEditor; 4],
    mode: Dropdown,
}
#[derive(Default)]
pub(in crate::panels) struct Controls {
    session: Option<Session>,
    popup: Option<Popup>,
    width: NumberEditor,
    cache: Option<(Context, Arc<Selection>)>,
    pub notice: Option<String>,
    pub cursor: Option<motion_ui::cursor::Cursor>,
    #[cfg(test)]
    hits: std::collections::BTreeMap<&'static str, Rect>,
}
fn text<'a>(locale: Locale, zh: &'a str, en: &'a str) -> &'a str {
    if locale == Locale::Zh {
        zh
    } else {
        en
    }
}
fn label(list: &mut PaintList, atlas: &mut TextAtlas, rect: Rect, value: &str, enabled: bool) {
    let styles = list.styles();
    text_style(
        list,
        atlas,
        rect,
        value,
        styles
            .resolve(
                "label.small",
                if enabled {
                    States::default()
                } else {
                    States::default().with(State::Disabled)
                },
            )
            .unwrap(),
        Align::Left,
    );
}
fn click(input: &Input, rect: Rect) -> bool {
    input
        .pressed
        .is_some_and(|(b, p)| b == MouseButton::Left && rect.contains(p))
}
pub(super) fn end(styles: &StyleSheet, atlas: &mut TextAtlas, locale: Locale) -> f32 {
    let pad = styles.metric("padding", 5.);
    styles.metric("toolbar.paint.start", 308.)
        + atlas.measure(text(locale, "填充", "Fill"))
        + atlas.measure(text(locale, "描边", "Stroke"))
        + styles.metric("toolbar.paint.swatch_width", 28.) * 2.
        + styles.metric("toolbar.paint.width_width", 58.)
        + styles.metric("toolbar.paint.unit_width", 18.)
        + pad * 7.
}
impl Controls {
    pub(super) fn text_menu(&mut self, request: &mut super::text_menu::Request<'_>) {
        request.visit(&mut self.width.text);
        if let Some(popup) = &mut self.popup {
            for field in &mut popup.fields { request.visit(&mut field.text); }
        }
    }
    fn selection(&mut self, state: &Value, ids: &[u64]) -> Result<Arc<Selection>, &'static str> {
        if state["observing"] == true {
            return Err("observing");
        }
        let context = state["revision"].as_u64().map(|revision| Context {
            root: state["root"].as_str().unwrap_or("").into(),
            composition: state["composition"].as_str().unwrap_or("").into(),
            revision,
            frame: state["frame"].as_f64().unwrap_or(0.),
            ids: ids.to_vec(),
        });
        if let (Some(context), Some((cached, selection))) = (&context, &self.cache) {
            if context == cached {
                return Ok(selection.clone());
            }
        }
        self.cache = None;
        let selection = Arc::new(Selection::read(state, ids)?);
        self.cache = context.map(|context| (context, selection.clone()));
        Ok(selection)
    }
    pub fn editing(&self) -> bool {
        self.popup.is_some() || self.width.interacting()
    }
    pub fn cancel(&mut self) -> bool {
        self.width.cancel();
        self.popup = None;
        self.session.take().is_some_and(|s| s.begun)
    }
    fn close(&mut self, commit: bool, out: &mut Vec<Action>) {
        if self.cancel() {
            out.push(Action::History(if commit { 3 } else { 4 }));
        }
    }
    pub fn prepare(
        &mut self,
        state: &Value,
        ids: &[u64],
        mask: bool,
        input: &Input,
        out: &mut Vec<Action>,
    ) {
        self.notice = None;
        let invalid = if self.session.is_some() {
            let current = self.selection(state, ids);
            self.session.as_ref().is_some_and(|s| {
                mask || current.map_or(true, |other| !s.selection.same_context(&other))
            })
        } else {
            false
        };
        if input.focus_lost
            || invalid
            || input
                .key_presses()
                .any(|(k, _, repeat)| k == Key::Escape && !repeat)
        {
            self.close(false, out);
        }
    }
    fn preview(&mut self, out: &mut Vec<Action>) {
        let Some(s) = &mut self.session else {
            return;
        };
        match s.selection.commands(&s.paint) {
            Ok(commands) => {
                if !s.begun {
                    out.push(Action::History(2));
                    s.begun = true;
                }
                out.push(Action::EditBatch(commands));
            }
            Err(error) => {
                log::error!(
                    "toolbar paint preview objects={:?} frame={} phase=prepare cause={error} target_count={}",
                    s.selection
                        .targets
                        .iter()
                        .take(16)
                        .map(|t| t.layer.id)
                        .collect::<Vec<_>>(),
                    s.selection.frame,
                    s.selection.targets.len()
                );
                self.notice = Some(error.to_string());
                self.close(false, out);
            }
        }
    }
    pub fn render(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        state: &Value,
        ids: &[u64],
        mask: bool,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        #[cfg(test)]
        self.hits.clear();
        self.cursor = None;
        let selection = if mask {
            Err("mask")
        } else {
            self.selection(state, ids)
        };
        let reason = selection.as_ref().err().copied();
        let summary = selection.as_ref().ok().and_then(|s| {
            match observe!(
                "toolbar.paint_sample",
                s.summary(),
                "objects={} phase=sample fallback=disabled",
                ids.len()
            ) {
                Ok(summary) => Some(summary),
                Err(error) => {
                    self.notice = Some(error.to_string());
                    None
                }
            }
        });
        let enabled = summary.is_some();
        let styles = list.styles();
        let pad = styles.metric("padding", 5.);
        let h = styles.metric("toolbar.paint.height", 26.);
        let y = area.min[1] + (area.height() - h) * 0.5;
        let mut x = styles.metric("toolbar.paint.start", 308.);
        let neutral = Input::default();
        let controls_input = if self.popup.is_some() {
            &neutral
        } else {
            input
        };
        for kind in [Kind::Fill, Kind::Stroke] {
            let title = match kind {
                Kind::Fill => text(locale, "填充", "Fill"),
                Kind::Stroke => text(locale, "描边", "Stroke"),
            };
            let label_width = atlas.measure(title);
            label(list, atlas, Rect::new(x, y, label_width, h), title, enabled);
            x += label_width + pad;
            let rect = Rect::new(x, y, styles.metric("toolbar.paint.swatch_width", 28.), h);
            let value = summary.as_ref().map(|s| match kind {
                Kind::Fill => &s.fill_color,
                Kind::Stroke => &s.stroke_color,
            });
            match value {
                Some(Shared::Value(color)) => {
                    color_well::swatch(list, rect, color, controls_input, enabled)
                }
                _ => {
                    box_style(
                        list,
                        rect,
                        styles
                            .resolve(
                                "color_well",
                                if enabled {
                                    States::default()
                                } else {
                                    States::default().with(State::Disabled)
                                },
                            )
                            .unwrap(),
                    );
                    text_style(
                        list,
                        atlas,
                        rect,
                        if matches!(value, Some(Shared::Mixed)) {
                            text(locale, "混合", "Mix")
                        } else {
                            text(locale, "无", "None")
                        },
                        styles
                            .resolve("label.small.muted", States::default())
                            .unwrap(),
                        Align::Center,
                    );
                }
            }
            #[cfg(test)]
            self.hits
                .insert(if kind == Kind::Fill { "fill" } else { "stroke" }, rect);
            if enabled && click(controls_input, rect) {
                self.close(true, out);
                self.session = Some(Session {
                    selection: selection.as_ref().unwrap().clone(),
                    paint: Paint::default(),
                    begun: false,
                });
                self.popup = Some(Popup {
                    kind,
                    anchor: rect,
                    opened: true,
                    fields: Default::default(),
                    mode: Default::default(),
                });
            }
            x = rect.max[0] + pad * 2.;
        }
        let rect = Rect::new(x, y, styles.metric("toolbar.paint.width_width", 58.), h);
        if self.width.interacting() && input.pressed.is_some_and(|(_, p)| !rect.contains(p)) {
            self.close(false, out);
        }
        let has_stroke = enabled
            && !summary
                .as_ref()
                .is_some_and(|s| s.stroke == Shared::Value(false));
        if has_stroke && (rect.contains(input.mouse) || self.width.dragging()) {
            self.cursor = Some(if self.width.text.active() {
                motion_ui::cursor::Cursor::Text
            } else {
                motion_ui::cursor::Cursor::ResizeEW
            });
        }
        #[cfg(test)]
        self.hits.insert("width", rect);
        if has_stroke && click(controls_input, rect) && !self.width.interacting() {
            self.session = Some(Session {
                selection: selection.as_ref().unwrap().clone(),
                paint: Paint::default(),
                begun: false,
            });
        }
        let value = summary.as_ref().map_or(0., |s| s.first_width);
        if has_stroke && (self.width.interacting() || click(controls_input, rect)) {
            self.width.drag_threshold = Some(styles.metric("motion.drag_threshold", 3.));
            let options = NumberOptions {
                speed: 0.1,
                min: 0.,
                max: 32768.,
                decimals: 2,
            };
            let events = self
                .width
                .update(f64::from(value), options, controls_input, rect);
            self.width
                .draw(list, atlas, rect, f64::from(value), options);
            for event in events {
                match event {
                    NumberEvent::Change { value } => {
                        if let Some(s) = &mut self.session {
                            s.paint.width = Some(value as f32);
                        }
                        self.preview(out);
                    }
                    NumberEvent::Commit { .. } => self.close(true, out),
                    NumberEvent::Cancel { .. } => self.close(false, out),
                    NumberEvent::Begin { .. } => {}
                }
            }
        } else if matches!(summary.as_ref().map(|s| &s.width), Some(Shared::Mixed)) {
            label(list, atlas, rect, text(locale, "混合", "Mixed"), has_stroke);
        } else {
            super::numeric::passive_with_precision(
                list,
                atlas,
                rect,
                f64::from(value),
                controls_input,
                has_stroke,
                2,
            );
        }
        label(
            list,
            atlas,
            Rect::new(
                rect.max[0] + pad,
                y,
                styles.metric("toolbar.paint.unit_width", 18.),
                h,
            ),
            "px",
            has_stroke,
        );
        if !enabled
            && input.mouse[0] >= styles.metric("toolbar.paint.start", 308.)
            && area.contains(input.mouse)
            && input.mouse[0] < end(&styles, atlas, locale)
        {
            self.notice = Some(
                match reason.unwrap_or("invalid") {
                    "mask" => text(
                        locale,
                        "蒙版工具不使用填充和描边；请切换到选择或钢笔工具",
                        "Masks do not use fill/stroke; switch to Select or Pen",
                    ),
                    "locked" => text(
                        locale,
                        "请先解锁所选图层",
                        "Unlock the selected layers first",
                    ),
                    "observing" => text(
                        locale,
                        "观察模式下无法修改样式",
                        "Paint is read-only while observing",
                    ),
                    "vector" => text(
                        locale,
                        "填充和描边仅适用于矢量图层",
                        "Fill and stroke require vector layers",
                    ),
                    "frame" => text(
                        locale,
                        "请移动到图层的整数帧后编辑样式",
                        "Move to an integral layer frame to edit paint",
                    ),
                    _ => text(
                        locale,
                        "选择矢量图层以编辑填充和描边",
                        "Select vector layers to edit fill and stroke",
                    ),
                }
                .into(),
            );
        }
    }
    pub fn overlay(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        size: [f32; 2],
        state: &Value,
        ids: &[u64],
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        let Some(mut popup) = self.popup.take() else {
            return;
        };
        if self.session.is_none() {
            return;
        }
        let selection = match self.selection(state, ids) {
            Ok(selection) => selection,
            Err(_) => {
                self.close(false, out);
                return;
            }
        };
        let summary = match selection.summary() {
            Ok(s) => s,
            Err(error) => {
                log::error!("toolbar paint objects={:?} frame={} phase=popup_sample cause={error} target_count={}",
                    ids.iter().take(16).collect::<Vec<_>>(), selection.frame, ids.len());
                self.notice = Some(error.to_string());
                self.close(false, out);
                return;
            }
        };
        let styles = list.styles();
        let pad = styles.metric("padding.panel", 8.);
        let row = styles.metric("camera.row.height", 28.);
        let gap = styles.metric("padding", 5.);
        let width = styles
            .metric("toolbar.paint.popup_width", 272.)
            .min(size[0]);
        let rect = Rect::new(
            popup.anchor.min[0].min(size[0] - width).max(0.),
            popup.anchor.max[1],
            width,
            row * 7. + pad * 2. + gap,
        );
        if !popup.opened
            && !popup.mode.open
            && input.pressed.is_some_and(|(_, p)| !rect.contains(p))
        {
            self.close(false, out);
            return;
        }
        let neutral = Input::default();
        let popup_input = if popup.opened { &neutral } else { input };
        popup.opened = false;
        let mut paint = list.child();
        box_style(
            &mut paint,
            rect,
            styles.resolve("popup", States::default()).unwrap(),
        );
        let title = format!(
            "{} · {} {}",
            if popup.kind == Kind::Fill {
                text(locale, "填充", "Fill")
            } else {
                text(locale, "描边", "Stroke")
            },
            selection.targets.len(),
            text(locale, "个矢量图层", "vector layer(s)")
        );
        label(
            &mut paint,
            atlas,
            Rect::new(rect.min[0] + pad, rect.min[1] + pad, width - pad * 2., row),
            &title,
            true,
        );
        let mode_rect = Rect::new(
            rect.min[0] + pad,
            rect.min[1] + pad + row,
            width - pad * 2.,
            row,
        );
        let values = match popup.kind {
            Kind::Fill => summary.first_fill,
            Kind::Stroke => summary.first_stroke,
        };
        let colored = match popup.kind {
            Kind::Fill => summary.fill != Shared::None,
            Kind::Stroke => summary.stroke != Shared::Value(false),
        };
        let field_input = if popup.mode.open {
            &neutral
        } else {
            popup_input
        };
        let mut changed = false;
        for (axis, name) in ["R", "G", "B", "A"].iter().enumerate() {
            let y = mode_rect.max[1] + axis as f32 * row;
            label(
                &mut paint,
                atlas,
                Rect::new(
                    rect.min[0] + pad,
                    y,
                    styles.metric("label.column", 72.),
                    row,
                ),
                name,
                colored,
            );
            let field = Rect::new(
                rect.min[0] + pad + styles.metric("label.column", 72.),
                y,
                width - pad * 2. - styles.metric("label.column", 72.),
                row,
            );
            if popup_input
                .pressed
                .is_some_and(|(_, point)| !field.contains(point))
            {
                popup.fields[axis].cancel();
            }
            #[cfg(test)]
            self.hits.insert(["r", "g", "b", "a"][axis], field);
            if colored {
                popup.fields[axis].drag_threshold =
                    Some(styles.metric("motion.drag_threshold", 3.));
                let options = NumberOptions {
                    speed: 0.005,
                    min: 0.,
                    max: 1.,
                    decimals: 3,
                };
                let events =
                    popup.fields[axis].update(f64::from(values[axis]), options, field_input, field);
                popup.fields[axis].draw(&mut paint, atlas, field, f64::from(values[axis]), options);
                if field.contains(input.mouse) || popup.fields[axis].dragging() {
                    self.cursor = Some(if popup.fields[axis].text.active() {
                        motion_ui::cursor::Cursor::Text
                    } else {
                        motion_ui::cursor::Cursor::ResizeEW
                    });
                }
                for event in events {
                    if let NumberEvent::Change { value } = event {
                        if let Some(s) = &mut self.session {
                            let channels = match popup.kind {
                                Kind::Fill => &mut s.paint.fill_color,
                                Kind::Stroke => &mut s.paint.stroke_color,
                            };
                            channels[axis] = Some(value as f32);
                        }
                        changed = true;
                    }
                }
            } else {
                super::numeric::passive_with_precision(
                    &mut paint,
                    atlas,
                    field,
                    f64::from(values[axis]),
                    field_input,
                    false,
                    3,
                );
            }
        }
        let footer = mode_rect.max[1] + row * 4. + gap;
        let button_width = (width - pad * 2. - gap) * 0.5;
        let apply = Rect::new(rect.min[0] + pad, footer, button_width, row);
        let cancel = Rect::new(apply.max[0] + gap, footer, button_width, row);
        #[cfg(test)]
        {
            self.hits.insert("apply", apply);
            self.hits.insert("cancel", cancel);
            self.hits.insert("mode", mode_rect);
        }
        let commit = ui::button_styled(
            &mut paint,
            atlas,
            apply,
            text(locale, "应用", "Apply"),
            field_input,
            true,
            &styles,
            "button",
        )
        .clicked;
        let cancel = ui::button_styled(
            &mut paint,
            atlas,
            cancel,
            text(locale, "取消", "Cancel"),
            field_input,
            true,
            &styles,
            "button",
        )
        .clicked;
        let mut labels = match popup.kind {
            Kind::Fill => vec![
                text(locale, "无填充", "No fill"),
                text(locale, "实色 · 非零环绕", "Solid · Nonzero"),
                text(locale, "实色 · 奇偶", "Solid · Even-odd"),
            ],
            Kind::Stroke => vec![
                text(locale, "无描边", "No stroke"),
                text(locale, "实色描边", "Solid stroke"),
            ],
        };
        let selected = match popup.kind {
            Kind::Fill => match summary.fill {
                Shared::Value(v) => v,
                Shared::None => 0,
                Shared::Mixed => labels.len(),
            },
            Kind::Stroke => match summary.stroke {
                Shared::Value(v) => usize::from(v),
                Shared::None => 0,
                Shared::Mixed => labels.len(),
            },
        };
        let choices = labels.len();
        labels.push(text(locale, "混合值", "Mixed values"));
        let items: Vec<_> = labels
            .iter()
            .enumerate()
            .map(|(i, label)| DropdownItem {
                label,
                enabled: i < choices,
            })
            .collect();
        if let Some(choice) = popup.mode.draw_inline(
            &mut paint,
            atlas,
            mode_rect,
            Rect::new(0., 0., size[0], size[1]),
            &items,
            selected,
            popup_input,
            true,
            false,
        ) {
            if let Some(s) = &mut self.session {
                match popup.kind {
                    Kind::Fill => s.paint.fill = Some(choice),
                    Kind::Stroke => s.paint.stroke = Some(choice != 0),
                }
            }
            changed = true;
        }
        list.append(paint);
        self.popup = Some(popup);
        if changed {
            self.preview(out);
        }
        if commit {
            self.close(true, out);
        } else if cancel {
            self.close(false, out);
        }
    }
}

#[cfg(test)]
mod tests;
