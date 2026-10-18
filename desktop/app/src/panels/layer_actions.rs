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
    Matte,
    Audio,
    Markers,
}
enum Choice {
    Edit(Command),
    Rename,
    Precompose,
    Page(Page),
    New(Action),
    SelectGenerated,
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
    root_highlighted: usize,
    root_scroll: usize,
    mouse: Option<[f32; 2]>,
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
    pub select_generated: Option<u64>,
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
            root_highlighted: 0,
            root_scroll: 0,
            mouse: None,
            opened: true,
        });
    }
    pub fn open_audio(&mut self, target: u64, anchor: [f32; 2], state: &Value, locale: Locale) {
        self.open(Some(target), anchor, state);
        if let Some(popup) = &mut self.popup {
            let root = entries(popup, state, locale);
            if let Some(index) = root
                .iter()
                .position(|e| matches!(e.choice, Choice::Page(Page::Audio)))
            {
                popup.root_highlighted = index;
                popup.page = Page::Audio;
            }
        }
    }
    pub fn open_markers(&mut self, target: u64, anchor: [f32; 2], state: &Value) {
        self.open(Some(target), anchor, state);
        self.popup.as_mut().unwrap().page = Page::Markers;
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
        if clicked(input, name) && (input.modifiers.control || input.modifiers.shift) {
            self.last_click = None;
        } else if clicked(input, name) && id != 0 {
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
        let neutral = Input::default();
        let active = if popup.opened { &neutral } else { input };
        let hover =
            popup.mouse != Some(active.mouse) || active.pressed.is_some() || active.wheel[1] != 0.;
        popup.mouse = Some(input.mouse);
        popup.opened = false;
        if active.focus_lost || active.keys.iter().any(|(k, _)| *k == Key::Escape) {
            self.popup = None;
            return;
        }
        let page = popup.page;
        let root_page = if page == Page::Markers {
            Page::Markers
        } else {
            Page::Root
        };
        popup.page = root_page;
        let root_entries = entries(popup, state, locale);
        popup.page = page;
        if root_entries.is_empty() {
            self.popup = None;
            return;
        }
        let row = list
            .styles()
            .metric("context.row.height", 20.)
            .clamp(18., 24.);
        let base_width = if root_page == Page::Markers {
            root_entries
                .iter()
                .map(|entry| atlas.measure(&entry.label) + 40.)
                .fold(220., f32::max)
                .min(320.)
        } else {
            220.
        };
        let width = base_width.min(
            if root_entries
                .iter()
                .any(|e| matches!(e.choice, Choice::Page(_)))
            {
                bounds.width() * 0.5
            } else {
                bounds.width()
            },
        );
        let count = menu_count(root_entries.len(), bounds, row);
        let card = popup_rect(popup.anchor, bounds, width, count as f32 * row + 6.);
        let mut root_highlighted = if page == root_page {
            popup.highlighted
        } else {
            popup.root_highlighted
        };
        let mut root_scroll = if page == root_page {
            popup.scroll
        } else {
            popup.root_scroll
        };
        let root_active = page == root_page;
        let root_chosen = draw_menu(
            list,
            atlas,
            card,
            bounds,
            row,
            &root_entries,
            active,
            root_active,
            &mut root_highlighted,
            &mut root_scroll,
            hover,
        );
        let over_root = (hover && card.contains(active.mouse)).then_some(root_highlighted);
        let hovered = over_root.filter(|i| root_entries[*i].enabled);
        let branch = hovered.or(root_chosen).and_then(|i| {
            if let Choice::Page(next) = root_entries[i].choice {
                Some(next)
            } else {
                None
            }
        });
        let right = active
            .key_presses()
            .any(|(k, m, r)| k == Key::Right && !r && !m.control && !m.alt);
        let left = active
            .key_presses()
            .any(|(k, m, r)| k == Key::Left && !r && !m.control && !m.alt);
        let branch = branch.or_else(|| {
            (page == root_page && right && root_entries[root_highlighted].enabled)
                .then(|| &root_entries[root_highlighted].choice)
                .and_then(|c| {
                    if let Choice::Page(next) = c {
                        Some(*next)
                    } else {
                        None
                    }
                })
        });
        if left {
            popup.page = root_page;
        } else if let Some(next) = branch {
            if popup.page != next {
                popup.page = next;
                popup.highlighted = 0;
                popup.scroll = 0;
            }
        } else if over_root.is_some() {
            popup.page = root_page;
        }
        popup.root_highlighted = root_highlighted;
        popup.root_scroll = root_scroll;
        let mut child_card = None;
        let mut child_choice = None;
        if popup.page != root_page {
            let children = entries(popup, state, locale);
            if !children.is_empty() {
                let height = menu_count(children.len(), bounds, row) as f32 * row + 6.;
                let parent_y =
                    card.min[1] + 3. + root_highlighted.saturating_sub(root_scroll) as f32 * row;
                // Prefer the right side; at the window edge open to the left.
                let child_width = children
                    .iter()
                    .map(|entry| atlas.measure(&entry.label) + 40.)
                    .fold(width, f32::max)
                    .min(320.)
                    .min((bounds.width() - card.width()).max(1.));
                let child_x = if card.max[0] + child_width <= bounds.max[0] {
                    card.max[0]
                } else {
                    (card.min[0] - child_width).max(bounds.min[0])
                };
                let child = Rect::new(
                    child_x,
                    parent_y.clamp(bounds.min[1], (bounds.max[1] - height).max(bounds.min[1])),
                    child_width,
                    height,
                );
                child_card = Some(child);
                let chosen = draw_menu(
                    list,
                    atlas,
                    child,
                    bounds,
                    row,
                    &children,
                    active,
                    true,
                    &mut popup.highlighted,
                    &mut popup.scroll,
                    hover,
                );
                if let Some(i) = chosen {
                    child_choice = children.into_iter().nth(i);
                }
            }
        } else {
            popup.highlighted = root_highlighted;
            popup.scroll = root_scroll;
        }
        if active
            .pressed
            .is_some_and(|(_, p)| !card.contains(p) && !child_card.is_some_and(|r| r.contains(p)))
        {
            self.popup = None;
            return;
        }
        let chosen = child_choice.or_else(|| {
            root_chosen.and_then(|i| {
                root_entries
                    .into_iter()
                    .nth(i)
                    .filter(|e| !matches!(e.choice, Choice::Page(_)))
            })
        });
        let Some(chosen) = chosen else { return };
        let target = self.popup.take().unwrap().target;
        match chosen.choice {
            Choice::Edit(command) => out.push(Action::Edit(scoped(command, state))),
            Choice::New(action) => out.push(action),
            Choice::SelectGenerated => self.select_generated = target,
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

fn menu_count(len: usize, bounds: Rect, row: f32) -> usize {
    len.min(((bounds.height() - 8.).max(row) / row).floor() as usize)
        .max(1)
}
fn draw_menu(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    card: Rect,
    bounds: Rect,
    row: f32,
    entries: &[Entry],
    input: &Input,
    keyboard: bool,
    highlighted: &mut usize,
    scroll: &mut usize,
    hover: bool,
) -> Option<usize> {
    let count = menu_count(entries.len(), bounds, row);
    *highlighted = (*highlighted).min(entries.len() - 1);
    *scroll = (*scroll).min(entries.len() - count);
    if card.contains(input.mouse) && input.wheel[1] != 0. {
        *scroll = (*scroll as i32 + if input.wheel[1] > 0. { 1 } else { -1 })
            .clamp(0, (entries.len() - count) as i32) as usize;
    }
    if keyboard && !input.composing && !input.ime_consumed {
        for (k, m, _) in input
            .key_presses()
            .filter(|(_, m, _)| !m.control && !m.alt && !m.shift)
        {
            let _ = m;
            if matches!(k, Key::Up | Key::Down) {
                for _ in 0..entries.len() {
                    *highlighted = if k == Key::Down {
                        (*highlighted + 1) % entries.len()
                    } else {
                        (*highlighted + entries.len() - 1) % entries.len()
                    };
                    if entries[*highlighted].enabled {
                        break;
                    }
                }
                *scroll = (*scroll)
                    .min(*highlighted)
                    .max(highlighted.saturating_sub(count - 1));
            }
        }
    }
    let styles = list.styles();
    box_style(
        list,
        card,
        styles.resolve("context_menu", States::default()).unwrap(),
    );
    let mut chosen = None;
    for (i, entry) in entries.iter().enumerate().skip(*scroll).take(count) {
        let area = Rect::new(
            card.min[0] + 3.,
            card.min[1] + 3. + (i - *scroll) as f32 * row,
            card.width() - 6.,
            row,
        );
        if hover && area.contains(input.mouse) {
            *highlighted = i;
        }
        let mut states = States::default();
        if i == *highlighted && entry.enabled {
            states = states.with(State::Selected);
        }
        if !entry.enabled {
            states = states.with(State::Disabled);
        }
        let style = styles.resolve("context_menu.row", states).unwrap();
        box_style(list, area, style);
        text_style(
            list,
            atlas,
            Rect::new(area.min[0] + 5., area.min[1], area.width() - 28., row),
            &entry.label,
            style,
            Align::Left,
        );
        if matches!(entry.choice, Choice::Page(_)) {
            submenu_arrow(list, area, style.foreground);
        }
        if entry.divider_after {
            paint::line(
                list,
                vec![
                    [area.min[0] + 7., area.max[1] - 1.],
                    [area.max[0] - 7., area.max[1] - 1.],
                ],
                styles.color("context.border"),
            );
        }
        if area.contains(input.mouse) {
            if let Some(reason) = entry.disabled_reason.as_deref() {
                disabled_hint(list, atlas, bounds, input.mouse, reason);
            }
        }
        if entry.enabled && clicked(input, area) {
            chosen = Some(i);
        }
    }
    if keyboard
        && !input.composing
        && !input.ime_consumed
        && input
            .key_presses()
            .any(|(k, m, r)| k == Key::Enter && !r && !m.control && !m.alt)
        && entries[*highlighted].enabled
    {
        chosen = Some(*highlighted);
    }
    chosen
}

fn disabled_hint(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    bounds: Rect,
    point: [f32; 2],
    reason: &str,
) {
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
    if !line.is_empty() {
        lines.push(line);
    }
    let line_height = atlas.line_height.max(1.);
    let height = (lines.len() as f32 * line_height + 8.).min(bounds.height().max(1.));
    let card = popup_rect([point[0] + 12., point[1] - 8.], bounds, width, height);
    let style = list.styles().resolve("tooltip", States::default()).unwrap();
    let mut content = list.child();
    box_style(&mut content, card, style);
    for (index, line) in lines.iter().enumerate() {
        ui::text(
            &mut content,
            atlas,
            Rect::new(
                card.min[0] + 6.,
                card.min[1] + 4. + index as f32 * line_height,
                (card.width() - 12.).max(0.),
                line_height,
            ),
            line,
            style.foreground,
            Align::Left,
        );
    }
    content.clip_to(bounds.intersection(card));
    list.append(content);
}

fn disabled_reason(choice: &Choice, layer: &Value, state: &Value, locale: Locale) -> String {
    if state["observing"] == true {
        return word(locale, "当前处于只读模式", "This session is read-only");
    }
    if matches!(choice, Choice::New(Action::EditBatch(_))) {
        return word(locale, "所有图层均已解锁", "All layers are already unlocked");
    }
    if layer["locked"] == true {
        return word(
            locale,
            "图层已锁定，请先解锁",
            "Unlock this layer before editing it",
        );
    }
    match choice {
        Choice::New(Action::AnalyzeAudio(_)) => word(
            locale,
            "请选择有音频轨道的音频或视频图层",
            "Choose an audio or video layer with a single audio source",
        ),
        Choice::Edit(Command::SplitAudioChannels { .. }) => word(
            locale,
            "需要尚未分离的立体声音源",
            "Choose a stereo source whose channels have not been separated",
        ),
        Choice::Edit(Command::ExtractAudio { .. }) => word(
            locale,
            "请选择带有可用音轨的视频图层",
            "Choose a video layer with a recoverable audio track",
        ),
        Choice::New(Action::CancelAudioAnalysis) => word(
            locale,
            "当前图层没有正在进行的音频分析",
            "No audio analysis is running for this layer",
        ),
        Choice::Page(Page::Blend) => word(
            locale,
            "音频图层不支持画面混合模式",
            "Audio layers do not support visual blend modes",
        ),
        Choice::Precompose if layer["three_d"] == true => word(
            locale,
            "预合成暂不支持三维图层",
            "Precompose does not yet support 3D layers",
        ),
        Choice::Precompose
            if state["project"]["expressions"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|e| {
                    e["enabled"] != false
                        && e["source"]
                            .as_str()
                            .is_some_and(|code| code.contains("index"))
                }) =>
        {
            word(
                locale,
                "请先处理使用图层索引的表达式",
                "Resolve expressions using layer indices before precomposing",
            )
        }
        Choice::Precompose => word(
            locale,
            "请先解除父级、遮罩或跨图层效果依赖",
            "Resolve parent, matte or cross-layer effect dependencies first",
        ),
        Choice::Edit(Command::Remove { .. }) => word(
            locale,
            "其他图层正在使用此图层作为轨道遮罩",
            "Another layer uses this layer as its track matte",
        ),
        Choice::Edit(Command::Reorder { index: 0, .. }) => word(
            locale,
            "图层已经位于最底部",
            "This layer is already at the bottom",
        ),
        Choice::Edit(Command::Reorder { .. }) => word(
            locale,
            "图层已经位于最顶部",
            "This layer is already at the top",
        ),
        Choice::Edit(Command::SplitLayerClip { .. }) => word(
            locale,
            "播放头必须位于图层入点和出点之间",
            "Place the playhead strictly between the layer's in and out points",
        ),
        Choice::Edit(Command::MoveLayerClip { .. }) => word(
            locale,
            "片段必须对齐整帧，且移动后不能超出时间范围",
            "Align the clip to whole frames and keep it within the timeline limits",
        ),
        Choice::Edit(Command::SetLayerRange { .. }) => word(
            locale,
            "播放头必须位于当前片段内，并保留有效长度",
            "Keep the playhead inside the clip and retain a valid duration",
        ),
        _ => word(
            locale,
            "当前图层状态不支持此操作",
            "This operation is unavailable for the current layer state",
        ),
    }
}

fn entries(popup: &Popup, state: &Value, locale: Locale) -> Vec<Entry> {
    entries_for(popup, state, locale)
}
fn audio_asset<'a>(state: &'a Value, layer: &Value) -> Option<&'a Value> {
    let id = match layer["content"]["kind"].as_str()? {
        "audio" => layer["content"]["audio"]["asset"].as_u64()?,
        "video" => {
            let video = layer["content"]["video"]["asset"].as_u64()?;
            state["project"]["video_assets"]
                .as_array()?
                .iter()
                .find(|a| a["id"] == video)?["audio_asset"]
                .as_u64()?
        }
        _ => return None,
    };
    state["project"]["audio_assets"]
        .as_array()?
        .iter()
        .find(|a| a["id"] == id)
}
fn entries_for(popup: &Popup, state: &Value, locale: Locale) -> Vec<Entry> {
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
                entry.disabled_reason = Some(word(
                    locale,
                    "当前合成已经有摄影机",
                    "This composition already has a camera",
                ));
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
    let unlocked = l["locked"] != true && state["observing"] != true;
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
    match popup.page {
        Page::Root => {
            add("开关", "Switches", true, Choice::Page(Page::Switch));
            add("时间", "Time", unlocked, Choice::Page(Page::Time));
            if super::audio::is_audio(l) {
                add("音频", "Audio", unlocked, Choice::Page(Page::Audio));
            }
            add(
                "混合模式",
                "Blend Mode",
                unlocked && l["content"]["kind"] != "audio",
                Choice::Page(Page::Blend),
            );
            let matte_visual = !matches!(
                l["content"]["kind"].as_str(),
                Some("null" | "audio" | "adjustment")
            );
            add(
                "遮罩",
                "Track Matte",
                unlocked && matte_visual,
                Choice::Page(Page::Matte),
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
        Page::Audio | Page::Markers => {
            let source = audio_asset(state, l);
            let has_generated = l["audio_markers"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|m| m["generated"] == true);
            if has_generated {
                add(
                    "选择本音频的分析标记",
                    "Select this audio's analysis markers",
                    true,
                    Choice::SelectGenerated,
                );
                add(
                    "删除本音频的分析标记",
                    "Delete this audio's analysis markers",
                    unlocked,
                    Choice::Edit(Command::AudioMarkers {
                        object: id,
                        action: motion_core::AudioMarkersAction::ClearGenerated,
                    }),
                );
            }
            if popup.page == Page::Markers {
                drop(add);
                for entry in &mut result {
                    if !entry.enabled {
                        entry.disabled_reason =
                            Some(disabled_reason(&entry.choice, l, state, locale));
                    }
                }
                return result;
            }
            if l["content"]["kind"] == "video" {
                add(
                    "音频分离",
                    "Extract audio",
                    unlocked && source.is_some(),
                    Choice::Edit(Command::ExtractAudio { object: id }),
                );
            }
            add(
                "分离左右声道",
                "Separate left/right channels",
                unlocked
                    && source.is_some_and(|a| a["channels"] == 2)
                    && l["audio_source_channel"]
                        .as_str()
                        .is_none_or(|c| c == "stereo"),
                Choice::Edit(Command::SplitAudioChannels { object: id }),
            );
            add(
                "生成节奏参考点",
                "Generate rhythm references",
                unlocked && source.is_some(),
                Choice::New(Action::AnalyzeAudio(id)),
            );
            add(
                "添加参考标记",
                "Add reference marker",
                unlocked && now >= range[0] && now < range[1],
                Choice::Edit(Command::AudioMarkers {
                    object: id,
                    action: motion_core::AudioMarkersAction::Add {
                        frame: frame(state),
                        label: String::new(),
                    },
                }),
            );
            add(
                "重置声道电平与关键帧",
                "Reset channel levels and keys",
                unlocked && l["audio_levels"].is_object(),
                Choice::Edit(Command::AudioLevels {
                    object: id,
                    action: motion_core::AudioLevelsAction::Clear,
                }),
            );
            if state["desktop_audio_analysis"]["object"] == id {
                add(
                    "取消音频分析",
                    "Cancel audio analysis",
                    state["desktop_audio_analysis"]["object"] == id,
                    Choice::New(Action::CancelAudioAnalysis),
                );
            }
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
                state["observing"] != true,
                Choice::Edit(Command::Flags {
                    object: id,
                    visible: l["visible"] == false,
                    locked: l["locked"] == true,
                }),
            );
            add(
                if l["locked"] != true {
                    "锁定图层"
                } else {
                    "解锁图层"
                },
                if l["locked"] != true {
                    "Lock Layer"
                } else {
                    "Unlock Layer"
                },
                state["observing"] != true,
                Choice::Edit(Command::Flags {
                    object: id,
                    visible: l["visible"] != false,
                    locked: l["locked"] != true,
                }),
            );
            let unlock = super::layer_locks::changes(state, None, false);
            add("全部解锁", "Unlock All Layers", !unlock.is_empty(), Choice::New(Action::EditBatch(unlock)));
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
        Page::Matte => {
            let current: Option<motion_core::compositing::TrackMatte> = observe!(
                "layer_menu.matte",
                serde_json::from_value(l["track_matte"].clone()),
                "object={id}"
            )
            .ok()
            .flatten();
            let (choices, groups, _) = super::timeline_columns::matte_choices(state, id, locale);
            for (index, choice) in choices.into_iter().enumerate() {
                let unchanged = choice.matte == current;
                let mut label = choice.label;
                if unchanged {
                    label.push_str(" ✓");
                }
                let added = add(
                    &label,
                    &label,
                    unlocked && choice.enabled && !unchanged,
                    Choice::Edit(Command::SetTrackMatte {
                        object: id,
                        matte: choice.matte,
                    }),
                );
                if groups.contains(&(index + 1)) {
                    divider_indices.push(added);
                }
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
    #[test]
    fn hover_keeps_root_visible_and_child_click_executes_in_main_and_floating_windows() {
        for floating in [false, true] {
            for right_edge in [false, true] {
                let mut engine = engine();
                let state = state(&engine);
                let original = engine.project().clone();
                let mut panels = Panels {
                    locale: Locale::En,
                    selected: Some(9),
                    focused: Some(Panel::Timeline),
                    ..Default::default()
                };
                let anchor = if right_edge {
                    [790., 350.]
                } else {
                    [120., 350.]
                };
                panels.layer_actions.owner = Some(Panel::Timeline);
                panels.layer_actions.open(Some(9), anchor, &state);
                let mut atlas = TextAtlas::from_system(12.).unwrap();
                let mut render = |panels: &mut Panels, input: &Input| {
                    let mut list = PaintList::default();
                    let out = if floating {
                        floating_chrome(
                            &mut list,
                            &mut atlas,
                            Panel::Timeline,
                            [800., 500.],
                            panels,
                            &state,
                            input,
                        )
                    } else {
                        chrome(
                            &mut list,
                            &mut atlas,
                            [800., 500.],
                            &dock::Node::dock(Panel::Timeline.dock()),
                            panels,
                            &state,
                            input,
                        )
                    };
                    (out, list)
                };
                assert!(render(&mut panels, &Input::default()).0.is_empty());
                let popup = panels.layer_actions.popup.as_ref().unwrap();
                let root = entries(popup, &state, Locale::En);
                let count = root.len();
                let card = popup_rect(
                    anchor,
                    Rect::new(0., 0., 800., 500.),
                    220.,
                    count as f32 * 20. + 6.,
                );
                let hover = [card.min[0] + 10., card.min[1] + 13.];
                assert!(render(
                    &mut panels,
                    &Input {
                        mouse: hover,
                        ..Default::default()
                    }
                )
                .0
                .is_empty());
                let popup = panels.layer_actions.popup.as_ref().unwrap();
                assert!(popup.page == Page::Switch);
                assert_eq!(engine.project(), &original); // Hover never edits.
                let child_x = if right_edge {
                    card.min[0] - 220.
                } else {
                    card.max[0]
                };
                let child_point = [child_x + 10., card.min[1] + 16.];
                let (actions, paint) = render(&mut panels, &click(MouseButton::Left, child_point));
                assert!(paint.glyphs.iter().any(|g| g.position[0] >= card.min[0]
                    && g.position[0] < card.max[0]
                    && g.position[1] >= card.min[1]));
                assert!(paint.glyphs.iter().any(|g| g.position[0] >= child_x
                    && g.position[0] < child_x + 220.
                    && g.position[1] >= card.min[1]));
                engine.apply(edit(actions)).unwrap();
                assert!(!engine.project().layers[1].visible);
                engine.undo().unwrap();
                assert_eq!(engine.project(), &original);
                assert!(!panels.layer_actions.editing());
            }
        }
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
    fn scoped_menu_action_executes_through_the_real_desktop_host() {
        let fixture = engine();
        let state = state(&fixture);
        let root = tempfile::tempdir().unwrap();
        motion_core::storage::save(root.path(), fixture.project()).unwrap();
        let host = crate::engine::Engine::start(
            root.path().into(),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let before = host.state().unwrap()["project"].clone();
        let mut panels = Panels {
            selected: Some(9),
            ..Default::default()
        };
        draw(&mut panels, &state, &click(MouseButton::Right, point()));
        let popup = panels.layer_actions.popup.as_ref().unwrap();
        let index = entries(popup, &state, panels.locale)
            .iter()
            .position(|e| matches!(e.choice, Choice::Edit(Command::Duplicate { object: 9 })))
            .unwrap();
        panels.layer_actions.popup.as_mut().unwrap().highlighted = index;
        let action = edit(draw(
            &mut panels,
            &state,
            &Input {
                keys: vec![(Key::Enter, Default::default())],
                ..Default::default()
            },
        ));
        let after = host.edit(serde_json::to_string(&action).unwrap()).unwrap();
        assert_eq!(after["project"]["layers"].as_array().unwrap().len(), 3);
        assert_eq!(host.history(0).unwrap()["project"], before);
    }
    #[test]
    fn inline_rename_reaches_the_real_host_in_main_and_floating_windows_and_both_languages() {
        for floating in [false, true] {
            for locale in [Locale::Zh, Locale::En] {
                let fixture = engine();
                let root = tempfile::tempdir().unwrap();
                motion_core::storage::save(root.path(), fixture.project()).unwrap();
                let host = crate::engine::Engine::start(
                    root.path().into(),
                    motion_desktop_media::platform(),
                    8 << 30,
                )
                .unwrap();
                let state = host.state().unwrap();
                let before = state["project"].clone();
                let mut panels = Panels {
                    locale,
                    selected: Some(9),
                    focused: Some(Panel::Timeline),
                    ..Default::default()
                };
                let mut atlas = TextAtlas::from_system(12.).unwrap();
                let mut render = |panels: &mut Panels, state: &Value, input: &Input| {
                    if floating {
                        floating_chrome(
                            &mut PaintList::default(),
                            &mut atlas,
                            Panel::Timeline,
                            [1280., 700.],
                            panels,
                            state,
                            input,
                        )
                    } else {
                        chrome(
                            &mut PaintList::default(),
                            &mut atlas,
                            [1280., 700.],
                            &dock::Node::dock(Panel::Timeline.dock()),
                            panels,
                            state,
                            input,
                        )
                    }
                };
                let enter = Input {
                    keys: vec![(Key::Enter, Default::default())],
                    ..Default::default()
                };
                assert!(render(&mut panels, &state, &enter).is_empty());
                assert!(panels.layer_actions.name.is_some());
                assert!(render(
                    &mut panels,
                    &state,
                    &Input {
                        text: "Renamed 图层".into(),
                        ..Default::default()
                    }
                )
                .is_empty());
                assert!(render(
                    &mut panels,
                    &state,
                    &Input {
                        composing: true,
                        keys: vec![
                            (Key::Enter, Default::default()),
                            (Key::Delete, Default::default()),
                            (Key::Space, Default::default())
                        ],
                        ..Default::default()
                    }
                )
                .is_empty());
                let command = edit(render(&mut panels, &state, &enter));
                let after = host.edit(serde_json::to_string(&command).unwrap()).unwrap();
                assert_eq!(after["project"]["layers"][1]["name"], "Renamed 图层");
                assert_eq!(host.save().unwrap()["project"], after["project"]);
                assert_eq!(
                    serde_json::to_value(motion_core::storage::load(root.path()).unwrap()).unwrap(),
                    after["project"]
                );
                let state = host.history(0).unwrap();
                assert_eq!(state["project"], before);
                assert!(render(&mut panels, &state, &enter).is_empty());
                render(
                    &mut panels,
                    &state,
                    &Input {
                        text: "Discard".into(),
                        ..Default::default()
                    },
                );
                assert!(render(
                    &mut panels,
                    &state,
                    &Input {
                        keys: vec![(Key::Escape, Default::default())],
                        ..Default::default()
                    }
                )
                .is_empty());
                assert_eq!(host.state().unwrap()["project"], before);
                assert_eq!(host.history(1).unwrap()["project"], after["project"]);
            }
        }
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
            root_highlighted: 0,
            root_scroll: 0,
            mouse: None,
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
            assert!(!entries(&switch, &state, Locale::En)[0].enabled);
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
        engine
            .apply(Command::Flags {
                object: 9,
                visible: true,
                locked: true,
            })
            .unwrap();
        let mut state = state(&engine);
        let mut popup = Popup {
            target: Some(9),
            anchor: [0.; 2],
            owner: None,
            context: context(&state),
            page: Page::Root,
            highlighted: 0,
            scroll: 0,
            opened: false,
            root_highlighted: 0,
            root_scroll: 0,
            mouse: None,
        };
        for locale in [Locale::Zh, Locale::En] {
            let locked = entries(&popup, &state, locale);
            assert!(locked.iter().filter(|e| !e.enabled).all(|e| e
                .disabled_reason
                .as_ref()
                .is_some_and(|r| r.contains(if locale == Locale::Zh {
                    "解锁"
                } else {
                    "Unlock"
                }))));
            assert!(locked
                .iter()
                .filter(|e| e.enabled)
                .all(|e| e.disabled_reason.is_none()));
            state["project"]["layers"][1]["locked"] = serde_json::json!(false);
            popup.page = Page::Time;
            state["frame"] = serde_json::json!(0);
            let edge = entries(&popup, &state, locale);
            let split = edge
                .iter()
                .find(|e| matches!(e.choice, Choice::Edit(Command::SplitLayerClip { .. })))
                .unwrap();
            assert!(!split.enabled);
            assert!(split
                .disabled_reason
                .as_ref()
                .unwrap()
                .contains(if locale == Locale::Zh {
                    "播放头"
                } else {
                    "playhead"
                }));
            popup.page = Page::Root;
            state["project"]["layers"][1]["three_d"] = serde_json::json!(true);
            let root = entries(&popup, &state, locale);
            assert!(root
                .iter()
                .find(|e| matches!(e.choice, Choice::Precompose))
                .unwrap()
                .disabled_reason
                .is_some());
            popup.target = None;
            state["project"]["camera"]["created"] = serde_json::json!(true);
            let empty = entries(&popup, &state, locale);
            let camera = empty
                .iter()
                .find(|e| matches!(e.choice, Choice::New(Action::Edit(Command::CreateCamera))))
                .unwrap();
            assert!(!camera.enabled && camera.disabled_reason.is_some());
            popup.target = Some(9);
            state["project"]["layers"][1]["locked"] = serde_json::json!(true);
        }
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        for bounds in [Rect::new(0., 0., 220., 120.), Rect::new(0., 0., 800., 500.)] {
            let mut paint = PaintList::default();
            disabled_hint(
                &mut paint,
                &mut atlas,
                bounds,
                bounds.max,
                "Unlock this layer before editing it; the current clip remains unchanged",
            );
            assert!(!paint.glyphs.is_empty());
            for glyph in &paint.glyphs {
                assert!(bounds.contains_rect(Rect::new(
                    glyph.position[0],
                    glyph.position[1],
                    glyph.size[0],
                    glyph.size[1]
                )));
            }
        }
    }

    #[test]
    fn scoped_menu_edit_survives_save_reopen_and_one_step_undo_in_both_languages() {
        for locale in [Locale::Zh, Locale::En] {
            let mut engine = engine();
            let before = engine.project().clone();
            let state = state(&engine);
            let mut panels = Panels {
                locale,
                selected: Some(7),
                ..Default::default()
            };
            assert!(draw(&mut panels, &state, &click(MouseButton::Right, point())).is_empty());
            let popup = panels.layer_actions.popup.as_ref().unwrap();
            let index = entries(popup, &state, locale)
                .iter()
                .position(|e| matches!(e.choice, Choice::Edit(Command::Duplicate { object: 9 })))
                .unwrap();
            let popup = panels.layer_actions.popup.as_mut().unwrap();
            popup.highlighted = index;
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
    #[test]
    fn lock_switch_and_unlock_all_work_in_both_window_routes_without_empty_history() {
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        for floating in [false, true] {
            for locale in [Locale::Zh, Locale::En] {
                let fixture = engine();
                let root = tempfile::tempdir().unwrap();
                motion_core::storage::save(root.path(), fixture.project()).unwrap();
                let host = crate::engine::Engine::start(root.path().into(), motion_desktop_media::platform(), 8 << 30).unwrap();
                let initial = host.state().unwrap();
                let mut panels = Panels { selected: Some(9), focused: Some(Panel::Timeline), locale, ..Default::default() };
                let mut render = |panels: &mut Panels, state: &Value, input: &Input| {
                    if floating {
                        floating_chrome(&mut PaintList::default(), &mut atlas, Panel::Timeline, [1280.,700.], panels, state, input)
                    } else {
                        chrome(&mut PaintList::default(), &mut atlas, [1280.,700.], &dock::Node::dock(Panel::Timeline.dock()), panels, state, input)
                    }
                };
                render(&mut panels, &initial, &Input::default());
                let mut choose = |panels: &mut Panels, state: &Value, index: usize| {
                    panels.layer_actions.open(Some(9), [400.,600.], state);
                    let popup = panels.layer_actions.popup.as_mut().unwrap();
                    popup.page = Page::Switch;
                    popup.highlighted = 0;
                    popup.opened = false;
                    // The child page uses its own highlight, after the root Switches row.
                    render(panels, state, &Input::default());
                    panels.layer_actions.popup.as_mut().unwrap().highlighted = index;
                    render(panels, state, &Input { keys:vec![(Key::Enter,Default::default())], ..Default::default() })
                };
                let actions = choose(&mut panels, &initial, 1);
                let locked = host.edit(serde_json::to_string(&edit(actions)).unwrap()).unwrap();
                assert_eq!(locked["project"]["layers"][1]["locked"], true);
                let actions = choose(&mut panels, &locked, 2);
                let [Action::EditBatch(commands)] = actions.as_slice() else { panic!("one unlock batch"); };
                let unlocked = host.edit(serde_json::to_string(commands).unwrap()).unwrap();
                assert_eq!(unlocked["project"], initial["project"]);
                assert_eq!(host.history(0).unwrap()["project"], locked["project"]);
                host.history(1).unwrap();
                host.save().unwrap();
                assert_eq!(motion_core::storage::load(root.path()).unwrap(), *fixture.project());
                let unchanged_revision = host.state().unwrap()["revision"].clone();
                // Escape abandons a live menu without any project command.
                panels.layer_actions.open(Some(9), [400.,600.], &unlocked);
                render(&mut panels, &unlocked, &Input::default());
                assert!(render(&mut panels, &unlocked, &Input { keys:vec![(Key::Escape,Default::default())], ..Default::default() }).is_empty());
                assert_eq!(host.state().unwrap()["revision"], unchanged_revision);
            }
        }
    }

}
