//! Preset projection and library commands; all geometry comes from style metrics.
use super::*;
#[derive(Clone)]
enum Kind {
    Background,
    Effect(Entry),
    Preset(crate::animation_presets::Entry),
}
pub(super) struct Context {
    owner: Option<motion_ui::dock::Panel>,
    anchor: [f32; 2],
    kind: Kind,
    objects: Vec<u64>,
    opened: bool,
    source: Option<crate::layer_clipboard::KeyframeSource>,
}
impl Context {
    pub(super) fn effect(
        entry: Entry,
        anchor: [f32; 2],
        objects: Vec<u64>,
        owner: Option<motion_ui::dock::Panel>,
    ) -> Self {
        Self {
            owner,
            anchor,
            kind: Kind::Effect(entry),
            objects,
            opened: true,
            source: None,
        }
    }
    pub(super) fn background(
        anchor: [f32; 2],
        objects: Vec<u64>,
        owner: Option<motion_ui::dock::Panel>,
    ) -> Self {
        Self {
            owner,
            anchor,
            kind: Kind::Background,
            objects,
            opened: true,
            source: None,
        }
    }
}
impl Controls {
    pub(crate) fn prepare_context_window(
        &mut self,
        owner: Option<motion_ui::dock::Panel>,
        input: &Input,
    ) -> bool {
        self.context_window = owner;
        let own = self
            .library_context
            .as_ref()
            .is_some_and(|menu| menu.owner == owner)
            || self
                .context_menu
                .as_ref()
                .is_some_and(|menu| menu.owner == owner);
        let foreign = self
            .library_context
            .as_ref()
            .is_some_and(|menu| menu.owner != owner)
            || self
                .context_menu
                .as_ref()
                .is_some_and(|menu| menu.owner != owner);
        let active_input = input.pressed.is_some()
            || input.released.is_some()
            || input.wheel != [0.; 2]
            || !input.keys.is_empty()
            || !input.text.is_empty()
            || input.composing;
        if foreign && active_input {
            self.library_context = None;
            self.context_menu = None;
            return true; // This event dismisses the foreign menu and owns the closing frame.
        }
        own
    }
    pub(crate) fn locate_library_effect(&mut self, key: String) {
        self.query = key.clone();
        self.library_selected = Some(key);
        self.library_tab = 0;
        self.library_scroll.offset = 0.;
        self.library_context = None;
    }
    pub(super) fn draw_presets(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        layer: &Value,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        let styles = list.styles();
        let pad = styles.metric("padding.panel", 8.);
        let gap = styles.metric("padding", 5.);
        let row = styles.metric("row.height", 22.);
        let icon = styles.metric("icon.slot", 20.);
        let button_width = styles.metric("text.style.action.width", 80.);
        let enabled = !self.presets_busy;
        let save = Rect::new(area.min[0] + pad, area.min[1], button_width, row);
        if ui::button_styled(
            list,
            atlas,
            save,
            &word(locale, "保存预设…", "Save preset…"),
            input,
            enabled && self.library_selection_editable && self.library_targets.len() == 1,
            &styles,
            "button.icon",
        )
        .clicked
        {
            out.push(Action::SaveAnimationPreset {
                object: layer["id"].as_u64().unwrap_or(0),
                effects: None,
            });
        }
        let refresh = Rect::new(save.max[0] + gap, save.min[1], button_width, row);
        if ui::button_styled(
            list,
            atlas,
            refresh,
            &word(
                locale,
                if enabled { "刷新" } else { "取消任务" },
                if enabled { "Refresh" } else { "Cancel" },
            ),
            input,
            true,
            &styles,
            "button.icon",
        )
        .clicked
        {
            out.push(if enabled {
                Action::RefreshAnimationPresets
            } else {
                Action::CancelAnimationPreset
            });
        }
        let query = self.query.trim().to_lowercase();
        let mut groups: std::collections::BTreeMap<String, Vec<_>> =
            std::collections::BTreeMap::new();
        for entry in &self.presets {
            let text = format!("{} {}", entry.name, entry.category).to_lowercase();
            if query.split_whitespace().all(|term| text.contains(term)) {
                groups
                    .entry(entry.category.clone())
                    .or_default()
                    .push(entry.clone());
            }
        }
        let body = Rect::new(
            area.min[0],
            save.max[1] + gap,
            area.width(),
            (area.height() - row * 2. - gap).max(0.),
        );
        let height = groups
            .iter()
            .map(|(category, entries)| {
                row * (1 + if self.preset_collapsed.contains(category) && query.is_empty() {
                    0
                } else {
                    entries.len()
                }) as f32
            })
            .sum::<f32>();
        let scroll = self.library_scroll.begin(list, body, height, input, false);
        let mut content = list.child();
        let mut y = scroll.content.min[1];
        let prior = if input.pressed.is_some() {
            self.library_last_click.take()
        } else {
            None
        };
        for (category, entries) in groups {
            let rect = Rect::new(
                scroll.content.min[0] + pad,
                y,
                (scroll.viewport.width() - pad * 2.).max(0.),
                row,
            );
            let expanded = !query.is_empty() || !self.preset_collapsed.contains(&category);
            let style = styles.resolve("row", States::default()).unwrap();
            text_style(
                &mut content,
                atlas,
                rect,
                &format!("{} {}", if expanded { "-" } else { "+" }, category),
                style,
                Align::Left,
            );
            if clicked(&scroll.input, rect) {
                if expanded {
                    self.preset_collapsed.insert(category);
                } else {
                    self.preset_collapsed.remove(&category);
                }
            }
            y += row;
            if !expanded {
                continue;
            }
            for entry in entries {
                let rect = Rect::new(
                    scroll.content.min[0] + pad,
                    y,
                    (scroll.viewport.width() - pad * 2.).max(0.),
                    row,
                );
                #[cfg(test)]
                self.library_row_hits
                    .insert(entry.path.to_string_lossy().into_owned(), rect);
                let style = styles
                    .resolve(
                        "row",
                        if rect.contains(scroll.input.mouse) {
                            States::default().with(State::Hovered)
                        } else {
                            States::default()
                        },
                    )
                    .unwrap();
                box_style(&mut content, rect, style);
                text_style(
                    &mut content,
                    atlas,
                    Rect::new(rect.min[0] + icon, y, (rect.width() - icon).max(0.), row),
                    &entry.name,
                    style,
                    Align::Left,
                );
                if scroll
                    .input
                    .pressed
                    .is_some_and(|(b, p)| b == MouseButton::Right && rect.contains(p))
                {
                    self.library_context = Some(Context {
                        owner: self.context_window,
                        anchor: scroll.input.mouse,
                        kind: Kind::Preset(entry.clone()),
                        objects: self.library_targets.clone(),
                        opened: true,
                        source: None,
                    });
                }
                if clicked(&scroll.input, rect) {
                    let now = std::time::Instant::now();
                    let key = entry.path.to_string_lossy().into_owned();
                    let double = prior.as_ref().is_some_and(|(old, when)| {
                        old == &key
                            && now.duration_since(*when) < std::time::Duration::from_millis(400)
                    });
                    self.library_selected = Some(key.clone());
                    self.library_last_click = Some((key, now));
                    if double && enabled && self.library_selection_editable {
                        out.push(Action::ApplyAnimationPreset {
                            path: entry.path,
                            objects: self.library_targets.clone(),
                        });
                    }
                }
                y += row;
            }
        }
        content.clip_to(scroll.viewport);
        list.append(content);
        if self.presets.is_empty() {
            let rect = Rect::new(
                body.min[0] + pad,
                body.min[1] + pad,
                (body.width() - pad * 2.).max(0.),
                row,
            );
            ui::text(
                list,
                atlas,
                rect,
                &word(
                    locale,
                    "暂无预设；右键可保存变换和效果",
                    "No presets; right-click to save animation",
                ),
                styles.colors().TEXT_MUTED,
                Align::Left,
            );
        }
        ui::text(
            list,
            atlas,
            Rect::new(
                area.min[0] + pad,
                area.max[1] - row,
                (area.width() - pad * 2.).max(0.),
                row,
            ),
            &word(
                locale,
                "像素值保持不变 · 从播放头开始 · 一次撤销",
                "Pixels preserved · Starts at playhead · One undo",
            ),
            styles.colors().TEXT_MUTED,
            Align::Left,
        );
    }
    pub(super) fn finish_library_context(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        bounds: Rect,
        state: &Value,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        let Some(mut context) = self.library_context.take() else {
            return;
        };
        if context.owner != self.context_window {
            self.library_context = Some(context);
            return;
        }
        if context.objects != self.library_targets {
            return;
        }
        let source = crate::layer_clipboard::KeyframeSource::of(state);
        if context.source.as_ref().is_some_and(|old| old != &source) {
            return;
        }
        context.source = Some(source);
        if input.focus_lost
            || !input.composing && input.key_presses().any(|(key, _, _)| key == Key::Escape)
        {
            return;
        }
        let objects = &context.objects;
        let layers: Vec<_> = objects
            .iter()
            .filter_map(|object| {
                state["project"]["layers"]
                    .as_array()?
                    .iter()
                    .find(|l| l["id"].as_u64() == Some(*object))
            })
            .collect();
        let editable = !self.presets_busy
            && state["observing"] != true
            && !objects.is_empty()
            && layers.len() == objects.len()
            && layers.iter().all(|l| {
                l["locked"] != true
                    && !matches!(l["content"]["kind"].as_str(), Some("audio" | "camera"))
            });
        let mut rows: Vec<(String, bool, Option<Action>)> = Vec::new();
        match &context.kind {
            Kind::Effect(entry) => {
                rows.push((entry.name.clone(), false, None));
                let mut request = entry.command(0);
                request.as_object_mut().unwrap().remove("object");
                request["objects"] = serde_json::json!(objects);
                rows.push((
                    word(locale, "应用到所选图层", "Apply to selected layers"),
                    editable && layers.iter().all(|l| entry.can_apply(l)),
                    Some(Action::Plugin(request)),
                ));
                let mut favorites = self.favorites.clone();
                let favorite = favorites.remove(&entry.key);
                if !favorite {
                    favorites.insert(entry.key.clone());
                }
                rows.push((
                    word(
                        locale,
                        if favorite {
                            "取消收藏"
                        } else {
                            "加入收藏"
                        },
                        if favorite {
                            "Remove from favorites"
                        } else {
                            "Add to favorites"
                        },
                    ),
                    true,
                    Some(Action::Favorites(favorites)),
                ));
                let found = layers.iter().find_map(|l| {
                    l["effects"]
                        .as_array()?
                        .iter()
                        .find(|e| e["plugin"] == entry.plugin && e["effect"] == entry.id)
                        .and_then(|e| Some((l["id"].as_u64()?, e["id"].as_u64()?)))
                });
                rows.push((
                    word(locale, "定位已添加的效果", "Reveal applied effect"),
                    found.is_some(),
                    found.map(|(object, effect)| Action::RevealLibraryEffect { object, effect }),
                ));
            }
            Kind::Preset(entry) => {
                rows.push((entry.name.clone(), false, None));
                rows.push((
                    word(locale, "应用到所选图层", "Apply to selected layers"),
                    editable,
                    Some(Action::ApplyAnimationPreset {
                        path: entry.path.clone(),
                        objects: objects.clone(),
                    }),
                ));
            }
            Kind::Background => {}
        }
        rows.push((
            word(
                locale,
                "保存此图层变换与效果为预设…",
                "Save this layer's animation…",
            ),
            editable && objects.len() == 1,
            objects.first().map(|object| Action::SaveAnimationPreset {
                object: *object,
                effects: None,
            }),
        ));
        rows.push((
            word(locale, "从文件应用预设…", "Apply preset from file…"),
            editable,
            Some(Action::OpenAnimationPreset),
        ));
        rows.push((
            word(locale, "刷新用户预设", "Refresh user presets"),
            !self.presets_busy,
            Some(Action::RefreshAnimationPresets),
        ));
        let styles = list.styles();
        let row = styles.metric("context.row.height", 20.);
        let pad = styles.metric("padding", 5.);
        let width = styles
            .metric("menu.popup.min_width", 280.)
            .min(bounds.width());
        let height = row * rows.len() as f32 + pad * 2.;
        let card = Rect::new(
            context.anchor[0].clamp(bounds.min[0], (bounds.max[0] - width).max(bounds.min[0])),
            context.anchor[1].clamp(bounds.min[1], (bounds.max[1] - height).max(bounds.min[1])),
            width,
            height,
        );
        let opened = std::mem::take(&mut context.opened);
        let neutral = Input::default();
        let input = if opened || input.composing {
            &neutral
        } else {
            input
        };
        if input
            .pressed
            .is_some_and(|(_, point)| !card.contains(point))
        {
            return;
        }
        box_style(
            list,
            card,
            styles.resolve("context_menu", States::default()).unwrap(),
        );
        let mut clicked_action = false;
        for (index, (label, enabled, action)) in rows.into_iter().enumerate() {
            let rect = Rect::new(
                card.min[0] + pad,
                card.min[1] + pad + index as f32 * row,
                width - pad * 2.,
                row,
            );
            #[cfg(test)]
            self.library_row_hits
                .insert(format!("__context:{index}"), rect);
            let states = if !enabled {
                States::default().with(State::Disabled)
            } else if rect.contains(input.mouse) {
                States::default().with(State::Hovered)
            } else {
                States::default()
            };
            let style = styles.resolve("context_menu.row", states).unwrap();
            box_style(list, rect, style);
            text_style(
                list,
                atlas,
                Rect::new(rect.min[0] + pad, rect.min[1], rect.width() - pad * 2., row),
                &label,
                style,
                Align::Left,
            );
            if enabled && clicked(input, rect) {
                if let Some(action) = action {
                    out.push(action);
                }
                clicked_action = true;
            }
        }
        if !clicked_action {
            self.library_context = Some(context);
        }
    }
}
