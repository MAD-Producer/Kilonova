//! Scoped timeline menus and name editing. Opening a menu never edits a project.
use super::*;
use motion_ui::style::{box_style, text_style, State, States};
use motion_ui::widgets::{TextEditor, TextEvent};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Root,
    Switch,
    Order,
    Time,
    Blend,
}
enum Choice {
    Edit(Command),
    Rename,
    Precompose,
    Page(Page),
    New(Action),
}
struct Entry {
    label: String,
    enabled: bool,
    disabled_reason: Option<String>,
    choice: Choice,
    divider_after: bool,
}
struct Popup {
    target: Option<u64>,
    anchor: [f32; 2],
    owner: Option<Panel>,
    context: (String, String),
    page: Page,
    highlighted: usize,
    scroll: usize,
    opened: bool,
}
struct NameEdit {
    object: u64,
    original: String,
    context: (String, String),
    owner: Option<Panel>,
    editor: TextEditor,
    rect: Rect,
    precompose: bool,
}
#[derive(Default)]
pub(crate) struct Controls {
    popup: Option<Popup>,
    name: Option<NameEdit>,
    owner: Option<Panel>,
    last_click: Option<(u64, Instant, [f32; 2])>,
}
fn context(state: &Value) -> (String, String) {
    (
        state["root"].as_str().unwrap_or("").into(),
        state["composition"]
            .as_str()
            .unwrap_or(
                state["project"]["composition_id"]
                    .as_str()
                    .unwrap_or("comp-main"),
            )
            .into(),
    )
}
fn layer(state: &Value, id: u64) -> Option<&Value> {
    state["project"]["layers"]
        .as_array()?
        .iter()
        .find(|l| l["id"].as_u64() == Some(id))
}
fn word(locale: Locale, zh: &str, en: &str) -> String {
    if locale == Locale::Zh { zh } else { en }.into()
}
fn scoped(command: Command, state: &Value) -> Command {
    Command::InComposition {
        composition: context(state).1,
        command: Box::new(command),
    }
}
fn frame(state: &Value) -> u32 {
    state["frame"]
        .as_f64()
        .unwrap_or(0.)
        .floor()
        .clamp(0., last_frame(state)) as u32
}

fn popup_rect(anchor: [f32; 2], bounds: Rect, width: f32, height: f32) -> Rect {
    Rect::new(
        anchor[0].clamp(bounds.min[0], (bounds.max[0] - width).max(bounds.min[0])),
        (anchor[1] - height).clamp(bounds.min[1], (bounds.max[1] - height).max(bounds.min[1])),
        width,
        height,
    )
}

fn submenu_arrow(list: &mut PaintList, area: Rect, color: motion_ui::theme::Color) {
    let x = area.max[0] - 11.;
    let y = area.min[1] + area.height() * 0.5;
    paint::line(
        list,
        vec![[x - 2.5, y - 4.], [x + 1.5, y], [x - 2.5, y + 4.]],
        color.with_alpha(0.82),
    );
}

impl Controls {
    pub fn editing(&self) -> bool {
        self.popup.is_some() || self.name.is_some()
    }
    pub fn modal_in(&self, owner: Option<Panel>) -> bool {
        self.popup.as_ref().is_some_and(|p| p.owner == owner)
            || self
                .name
                .as_ref()
                .is_some_and(|n| n.precompose && n.owner == owner)
    }
    pub fn text_in(&self, owner: Option<Panel>) -> bool {
        self.name.as_ref().is_some_and(|n| n.owner == owner)
    }
    pub fn cancel(&mut self) {
        self.popup = None;
        self.name = None;
        self.last_click = None;
    }
    pub fn open(&mut self, target: Option<u64>, anchor: [f32; 2], state: &Value) {
        self.popup = Some(Popup {
            target,
            anchor,
            owner: self.owner,
            context: context(state),
            page: Page::Root,
            highlighted: 0,
            scroll: 0,
            opened: true,
        });
    }
    fn begin_name(&mut self, id: u64, precompose: bool, state: &Value, locale: Locale) {
        let Some(l) = layer(state, id).filter(|l| l["locked"] != true) else {
            return;
        };
        let original = l["name"].as_str().unwrap_or("").to_string();
        let mut editor = TextEditor::default();
        editor.begin(&if precompose {
            format!("{} {}", original, word(locale, "预合成", "Precomp"))
        } else {
            original.clone()
        });
        self.name = Some(NameEdit {
            object: id,
            original,
            context: context(state),
            owner: self.owner,
            editor,
            rect: Rect::default(),
            precompose,
        });
    }
    fn finish_name(&mut self, state: &Value, locale: Locale, out: &mut Vec<Action>) {
        let Some(edit) = &mut self.name else { return };
        let value = edit.editor.text.trim().to_string();
        if value.is_empty() || value.len() > 1024 {
            edit.editor.error = Some(word(
                locale,
                "名称不能为空，且不能超过 1024 字节",
                "Name must be nonempty and at most 1024 bytes",
            ));
            return;
        }
        let edit = self.name.take().unwrap();
        let command = if edit.precompose {
            Command::Composition {
                action: motion_core::CompositionAction::Precompose {
                    objects: vec![edit.object],
                    name: value,
                    range: "composition".into(),
                },
            }
        } else {
            if value == edit.original {
                return;
            }
            Command::Rename {
                object: edit.object,
                name: value,
            }
        };
        out.push(Action::Edit(scoped(command, state)));
    }
    /// Process text before panel focus changes, so an outside click commits it.
    /// The closing frame remains owned by a menu/dialog and cannot delete a layer.
    pub fn prepare(
        &mut self,
        owner: Option<Panel>,
        state: &Value,
        input: &Input,
        locale: Locale,
        selected: Option<u64>,
        timeline_focused: bool,
        out: &mut Vec<Action>,
    ) -> bool {
        self.owner = owner;
        let was_modal = self.modal_in(owner);
        let was_text = self.text_in(owner);
        if self.popup.as_ref().is_some_and(|p| {
            p.context != context(state)
                || p.target.is_some_and(|id| {
                    if id == 0 {
                        state["project"]["camera"]["created"] != true
                    } else {
                        layer(state, id).is_none()
                    }
                })
        }) {
            self.popup = None;
        }
        if self.popup.as_ref().is_some_and(|p| p.owner != owner) && input.pressed.is_some() {
            self.popup = None;
        }
        if let Some(edit) = &self.name {
            if edit.context != context(state)
                || layer(state, edit.object).is_none_or(|l| {
                    l["locked"] == true || l["name"].as_str() != Some(&edit.original)
                })
            {
                self.name = None;
            }
        }
        if let Some(edit) = &mut self.name {
            let owned = edit.owner == owner;
            let outside = input
                .pressed
                .is_some_and(|(_, p)| !owned || !edit.rect.contains(p));
            if owned && input.focus_lost {
                self.name = None;
                return true;
            }
            if owned {
                let mut text_input = input.clone();
                if input.composing || input.ime_consumed {
                    text_input.keys.clear();
                }
                match edit.editor.input(&text_input) {
                    Some(TextEvent::Cancel) => {
                        self.name = None;
                        return true;
                    }
                    Some(TextEvent::Commit(_)) => {
                        self.finish_name(state, locale, out);
                        return true;
                    }
                    None => {}
                }
            }
            if outside
                && !input.composing
                && !input.ime_consumed
                && self.name.as_ref().is_some_and(|n| !n.precompose)
            {
                self.finish_name(state, locale, out);
            }
        } else if timeline_focused
            && !was_modal
            && !input.composing
            && !input.ime_consumed
            && input
                .key_presses()
                .any(|(k, m, r)| k == Key::Enter && !r && !m.control && !m.alt && !m.shift)
        {
            if let Some(id) = selected {
                self.begin_name(id, false, state, locale);
                return true;
            }
        }
        was_modal || was_text || self.modal_in(owner)
    }
    pub fn row(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        name: Rect,
        row: Rect,
        id: u64,
        state: &Value,
        input: &Input,
        locale: Locale,
    ) -> bool {
        if right_clicked(input, row) {
            self.open(Some(id), input.mouse, state);
        }
        if clicked(input, name) && id != 0 {
            let now = Instant::now();
            let double = self.last_click.as_ref().is_some_and(|(old, time, p)| {
                *old == id
                    && now.duration_since(*time) < Duration::from_millis(400)
                    && ((p[0] - input.mouse[0])
                        .abs()
                        .max((p[1] - input.mouse[1]).abs())
                        < 4.)
            });
            self.last_click = Some((id, now, input.mouse));
            if double {
                self.begin_name(id, false, state, locale);
                self.last_click = None;
            }
        }
        if let Some(edit) = self
            .name
            .as_mut()
            .filter(|e| e.object == id && !e.precompose && e.owner == self.owner)
        {
            edit.rect = name;
            edit.editor.draw(list, atlas, name);
            return true;
        }
        false
    }
    pub fn finish_window(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        bounds: Rect,
        state: &Value,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        if self
            .name
            .as_ref()
            .is_some_and(|n| n.precompose && n.owner == self.owner)
        {
            let width = 360f32.min(bounds.width());
            let card = Rect::new(
                bounds.min[0] + (bounds.width() - width) * 0.5,
                bounds.min[1] + (bounds.height() - 142.).max(0.) * 0.5,
                width,
                142f32.min(bounds.height()),
            );
            let colors = list.styles().colors();
            paint::rect(list, card, colors.CHROME);
            paint::stroke(list, card, colors.BORDER, 1.);
            label(
                list,
                atlas,
                Rect::new(card.min[0] + 12., card.min[1] + 8., width - 24., 24.),
                &word(locale, "预合成名称", "Precomposition name"),
                false,
            );
            let edit = self.name.as_mut().unwrap();
            edit.rect = Rect::new(card.min[0] + 12., card.min[1] + 40., width - 24., 28.);
            edit.editor.draw(list, atlas, edit.rect);
            if ui::button(
                list,
                atlas,
                Rect::new(card.max[0] - 170., card.min[1] + 91., 76., 26.),
                &word(locale, "取消", "Cancel"),
                input,
                true,
            )
            .clicked
            {
                self.name = None;
            }
            if ui::button(
                list,
                atlas,
                Rect::new(card.max[0] - 88., card.min[1] + 91., 76., 26.),
                &word(locale, "确定", "Create"),
                input,
                self.name.is_some(),
            )
            .clicked
            {
                self.finish_name(state, locale, out);
            }
        }
        let Some(popup) = self.popup.as_mut().filter(|p| p.owner == self.owner) else {
            return;
        };
        let entries = entries(popup, state, locale);
        if entries.is_empty() {
            self.popup = None;
            return;
        }
        let styles = list.styles();
        let row = styles.metric("context.row.height", 20.).clamp(18., 24.);
        let count = entries
            .len()
            .min(((bounds.height() - 8.).max(row) / row).floor() as usize)
            .max(1);
        popup.scroll = popup.scroll.min(entries.len() - count);
        popup.highlighted = popup.highlighted.min(entries.len() - 1);
        let width = 220f32.min(bounds.width());
        let height = count as f32 * row + 6.;
        // Context menus grow upward from the pointer. Near a window
        // edge we clamp the card while keeping the click as close as possible
        // to its lower-left corner.
        let card = popup_rect(popup.anchor, bounds, width, height);
        let neutral = Input::default();
        let active = if popup.opened { &neutral } else { input };
        popup.opened = false;
        if active.focus_lost
            || active.keys.iter().any(|(k, _)| *k == Key::Escape)
            || active.pressed.is_some_and(|(_, p)| !card.contains(p))
        {
            self.popup = None;
            return;
        }
        if card.contains(active.mouse) && active.wheel[1] != 0. {
            popup.scroll = (popup.scroll as i32 + if active.wheel[1] > 0. { 1 } else { -1 })
                .clamp(0, (entries.len() - count) as i32) as usize;
        }
        for (k, m, _) in active
            .key_presses()
            .filter(|(_, m, _)| !m.control && !m.alt && !m.shift)
        {
            let _ = m;
            if matches!(k, Key::Up | Key::Down) {
                for _ in 0..entries.len() {
                    popup.highlighted = if k == Key::Down {
                        (popup.highlighted + 1) % entries.len()
                    } else {
                        (popup.highlighted + entries.len() - 1) % entries.len()
                    };
                    if entries[popup.highlighted].enabled {
                        break;
                    }
                }
                popup.scroll = popup
                    .scroll
                    .min(popup.highlighted)
                    .max(popup.highlighted.saturating_sub(count - 1));
            }
        }
        box_style(
            list,
            card,
            styles.resolve("context_menu", States::default()).unwrap(),
        );
        let mut chosen = None;
        let mut hint = None;
        for (index, e) in entries.iter().enumerate().skip(popup.scroll).take(count) {
            let area = Rect::new(
                card.min[0] + 3.,
                card.min[1] + 3. + (index - popup.scroll) as f32 * row,
                width - 6.,
                row,
            );
            if area.contains(active.mouse) && e.enabled {
                popup.highlighted = index;
            }
            if area.contains(active.mouse) && !e.enabled {
                hint = e.disabled_reason.as_deref();
            }
            let mut states = States::default();
            if index == popup.highlighted && e.enabled {
                states = states.with(State::Selected);
            }
            if !e.enabled {
                states = states.with(State::Disabled);
            }
            let style = styles.resolve("context_menu.row", states).unwrap();
            box_style(list, area, style);
            text_style(
                list,
                atlas,
                Rect::new(
                    area.min[0] + 5.,
                    area.min[1],
                    area.width() - 28.,
                    area.height(),
                ),
                &e.label,
                style,
                Align::Left,
            );
            if matches!(e.choice, Choice::Page(_)) {
                submenu_arrow(list, area, style.foreground);
            }
            if e.divider_after {
                paint::line(
                    list,
                    vec![
                        [area.min[0] + 7., area.max[1] - 1.],
                        [area.max[0] - 7., area.max[1] - 1.],
                    ],
                    styles.color("context.border"),
                );
            }
            if e.enabled && clicked(active, area) {
                chosen = Some(index);
            }
        }
        if let Some(reason) = hint {
            disabled_hint(list, atlas, bounds, active.mouse, reason);
        }
        if !active.composing
            && !active.ime_consumed
            && active
                .key_presses()
                .any(|(k, m, r)| k == Key::Enter && !r && !m.control && !m.alt)
            && entries[popup.highlighted].enabled
        {
            chosen = Some(popup.highlighted);
        }
        let Some(index) = chosen else { return };
        let chosen = entries.into_iter().nth(index).unwrap();
        if let Choice::Page(page) = chosen.choice {
            let p = self.popup.as_mut().unwrap();
            p.page = page;
            p.highlighted = 0;
            p.scroll = 0;
            return;
        }
        let target = self.popup.take().unwrap().target;
        match chosen.choice {
            Choice::Edit(command) => out.push(Action::Edit(scoped(command, state))),
            Choice::New(action) => out.push(action),
            Choice::Rename => {
                if let Some(id) = target {
                    self.begin_name(id, false, state, locale)
                }
            }
            Choice::Precompose => {
                if let Some(id) = target {
                    self.begin_name(id, true, state, locale)
                }
            }
            Choice::Page(_) => {}
        }
    }
}

fn disabled_hint(list: &mut PaintList, atlas: &mut TextAtlas, bounds: Rect, point: [f32; 2], reason: &str) {
    let width = (atlas.measure(reason) + 12.).min((bounds.width() - 8.).max(1.));
    let mut lines = Vec::new();
    let mut line = String::new();
    for character in reason.chars() {
        let mut next = line.clone();
        next.push(character);
        if !line.is_empty() && atlas.measure(&next) > (width - 12.).max(1.) {
            lines.push(std::mem::take(&mut line));
        }
        line.push(character);
    }
    if !line.is_empty() { lines.push(line); }
    let line_height = atlas.line_height.max(1.);
    let height = (lines.len() as f32 * line_height + 8.).min(bounds.height().max(1.));
    let card = popup_rect([point[0] + 12., point[1] - 8.], bounds, width, height);
    let style = list.styles().resolve("tooltip", States::default()).unwrap();
    let mut content = list.child();
    box_style(&mut content, card, style);
    for (index, line) in lines.iter().enumerate() {
        ui::text(&mut content, atlas,
            Rect::new(card.min[0] + 6., card.min[1] + 4. + index as f32 * line_height,
                (card.width() - 12.).max(0.), line_height), line, style.foreground, Align::Left);
    }
    content.clip_to(bounds.intersection(card));
    list.append(content);
}

fn disabled_reason(choice: &Choice, layer: &Value, state: &Value, locale: Locale) -> String {
    if layer["locked"] == true { return word(locale, "图层已锁定，请先解锁", "Unlock this layer before editing it"); }
    match choice {
        Choice::Page(Page::Blend) => word(locale, "音频图层不支持画面混合模式", "Audio layers do not support visual blend modes"),
        Choice::Precompose if layer["three_d"] == true => word(locale, "预合成暂不支持三维图层", "Precompose does not yet support 3D layers"),
        Choice::Precompose if state["project"]["expressions"].as_array().into_iter().flatten().any(|e|
            e["enabled"] != false && e["source"].as_str().is_some_and(|code| code.contains("index"))) =>
            word(locale, "请先处理使用图层索引的表达式", "Resolve expressions using layer indices before precomposing"),
        Choice::Precompose => word(locale, "请先解除父级、遮罩或跨图层效果依赖", "Resolve parent, matte or cross-layer effect dependencies first"),
        Choice::Edit(Command::Remove { .. }) => word(locale, "其他图层正在使用此图层作为轨道遮罩", "Another layer uses this layer as its track matte"),
        Choice::Edit(Command::Reorder { index: 0, .. }) => word(locale, "图层已经位于最底部", "This layer is already at the bottom"),
        Choice::Edit(Command::Reorder { .. }) => word(locale, "图层已经位于最顶部", "This layer is already at the top"),
        Choice::Edit(Command::SplitLayerClip { .. }) => word(locale, "播放头必须位于图层入点和出点之间", "Place the playhead strictly between the layer's in and out points"),
        Choice::Edit(Command::MoveLayerClip { .. }) => word(locale, "片段必须对齐整帧，且移动后不能超出时间范围", "Align the clip to whole frames and keep it within the timeline limits"),
        Choice::Edit(Command::SetLayerRange { .. }) => word(locale, "播放头必须位于当前片段内，并保留有效长度", "Keep the playhead inside the clip and retain a valid duration"),
        _ => word(locale, "当前图层状态不支持此操作", "This operation is unavailable for the current layer state"),
    }
}

fn entries(popup: &Popup, state: &Value, locale: Locale) -> Vec<Entry> {
    let mut result = vec![];
    let mut divider_indices = vec![];
    let mut add = |zh: &str, en: &str, enabled: bool, choice: Choice| {
        let index = result.len();
        result.push(Entry {
            label: word(locale, zh, en),
            enabled,
            disabled_reason: None,
            choice,
            divider_after: false,
        });
        index
    };
    let Some(id) = popup.target else {
        for (zh, en, action) in [
            ("新建纯色", "New Solid", Action::AddSolid),
            ("新建文字", "New Text", Action::NewText),
            ("新建空对象", "New Null", Action::AddNull),
            ("新建调整图层", "New Adjustment", Action::AddAdjustment),
            (
                "新建摄影机",
                "New Camera",
                Action::Edit(Command::CreateCamera),
            ),
            (
                "新建矩形",
                "New Rectangle",
                Action::AddShape(motion_core::vector::ShapeKind::Rectangle),
            ),
            (
                "新建椭圆",
                "New Ellipse",
                Action::AddShape(motion_core::vector::ShapeKind::Ellipse),
            ),
        ] {
            let enabled = !matches!(action, Action::Edit(Command::CreateCamera))
                || state["project"]["camera"]["created"] != true;
            add(zh, en, enabled, Choice::New(action));
        }
        drop(add);
        for entry in &mut result {
            if !entry.enabled {
                entry.disabled_reason = Some(word(locale, "当前合成已经有摄影机", "This composition already has a camera"));
            }
        }
        return result;
    };
    if id == 0 {
        add(
            "摄影机属性",
            "Camera Properties",
            true,
            Choice::New(Action::ShowPanel(Panel::Properties.dock())),
        );
        add(
            "移除摄影机",
            "Remove Camera",
            true,
            Choice::Edit(Command::Remove {
                object: 0,
                frame: frame(state),
            }),
        );
        return result;
    }
    let Some(l) = layer(state, id) else {
        return result;
    };
    let unlocked = l["locked"] != true;
    let list = state["project"]["layers"].as_array().unwrap();
    let index = list.iter().position(|l| l["id"] == id).unwrap();
    let timeline = state["timeline_layers"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|t| t["object"] == id);
    let range = timeline
        .map(|t| {
            [
                t["subframe_range"]["in_frame"]
                    .as_f64()
                    .or_else(|| t["in_frame"].as_f64())
                    .unwrap_or(0.),
                t["subframe_range"]["out_frame"]
                    .as_f64()
                    .or_else(|| t["out_frame"].as_f64())
                    .unwrap_or(last_frame(state) + 1.),
            ]
        })
        .unwrap_or([0., last_frame(state) + 1.]);
    let now = frame(state) as f64;
    if popup.page != Page::Root {
        add("‹ 返回", "‹ Back", true, Choice::Page(Page::Root));
    }
    match popup.page {
        Page::Root => {
            add("开关", "Switches", true, Choice::Page(Page::Switch));
            add("时间", "Time", unlocked, Choice::Page(Page::Time));
            add(
                "混合模式",
                "Blend Mode",
                unlocked && l["content"]["kind"] != "audio",
                Choice::Page(Page::Blend),
            );
            divider_indices.push(add("排列", "Arrange", unlocked, Choice::Page(Page::Order)));
            let precompose = unlocked
                && l["three_d"] != true
                && !list.iter().any(|other| {
                    other["track_matte"]["source"] == id
                        || (other["id"] == id && other["track_matte"].is_object())
                        || other["effects"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|e| e["image_input"]["layer"] == id && other["id"] != id)
                });
            let precompose = precompose
                && l["parent"]["object"].is_null()
                && state["project"]["camera"]["parent"]["object"] != id
                && !list.iter().any(|other| {
                    other["parent"]["object"] == id
                        || other["effects"].as_array().into_iter().flatten().any(|e| {
                            (other["id"] == id && e["scene"].is_object())
                                || (e["scene"]["source_layer"] == id && other["id"] != id)
                                || (other["id"] == id
                                    && e["image_input"]["layer"]
                                        .as_u64()
                                        .is_some_and(|input| input != id))
                        })
                })
                && !state["project"]["expressions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|e| {
                        e["enabled"] != false
                            && e["source"]
                                .as_str()
                                .is_some_and(|code| code.contains("index"))
                    });
            divider_indices.push(add(
                "预合成…",
                "Precompose…",
                precompose,
                Choice::Precompose,
            ));
            add(
                "重命名    Enter",
                "Rename    Enter",
                unlocked,
                Choice::Rename,
            );
            add(
                "复制图层    Ctrl+D",
                "Duplicate    Ctrl+D",
                unlocked,
                Choice::Edit(Command::Duplicate { object: id }),
            );
            let referenced = list.iter().any(|l| l["track_matte"]["source"] == id);
            add(
                "删除图层    Delete",
                "Delete Layer    Delete",
                unlocked && !referenced,
                Choice::Edit(Command::Remove {
                    object: id,
                    frame: frame(state),
                }),
            );
        }
        Page::Switch => {
            add(
                if l["visible"] == false {
                    "显示图层"
                } else {
                    "隐藏图层"
                },
                if l["visible"] == false {
                    "Show Layer"
                } else {
                    "Hide Layer"
                },
                true,
                Choice::Edit(Command::Flags {
                    object: id,
                    visible: l["visible"] == false,
                    locked: l["locked"] == true,
                }),
            );
            add(
                if unlocked {
                    "锁定图层"
                } else {
                    "解锁图层"
                },
                if unlocked {
                    "Lock Layer"
                } else {
                    "Unlock Layer"
                },
                true,
                Choice::Edit(Command::Flags {
                    object: id,
                    visible: l["visible"] != false,
                    locked: unlocked,
                }),
            );
        }
        Page::Order => {
            for (zh, en, dest) in [
                ("上移一层", "Move Up", (index + 1).min(list.len() - 1)),
                ("下移一层", "Move Down", index.saturating_sub(1)),
                ("置顶", "Move to Top", list.len() - 1),
                ("置底", "Move to Bottom", 0),
            ] {
                add(
                    zh,
                    en,
                    unlocked && dest != index,
                    Choice::Edit(Command::Reorder {
                        object: id,
                        index: dest,
                    }),
                );
            }
        }
        Page::Time => {
            divider_indices.push(add(
                "在播放头处分割图层",
                "Split at Playhead",
                unlocked && now > range[0] && now < range[1],
                Choice::Edit(Command::SplitLayerClip {
                    object: id,
                    frame: frame(state),
                }),
            ));
            for (zh, en, to) in [
                ("跳到入点", "Go to In Point", range[0]),
                ("跳到出点", "Go to Out Point", (range[1] - 1.).max(range[0])),
            ] {
                add(
                    zh,
                    en,
                    true,
                    Choice::New(Action::Seek(to.clamp(0., last_frame(state)))),
                );
            }
            for (zh, en, next) in [
                ("入点移到播放头", "Move In to Playhead", now),
                (
                    "出点移到播放头",
                    "Move Out to Playhead",
                    now + 1. - (range[1] - range[0]),
                ),
            ] {
                let valid = range[0].fract() == 0.
                    && range[1].fract() == 0.
                    && (next.fract() == 0.)
                    && next >= -(motion_core::MAX_FRAMES as f64)
                    && next + (range[1] - range[0]) <= motion_core::MAX_FRAMES as f64;
                add(
                    zh,
                    en,
                    unlocked && valid,
                    Choice::Edit(Command::MoveLayerClip {
                        object: id,
                        in_frame: next as i32,
                    }),
                );
            }
            for (zh, en, start, end) in [
                ("入点裁剪到播放头", "Trim In to Playhead", now, range[1]),
                (
                    "出点裁剪到播放头",
                    "Trim Out to Playhead",
                    range[0],
                    now + 1.,
                ),
            ] {
                add(
                    zh,
                    en,
                    unlocked && start < end && start >= range[0] && end <= range[1],
                    Choice::Edit(Command::SetLayerRange {
                        object: id,
                        in_frame: start,
                        out_frame: end,
                    }),
                );
            }
        }
        Page::Blend => {
            for (zh, en, mode) in blend_modes() {
                add(
                    zh,
                    en,
                    unlocked,
                    Choice::Edit(Command::SetLayerBlend {
                        object: id,
                        mode: Some(mode),
                        space: None,
                    }),
                );
            }
        }
    }
    drop(add);
    for entry in &mut result {
        if !entry.enabled {
            entry.disabled_reason = Some(disabled_reason(&entry.choice, l, state, locale));
        }
    }
    for index in divider_indices {
        result[index].divider_after = true;
    }
    result
}

fn blend_modes() -> Vec<(
    &'static str,
    &'static str,
    motion_core::compositing::BlendMode,
)> {
    use motion_core::compositing::BlendMode::*;
    vec![
        ("正常", "Normal", Normal),
        ("相加", "Add", Add),
        ("变暗", "Darken", Darken),
        ("正片叠底", "Multiply", Multiply),
        ("颜色加深", "Color Burn", ColorBurn),
        ("变亮", "Lighten", Lighten),
        ("滤色", "Screen", Screen),
        ("颜色减淡", "Color Dodge", ColorDodge),
        ("叠加", "Overlay", Overlay),
        ("柔光", "Soft Light", SoftLight),
        ("强光", "Hard Light", HardLight),
        ("差值", "Difference", Difference),
        ("排除", "Exclusion", Exclusion),
        ("相减", "Subtract", Subtract),
        ("相除", "Divide", Divide),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state(engine: &motion_core::Engine) -> Value {
        let p = engine.project();
        serde_json::json!({"root":"project-a","composition":p.composition_id,"frame":21,"project":p,"timeline_layers":p.layers.iter().map(|l|{let clip=l.clip(p.frames);serde_json::json!({"object":l.id,"in_frame":clip.in_frame,"out_frame":clip.out_frame})}).collect::<Vec<_>>()})
    }
    fn engine() -> motion_core::Engine {
        let mut p = motion_core::Project::new(640, 360, 30, 120).unwrap();
        for id in [7, 9] {
            p.layers.push(motion_core::Layer::solid(
                id,
                &format!("Layer {id}"),
                [50.; 2],
                [320., 180., 0.],
                [1.; 4],
            ));
        }
        motion_core::Engine::new(p).unwrap()
    }
    fn draw(panels: &mut Panels, state: &Value, input: &Input) -> Vec<Action> {
        let layout = dock::Node::dock(Panel::Timeline.dock());
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        chrome(
            &mut PaintList::default(),
            &mut atlas,
            [1280., 700.],
            &layout,
            panels,
            state,
            input,
        )
    }
    fn point() -> [f32; 2] {
        let layout = dock::Node::dock(Panel::Timeline.dock());
        let frame = frames(&layout, [1280., 700.]).remove(0);
        [140., frame.content.min[1] + 64. + 11.]
    }

    #[test]
    fn popup_uses_pointer_as_lower_left_anchor_and_clamps_to_window() {
        let bounds = Rect::new(0., 0., 800., 600.);
        let card = popup_rect([120., 520.], bounds, 274., 220.);
        assert_eq!(card.min, [120., 300.]);
        assert_eq!(card.max, [394., 520.]);
        let clamped = popup_rect([760., 100.], bounds, 274., 220.);
        assert_eq!(clamped.min, [526., 0.]);
        assert_eq!(clamped.max, [800., 220.]);
    }
    fn click(button: MouseButton, p: [f32; 2]) -> Input {
        Input {
            mouse: p,
            pressed: Some((button, p)),
            ..Default::default()
        }
    }
    fn edit(actions: Vec<Action>) -> Command {
        actions
            .into_iter()
            .find_map(|a| {
                if let Action::Edit(c) = a {
                    Some(c)
                } else {
                    None
                }
            })
            .unwrap()
    }
    #[test]
    fn right_click_targets_hit_layer_and_duplicate_is_one_undo() {
        let mut engine = engine();
        let before = engine.project().clone();
        let mut panels = Panels {
            selected: Some(7),
            ..Default::default()
        };
        let state = state(&engine);
        let p = point();
        assert!(draw(&mut panels, &state, &click(MouseButton::Right, p)).is_empty());
        assert_eq!(panels.selected, Some(9));
        assert_eq!(engine.project(), &before);
        let duplicate = entries(
            panels.layer_actions.popup.as_ref().unwrap(),
            &state,
            Locale::En,
        )
        .iter()
        .position(|entry| entry.label.starts_with("Duplicate"))
        .unwrap();
        let popup = panels.layer_actions.popup.as_mut().unwrap();
        popup.highlighted = duplicate;
        popup.opened = false;
        let command = edit(draw(
            &mut panels,
            &state,
            &Input {
                keys: vec![(Key::Enter, Default::default())],
                ..Default::default()
            },
        ));
        engine.apply(command).unwrap();
        assert_eq!(engine.project().layers.len(), 3);
        assert_eq!(engine.project().layers.last().unwrap().name, "Layer 9 副本");
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
    }
    #[test]
    fn name_enter_commit_cancel_ime_and_outside_click_are_scoped() {
        let mut engine = engine();
        let before = engine.project().clone();
        let state = state(&engine);
        let mut panels = Panels {
            selected: Some(9),
            focused: Some(Panel::Timeline),
            ..Default::default()
        };
        let enter = Input {
            keys: vec![(Key::Enter, Default::default())],
            ..Default::default()
        };
        assert!(draw(&mut panels, &state, &enter).is_empty());
        assert!(panels.layer_actions.text_in(Some(Panel::Timeline)));
        draw(
            &mut panels,
            &state,
            &Input {
                text: "我的图层".into(),
                ..Default::default()
            },
        );
        assert!(draw(
            &mut panels,
            &state,
            &Input {
                composing: true,
                ..enter.clone()
            }
        )
        .is_empty());
        assert!(panels.layer_actions.text_in(Some(Panel::Timeline)));
        let command = edit(draw(&mut panels, &state, &enter));
        engine.apply(command).unwrap();
        assert_eq!(engine.project().layers[1].name, "我的图层");
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
        draw(&mut panels, &state, &enter);
        draw(
            &mut panels,
            &state,
            &Input {
                text: "草稿".into(),
                ..Default::default()
            },
        );
        assert!(draw(
            &mut panels,
            &state,
            &Input {
                keys: vec![(Key::Escape, Default::default())],
                ..Default::default()
            }
        )
        .is_empty());
        assert!(!panels.layer_actions.text_in(Some(Panel::Timeline)));
        draw(&mut panels, &state, &enter);
        draw(
            &mut panels,
            &state,
            &Input {
                text: "外部提交".into(),
                ..Default::default()
            },
        );
        let command = edit(draw(
            &mut panels,
            &state,
            &click(MouseButton::Left, [700., 500.]),
        ));
        engine.apply(command).unwrap();
        assert_eq!(engine.project().layers[1].name, "外部提交");
    }
    #[test]
    fn double_click_name_only_opens_for_unlocked_layers() {
        let mut engine = engine();
        let state = state(&engine);
        let mut panels = Panels::default();
        let p = point();
        draw(&mut panels, &state, &click(MouseButton::Left, p));
        draw(&mut panels, &state, &click(MouseButton::Left, p));
        assert!(panels.layer_actions.text_in(Some(Panel::Timeline)));
        panels.layer_actions.cancel();
        engine
            .apply(Command::Flags {
                object: 9,
                visible: true,
                locked: true,
            })
            .unwrap();
        let state = super::tests::state(&engine);
        draw(&mut panels, &state, &click(MouseButton::Left, p));
        draw(&mut panels, &state, &click(MouseButton::Left, p));
        assert!(!panels.layer_actions.text_in(Some(Panel::Timeline)));
    }
    #[test]
    fn locked_menu_disables_editing_but_allows_unlock_and_checks_split_endpoints() {
        let mut engine = engine();
        engine
            .apply(Command::Flags {
                object: 9,
                visible: true,
                locked: true,
            })
            .unwrap();
        let mut state = state(&engine);
        let popup = Popup {
            target: Some(9),
            anchor: [0.; 2],
            owner: None,
            context: context(&state),
            page: Page::Root,
            highlighted: 0,
            scroll: 0,
            opened: false,
        };
        let items = entries(&popup, &state, Locale::Zh);
        assert!(items[0].enabled);
        assert!(items[1..].iter().all(|item| !item.enabled));
        let mut switch = popup;
        switch.page = Page::Switch;
        let switch_items = entries(&switch, &state, Locale::Zh);
        assert!(switch_items[1].enabled);
        engine
            .apply(Command::Flags {
                object: 9,
                visible: true,
                locked: false,
            })
            .unwrap();
        state["project"] = serde_json::to_value(engine.project()).unwrap();
        state["timeline_layers"][1]["in_frame"] = serde_json::json!(10);
        state["timeline_layers"][1]["out_frame"] = serde_json::json!(90);
        switch.page = Page::Time;
        for f in [10, 90] {
            state["frame"] = serde_json::json!(f);
            assert!(!entries(&switch, &state, Locale::En)[1].enabled);
        }
    }
    #[test]
    fn context_change_or_removal_rejects_open_menu_and_name_draft() {
        let engine = engine();
        let mut state = state(&engine);
        let mut controls = Controls::default();
        controls.open(Some(9), [0.; 2], &state);
        state["composition"] = Value::String("different".into());
        let mut out = vec![];
        assert!(controls.prepare(
            None,
            &state,
            &Input::default(),
            Locale::Zh,
            None,
            false,
            &mut out
        ));
        assert!(!controls.editing());
        assert!(out.is_empty());
        controls.begin_name(9, false, &state, Locale::Zh);
        state["project"]["layers"] = serde_json::json!([]);
        controls.prepare(
            None,
            &state,
            &Input::default(),
            Locale::Zh,
            None,
            false,
            &mut out,
        );
        assert!(!controls.editing());
        assert!(out.is_empty());
    }
    #[test]
    fn floating_menu_has_independent_window_owner_and_outside_close_consumes_frame() {
        let engine = engine();
        let state = state(&engine);
        let mut panels = Panels::default();
        let p = [140., 105.];
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        assert!(floating_chrome(
            &mut PaintList::default(),
            &mut atlas,
            Panel::Timeline,
            [800., 500.],
            &mut panels,
            &state,
            &click(MouseButton::Right, p)
        )
        .is_empty());
        assert!(panels.layer_actions.modal_in(Some(Panel::Timeline)));
        assert!(!panels.layer_actions.modal_in(None));
        let actions = floating_chrome(
            &mut PaintList::default(),
            &mut atlas,
            Panel::Timeline,
            [800., 500.],
            &mut panels,
            &state,
            &click(MouseButton::Left, [750., 400.]),
        );
        assert!(actions.is_empty());
        assert!(!panels.layer_actions.editing());
    }
    #[test]
    fn precompose_requires_confirmation_and_reuses_atomic_model_command() {
        let mut engine = engine();
        let before = engine.project().clone();
        let state = state(&engine);
        let mut controls = Controls::default();
        controls.begin_name(9, true, &state, Locale::En);
        let mut out = vec![];
        controls.name.as_mut().unwrap().editor.text = "New Comp".into();
        controls.finish_name(&state, Locale::En, &mut out);
        engine.apply(edit(out)).unwrap();
        assert_eq!(engine.project().compositions.len(), 1);
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
    }

    #[test]
    fn unavailable_actions_explain_lock_clip_edges_dependencies_and_camera_in_both_languages() {
        let mut engine = engine();
        engine.apply(Command::Flags { object: 9, visible: true, locked: true }).unwrap();
        let mut state = state(&engine);
        let mut popup = Popup { target: Some(9), anchor: [0.; 2], owner: None,
            context: context(&state), page: Page::Root, highlighted: 0, scroll: 0, opened: false };
        for locale in [Locale::Zh, Locale::En] {
            let locked = entries(&popup, &state, locale);
            assert!(locked.iter().filter(|e| !e.enabled).all(|e| e.disabled_reason.as_ref().is_some_and(|r|
                r.contains(if locale == Locale::Zh { "解锁" } else { "Unlock" }))));
            assert!(locked.iter().filter(|e| e.enabled).all(|e| e.disabled_reason.is_none()));
            state["project"]["layers"][1]["locked"] = serde_json::json!(false);
            popup.page = Page::Time;
            state["frame"] = serde_json::json!(0);
            let edge = entries(&popup, &state, locale);
            let split = edge.iter().find(|e| matches!(e.choice, Choice::Edit(Command::SplitLayerClip { .. }))).unwrap();
            assert!(!split.enabled);
            assert!(split.disabled_reason.as_ref().unwrap().contains(if locale == Locale::Zh { "播放头" } else { "playhead" }));
            popup.page = Page::Root;
            state["project"]["layers"][1]["three_d"] = serde_json::json!(true);
            let root = entries(&popup, &state, locale);
            assert!(root.iter().find(|e| matches!(e.choice, Choice::Precompose)).unwrap().disabled_reason.is_some());
            popup.target = None;
            state["project"]["camera"]["created"] = serde_json::json!(true);
            let empty = entries(&popup, &state, locale);
            let camera = empty.iter().find(|e| matches!(e.choice, Choice::New(Action::Edit(Command::CreateCamera)))).unwrap();
            assert!(!camera.enabled && camera.disabled_reason.is_some());
            popup.target = Some(9);
            state["project"]["layers"][1]["locked"] = serde_json::json!(true);
        }
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        for bounds in [Rect::new(0., 0., 220., 120.), Rect::new(0., 0., 800., 500.)] {
            let mut paint = PaintList::default();
            disabled_hint(&mut paint, &mut atlas, bounds, bounds.max,
                "Unlock this layer before editing it; the current clip remains unchanged");
            assert!(!paint.glyphs.is_empty());
            for glyph in &paint.glyphs {
                assert!(bounds.contains_rect(Rect::new(glyph.position[0], glyph.position[1], glyph.size[0], glyph.size[1])));
            }
        }
    }

    #[test]
    fn scoped_menu_edit_survives_save_reopen_and_one_step_undo_in_both_languages() {
        for locale in [Locale::Zh, Locale::En] {
            let mut engine = engine();
            let before = engine.project().clone();
            let state = state(&engine);
            let mut panels = Panels { locale, selected: Some(7), ..Default::default() };
            assert!(draw(&mut panels, &state, &click(MouseButton::Right, point())).is_empty());
            let popup = panels.layer_actions.popup.as_ref().unwrap();
            let index = entries(popup, &state, locale).iter().position(|e|
                matches!(e.choice, Choice::Edit(Command::Duplicate { object: 9 }))).unwrap();
            let popup = panels.layer_actions.popup.as_mut().unwrap();
            popup.highlighted = index;
            popup.opened = false;
            let command = edit(draw(&mut panels, &state, &Input {
                keys: vec![(Key::Enter, Default::default())], ..Default::default() }));
            engine.apply(command).unwrap();
            let after = engine.project().clone();
            let directory = tempfile::tempdir().unwrap();
            motion_core::storage::save(directory.path(), &after).unwrap();
            assert_eq!(motion_core::storage::load(directory.path()).unwrap(), after);
            engine.undo().unwrap();
            assert_eq!(engine.project(), &before);
            engine.redo().unwrap();
            assert_eq!(engine.project(), &after);
        }
    }
}
