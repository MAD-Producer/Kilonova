//! Character and paragraph drafts share one authored style and one transaction.
//! Numeric scrubbing edits only the draft; expensive rasterization is explicit
//! and asynchronous, after Apply. The original layer remains until preflight.
use super::*;
use motion_core::{TextAlign, TextStyle};
use motion_ui::widgets::{NumberEditor, NumberEvent, TextEditor, TextEvent, TextViewport};
use std::path::PathBuf;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Ticket {
    pub object: u64,
    serial: u64,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Size,
    Leading,
    Tracking,
    Stroke,
    Width,
    Left,
    Right,
    First,
    Face,
}
struct Draft {
    ticket: Ticket,
    source: crate::layer_clipboard::KeyframeSource,
    text: TextEditor,
    viewport: TextViewport,
    initial_text: String,
    style: TextStyle,
    initial_style: TextStyle,
    font: String,
    initial_font: String,
    file: Option<PathBuf>,
    face: u32,
    pending: bool,
    message: Option<String>,
}
impl Draft {
    fn dirty(&self) -> bool {
        self.text.text != self.initial_text
            || self.style != self.initial_style
            || self.font != self.initial_font
            || self.file.is_some()
    }
    fn value(&self, field: Field) -> f64 {
        f64::from(match field {
            Field::Size => self.style.font_size,
            Field::Leading => self.style.line_height.unwrap_or(0.),
            Field::Tracking => self.style.tracking,
            Field::Stroke => self.style.stroke_width,
            Field::Width => self.style.width as f32,
            Field::Left => self.style.indent_left,
            Field::Right => self.style.indent_right,
            Field::First => self.style.indent_first,
            Field::Face => self.face as f32,
        })
    }
    fn set(&mut self, field: Field, value: f64) {
        let value = value as f32;
        match field {
            Field::Size => self.style.font_size = value,
            Field::Leading => self.style.line_height = (value != 0.).then_some(value),
            Field::Tracking => self.style.tracking = value,
            Field::Stroke => self.style.stroke_width = value,
            Field::Width => self.style.width = value.round().max(0.) as u32,
            Field::Left => self.style.indent_left = value,
            Field::Right => self.style.indent_right = value,
            Field::First => self.style.indent_first = value,
            Field::Face => self.face = value.round().max(0.) as u32,
        }
        self.message = None;
    }
}
#[derive(Default)]
pub(crate) struct State {
    draft: Option<Draft>,
    numeric: Option<(Field, NumberEditor)>,
    font: Dropdown,
    alignment: Dropdown,
    owner: Option<Panel>,
    serial: u64,
    invalid: bool,
    #[cfg(test)]
    pub(crate) fields: Vec<Rect>,
    #[cfg(test)]
    pub(crate) apply_rect: Option<Rect>,
    source_rect: Option<Rect>,
    #[cfg(test)]
    font_rect: Option<Rect>,
    #[cfg(test)]
    alignment_rect: Option<Rect>,
    #[cfg(test)]
    cancel_rect: Option<Rect>,
}
impl State {
    pub(super) fn owner(&self) -> Option<Panel> {
        (self.numeric.is_some()
            || self.font.open
            || self.alignment.open
            || self
                .draft
                .as_ref()
                .is_some_and(|d| d.text.active() || d.pending))
        .then_some(self.owner)
        .flatten()
    }
    pub(super) fn text_menu(&mut self, request: &mut super::super::text_menu::Request<'_>) {
        if let Some((_, field)) = &mut self.numeric {
            request.visit(&mut field.text);
        }
        if let Some(draft) = &mut self.draft {
            request.visit(&mut draft.text);
        }
    }
    pub(super) fn cancel_field(&mut self) {
        if let Some((field, mut editor)) = self.numeric.take() {
            for event in editor.cancel() {
                if let NumberEvent::Cancel { value } = event {
                    if let Some(draft) = &mut self.draft {
                        draft.set(field, value);
                    }
                }
            }
        }
        if let Some(draft) = &mut self.draft {
            draft.text.cancel();
        }
        self.font.open = false;
        self.alignment.open = false;
    }
    fn refresh(&mut self, layer: &Value, state: &Value) -> Result<(), String> {
        let object = layer["id"].as_u64().ok_or("text layer has no object ID")?;
        let source = crate::layer_clipboard::KeyframeSource::of(state);
        if self.draft.as_ref().is_some_and(|d| {
            d.pending
                || d.dirty()
                || d.ticket.object == object && (d.source == source || self.owner().is_some())
        }) {
            return Ok(());
        }
        let style: TextStyle =
            serde_json::from_value(layer["content"]["style"].clone()).map_err(|e| e.to_string())?;
        style.validate().map_err(|e| e.to_string())?;
        self.cancel_field();
        self.serial = self
            .serial
            .checked_add(1)
            .ok_or("text draft identity exhausted")?;
        let text = layer["content"]["text"]
            .as_str()
            .ok_or("text content missing")?
            .to_owned();
        let font = layer["content"]["font"]
            .as_str()
            .ok_or("text font missing")?
            .to_owned();
        let mut editor = TextEditor::default();
        editor.text = text.clone();
        self.draft = Some(Draft {
            ticket: Ticket {
                object,
                serial: self.serial,
            },
            source,
            text: editor,
            viewport: TextViewport::default(),
            initial_text: text,
            initial_style: style.clone(),
            style,
            initial_font: font.clone(),
            font,
            file: None,
            face: 0,
            pending: false,
            message: None,
        });
        Ok(())
    }
    pub(crate) fn choose_font(&mut self, ticket: &Ticket, file: PathBuf) {
        if let Some(draft) = self
            .draft
            .as_mut()
            .filter(|d| &d.ticket == ticket && !d.pending)
        {
            draft.file = Some(file);
            draft.face = 0;
            draft.message = None;
        }
    }
    pub(crate) fn response(
        &mut self,
        ticket: &Ticket,
        result: &Result<Value, String>,
        locale: Locale,
    ) {
        if !self.draft.as_ref().is_some_and(|d| &d.ticket == ticket) {
            return;
        }
        match result {
            Ok(state) if state["textEdit"]["cancelled"] != true => {
                self.cancel_field();
                self.draft = None;
            }
            _ => {
                let draft = self.draft.as_mut().expect("matched draft");
                draft.pending = false;
                draft.message = Some(match result {
                    Err(cause) => locale.error(cause),
                    Ok(_) => localized(
                        locale,
                        "工程已改变，草稿未应用；取消后重新载入。",
                        "Document changed; draft was not applied. Cancel to reload.",
                    )
                    .into(),
                });
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    panel: Panel,
    out: &mut Vec<Action>,
) {
    let locale = panels.locale;
    let t = |zh, en| localized(locale, zh, en);
    let styles = list.styles();
    let pad = styles.metric("padding.panel", 8.);
    let row = styles.metric("row.height", 22.);
    let control = styles.metric("auxiliary.control.height", 26.).max(row);
    let gap = styles.metric("auxiliary.gap", 6.);
    let field_height = styles.metric("field.height", 18.);
    let source_height = styles.metric("text.style.source.height", 88.);
    let action_width = styles.metric("text.style.action.width", 80.);
    if input.focus_lost {
        // Suspend this panel's field/popup ownership without discarding its
        // text or layout draft when another native window receives focus.
        panels.auxiliary.text_style.cancel_field();
    }
    let layer = selected(panels, state).cloned();
    let Some(layer) = layer.filter(|l| l["content"]["kind"] == "text") else {
        panels.auxiliary.text_style.cancel_field();
        panels.auxiliary.text_style.draft = None;
        let mut y = area.min[1] + pad;
        line(
            list,
            atlas,
            area,
            &mut y,
            t("选择文字图层", "Select a text layer"),
            true,
        );
        if panel == Panel::Character {
            button(
                list,
                atlas,
                Rect::new(
                    area.min[0] + pad,
                    y,
                    (area.width() - pad * 2.).max(0.),
                    control,
                ),
                locale.t(T::NewText),
                input,
                true,
                Action::NewText,
                out,
            );
        }
        return;
    };
    // Keep ownership while borrowing the other auxiliary controls (fill popup).
    let mut controls = std::mem::take(&mut panels.auxiliary.text_style);
    if let Err(cause) = controls.refresh(&layer, state) {
        if !controls.invalid {
            log::error!(
                "text panel operation=read_style object={} cause={}",
                layer["id"],
                crate::fallback_observation::error_cause(&cause)
            );
        }
        controls.invalid = true;
        let mut y = area.min[1] + pad;
        line(list, atlas, area, &mut y, &locale.error(&cause), false);
        panels.auxiliary.text_style = controls;
        return;
    }
    controls.invalid = false;
    #[cfg(test)]
    {
        controls.fields.clear();
        controls.apply_rect = None;
    }
    let draft = controls.draft.as_ref().expect("refreshed draft");
    // Include the shared line() helper's control height in the scroll extent.
    // Feedback always occupies one row so errors never move Apply or Cancel.
    let height = pad * 2.
        + if panel == Panel::Character {
            row * (8. + if draft.file.is_some() { 2. } else { 0. })
                + control * 2.
                + source_height
                + gap * 2.
        } else {
            row * 7. + control
        };
    let neutral = Input::default();
    let scroll_input = if controls.font.open
        || controls.alignment.open
        || panel == Panel::Character
            && draft.text.active()
            && controls
                .source_rect
                .is_some_and(|r| r.contains(input.mouse))
    {
        &neutral
    } else {
        input
    };
    let mut y = body_y(&styles, panel, area, height, panels, scroll_input);
    let x = area.min[0] + pad;
    let width = (area.width() - pad * 2.).max(0.);
    let caption = styles.metric("label.column", 72.).min(width * 0.45);
    let value_rect = |y| {
        Rect::new(
            x + caption,
            y + (row - field_height) * 0.5,
            (width - caption).max(0.),
            field_height,
        )
    };
    let current = crate::layer_clipboard::KeyframeSource::of(state);
    let draft = controls.draft.as_mut().expect("refreshed draft");
    let stale = draft.source != current || Some(draft.ticket.object) != panels.selected;
    let enabled = !stale && !draft.pending && layer["locked"] != true && state["observing"] != true;
    let popup_input = if enabled { input } else { &neutral };
    let input = if controls.font.open || controls.alignment.open {
        &neutral
    } else {
        popup_input
    };
    label(
        list,
        atlas,
        Rect::new(x, y, width, row),
        &format!("{} · #{}", t("文字草稿", "Text draft"), draft.ticket.object),
        true,
    );
    y += row;
    let mut font_rect = None;
    let mut align_rect = None;
    let mut fonts: Vec<(String, String)> = state["project"]["fonts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|font| Some((font["id"].as_str()?.into(), font["name"].as_str()?.into())))
        .collect();
    let builtin = motion_media::fonts::FontStore::builtin_id();
    if !fonts.iter().any(|(id, _)| id == &builtin) {
        fonts.push((builtin, "IBM Plex Mono".into()));
    }
    if !fonts.iter().any(|(id, _)| id == &draft.font) {
        fonts.push((
            draft.font.clone(),
            format!("{}: {}", t("字体不可用", "Unavailable font"), draft.font),
        ));
    }
    if panel == Panel::Character {
        label(
            list,
            atlas,
            Rect::new(x, y, caption, row),
            t("字体", "Font"),
            true,
        );
        font_rect = Some(value_rect(y));
        y += row;
        if let Some(path) = &draft.file {
            label(
                list,
                atlas,
                Rect::new(x, y, width, row),
                path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(t("字体文件", "Font file")),
                true,
            );
            y += row;
        }
    } else {
        label(
            list,
            atlas,
            Rect::new(x, y, caption, row),
            t("对齐", "Alignment"),
            true,
        );
        align_rect = Some(value_rect(y));
        y += row;
    }
    let fields: Vec<(Field, &str)> = if panel == Panel::Character {
        let mut fields = vec![
            (Field::Size, t("字号 px", "Size px")),
            (Field::Leading, t("行距 px", "Leading px")),
            (Field::Tracking, t("字距 px", "Tracking px")),
            (Field::Stroke, t("描边 px", "Outline px")),
        ];
        if draft.file.is_some() {
            fields.push((Field::Face, t("字体索引", "Face index")));
        }
        fields
    } else {
        vec![
            (Field::Width, t("宽度 px", "Width px")),
            (Field::Left, t("左缩进 px", "Left px")),
            (Field::Right, t("右缩进 px", "Right px")),
            (Field::First, t("首行 px", "First line px")),
        ]
    };
    for (field, title) in fields {
        let rect = value_rect(y);
        label(list, atlas, Rect::new(x, y, caption, row), title, true);
        y += row;
        #[cfg(test)]
        {
            controls.fields.push(rect);
        }
        if input
            .pressed
            .is_some_and(|(b, p)| b == MouseButton::Left && rect.contains(p))
            && controls
                .numeric
                .as_ref()
                .is_none_or(|(old, _)| *old != field)
        {
            draft.text.cancel();
            controls.numeric = Some((field, NumberEditor::default()));
            controls.owner = Some(panel);
        }
        if let Some((active, editor)) = controls
            .numeric
            .as_mut()
            .filter(|(active, _)| *active == field)
        {
            let options = NumberOptions {
                speed: 1.,
                min: match field {
                    Field::Size => 4.,
                    Field::Width => 1.,
                    Field::Face => 0.,
                    _ => f64::NEG_INFINITY,
                },
                max: match field {
                    Field::Size => 256.,
                    Field::Width => 8192.,
                    Field::Face => 255.,
                    _ => f64::INFINITY,
                },
                decimals: if matches!(field, Field::Width | Field::Face) {
                    0
                } else {
                    2
                },
            };
            editor.drag_threshold = Some(styles.metric("motion.drag_threshold", 3.));
            let typed = editor.text.active().then(|| editor.text.clone());
            let events = editor.update(draft.value(*active), options, input, rect);
            let invalid_integer = matches!(field, Field::Width | Field::Face)
                && typed.is_some()
                && events.iter().any(|event| {
                    matches!(event,
                    NumberEvent::Change {value} | NumberEvent::Commit {value} if value.fract()!=0.)
                });
            if invalid_integer {
                // Retain the entered value and focus until corrected or cancelled.
                // Pointer scrubbing rounds discrete fields, like their readout.
                editor.text = typed.expect("typed integer");
                // TextEditor returns Commit without closing; NumberEditor normally
                // closes it. Replay this input on the retained editor to preserve
                // fresh text, the selection and local undo when validation rejects.
                let _ = editor.text.input(input);
                let cause = t("此字段必须为整数", "This field requires an integer");
                editor.text.error = Some(cause.into());
                draft.message = Some(cause.into());
            } else {
                for event in events {
                    match event {
                        NumberEvent::Change { value }
                        | NumberEvent::Commit { value }
                        | NumberEvent::Cancel { value } => {
                            draft.set(field, value);
                        }
                        NumberEvent::Begin { .. } => {}
                    }
                }
            }
            editor.draw(list, atlas, rect, draft.value(*active), options);
            if !editor.interacting() {
                controls.numeric = None;
            }
        } else {
            numeric::passive_with_precision(
                list,
                atlas,
                rect,
                draft.value(field),
                input,
                enabled,
                if matches!(field, Field::Width | Field::Face) {
                    0
                } else {
                    2
                },
            );
        }
    }
    if panel == Panel::Character {
        line(
            list,
            atlas,
            area,
            &mut y,
            t(
                "行距 0 为自动；描边同填色、方角。",
                "0 = auto; outline uses fill",
            ),
            true,
        );
        // Existing fill transaction and popup remain independent of layout.
        let swatch = Rect::new(
            x + caption,
            y + (row - field_height) * 0.5,
            field_height,
            field_height,
        );
        label(
            list,
            atlas,
            Rect::new(x, y, caption, row),
            t("填色", "Fill"),
            true,
        );
        let color: [f32; 4] =
            std::array::from_fn(|i| layer["content"]["color"][i].as_f64().unwrap_or(1.) as f32);
        color_well::swatch(list, swatch, &color, input, enabled && !draft.dirty());
        let color_caption = Rect::new(
            swatch.max[0] + gap,
            y,
            (area.max[0] - pad - swatch.max[0] - gap).max(0.),
            row,
        );
        if enabled && !draft.dirty() && (clicked(input, swatch) || clicked(input, color_caption)) {
            panels.auxiliary.color_popup = Some(ColorPopup {
                object: draft.ticket.object,
                anchor: swatch,
                opened: true,
            });
        }
        ui::text(
            list,
            atlas,
            color_caption,
            &color_well::caption(&color),
            styles.color(if enabled && !draft.dirty() {
                "focus"
            } else {
                "disabled"
            }),
            Align::Left,
        );
        y += row + gap;
        let source_rect = Rect::new(x, y, width, source_height);
        y += source_height + gap;
        controls.source_rect = Some(source_rect);
        if input
            .pressed
            .is_some_and(|(b, p)| b == MouseButton::Left && source_rect.contains(p))
            && !draft.text.active()
        {
            controls.numeric = None;
            let text = draft.text.text.clone();
            draft.text.begin_multiline(&text, 16384);
            controls.owner = Some(panel);
        }
        if draft.text.active() {
            if matches!(
                draft
                    .text
                    .multiline_view(&mut draft.viewport, list, atlas, source_rect, input),
                Some(TextEvent::Commit(_))
            ) {
                draft.text.cancel();
            }
        } else {
            let style = styles
                .resolve("field.number", States::default())
                .expect("required number field style");
            for (index, text) in draft
                .text
                .text
                .lines()
                .take((source_height / row) as usize)
                .enumerate()
            {
                text_style(
                    list,
                    atlas,
                    Rect::new(
                        source_rect.min[0],
                        source_rect.min[1] + index as f32 * row,
                        width,
                        row,
                    ),
                    text,
                    style,
                    Align::Left,
                );
            }
        }
    }
    if stale {
        draft.message = Some(
            t(
                "工程或选择已改变；取消草稿以重新载入。",
                "Document or selection changed; cancel draft to reload.",
            )
            .into(),
        );
    }
    let status = if draft.pending {
        t("正在排版…", "Laying out text…")
    } else {
        draft
            .message
            .as_deref()
            .unwrap_or(t("应用前仅修改草稿", "Edits stay in the draft until Apply"))
    };
    line(list, atlas, area, &mut y, status, draft.message.is_none());
    let apply = Rect::new(x, y, action_width.min(width), row);
    let cancel = Rect::new(
        (apply.max[0] + gap).min(area.max[0] - pad),
        y,
        (width - action_width - gap).min(action_width).max(0.),
        row,
    );
    #[cfg(test)]
    {
        controls.apply_rect = Some(apply);
    }
    #[cfg(test)]
    {
        controls.cancel_rect = Some(cancel);
        controls.font_rect = font_rect;
        controls.alignment_rect = align_rect;
    }
    let can_apply = enabled
        && draft.dirty()
        && controls.numeric.is_none()
        && !input.composing
        && !input.ime_consumed;
    let apply_pressed =
        ui::button(list, atlas, apply, t("应用", "Apply"), input, can_apply).clicked;
    let cancel_pressed = ui::button(
        list,
        atlas,
        cancel,
        t("取消草稿", "Cancel draft"),
        input,
        !draft.pending,
    )
    .clicked;
    if apply_pressed {
        let validation = draft
            .style
            .validate()
            .map_err(|e| e.to_string())
            .and_then(|_| {
                if draft.text.text.chars().count() > 4096 {
                    Err(t("文字不能超过 4096 字符", "Text exceeds 4096 characters").into())
                } else {
                    Ok(())
                }
            });
        match validation {
            Err(cause) => draft.message = Some(locale.error(&cause)),
            Ok(()) => {
                let mut request = serde_json::json!({"op":"font_edit_text","object":draft.ticket.object,"text":draft.text.text,
                "style":draft.style,"font":draft.font,"revision":state["revision"]});
                if let Some(path) = &draft.file {
                    request["font_path"] = path.to_string_lossy().as_ref().into();
                    request["face_index"] = draft.face.into();
                }
                draft.pending = true;
                draft.text.cancel();
                controls.owner = Some(panel);
                out.push(Action::TextApply {
                    ticket: draft.ticket.clone(),
                    source: draft.source.clone(),
                    request,
                });
            }
        }
    }
    let import_index = fonts.len();
    if let Some(rect) = font_rect {
        let mut items: Vec<_> = fonts
            .iter()
            .map(|(_, name)| DropdownItem {
                label: name.as_str(),
                enabled: true,
            })
            .collect();
        let file_label = draft
            .file
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|p| p.to_str())
            .map(str::to_owned);
        if let Some(label) = file_label.as_deref() {
            items.push(DropdownItem {
                label,
                enabled: true,
            });
        }
        items.push(DropdownItem {
            label: t("选择字体文件…", "Choose font file…"),
            enabled: true,
        });
        let selected = if draft.file.is_some() {
            import_index
        } else {
            fonts
                .iter()
                .position(|(id, _)| id == &draft.font)
                .unwrap_or(0)
        };
        if let Some(index) = controls.font.draw(
            list,
            atlas,
            rect,
            area,
            &items,
            selected,
            popup_input,
            enabled,
            false,
        ) {
            if index < fonts.len() {
                draft.font = fonts[index].0.clone();
                draft.file = None;
            } else if index == items.len() - 1 {
                out.push(Action::TextChooseFont {
                    ticket: draft.ticket.clone(),
                });
            }
            controls.owner = Some(panel);
            draft.text.cancel();
            controls.numeric = None;
        }
        if controls.font.open {
            controls.owner = Some(panel);
            draft.text.cancel();
            controls.numeric = None;
        }
    }
    if let Some(rect) = align_rect {
        let items = [
            t("左对齐", "Left"),
            t("居中", "Center"),
            t("右对齐", "Right"),
        ]
        .map(|label| DropdownItem {
            label,
            enabled: true,
        });
        let selected = match draft.style.alignment {
            TextAlign::Left => 0,
            TextAlign::Center => 1,
            TextAlign::Right => 2,
        };
        if let Some(index) = controls.alignment.draw(
            list,
            atlas,
            rect,
            area,
            &items,
            selected,
            popup_input,
            enabled,
            false,
        ) {
            draft.style.alignment = [TextAlign::Left, TextAlign::Center, TextAlign::Right][index];
            controls.numeric = None;
            draft.text.cancel();
        }
        if controls.alignment.open {
            controls.owner = Some(panel);
            controls.numeric = None;
            draft.text.cancel();
        }
    }
    if cancel_pressed {
        controls.cancel_field();
        controls.draft = None;
    }
    panels.auxiliary.text_style = controls;
}
