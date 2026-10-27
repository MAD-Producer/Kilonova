//! Flat package catalogue projected into metadata categories, never package headings.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub key: String,
    pub name: String,
    pub english: String,
    pub original_name: String,
    pub provider: String,
    pub raw_category: String,
    pub category: String,
    pub plugin: String,
    pub version: String,
    pub hash: String,
    pub id: String,
    pub renderer: String,
    pub supported: bool,
}
impl Entry {
    pub(crate) fn command(&self, object: u64) -> Value {
        serde_json::json!({"op":"add","object":object,"plugin":self.plugin,"version":self.version,"hash":self.hash,"effect":self.id})
    }
    pub(crate) fn matches(&self, query: &str) -> bool {
        let haystack = format!(
            "{} {} {} {} {} {} {} {} {}",
            self.name,
            self.english,
            self.original_name,
            self.id,
            self.category,
            self.raw_category,
            self.provider,
            self.plugin,
            self.key
        )
        .to_lowercase();
        query.split_whitespace().all(|term| haystack.contains(term))
    }
    pub(crate) fn can_apply(&self, layer: &Value) -> bool {
        self.supported
            && layer["id"].as_u64().is_some_and(|id| id != 0)
            && layer["locked"] != true
            && !matches!(layer["content"]["kind"].as_str(), Some("audio" | "null" | "camera"))
            && (layer["content"]["kind"] != "adjustment" || self.renderer == "image")
    }
}
fn version(value: &str) -> (Vec<u64>, String) {
    (
        value.split('.').map(|v| v.parse().unwrap_or(0)).collect(),
        value.into(),
    )
}
pub(crate) fn entries(catalogue: &Value, locale: Locale) -> Vec<Entry> {
    let mut chosen: BTreeMap<String, Entry> = BTreeMap::new();
    for p in catalogue["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| p["enabled"] != false)
    {
        let m = &p["manifest"];
        let plugin = m["id"].as_str().unwrap_or("");
        if plugin.starts_with("local.motionstudio.bound."){continue;}
        let ver = m["version"].as_str().unwrap_or("");
        let hash = p["hash"].as_str().unwrap_or("");
        for e in m["effects"].as_array().into_iter().flatten() {
            let id = e["id"].as_str().unwrap_or("");
            if id.is_empty() || plugin.is_empty() {
                continue;
            }
            let key = format!("{plugin}/{id}");
            let local = if locale == Locale::Zh {
                e["name"].as_str()
            } else {
                e["english_name"].as_str()
            }
            .filter(|s| !s.trim().is_empty())
            .or_else(|| e["name"].as_str())
            .unwrap_or(id);
            let entry = Entry {
                key: key.clone(),
                name: local.into(),
                english: e["english_name"].as_str().unwrap_or("").into(),
                original_name: e["name"].as_str().unwrap_or(id).into(),
                provider: m["name"].as_str().unwrap_or(plugin).into(),
                raw_category: e["category"].as_str().unwrap_or("").into(),
                category: category(e, locale),
                plugin: plugin.into(),
                version: ver.into(),
                hash: hash.into(),
                id: id.into(),
                renderer: e["renderer"].as_str().unwrap_or("image").into(),
                supported: e["compatibility"] != "unsupported",
            };
            if chosen.get(&key).is_none_or(|old| {
                (version(&entry.version), &entry.hash) > (version(&old.version), &old.hash)
            }) {
                chosen.insert(key, entry);
            }
        }
    }
    chosen.into_values().collect()
}
impl Controls {
    #[cfg(test)]
    pub(crate) fn library_row(&self, key: &str) -> Option<Rect> { self.library_row_hits.get(key).copied() }
    pub(crate) fn cancel_library_click(&mut self) { self.library_last_click = None; }
    pub(crate) fn take_library_drag(&mut self) -> Option<(Entry, [f32; 2])> { self.library_drag.take() }
    pub(crate) fn record_recent(&mut self, key: String) {
        self.recent.retain(|k| k != &key);
        self.recent.insert(0, key);
        self.recent.truncate(24);
        self.library_last_click = None;
    }
    pub(crate) fn recent_keys(&self) -> &[String] {
        &self.recent
    }
    pub(crate) fn preview_categories(&mut self, locale: Locale) {
        let entries = entries(&self.catalogue, locale);
        self.collapsed_categories = entries.iter().map(|e| e.category.clone()).collect();
        self.library_initialized = true;
        if let Some(effect) = entries
            .iter()
            .find(|e| e.id == "glow")
            .or_else(|| entries.first())
        {
            self.collapsed_categories.remove(&effect.category);
        }
        self.query.clear();
        self.library_tab = 0;
        self.library_scroll.offset = 0.;
    }
    pub(crate) fn refresh_catalogue(&mut self, catalogue: std::sync::Arc<Value>) {
        let hashes: BTreeSet<_> = self.catalogue["packages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| p["hash"].as_str())
            .map(str::to_string)
            .collect();
        for p in catalogue["packages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|p| p["enabled"] != false && !hashes.contains(p["hash"].as_str().unwrap_or("")))
        {
            for e in p["manifest"]["effects"].as_array().into_iter().flatten() {
                self.collapsed_categories.remove(&category(e, Locale::Zh));
                self.collapsed_categories.remove(&category(e, Locale::En));
            }
        }
        self.catalogue = catalogue;
    }
    pub(super) fn draw_library(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        layer: &Value,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        #[cfg(test)]
        self.library_row_hits.clear();
        let styles = list.styles();
        let colors = styles.colors();
        let zh = locale == Locale::Zh;
        let row = styles.metric("row.height", 22.);
        let pad = 8.;
        let top = area.min[1] + pad;
        let import_width = 56.;
        let response = self.search.draw(
            list,
            atlas,
            Rect::new(
                area.min[0] + pad,
                top,
                (area.width() - pad * 3. - import_width).max(1.),
                24.,
            ),
            &mut self.query,
            if self.library_tab == 3 {
                if zh { "搜索预设" } else { "Search presets" }
            } else { locale.t(T::SearchEffects) },
            input,
            true,
        );
        if ui::button_styled(
            list,
            atlas,
            Rect::new(area.max[0] - pad - import_width, top, import_width, 24.),
            if self.importing_package {
                if zh {
                    "导入中"
                } else {
                    "Importing"
                }
            } else {
                if self.library_tab == 3 { if zh { "应用文件" } else { "Apply…" } } else {
                if zh {
                    "导入"
                } else {
                    "Import"
                }
                }
            },
            input,
            !self.importing_package && (self.library_tab != 3
                || !self.presets_busy && self.library_selection_editable),
            &styles,
            "button.icon",
        )
        .clicked
        {
            out.push(if self.library_tab == 3 { Action::OpenAnimationPreset } else { Action::ImportEffectPackage });
        }
        let tabs_y = top + 30.;
        for (index, title) in [
            if zh { "全部" } else { "All" },
            if zh { "收藏" } else { "Favorites" },
            if zh { "最近" } else { "Recent" },
            if zh { "预设" } else { "Presets" },
        ]
        .into_iter()
        .enumerate()
        {
            let rect = Rect::new(area.min[0] + pad + index as f32 * 58., tabs_y, 54., 22.);
            #[cfg(test)]
            self.library_row_hits.insert(format!("__tab:{index}"), rect);
            ui::text(
                list,
                atlas,
                rect,
                title,
                if index == self.library_tab {
                    colors.ACCENT
                } else {
                    colors.TEXT_MUTED
                },
                Align::Center,
            );
            if index == self.library_tab {
                paint::rect(
                    list,
                    Rect::new(rect.min[0] + 4., rect.max[1] - 1., rect.width() - 8., 1.),
                    colors.ACCENT,
                );
            }
            if clicked(input, rect) {
                if (self.library_tab == 3) != (index == 3) {
                    self.query.clear();
                    self.search.editor.cancel();
                    self.library_last_click = None;
                }
                self.library_tab = index;
                self.library_scroll.offset = 0.;
            }
        }
        if self.library_tab == 3 {
            let pad = styles.metric("padding.panel", 8.);
            let top = tabs_y + styles.metric("row.height", 22.) + pad;
            self.draw_presets(list, atlas, Rect::new(area.min[0], top, area.width(), area.max[1] - top), layer, input, locale, out);
            return;
        }
        let query = self.query.trim().to_lowercase();
        let all = entries(&self.catalogue, locale);
        let total = all.len();
        let selected_info = self
            .library_selected
            .as_ref()
            .and_then(|key| all.iter().find(|e| &e.key == key))
            .map(|e| format!("{} · {}", e.provider, e.version));
        let mut groups: BTreeMap<String, Vec<Entry>> = BTreeMap::new();
        for entry in all.into_iter().filter(|e| {
            e.matches(&query)
                && match self.library_tab {
                    1 => self.favorites.contains(&e.key),
                    2 => self.recent.contains(&e.key),
                    _ => true,
                }
        }) {
            groups
                .entry(entry.category.clone())
                .or_default()
                .push(entry);
        }
        if !self.library_initialized && !groups.is_empty() {
            self.collapsed_categories = groups.keys().cloned().collect();
            self.library_initialized = true;
        }
        if response.changed {
            self.library_scroll.offset = 0.;
        }
        let shown: usize = groups.values().map(Vec::len).sum();
        if self
            .library_selected
            .as_ref()
            .is_some_and(|key| !groups.values().flatten().any(|e| &e.key == key))
        {
            self.library_selected = None;
        }
        let height = groups
            .iter()
            .map(|(name, es)| {
                row + if !query.is_empty() || !self.collapsed_categories.contains(name) {
                    es.len() as f32 * row
                } else {
                    0.
                }
            })
            .sum::<f32>()
            + 8.;
        let body = Rect::new(
            area.min[0],
            tabs_y + 28.,
            area.width(),
            (area.max[1] - tabs_y - 28. - 24.).max(1.),
        );
        let scroll = self.library_scroll.begin(list, body, height, input, false);
        let mut content = list.child();
        let mut y = scroll.content.min[1] + 4.;
        let child = &scroll.input;
        let prior_click = if input.pressed.is_some() {
            self.library_last_click.take()
        } else {
            None
        };
        let mut apply = None;
        for (name, mut effects) in groups {
            effects.sort_by(|a, b| {
                if self.library_tab == 2 {
                    self.recent
                        .iter()
                        .position(|k| k == &a.key)
                        .cmp(&self.recent.iter().position(|k| k == &b.key))
                } else {
                    a.name.cmp(&b.name).then_with(|| a.key.cmp(&b.key))
                }
            });
            let expanded = !query.is_empty() || !self.collapsed_categories.contains(&name);
            let header = Rect::new(
                scroll.viewport.min[0] + 4.,
                y,
                (scroll.viewport.width() - 8.).max(1.),
                row,
            );
            if header.contains(child.mouse) {
                paint::rect(&mut content, header, colors.TEXT.with_alpha(0.04));
            }
            let x = header.min[0] + 9.;
            let cy = y + row * 0.5;
            paint::line(
                &mut content,
                if expanded {
                    vec![[x - 3., cy - 1.], [x, cy + 2.], [x + 3., cy - 1.]]
                } else {
                    vec![[x - 1., cy - 3.], [x + 2., cy], [x - 1., cy + 3.]]
                },
                colors.TEXT_MUTED,
            );
            ui::text(
                &mut content,
                atlas,
                Rect::new(header.min[0] + 22., y, (header.width() - 66.).max(1.), row),
                &name,
                colors.TEXT,
                Align::Left,
            );
            ui::text(
                &mut content,
                atlas,
                Rect::new(header.max[0] - 36., y, 30., row),
                &effects.len().to_string(),
                colors.TEXT_MUTED.with_alpha(0.7),
                Align::Right,
            );
            if clicked(child, header) {
                if expanded {
                    self.collapsed_categories.insert(name.clone());
                } else {
                    self.collapsed_categories.remove(&name);
                }
            }
            y += row;
            if !expanded {
                continue;
            }
            for e in effects {
                let rect = Rect::new(
                    scroll.viewport.min[0] + 4.,
                    y,
                    (scroll.viewport.width() - 8.).max(1.),
                    row,
                );
                if child.pressed.is_some_and(|(button, point)| button == MouseButton::Right && rect.contains(point)) {
                    self.library_context = Some(super::library_actions::Context::effect(e.clone(), child.mouse, self.library_targets.clone(), self.context_window));
                    self.library_last_click = None;
                }
                #[cfg(test)]
                if rect.min[1] >= scroll.viewport.min[1] && rect.max[1] <= scroll.viewport.max[1] {
                    self.library_row_hits.insert(e.key.clone(), rect);
                }
                let mut states = motion_ui::style::States::default();
                if !e.supported {
                    states = states.with(motion_ui::style::State::Disabled);
                }
                if rect.contains(child.mouse) {
                    states = states.with(motion_ui::style::State::Hovered);
                }
                if self.library_selected.as_ref() == Some(&e.key) {
                    states = states.with(motion_ui::style::State::Selected);
                }
                let style = styles.resolve("row", states).unwrap();
                motion_ui::style::box_style(&mut content, rect, style);
                motion_ui::icons::draw(
                    &mut content,
                    Rect::new(rect.min[0] + 20., y, 16., row),
                    motion_ui::icons::Icon::Fx,
                    12.,
                    if e.supported {
                        colors.TEXT_MUTED
                    } else {
                        styles.color("disabled")
                    },
                );
                motion_ui::style::text_style(
                    &mut content,
                    atlas,
                    Rect::new(rect.min[0] + 42., y, (rect.width() - 70.).max(1.), row),
                    &e.name,
                    style,
                    Align::Left,
                );
                let star = Rect::new(rect.max[0] - 24., y, 20., row);
                if let Some(value) = motion_ui::widgets::StarRating::draw(
                    &mut content,
                    star,
                    usize::from(self.favorites.contains(&e.key)),
                    1,
                    child,
                    true,
                    false,
                ) {
                    if value == 0 {
                        self.favorites.remove(&e.key);
                    } else {
                        self.favorites.insert(e.key.clone());
                    }
                    out.push(Action::Favorites(self.favorites.clone()));
                }
                if clicked(
                    child,
                    Rect::new(rect.min[0], y, (rect.width() - 28.).max(1.), row),
                ) {
                    let now = std::time::Instant::now();
                    #[cfg(test)]
                    let now = self.library_test_click_time.unwrap_or(now);
                    let double = prior_click.as_ref().is_some_and(|(key, time)| {
                        key == &e.key
                            && now.duration_since(*time) < std::time::Duration::from_millis(400)
                    });
                    self.library_selected = Some(e.key.clone());
                    self.library_last_click = Some((e.key.clone(), now));
                    if double && e.can_apply(layer) {
                        apply = Some(e.clone());
                    } else if e.supported {
                        self.library_drag = child.pressed.map(|(_, point)| (e.clone(), point));
                    }
                }
                if !response.active
                    && !response.submitted
                    && self.library_selected.as_ref() == Some(&e.key)
                    && input
                        .key_presses()
                        .any(|(k, _, _)| k == motion_ui::input::Key::Enter)
                    && e.can_apply(layer)
                {
                    apply = Some(e);
                }
                y += row;
            }
        }
        content.clip_to(scroll.viewport);
        list.append(content);
        if shown == 0 {
            ui::text(
                list,
                atlas,
                body.shrink(12.),
                if zh {
                    "没有匹配的效果"
                } else {
                    "No matching effects"
                },
                colors.TEXT_MUTED,
                Align::Left,
            );
        }
        if let Some(entry) = apply {
            self.recent.retain(|k| k != &entry.key);
            self.recent.insert(0, entry.key.clone());
            self.recent.truncate(24);
            out.push(Action::Plugin(entry.command(layer["id"].as_u64().unwrap())));
        }
        let footer = Rect::new(
            area.min[0] + 8.,
            area.max[1] - 24.,
            (area.width() - 16.).max(1.),
            24.,
        );
        ui::text(
            list,
            atlas,
            footer,
            &if layer["id"].as_u64().unwrap_or(0) == 0 {
                if zh {
                    format!("{total} 个效果 · 选择图层后双击添加")
                } else {
                    format!("{total} effects · Select a layer to apply")
                }
            } else {
                selected_info.unwrap_or_else(|| {
                    if zh {
                        format!("{shown} 个效果 · 双击或 Enter 添加")
                    } else {
                        format!("{shown} effects · Double-click or Enter")
                    }
                })
            },
            colors.TEXT_MUTED,
            Align::Left,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn catalogue() -> Value {
        serde_json::json!({"packages":[
            {"enabled":true,"hash":"a","manifest":{"id":"a","version":"1.0.0","name":"Package One","effects":[{"id":"blur","name":"模糊","english_name":"Blur","category":"自定义/空间_风格"},{"id":"glow","name":"辉光","english_name":"Glow","category":""}]}},
            {"enabled":true,"hash":"b","manifest":{"id":"b","version":"1.0.0","name":"Package Two","effects":[{"id":"other","name":"另外","english_name":"Other","category":"自定义/空间_风格"}]}},
            {"enabled":false,"hash":"disabled","manifest":{"id":"off","version":"1.0.0","effects":[{"id":"invisible","category":"隐藏"}]}}
        ]})
    }
    fn click(x: f32, y: f32) -> Input {
        Input {
            mouse: [x, y],
            pressed: Some((MouseButton::Left, [x, y])),
            ..Default::default()
        }
    }
    fn draw(control: &mut Controls, layer: &Value, input: &Input) -> Vec<Action> {
        let now = control
            .library_test_click_time
            .unwrap_or_else(std::time::Instant::now)
            + std::time::Duration::from_millis(100);
        draw_at(control, layer, input, now)
    }
    fn draw_at(
        control: &mut Controls,
        layer: &Value,
        input: &Input,
        now: std::time::Instant,
    ) -> Vec<Action> {
        // Synthetic clicks carry explicit time, independently of font loading.
        control.library_test_click_time = Some(now);
        let mut out = vec![];
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        control.draw_library(
            &mut PaintList::default(),
            &mut atlas,
            Rect::new(0., 0., 300., 400.),
            layer,
            input,
            Locale::Zh,
            &mut out,
        );
        out
    }
    #[test]
    fn categories_use_manifest_metadata_and_never_package_names_or_effect_id_guesses() {
        let es = entries(&catalogue(), Locale::Zh);
        assert_eq!(es.len(), 3);
        assert_eq!(
            es.iter()
                .filter(|e| e.category == "自定义/空间_风格")
                .count(),
            2
        );
        assert_eq!(
            es.iter().find(|e| e.id == "glow").unwrap().category,
            "未分类"
        );
        assert!(es.iter().all(|e| !e.category.starts_with("Package")));
        assert!(es
            .iter()
            .find(|e| e.id == "blur")
            .unwrap()
            .matches("blur 自定义"));
    }
    #[test]
    fn newest_enabled_version_wins_without_collapsing_different_providers() {
        let mut c = catalogue();
        let mut newer = c["packages"][0].clone();
        newer["manifest"]["version"] = "2.10.0".into();
        newer["hash"] = "new".into();
        c["packages"].as_array_mut().unwrap().push(newer);
        let mut older = c["packages"][0].clone();
        older["manifest"]["version"] = "2.9.0".into();
        older["hash"] = "old".into();
        c["packages"].as_array_mut().unwrap().push(older);
        let es = entries(&c, Locale::Zh);
        assert_eq!(es.len(), 3);
        assert_eq!(
            es.iter().find(|e| e.key == "a/blur").unwrap().version,
            "2.10.0"
        );
    }
    #[test]
    fn import_button_works_without_a_selected_layer_and_browsing_does_not_apply() {
        let mut controls = Controls {
            catalogue: std::sync::Arc::new(catalogue()),
            ..Default::default()
        };
        let layer = serde_json::json!({"id":0,"locked":true});
        assert!(matches!(
            draw(&mut controls, &layer, &click(265., 20.)).as_slice(),
            [Action::ImportEffectPackage]
        ));
        assert!(draw(&mut controls, &layer, &click(50., 81.)).is_empty());
        assert!(draw(&mut controls, &layer, &click(80., 103.)).is_empty());
        assert!(draw(&mut controls, &layer, &click(80., 103.)).is_empty());
    }
    #[test]
    fn row_selection_double_click_and_enter_apply_exact_package_identity() {
        let mut controls = Controls {
            catalogue: std::sync::Arc::new(
                serde_json::json!({"packages":[{"enabled":true,"hash":"a","manifest":{"id":"a","version":"1.0.0","name":"A","effects":[{"id":"blur","name":"模糊","english_name":"Blur","category":"模糊与锐化"}]}}]}),
            ),
            ..Default::default()
        };
        let layer = serde_json::json!({"id":7,"locked":false,"content":{"kind":"solid"}});
        assert!(draw(&mut controls, &layer, &Input::default()).is_empty());
        assert!(draw(&mut controls, &layer, &click(40., 81.)).is_empty());
        assert!(draw(&mut controls, &layer, &click(80., 103.)).is_empty());
        let action = draw(&mut controls, &layer, &click(80., 103.));
        assert!(
            matches!(action.as_slice(),[Action::Plugin(v)]if v["object"]==7&&v["plugin"]=="a"&&v["hash"]=="a"&&v["effect"]=="blur")
        );
        controls.library_last_click = None;
        assert!(matches!(
            draw(
                &mut controls,
                &layer,
                &Input {
                    keys: vec![(motion_ui::input::Key::Enter, Default::default())],
                    ..Default::default()
                }
            )
            .as_slice(),
            [Action::Plugin(_)]
        ));
    }
    #[test]
    fn double_click_interval_and_intervening_click_preserve_exact_package_identity() {
        use std::time::{Duration, Instant};
        for interval in [399, 400, 401] {
            let mut controls = Controls {
                catalogue: std::sync::Arc::new(
                    serde_json::json!({"packages":[{"enabled":true,"hash":"a","manifest":{"id":"a","version":"1.0.0","name":"A","effects":[{"id":"blur","name":"模糊","english_name":"Blur","category":"模糊与锐化"}]}}]}),
                ),
                ..Default::default()
            };
            let layer = serde_json::json!({"id":7,"locked":false,"content":{"kind":"solid"}});
            draw(&mut controls, &layer, &Input::default());
            draw(&mut controls, &layer, &click(40., 81.));
            let now = Instant::now();
            assert!(draw_at(&mut controls, &layer, &click(80., 103.), now).is_empty());
            let action = draw_at(
                &mut controls,
                &layer,
                &click(80., 103.),
                now + Duration::from_millis(interval),
            );
            if interval < 400 {
                assert!(
                    matches!(action.as_slice(), [Action::Plugin(v)] if v["object"]==7 && v["plugin"]=="a" && v["hash"]=="a" && v["effect"]=="blur")
                );
            } else {
                assert!(action.is_empty());
                assert!(
                    matches!(draw_at(&mut controls, &layer, &click(80.,103.), now + Duration::from_millis(interval + 100)).as_slice(), [Action::Plugin(v)] if v["effect"]=="blur")
                );
            }
            controls.library_last_click = None;
            assert!(draw_at(
                &mut controls,
                &layer,
                &click(80., 103.),
                now + Duration::from_millis(1000)
            )
            .is_empty());
            assert!(draw_at(
                &mut controls,
                &layer,
                &click(280., 300.),
                now + Duration::from_millis(1100)
            )
            .is_empty());
            assert!(draw_at(
                &mut controls,
                &layer,
                &click(80., 103.),
                now + Duration::from_millis(1200)
            )
            .is_empty());
        }
    }
    #[test]
    fn delayed_fixture_work_preserves_synthetic_double_click_interval_and_disabled_target() {
        use std::time::{Duration, Instant};
        let mut controls = Controls {
            catalogue: std::sync::Arc::new(
                serde_json::json!({"packages":[{"enabled":true,"hash":"a","manifest":{"id":"a","version":"1.0.0","name":"A","effects":[{"id":"blur","name":"模糊","english_name":"Blur","category":"模糊与锐化"}]}}]}),
            ),
            ..Default::default()
        };
        let mut layer = serde_json::json!({"id":7,"locked":false,"content":{"kind":"solid"}});
        draw(&mut controls, &layer, &Input::default());
        draw(&mut controls, &layer, &click(40., 81.));
        let now = Instant::now();
        assert!(draw_at(&mut controls, &layer, &click(80., 103.), now).is_empty());
        std::thread::sleep(Duration::from_millis(450));
        assert!(
            matches!(draw_at(&mut controls,&layer,&click(80.,103.),now+Duration::from_millis(100)).as_slice(), [Action::Plugin(v)] if v["object"]==7 && v["plugin"]=="a" && v["hash"]=="a" && v["effect"]=="blur")
        );
        controls.library_last_click = None;
        layer["locked"] = true.into();
        assert!(draw_at(
            &mut controls,
            &layer,
            &click(80., 103.),
            now + Duration::from_millis(200)
        )
        .is_empty());
        assert!(draw_at(
            &mut controls,
            &layer,
            &click(80., 103.),
            now + Duration::from_millis(300)
        )
        .is_empty());
    }
}

#[cfg(test)]
mod scroll_tests {
    use super::*;
    #[test]fn scrolling_the_library_keeps_the_import_button_fixed(){
        let effects:Vec<_>=(0..100).map(|i|serde_json::json!({"id":format!("effect{i}"),"name":format!("效果{i}"),"english_name":format!("Effect{i}"),"category":"测试分类"})).collect();
        let mut controls=Controls{catalogue:std::sync::Arc::new(serde_json::json!({"packages":[{"hash":"h","manifest":{"id":"p","version":"1.0.0","name":"P","effects":effects}}]})),..Default::default()};
        controls.preview_categories(Locale::Zh);let mut atlas=TextAtlas::from_system(12.).unwrap();let layer=serde_json::json!({"id":0,"locked":true});let mut out=vec![];
        controls.draw_library(&mut PaintList::default(),&mut atlas,Rect::new(0.,0.,300.,400.),&layer,&Input{mouse:[100.,200.],wheel:[0.,500.],..Default::default()},Locale::Zh,&mut out);assert!(controls.library_scroll.offset>0.);assert!(out.is_empty());
        controls.draw_library(&mut PaintList::default(),&mut atlas,Rect::new(0.,0.,300.,400.),&layer,&Input{mouse:[265.,20.],pressed:Some((MouseButton::Left,[265.,20.])),..Default::default()},Locale::Zh,&mut out);assert!(matches!(out.as_slice(),[Action::ImportEffectPackage]));
    }
}

#[cfg(test)]mod bound_tests{
    use super::*;
    #[test]fn bound_runtime_versions_are_hidden_from_public_effect_search(){
        let catalogue=serde_json::json!({"packages":[{"manifest":{"id":"local.motionstudio.bound.abc","version":"0.0.0+123","effects":[{"id":"node","name":"节点效果","category":"颜色"}]}},{"manifest":{"id":"published.node.effect","version":"1.0.0","effects":[{"id":"node","name":"节点效果","category":"颜色"}]}}]});
        let visible=entries(&catalogue,Locale::Zh);assert_eq!(visible.len(),1);assert_eq!(visible[0].plugin,"published.node.effect");
    }
}
