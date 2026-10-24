//! Catalogue and parameter controls share the core effect command contracts.
use crate::{
    i18n::{Locale, Text as T},
    panels::{color_well, Action},
};
use motion_core::EffectParam;
use motion_core::{Command, EffectAction};
use motion_ui::{
    icons::{self, Icon},
    input::{Input, Key, MouseButton, Rect},
    paint::{self, PaintList},
    style::{box_style, text_style, State, States},
    text::TextAtlas,
    ui::{self, Align},
};
use serde_json::Value;
use std::collections::HashMap;
mod curve;
mod key_navigation;
mod library;
mod track_cache;
#[cfg(test)]
mod track_cache_tests;
#[cfg(test)]
mod removal_tests;
pub(crate) use library::{entries, Entry};

fn word(locale: Locale, zh: &str, en: &str) -> String {
    if locale == Locale::Zh {
        zh.into()
    } else {
        en.into()
    }
}

fn context_row(card: Rect, width: f32, row: f32, index: usize) -> Rect {
    Rect::new(
        card.min[0] + 3.,
        card.min[1] + 3. + index as f32 * row,
        width - 6.,
        row,
    )
}

#[derive(Default)]
pub struct Controls {
    pub(super) property_filter: super::types::PropertyFilter,
    pub(crate) selection_probe: super::property_focus::HitProbe,
    pub(super) expressions: super::expressions::Controls,
    pub catalogue: std::sync::Arc<Value>,
    pub cursor: bool,
    pub drag_readout: Option<([f32; 2], String)>,
    pub choosing: bool,
    library_tab: usize,
    library_initialized: bool,
    library_scroll: motion_ui::widgets::ScrollArea,
    library_selected: Option<String>,
    library_drag: Option<(Entry, [f32; 2])>,
    #[cfg(test)]
    library_row_hits: HashMap<String, Rect>,
    library_last_click: Option<(String, std::time::Instant)>,
    /// Synthetic input time; absent from production builds.
    #[cfg(test)]
    library_test_click_time: Option<std::time::Instant>,
    pub importing_package: bool,
    recent: Vec<String>,
    query: String,
    search: motion_ui::widgets::SearchField,
    dropdowns: HashMap<(u64, String), motion_ui::widgets::Dropdown>,
    enum_owner: Option<(u64, String)>,
    pub favorites: std::collections::BTreeSet<String>,
    field: Option<Field>,
    revision: Option<(String, String, u64)>,
    tracks: track_cache::TrackCache,
    curve_editors: HashMap<(u64, u64, String), curve::Editor>,
    collapsed_categories: std::collections::HashSet<String>,
    collapsed_effects: std::collections::HashSet<u64>,
    revealed_effect: Option<(u64, u64)>,
    reveal_pending: bool,
    context_menu: Option<EffectContextMenu>,
    color_popup: Option<ColorPopup>,
    collapsed_curves: std::collections::HashSet<(u64, u64, String)>,
    key_channel: Option<key_navigation::Channel>,
    // Display projection only; key commands remain with the controller selected
    // by the last real input, even when both effect hosts are visible.
    mirrored_channel: Option<key_navigation::Channel>,
    selection_intent: bool,
    /// component -2 = popup frame, -1 = swatch, 0.. = popup channel fields.
    #[cfg(test)]
    pub(crate) test_rects: HashMap<(u64, u64, String, i64), Rect>,
}
#[derive(Clone)]
struct ColorPopup {
    object: u64,
    effect: u64,
    param: String,
    anchor: Rect,
    opened: bool,
}
struct EffectContextMenu {
    anchor: [f32; 2],
    object: u64,
    control: String,
    effect: String,
    category: Option<String>,
    instance: Option<(u64, usize)>,
    catalogue: bool,
    expression: Option<motion_core::ExpressionTarget>,
    expression_supported: bool,
    opened: bool,
}
struct Field {
    object: u64,
    effect: u64,
    param: String,
    component: usize,
    number: super::numeric::Field,
    rect: Rect,
}
fn clicked(input: &Input, rect: Rect) -> bool {
    input
        .pressed
        .is_some_and(|(b, p)| b == MouseButton::Left && rect.contains(p))
}
fn text(list: &mut PaintList, atlas: &mut TextAtlas, rect: Rect, value: &str, muted: bool) {
    let styles = list.styles();
    let colors = styles.colors();

    ui::text(
        list,
        atlas,
        rect,
        value,
        if muted {
            colors.TEXT_MUTED
        } else {
            colors.TEXT
        },
        Align::Left,
    );
}
fn press(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    input: &Input,
    rect: Rect,
    label: &str,
    enabled: bool,
) -> bool {
    let styles = list.styles();
    ui::button_styled(
        list,
        atlas,
        rect,
        label,
        input,
        enabled,
        &styles,
        "button.icon",
    )
    .clicked
}
fn press_icon(list: &mut PaintList, input: &Input, rect: Rect, icon: Icon, enabled: bool) -> bool {
    ui::button_icon(list, rect, icon, input, enabled).clicked
}

pub(super) fn category(effect: &Value, locale: Locale) -> String {
    let key = effect["category"].as_str().unwrap_or("").trim();
    let zh = locale == Locale::Zh;
    match key {
        "" => {
            if zh {
                "未分类"
            } else {
                "Uncategorized"
            }
        }
        "blur-sharpen" => {
            if zh {
                "模糊与锐化"
            } else {
                "Blur & Sharpen"
            }
        }
        "color-correction" => {
            if zh {
                "颜色校正"
            } else {
                "Color Correction"
            }
        }
        "lighting" => {
            if zh {
                "光效"
            } else {
                "Lighting"
            }
        }
        "particles" => {
            if zh {
                "粒子"
            } else {
                "Particles"
            }
        }
        "distort" => {
            if zh {
                "扭曲"
            } else {
                "Distort"
            }
        }
        "noise-grain" => {
            if zh {
                "噪波与颗粒"
            } else {
                "Noise & Grain"
            }
        }
        _ => key,
    }
    .to_string()
}

fn dimensions(kind: &str) -> usize {
    match kind {
        "vec2" => 2,
        "vec3" => 3,
        "color" => 4,
        _ => 1,
    }
}
impl Controls {
    pub(super) fn text_menu(&mut self, request: &mut super::text_menu::Request<'_>) {
        if request.query(&mut self.search.editor, &mut self.query) {
            self.library_scroll.offset = 0.;
            self.library_selected = None;
        }
        if let Some(field) = &mut self.field { request.visit(&mut field.number.editor.text); }
        self.expressions.text_menu(request);
    }
    pub(crate) fn reveal_effect(&mut self, object: u64, effect: u64) {
        self.choosing = false;
        self.property_filter = Default::default();
        self.collapsed_effects.remove(&effect);
        self.revealed_effect = Some((object, effect));
        self.reveal_pending = true;
    }

    pub(super) fn take_reveal_offset(
        &mut self, layer: &Value, styles: &motion_ui::style::StyleSheet,
        locale: Locale, width: f32,
    ) -> Option<f32> {
        let (object, effect) = self.revealed_effect?;
        if !self.reveal_pending || layer["id"] != object { return None; }
        self.reveal_pending = false;
        let effects = layer["effects"].as_array()?;
        let index = effects.iter().position(|item| item["id"] == effect)?;
        if index == 0 { return Some(0.); }
        // A one-off projection through the same height calculation as drawing,
        // including preceding curves and collapsed groups. Never estimate rows.
        let mut prefix = layer.clone();
        prefix["effects"] = effects[..index].to_vec().into();
        Some((self.height(&prefix, styles, locale, width)
            - styles.metric("padding.panel", 8.)).max(0.))
    }

    pub(super) fn reveal_properties(&mut self, state: &Value, object: u64) {
        for effect in state["project"]["layers"].as_array().into_iter().flatten()
            .filter(|layer| layer["id"] == object)
            .flat_map(|layer| layer["effects"].as_array().into_iter().flatten()) {
            if let Some(id) = effect["id"].as_u64() { self.collapsed_effects.remove(&id); }
        }
    }
    pub fn editing(&self) -> bool {
        self.expressions.editing()
            || self.field.is_some()
            || self.choosing
            || self.enum_owner.is_some()
            || self.context_menu.is_some()
            || self.color_popup.is_some()
            || self
                .curve_editors
                .values()
                .any(|editor| editor.dragging() || editor.modal())
    }
    pub(crate) fn library_search_editing(&self) -> bool {
        self.search.editor.active()
    }
    pub(crate) fn curve_gesture_active(&self) -> bool {
        self.curve_editors.values().any(|editor| editor.dragging())
    }
    pub fn cancel(&mut self) -> bool {
        self.expressions.cancel();
        let mut begun = self.field.take().is_some_and(|mut f| f.number.cancel());
        for editor in self.curve_editors.values_mut() {
            begun |= editor.cancel();
        }
        self.choosing = false;
        self.enum_owner = None;
        self.context_menu = None;
        self.color_popup = None;
        for dropdown in self.dropdowns.values_mut() {
            dropdown.open = false;
        }
        begun
    }
    pub(super) fn definition(&self, effect: &Value) -> Option<Value> {
        self.catalogue["packages"].as_array()?.iter().find(|p| {
            p["hash"] == effect["hash"]
                && p["manifest"]["id"] == effect["plugin"]
                && p["manifest"]["version"] == effect["version"]
        })?["manifest"]["effects"]
            .as_array()?
            .iter()
            .find(|d| d["id"] == effect["effect"])
            .cloned()
    }
    pub fn height(
        &self,
        layer: &Value,
        styles: &motion_ui::style::StyleSheet,
        locale: Locale,
        width: f32,
    ) -> f32 {
        let row = styles
            .metric("effect.row.height", 26.)
            .max(styles.metric("row.height", 22.));
        let header = styles
            .metric("effect.header.height", 30.)
            .max(styles.metric("row.height", 22.));
        let pad = styles.metric("padding.panel", 8.);
        let toolbar =
            styles.metric("effect.toolbar.height", 28.) + styles.metric("effect.toolbar.gap", 6.);
        let secondary = styles.metric("effect.secondary.height", 24.)
            + styles.metric("effect.secondary.gap", 4.);
        let top = pad * 2. + toolbar;
        if self.choosing {
            let mut counts: HashMap<String, usize> = HashMap::new();
            let query = self.query.trim().to_lowercase();
            for entry in entries(&self.catalogue, locale)
                .into_iter()
                .filter(|entry| entry.supported && entry.matches(&query))
            {
                if self.library_tab == 1 && !self.favorites.contains(&entry.key)
                    || self.library_tab == 2 && !self.recent.contains(&entry.key)
                {
                    continue;
                }
                *counts.entry(entry.category).or_default() += 1;
            }
            return top
                + toolbar
                + secondary
                + counts
                    .iter()
                    .map(|(category, count)| {
                        styles.metric("effect.category.height", 28.)
                            + styles.metric("effect.category.gap", 4.)
                            + if self.collapsed_categories.contains(category) {
                                0.
                            } else {
                                *count as f32 * styles.metric("row.height", 22.)
                            }
                    })
                    .sum::<f32>();
        }
        let effects = layer["effects"].as_array();
        top + if effects.is_none_or(Vec::is_empty) {
            secondary
        } else {
            effects
                .into_iter()
                .flatten()
                .map(|effect| {
                    styles.metric("effect.group.gap", 8.)
                        + header
                        + if self
                            .collapsed_effects
                            .contains(&effect["id"].as_u64().unwrap_or(0))
                        {
                            0.
                        } else {
                            self.definition(effect).map_or(0., |definition| {
                                definition["params"]
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                                    .map(|parameter| {
                                        row + if parameter["kind"] == "curve"
                                            && !self.collapsed_curves.contains(&(
                                                layer["id"].as_u64().unwrap_or(0),
                                                effect["id"].as_u64().unwrap_or(0),
                                                parameter["id"].as_str().unwrap_or("").into(),
                                            )) {
                                            curve::height(
                                                styles,
                                                (width
                                                    - pad * 2.
                                                    - styles.metric("tree.indent", 14.))
                                                .max(0.)
                                                .min(styles.metric("effect.curve.width", 320.)),
                                            )
                                        } else {
                                            0.
                                        }
                                    })
                                    .sum::<f32>()
                            })
                        }
                        + if effect["node_graph"].is_object() {
                            row
                        } else {
                            0.
                        }
                })
                .sum::<f32>()
        }
    }
    pub fn render(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: &mut f32,
        layer: &Value,
        state: &Value,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        if input.pressed.is_some() {
            self.clear_key_channel();
        }
        self.render_mode(
            false, list, atlas, area, y, layer, state, input, locale, out,
        );
    }

    pub fn finish_context_menu(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        bounds: Rect,
        state: &Value,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        let categories = if self
            .context_menu
            .as_ref()
            .is_some_and(|menu| !menu.catalogue)
        {
            vec![]
        } else {
            self.context_categories(locale)
        };
        let Some(menu) = self.context_menu.as_mut() else {
            return;
        };
        let opened = std::mem::take(&mut menu.opened);
        let instance = menu.instance;
        let expression = menu.expression.clone();
        let expression_supported = menu.expression_supported;
        let neutral = Input::default();
        let input = if opened { &neutral } else { input };
        let anchor = menu.anchor;
        let object = menu.object;
        let control = menu.control.clone();
        let effect = menu.effect.clone();
        let mut active_category = menu.category.clone();
        let styles = list.styles();
        let row = styles.metric("context.row.height", 20.).clamp(18., 24.);
        let width = 250f32.min(bounds.width());
        let action_count = if instance.is_some() { 3 } else { 0 };
        let height =
            (4 + action_count + categories.len() + usize::from(expression.is_some())) as f32 * row
                + 6.;
        let card = Rect::new(
            anchor[0].clamp(bounds.min[0], (bounds.max[0] - width).max(bounds.min[0])),
            (anchor[1] - height).clamp(bounds.min[1], (bounds.max[1] - height).max(bounds.min[1])),
            width,
            height,
        );
        if input.focus_lost
            || input.keys.iter().any(|(key, _)| *key == Key::Escape)
            || input
                .pressed
                .is_some_and(|(_, point)| !card.contains(point) && active_category.is_none())
        {
            self.context_menu = None;
            return;
        }
        box_style(
            list,
            card,
            styles.resolve("context_menu", States::default()).unwrap(),
        );
        let mut chosen = None;
        let removal_count = super::effect_removal::commands(state, &[object]).map(Iterator::count);
        let clear_effects = match (locale, removal_count) {
            (Locale::Zh, Some(count)) => format!("移除此图层全部{count}项效果"),
            (Locale::En, Some(count)) => format!("Remove this layer's effects ({count})"),
            (Locale::Zh, None) => "移除此图层全部效果".into(),
            (Locale::En, None) => "Remove this layer's effects".into(),
        };
        let locked = state["observing"] == true
            || state["project"]["layers"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|layer| layer["id"].as_u64() == Some(object))
                .is_none_or(|layer| layer["locked"] == true);
        let mut header: Vec<(String, bool, Option<EffectAction>)> = vec![
            (control.clone(), false, None),
            (effect.clone(), false, None),
        ];
        if let Some((id, index)) = instance {
            header.extend([
                (
                    word(locale, "上移效果", "Move Up"),
                    !locked && index > 0,
                    Some(EffectAction::Move {
                        effect: id,
                        index: index.saturating_sub(1),
                    }),
                ),
                (
                    word(locale, "复制效果", "Duplicate Effect"),
                    !locked,
                    Some(EffectAction::Duplicate { effect: id }),
                ),
                (
                    word(locale, "删除效果", "Remove Effect"),
                    !locked,
                    Some(EffectAction::Remove { effect: id }),
                ),
            ]);
        }
        let clear_index = header.len();
        header.push((clear_effects, removal_count.is_some_and(|count| count > 0), None));
        let expression_index = expression.as_ref().map(|_| {
            let index = header.len();
            header.push((if expression_supported {
                word(locale, "编辑表达式", "Edit expression")
            } else {
                word(locale, "表达式不可用：需连续可动画参数", "Expressions: continuous params only")
            }, !locked && expression_supported, None));
            index
        });
        let header_count = header.len();
        for (index, (label, enabled, _)) in header.iter().enumerate() {
            let area = context_row(card, width, row, index);
            #[cfg(test)]
            if let Some((id, _)) = instance {
                self.test_rects
                    .insert((object, id, "__menu".into(), index as i64), area);
            }
            let mut states = States::default();
            if *enabled && area.contains(input.mouse) {
                states = states.with(State::Selected);
            }
            if !*enabled {
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
                    area.width() - 10.,
                    area.height(),
                ),
                label,
                style,
                Align::Left,
            );
            if *enabled && clicked(input, area) {
                chosen = Some(index);
            }
        }
        paint::line(
            list,
            vec![
                [
                    card.min[0] + 4.,
                    card.min[1] + header_count as f32 * row + 3.,
                ],
                [
                    card.max[0] - 4.,
                    card.min[1] + header_count as f32 * row + 3.,
                ],
            ],
            styles.color("context.border"),
        );
        let mut hovered = None;
        for (offset, category) in categories.iter().enumerate() {
            let index = offset + header_count;
            let area = context_row(card, width, row, index);
            let selected = active_category.as_deref() == Some(category.as_str());
            let mut states = States::default();
            if area.contains(input.mouse) || selected {
                states = states.with(State::Selected);
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
                category,
                style,
                Align::Left,
            );
            paint::line(
                list,
                vec![
                    [area.max[0] - 13., area.min[1] + row * 0.5 - 4.],
                    [area.max[0] - 9., area.min[1] + row * 0.5],
                    [area.max[0] - 13., area.min[1] + row * 0.5 + 4.],
                ],
                style.foreground,
            );
            if area.contains(input.mouse) {
                hovered = Some(category.clone());
            }
        }
        if let Some(category) = hovered {
            active_category = Some(category);
        }
        if let Some(category) = active_category.as_deref() {
            self.draw_effect_submenu(list, atlas, card, row, category, object, input, locale, out);
        }
        if let Some(menu) = self.context_menu.as_mut() {
            menu.category = active_category;
        }
        if chosen.is_some() && chosen == expression_index {
            self.expressions.open(
                state,
                expression.unwrap(),
                motion_ui::dock::Panel::EffectControls,
            );
            self.context_menu = None;
        } else if let Some(operation) = chosen.and_then(|index| header[index].2.clone()) {
            out.push(Action::Edit(Command::Effect {
                object,
                action: operation,
            }));
            self.context_menu = None;
        } else if chosen == Some(clear_index) {
            let commands = super::effect_removal::commands(state, &[object])
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
            if !commands.is_empty() {
                out.push(Action::EditBatch(commands));
            }
            self.context_menu = None;
        } else if chosen.is_some() {
            self.context_menu = None;
        }
    }

    fn context_categories(&self, locale: Locale) -> Vec<String> {
        let mut categories = std::collections::BTreeSet::new();
        for package in self.catalogue["packages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|package| package["enabled"] != false)
        {
            let fallback = package["manifest"]["name"].as_str().unwrap_or("Effects");
            for effect in package["manifest"]["effects"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let category = effect["category"].as_str().unwrap_or(fallback);
                let category = if locale == Locale::Zh {
                    effect["category_name"].as_str().unwrap_or(category)
                } else {
                    category
                };
                categories.insert(category.to_string());
            }
        }
        categories.into_iter().collect()
    }

    fn draw_effect_submenu(
        &self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        card: Rect,
        row: f32,
        category: &str,
        object: u64,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        let mut effects = Vec::new();
        for package in self.catalogue["packages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|package| package["enabled"] != false)
        {
            let fallback = package["manifest"]["name"].as_str().unwrap_or("Effects");
            for effect in package["manifest"]["effects"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|effect| effect["category"].as_str().unwrap_or(fallback) == category)
            {
                effects.push((
                    package["manifest"]["id"].as_str().unwrap_or("").to_string(),
                    package["manifest"]["version"]
                        .as_str()
                        .unwrap_or("")
                        .to_string(),
                    package["hash"].as_str().unwrap_or("").to_string(),
                    effect["id"].as_str().unwrap_or("").to_string(),
                    if locale == Locale::Zh {
                        effect["name"].as_str().unwrap_or("Effect")
                    } else {
                        effect["english_name"].as_str().unwrap_or("Effect")
                    }
                    .to_string(),
                ));
            }
        }
        effects.sort_by(|a, b| a.4.cmp(&b.4));
        let width = 250f32.min((card.max[0] - card.min[0]).max(1.));
        let submenu = Rect::new(
            card.max[0] - 1.,
            (card.max[1] - effects.len() as f32 * row - 6.).max(card.min[1]),
            width,
            effects.len() as f32 * row + 6.,
        );
        let styles = list.styles();
        box_style(
            list,
            submenu,
            styles.resolve("context_menu", States::default()).unwrap(),
        );
        for (index, (plugin, version, hash, effect_id, name)) in effects.iter().enumerate() {
            let area = context_row(submenu, width, row, index);
            let mut states = States::default();
            if area.contains(input.mouse) {
                states = states.with(State::Selected);
            }
            let style = styles.resolve("context_menu.row", states).unwrap();
            box_style(list, area, style);
            text_style(
                list,
                atlas,
                Rect::new(
                    area.min[0] + 5.,
                    area.min[1],
                    area.width() - 10.,
                    area.height(),
                ),
                name,
                style,
                Align::Left,
            );
            if area.contains(input.mouse)
                && input.pressed.is_some_and(|(button, point)| {
                    button == MouseButton::Left && area.contains(point)
                })
            {
                out.push(Action::Plugin(serde_json::json!({
                    "op": "add",
                    "object": object,
                    "plugin": plugin,
                    "version": version,
                    "hash": hash,
                    "effect": effect_id
                })));
                break;
            }
        }
    }
    pub fn render_library(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: &mut f32,
        layer: &Value,
        state: &Value,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        self.draw_library(list, atlas, area, layer, input, locale, out);
        *y = area.max[1];
    }

    fn render_mode(
        &mut self,
        library: bool,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: &mut f32,
        layer: &Value,
        state: &Value,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        let styles = list.styles();
        let colors = styles.colors();
        let pad = styles.metric("padding.panel", 8.);
        let gap = styles.metric("padding", 5.);
        let slot = styles.metric("icon.slot", 20.);
        let toolbar = styles.metric("effect.toolbar.height", 28.);
        let toolbar_gap = styles.metric("effect.toolbar.gap", 6.);
        let secondary = styles.metric("effect.secondary.height", 24.);
        let secondary_gap = styles.metric("effect.secondary.gap", 4.);
        let category_row = styles.metric("effect.category.height", 28.);
        let category_gap = styles.metric("effect.category.gap", 4.);

        let original_input = input;
        let neutral = Input::default();
        let modal_owner = self.enum_owner.clone();
        let curve_owner = self
            .curve_editors
            .iter()
            .find(|(_, editor)| editor.modal())
            .map(|(key, _)| key.clone());
        let input = if modal_owner.is_some()
            || self.color_popup.is_some()
            || self.context_menu.is_some()
            || curve_owner.is_some()
        {
            &neutral
        } else {
            input
        };
        let mut overlays = list.child();
        self.cursor = false;
        self.drag_readout = None;
        let object = layer["id"].as_u64().unwrap();
        let locked = layer["locked"].as_bool().unwrap_or(false) || state["observing"] == true;
        for ((owner, effect, parameter), editor) in &mut self.curve_editors {
            if (locked
                || *owner != object
                || !layer["effects"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|item| item["id"].as_u64() == Some(*effect)
                        && item["params"].get(parameter).is_some()))
                && editor.cancel()
            {
                out.push(Action::History(4));
            }
        }
        let frame = state["frame"].as_f64().unwrap_or(0.0).floor() as u32;
        let local = state["frame"].as_f64().unwrap_or(0.0)
            - layer["timeline"]["offset_frame"].as_f64().unwrap_or(0.0);
        if self.field.as_ref().is_some_and(|f| {
            f.object != object || locked
                || !layer["effects"].as_array().into_iter().flatten()
                    .any(|effect| effect["id"].as_u64() == Some(f.effect)
                        && effect["params"].get(&f.param).is_some())
                || input.pressed.is_some_and(|(_, p)| !f.rect.contains(p))
        }) {
            if self.cancel() {
                out.push(Action::History(4));
            }
        }
        let revision = (
            state["root"].as_str().unwrap_or("").into(),
            state["composition"].as_str().unwrap_or("").into(),
            state["revision"].as_u64().unwrap_or(0),
        );
        if self.revision.as_ref() != Some(&revision) {
            self.tracks.clear();
            self.revision = Some(revision);
        }
        if !library {
            let node_width = styles.metric("effect.action.size", 26.);
            let node = Rect::new(area.max[0] - pad - node_width, *y, node_width, toolbar);
            if press(
                list,
                atlas,
                input,
                Rect::new(
                    area.min[0] + pad,
                    *y,
                    (node.min[0] - gap - area.min[0] - pad).max(0.),
                    toolbar,
                ),
                locale.t(T::AddEffect),
                !locked,
            ) {
                self.choosing = !self.choosing;
                self.query.clear();
                if self.choosing {
                    self.color_popup = None;
                }
            }
            if press_icon(
                list,
                input,
                node,
                Icon::Node,
                !locked
                    && object != 0
                    && !matches!(layer["content"]["kind"].as_str(), Some("audio" | "null")),
            ) {
                out.push(Action::NewNodeEffect(object));
            }
            if node.contains(input.mouse) {
                self.drag_readout =
                    Some((input.mouse, word(locale, "新建节点效果", "New Node Effect")));
            }
            *y += toolbar + toolbar_gap;
        }
        if layer["effects"].as_array().is_none_or(Vec::is_empty) && !self.choosing {
            text(
                list,
                atlas,
                Rect::new(
                    area.min[0] + pad,
                    *y,
                    (area.width() - pad * 2.).max(0.),
                    secondary,
                ),
                locale.t(T::NoEffects),
                true,
            );
            *y += secondary + secondary_gap;
        }
        if self.choosing {
            let search = Rect::new(
                area.min[0] + pad,
                *y,
                (area.width() - pad * 2.).max(0.),
                toolbar,
            );
            self.search.draw(
                list,
                atlas,
                search,
                &mut self.query,
                locale.t(T::SearchEffects),
                input,
                true,
            );
            *y += toolbar + toolbar_gap;

            for (index, label) in [
                if locale == Locale::Zh {
                    "全部"
                } else {
                    "All"
                },
                if locale == Locale::Zh {
                    "收藏"
                } else {
                    "Favorites"
                },
                if locale == Locale::Zh {
                    "最近"
                } else {
                    "Recent"
                },
            ]
            .into_iter()
            .enumerate()
            {
                let rect = Rect::new(
                    area.min[0]
                        + pad
                        + index as f32
                            * (styles.metric("effect.library.tab_width", 54.)
                                + styles.metric("effect.library.tab_gap", 4.)),
                    *y,
                    styles.metric("effect.library.tab_width", 54.),
                    styles.metric("row.height", 22.),
                );
                let states = if index == self.library_tab {
                    motion_ui::style::States::default().with(motion_ui::style::State::Selected)
                } else {
                    Default::default()
                };
                if ui::button_with_states(
                    list,
                    atlas,
                    rect,
                    label,
                    input,
                    true,
                    &styles,
                    "button.icon",
                    states,
                )
                .clicked
                {
                    self.library_tab = index;
                }
            }
            *y += secondary + secondary_gap;
            let mut categories: HashMap<String, Vec<(String, String, String, String)>> =
                HashMap::new();
            let query = self.query.trim().to_lowercase();
            for e in entries(&self.catalogue, locale)
                .into_iter()
                .filter(|e| e.supported && e.matches(&query))
            {
                if self.library_tab == 1 && !self.favorites.contains(&e.key)
                    || self.library_tab == 2 && !self.recent.contains(&e.key)
                {
                    continue;
                }
                categories
                    .entry(e.category)
                    .or_default()
                    .push((e.plugin, e.version, e.hash, e.id));
            }
            for effects in categories.values_mut() {
                effects.sort_by_key(|(plugin, _, _, effect)| {
                    (
                        !self.favorites.contains(&format!("{plugin}/{effect}")),
                        effect.clone(),
                    )
                });
            }
            // Sort categories alphabetically
            let mut sorted_categories: Vec<_> = categories.iter().collect();
            sorted_categories.sort_by_key(|(name, _)| *name);

            // Render categorized effects with collapsible headers
            for (category_name, effects) in sorted_categories {
                if effects.is_empty() {
                    continue;
                }

                let is_collapsed = self.collapsed_categories.contains(category_name);
                let header_rect = Rect::new(
                    area.min[0] + pad,
                    *y,
                    (area.width() - pad * 2.).max(0.),
                    category_row,
                );

                // Category header background
                paint::rect(list, header_rect, styles.color("header"));

                // Collapse/expand chevron
                let chevron_rect = Rect::new(area.min[0] + pad, *y, slot, category_row);
                icons::draw(
                    list,
                    chevron_rect,
                    if is_collapsed {
                        Icon::ChevronRight
                    } else {
                        Icon::ChevronDown
                    },
                    styles.metric("icon.small.size", 14.),
                    colors.TEXT,
                );

                // Category name
                ui::text(
                    list,
                    atlas,
                    Rect::new(
                        area.min[0] + pad + slot,
                        *y,
                        (area.width() - pad * 2. - slot * 2. - gap).max(0.),
                        category_row,
                    ),
                    category_name,
                    colors.TEXT,
                    Align::Left,
                );

                // Effect count badge
                let count_text = format!("{}", effects.len());
                ui::text(
                    list,
                    atlas,
                    Rect::new(area.max[0] - pad - slot, *y, slot, category_row),
                    &count_text,
                    colors.TEXT_MUTED,
                    Align::Right,
                );

                // Toggle collapse on click
                if clicked(input, header_rect) {
                    if is_collapsed {
                        self.collapsed_categories.remove(category_name);
                    } else {
                        self.collapsed_categories.insert(category_name.to_string());
                    }
                }

                *y += category_row + category_gap;

                // Render effects if not collapsed
                if !is_collapsed {
                    for (plugin_id, version, hash, effect_id) in effects {
                        // Find effect definition for display name
                        let effect_name = self.catalogue["packages"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .find(|p| {
                                p["manifest"]["id"].as_str() == Some(plugin_id)
                                    && p["hash"].as_str() == Some(hash)
                            })
                            .and_then(|p| {
                                p["manifest"]["effects"]
                                    .as_array()?
                                    .iter()
                                    .find(|e| e["id"].as_str() == Some(effect_id))
                            })
                            .and_then(|e| {
                                if locale == Locale::Zh {
                                    e["name"].as_str()
                                } else {
                                    e["english_name"].as_str()
                                }
                            })
                            .unwrap_or("Effect");

                        let favorite_key = format!("{plugin_id}/{effect_id}");
                        let star = Rect::new(area.max[0] - 32., *y, 20., 22.);
                        if let Some(value) = motion_ui::widgets::StarRating::draw(
                            list,
                            star,
                            usize::from(self.favorites.contains(&favorite_key)),
                            1,
                            input,
                            true,
                            false,
                        ) {
                            if value == 0 {
                                self.favorites.remove(&favorite_key);
                            } else {
                                self.favorites.insert(favorite_key.clone());
                            }
                            out.push(Action::Favorites(self.favorites.clone()));
                        }
                        let row =
                            Rect::new(area.min[0] + 12., *y, (area.width() - 52.).max(0.), 22.);
                        let mut states = motion_ui::style::States::default();
                        if row.contains(input.mouse) {
                            states = states.with(motion_ui::style::State::Hovered);
                        }
                        let style = styles.resolve("row", states).unwrap();
                        motion_ui::style::box_style(list, row, style);
                        motion_ui::icons::draw(
                            list,
                            Rect::new(row.min[0] + 4., *y, 16., 22.),
                            motion_ui::icons::Icon::Fx,
                            14.,
                            colors.TEXT_MUTED,
                        );
                        motion_ui::style::text_style(
                            list,
                            atlas,
                            Rect::new(row.min[0] + 26., *y, (row.width() - 28.).max(0.), 22.),
                            effect_name,
                            style,
                            Align::Left,
                        );
                        if clicked(input, row) && !locked {
                            self.recent.retain(|id| id != &favorite_key);
                            self.recent.insert(0, favorite_key);
                            self.recent.truncate(24);
                            out.push(Action::Plugin(serde_json::json!({
                                "op": "add",
                                "object": object,
                                "plugin": plugin_id,
                                "version": version,
                                "hash": hash,
                                "effect": effect_id
                            })));
                            self.choosing = false;
                        }
                        *y += styles.metric("row.height", 22.);
                    }
                }
            }

            return;
        }
        for (index, effect) in layer["effects"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let id = effect["id"].as_u64().unwrap();
            if matches!(self.property_filter,super::types::PropertyFilter::Properties(_))
                || self.property_filter == super::types::PropertyFilter::Animated
                    && !effect["params"].as_object().into_iter().flatten().any(|(name,parameter)|
                        super::modified_properties::effect_parameter_visible(self.property_filter,parameter,state,object,id,name))
            { continue; }
            *y += styles.metric("effect.group.gap", 8.);
            let Some(def) = observe!(
                "effects.definition",
                self.definition(effect).ok_or("no matching definition"),
                "object={object} effect={id} hash={} plugin={} version={} kind={}",
                effect["hash"],
                effect["plugin"],
                effect["version"],
                effect["effect"]
            )
            .ok() else {
                continue;
            };
            let title = if locale == Locale::Zh {
                def["name"].as_str()
            } else {
                def["english_name"].as_str()
            }
            .unwrap_or("Effect");
            let enabled = effect["enabled"].as_bool().unwrap_or(true);
            let is_collapsed = self.collapsed_effects.contains(&id);

            let pad = styles.metric("padding.panel", 8.);
            let gap = styles.metric("padding", 5.);
            let row = styles
                .metric("effect.row.height", 26.)
                .max(styles.metric("row.height", 22.));
            let header = styles
                .metric("effect.header.height", 30.)
                .max(styles.metric("row.height", 22.));
            let slot = styles.metric("icon.slot", 20.);
            let action = styles.metric("effect.action.size", 26.);
            let toggle = styles.metric("effect.enable.width", 28.);
            let header_rect = Rect::new(
                area.min[0] + pad,
                *y,
                (area.width() - pad * 2.).max(0.),
                header,
            );
            if input.pressed.is_some_and(|(button, point)| {
                button == MouseButton::Right && header_rect.contains(point)
            }) {
                self.context_menu = Some(EffectContextMenu {
                    anchor: input.mouse,
                    object,
                    control: word(locale, "效果控件", "Effect Controls"),
                    effect: title.to_string(),
                    category: None,
                    instance: Some((id, index)),
                    catalogue: true,
                    expression: None,
                    expression_supported: false,
                    opened: true,
                });
            }
            {
                box_style(
                    list,
                    header_rect,
                    styles
                        .resolve(
                            "effect.header",
                            if self.revealed_effect == Some((object, id)) {
                                States::default().with(State::Selected)
                            } else if header_rect.contains(input.mouse) {
                                States::default().with(State::Hovered)
                            } else {
                                States::default()
                            },
                        )
                        .unwrap(),
                );
            }
            if press_icon(
                list,
                input,
                Rect::new(header_rect.min[0], *y, slot, header),
                if is_collapsed {
                    Icon::ChevronRight
                } else {
                    Icon::ChevronDown
                },
                true,
            ) {
                if is_collapsed {
                    self.collapsed_effects.remove(&id);
                } else {
                    self.collapsed_effects.insert(id);
                }
            }
            let actions_x = header_rect.max[0] - action;
            let enable_rect = Rect::new(actions_x - toggle, *y, toggle, header);
            if motion_ui::widgets::Checkbox::draw(
                list,
                atlas,
                enable_rect,
                "",
                enabled,
                input,
                !locked,
                false,
                None,
            )
            .clicked
            {
                out.push(Action::Edit(Command::Effect {
                    object,
                    action: EffectAction::Enable {
                        effect: id,
                        enabled: !enabled,
                    },
                }));
            }
            let name_x = header_rect.min[0] + slot;
            text(
                list,
                atlas,
                Rect::new(
                    name_x,
                    *y,
                    (enable_rect.min[0] - gap - name_x).max(0.),
                    header,
                ),
                title,
                !enabled,
            );
            let more = Rect::new(actions_x, *y + (header - action) * 0.5, action, action);
            if press_icon(list, input, more, Icon::More, !locked) {
                self.context_menu = Some(EffectContextMenu {
                    anchor: [more.min[0], more.max[1]],
                    object,
                    control: word(locale, "效果操作", "Effect Actions"),
                    effect: title.into(),
                    category: None,
                    instance: Some((id, index)),
                    catalogue: false,
                    expression: None,
                    expression_supported: false,
                    opened: true,
                });
            }
            #[cfg(test)]
            {
                self.test_rects
                    .insert((object, id, "__actions".into(), 0), more);
                self.test_rects.insert(
                    (object, id, "__title".into(), 0),
                    Rect::new(
                        name_x,
                        *y,
                        (enable_rect.min[0] - gap - name_x).max(0.),
                        header,
                    ),
                );
            }
            if header_rect.contains(input.mouse) && input.pressed.is_none() {
                self.drag_readout = Some((input.mouse, title.into()));
            }
            *y += header;
            if effect["node_graph"].is_object() {
                if ui::button_styled(
                    list,
                    atlas,
                    Rect::new(
                        header_rect.min[0] + slot,
                        *y,
                        (header_rect.width() - slot).max(0.),
                        row,
                    ),
                    &word(locale, "编辑节点", "Edit Nodes"),
                    input,
                    !locked,
                    &styles,
                    "button.icon",
                )
                .clicked
                {
                    out.push(Action::EditNodeEffect { object, effect: id });
                }
                *y += row;
            }
            // Skip parameters if collapsed
            if is_collapsed {
                continue;
            }

            // Render effect parameters
            let mut parameters: Vec<_> = def["params"].as_array().into_iter().flatten().collect();
            parameters.sort_by_key(|parameter| parameter["kind"] == "curve");
            for parameter in parameters {
                let param = parameter["id"].as_str().unwrap();
                if !super::modified_properties::effect_parameter_visible(
                    self.property_filter,&effect["params"][param],state,object,id,param) { continue; }
                let kind = parameter["kind"].as_str().unwrap_or("float");
                let implemented = parameter["implemented"].as_bool().unwrap_or(true);
                let name = parameter["name"].as_str().unwrap_or(param);
                let watch_width = styles.metric("effect.watch.width", 24.);
                let left = header_rect.min[0] + super::key_navigation::width(&styles) + styles.metric("tree.indent", 14.);
                let caption_x = left + watch_width + gap;
                let right = (area.max[0] - pad).max(caption_x);
                let label_x = caption_x;
                let components = dimensions(kind);
                let minimum_value = if components > 1 && kind != "color" {
                    components as f32 * (atlas.measure("000.000") + gap)
                } else {
                    styles.metric("value.min_width", 64.)
                };
                let label_width = styles
                    .metric("label.column", 72.)
                    .min((right - label_x - gap - minimum_value).max(0.));
                let value_x = (label_x + label_width + gap).min(right);
                let values = Rect::new(value_x, *y, (right - value_x).max(0.), row);
                self.selection_probe.record(Rect::new(label_x, *y, label_width, row), area);
                if clicked(input, Rect::new(label_x, *y, (right - label_x).max(0.), row)) {
                    self.choose_parameter(state, object, id, param);
                }
                crate::panels::animation_controls::highlight_row(list,
                    Rect::new(area.min[0], *y, area.width(), row),
                    self.parameter_selected(state, object, id, param));
                text(
                    list,
                    atlas,
                    Rect::new(label_x, *y, label_width, row),
                    name,
                    !implemented,
                );
                #[cfg(test)]
                self.test_rects.insert(
                    (object, id, param.into(), -3),
                    Rect::new(left, *y, (right - left).max(0.), row),
                );
                #[cfg(test)]
                self.test_rects
                    .insert((object, id, param.into(), -4), values);
                #[cfg(test)]
                self.test_rects.insert(
                    (object, id, param.into(), -7),
                    Rect::new(label_x, *y, label_width, row),
                );
                let parameter_row = Rect::new(left, *y, (right - left).max(0.), row);
                if Rect::new(label_x, *y, label_width, row).contains(input.mouse) {
                    self.drag_readout = Some((input.mouse, name.into()));
                }
                if input.pressed.is_some_and(|(button, point)| {
                    button == MouseButton::Right && parameter_row.contains(point)
                }) {
                    self.context_menu = Some(EffectContextMenu {
                        anchor: input.mouse,
                        object,
                        control: name.into(),
                        effect: title.into(),
                        category: None,
                        instance: Some((id, index)),
                        catalogue: true,
                        expression: Some(motion_core::ExpressionTarget::Effect {
                                object,
                                effect: id,
                                param: param.into(),
                            }),
                        expression_supported: matches!(kind, "float" | "vec2" | "vec3" | "color")
                            && implemented && parameter["animatable"] == true,
                        opened: true,
                    });
                }
                let Ok(track) = self.tracks.get_or_try_insert(object, id, param, || {
                    observe!("effects.parameter", serde_json::from_value::<EffectParam>(effect["params"][param].clone()),
                        "object={object} effect={id} param={param}")
                }) else { continue };
                self.selection_intent |= key_navigation::parameter_navigation(&mut self.key_channel, list,
                    Rect::new(area.min[0], *y, area.width(), row), state, input, object, id, param,
                    track, local, locked, out);
                if kind == "curve" {
                    let key = (object, id, param.to_string());
                    let open = !self.collapsed_curves.contains(&key);
                    let disclosure = Rect::new(left, *y, watch_width, row);
                    let toggle = press_icon(
                        list,
                        input,
                        disclosure,
                        if open {
                            Icon::ChevronDown
                        } else {
                            Icon::ChevronRight
                        },
                        true,
                    ) || clicked(
                        input,
                        Rect::new(label_x, *y, (right - label_x).max(0.), row),
                    );
                    if toggle {
                        if open {
                            self.collapsed_curves.insert(key.clone());
                        } else {
                            self.collapsed_curves.remove(&key);
                        }
                        if self
                            .curve_editors
                            .get_mut(&key)
                            .is_some_and(curve::Editor::cancel)
                        {
                            out.push(Action::History(4));
                        }
                    }
                    *y += row;
                    // This frame follows the same expansion snapshot as its height and hit rectangles.
                    if open {
                        if let Some(current) = observe!(
                            "effects.curve_track",
                            track.curve.as_ref().ok_or("curve parameter has no curve track"),
                            "object={object} effect={id} param={param}"
                        ).ok().map(|track| curve::value_at(track, local))
                        {
                            let width = (right - left)
                                .max(0.)
                                .min(styles.metric("effect.curve.width", 320.));
                            let body = Rect::new(left, *y, width, curve::height(&styles, width));
                            #[cfg(test)]
                            self.test_rects.insert((object, id, param.into(), -5), body);
                            self.curve_editors.entry(key.clone()).or_default().draw(
                                list,
                                atlas,
                                body,
                                area,
                                if effect["effect"] == "curves" {
                                    self.tracks.get_or_try_insert(object, id, "p0002", || {
                                        observe!(
                                            "effects.curve_channel_parameter",
                                            serde_json::from_value::<EffectParam>(
                                                effect["params"]["p0002"].clone(),
                                            ),
                                            "object={object} effect={id} param=p0002"
                                        )
                                    })
                                    .ok()
                                    .and_then(|parameter| {
                                        let channel = observe!("effects.curve_channel_sample", parameter.sample(local),
                                            "object={object} effect={id} param=p0002 frame={local}").ok()?[0];
                                        let _ = observe!(
                                            "effects.curve_channel",
                                            (channel.is_finite()
                                                && channel >= 1.
                                                && channel <= 5.
                                                && channel.fract() == 0.)
                                                .then_some(())
                                                .ok_or("invalid channel converted to index"),
                                            "object={object} effect={id} value={channel}"
                                        );
                                        Some((channel as usize).saturating_sub(1))
                                    })
                                } else {
                                    None
                                },
                                if curve_owner.as_ref() == Some(&key) {
                                    original_input
                                } else {
                                    input
                                },
                                !locked && implemented && state["observing"] != true,
                                &current,
                                |value| Command::Effect {
                                    object,
                                    action: EffectAction::SetCurveObject {
                                        effect: id,
                                        param: param.into(),
                                        frame,
                                        value,
                                    },
                                },
                                out,
                                &mut self.drag_readout,
                            );
                            *y += body.height();
                        }
                    }
                    continue;
                }
                let animated = !track.track.keys.is_empty();
                let expression_supported = !track.kind.discrete()
                    && track.kind != motion_effects::ParamKind::Curve
                    && track.animatable
                    && track.implemented;
                let expression_target = motion_core::ExpressionTarget::Effect {
                    object,
                    effect: id,
                    param: param.into(),
                };
                let sampled = observe!("effects.parameter_sample", track.sample(local),
                    "object={object} effect={id} param={param} frame={local}");
                if let Ok(mut value) = sampled {
                    let animate = Rect::new(left, *y, watch_width, row);
                    let mut watch_state = States::default();
                    if animated {
                        watch_state = watch_state.with(State::Selected);
                    }
                    if locked || !implemented || !parameter["animatable"].as_bool().unwrap_or(false) {
                        watch_state = watch_state.with(State::Disabled);
                    }
                    if icons::button(list, animate, Icon::Stopwatch, input, &styles, watch_state)
                        .clicked
                    {
                        if input.modifiers.alt {
                            if expression_supported {
                                self.expressions.open(
                                    state,
                                    expression_target.clone(),
                                    motion_ui::dock::Panel::EffectControls,
                                );
                            }
                        } else {
                            out.push(Action::Edit(Command::Effect {
                                object,
                                action: EffectAction::Animate {
                                    effect: id,
                                    param: param.into(),
                                    frame,
                                    enabled: !animated,
                                },
                            }));
                        }
                    }
                    if kind == "color" {
                        self.color_row(
                            list,
                            atlas,
                            values,
                            *y,
                            object,
                            id,
                            param,
                            &value,
                            input,
                            !locked && implemented,
                            out,
                        );
                        *y += row;
                        if self.expressions.visible(
                            state,
                            &expression_target,
                            motion_ui::dock::Panel::EffectControls,
                        ) {
                            self.expressions.row(
                                list,
                                atlas,
                                Rect::new(label_x, *y, (value_x - label_x).max(0.), row),
                                Rect::new(value_x, *y, values.width(), row),
                                state,
                                expression_target.clone(),
                                input,
                                locale,
                                !locked && implemented && state["observing"] != true,
                                out,
                            );
                            *y += row;
                        }
                        continue;
                    }
                    for component in 0..dimensions(kind) {
                        let count = dimensions(kind);
                        let cell = if kind == "bool" || kind == "enum" {
                            values.width()
                        } else {
                            (values.width() / count as f32).min(styles.metric("value.min_width", 64.))
                        };
                        let x = values.min[0] + component as f32 * cell;
                        let rect = Rect::new(x, *y, (cell - gap).max(0.), row);
                        if count > 1 && component + 1 < count {
                            ui::text(
                                list,
                                atlas,
                                Rect::new(rect.max[0], *y, gap, row),
                                ",",
                                colors.TEXT_MUTED,
                                Align::Left,
                            );
                        }
                        if kind == "bool" || kind == "enum" {
                            let enabled = !locked && implemented;
                            let changed =
                                if kind == "bool" {
                                    motion_ui::widgets::Checkbox::draw(
                                        list,
                                        atlas,
                                        rect,
                                        if value[0] > 0.5 {
                                            locale.t(T::Enabled)
                                        } else {
                                            locale.t(T::Disabled)
                                        },
                                        value[0] > 0.5,
                                        input,
                                        enabled,
                                        false,
                                        None,
                                    )
                                    .clicked
                                    .then_some(if value[0] > 0.5 { 0. } else { 1. })
                                } else {
                                    let key = (id, param.to_string());
                                    let curve_channel =
                                        effect["effect"] == "curves" && param == "p0002";
                                    let items: Vec<_> = parameter["options"]
                                        .as_array()
                                        .into_iter()
                                        .flatten()
                                        .enumerate()
                                        .map(|(index, o)| motion_ui::widgets::DropdownItem {
                                            label: if curve_channel {
                                                ["RGB", "R", "G", "B", "A"]
                                                    .get(index)
                                                    .copied()
                                                    .unwrap_or("")
                                            } else {
                                                o.as_str().unwrap_or("")
                                            },
                                            enabled: true,
                                        })
                                        .collect();
                                    let dropdown = self.dropdowns.entry(key.clone()).or_default();
                                    let result = dropdown.draw(
                                        &mut overlays,
                                        atlas,
                                        rect,
                                        area,
                                        &items,
                                        (value[0] - parameter["min"].as_f64().unwrap_or(0.) as f32)
                                            .max(0.) as usize,
                                        if modal_owner.as_ref() == Some(&key) {
                                            original_input
                                        } else {
                                            input
                                        },
                                        enabled,
                                        false,
                                    );
                                    if dropdown.open {
                                        self.enum_owner = Some(key);
                                    } else if self.enum_owner.as_ref() == Some(&key) {
                                        self.enum_owner = None;
                                    }
                                    result.map(|i| {
                                        i as f32 + parameter["min"].as_f64().unwrap_or(0.) as f32
                                    })
                                };
                            if let Some(next) = changed {
                                value[0] = next;
                                out.push(Action::Edit(Command::Effect {
                                    object,
                                    action: EffectAction::Set {
                                        effect: id,
                                        param: param.into(),
                                        frame,
                                        value,
                                    },
                                }));
                            }
                        } else {
                            let min = parameter["min"].as_f64().unwrap_or(-1e9);
                            let max = parameter["max"].as_f64().unwrap_or(1e9);
                            self.channel_field(
                                list,
                                atlas,
                                rect,
                                object,
                                id,
                                param,
                                component,
                                &mut value,
                                min,
                                max,
                                frame,
                                input,
                                !locked && implemented,
                                out,
                            );
                        }
                    }
                }
                *y += row;
                if self.expressions.visible(
                    state,
                    &expression_target,
                    motion_ui::dock::Panel::EffectControls,
                ) {
                    self.expressions.row(
                        list,
                        atlas,
                        Rect::new(label_x, *y, (value_x - label_x).max(0.), row),
                        Rect::new(value_x, *y, values.width(), row),
                        state,
                        expression_target.clone(),
                        input,
                        locale,
                        !locked && implemented && state["observing"] != true,
                        out,
                    );
                    *y += row;
                }
            }
        }
        self.color_overlay(
            &mut overlays,
            atlas,
            area,
            layer,
            state,
            original_input,
            locale,
            out,
        );
        list.append(overlays);
    }
    /// One row for a color parameter: swatch plus caption, popup edits channels.
    #[allow(clippy::too_many_arguments)]
    fn color_row(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: f32,
        object: u64,
        effect: u64,
        param: &str,
        values: &[f32; 4],
        input: &Input,
        enabled: bool,
        out: &mut Vec<Action>,
    ) {
        let styles = list.styles();
        let row = styles
            .metric("effect.row.height", 26.)
            .max(styles.metric("row.height", 22.));
        let edge = styles.metric("field.height", 18.).min(row);
        let preview = Rect::new(
            area.min[0],
            y + (row - edge) * 0.5,
            (edge + styles.metric("icon.slot", 20.)).min(area.width()),
            edge,
        );
        color_well::swatch(list, preview, values, input, enabled);
        #[cfg(test)]
        self.test_rects
            .insert((object, effect, param.into(), -1), preview);
        let text_area = Rect::new(
            preview.max[0] + styles.metric("padding", 5.),
            y,
            (area.max[0] - preview.max[0] - styles.metric("padding", 5.)).max(0.),
            row,
        );
        ui::text(
            list,
            atlas,
            text_area,
            &color_well::caption(values),
            styles.colors().TEXT_MUTED,
            Align::Left,
        );
        if enabled && (clicked(input, preview) || clicked(input, text_area)) {
            if self.field.take().is_some_and(|mut f| f.number.cancel()) {
                out.push(Action::History(4));
            }
            self.color_popup = Some(ColorPopup {
                object,
                effect,
                param: param.into(),
                anchor: preview,
                opened: true,
            });
        }
    }
    /// Popup with the four channel editors, drawn above the panel body.
    #[allow(clippy::too_many_arguments)]
    fn color_overlay(
        &mut self,
        paint: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        layer: &Value,
        state: &Value,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        let Some(popup) = self.color_popup.clone() else {
            return;
        };
        let frame = state["frame"].as_f64().unwrap_or(0.).floor().max(0.) as u32;
        let local = state["frame"].as_f64().unwrap_or(0.)
            - layer["timeline"]["offset_frame"].as_f64().unwrap_or(0.0);
        let locked = layer["locked"].as_bool().unwrap_or(false);
        let track = layer["effects"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|e| e["id"].as_u64() == Some(popup.effect))
            .and_then(|e| {
                observe!("effect.color_track", serde_json::from_value::<EffectParam>(e["params"][&popup.param].clone()), "object={} effect={} param={} fallback=close_popup", popup.object, popup.effect, popup.param).ok()
            });
        let Some(track) = track else {
            self.color_popup = None;
            return;
        };
        let Ok(mut value) = observe!("effects.color_sample", track.sample(local),
            "object={} effect={} param={} frame={local}", popup.object, popup.effect, popup.param)
            else { return; };
        let styles = paint.styles();
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
        #[cfg(test)]
        self.test_rects
            .insert((popup.object, popup.effect, popup.param.clone(), -2), rect);
        if !popup.opened
            && input
                .pressed
                .is_some_and(|(_, point)| !rect.contains(point))
        {
            if self.cancel() {
                out.push(Action::History(4));
            }
            return;
        }
        if let Some(popup) = &mut self.color_popup {
            popup.opened = false;
        }
        let neutral = Input::default();
        let input = if popup.opened { &neutral } else { input };
        box_style(
            paint,
            rect,
            styles.resolve("popup", States::default()).unwrap(),
        );
        let title = Rect::new(
            rect.min[0] + pad,
            rect.min[1] + pad,
            (width - pad * 2. - slot).max(0.),
            row,
        );
        text(paint, atlas, title, "RGBA", false);
        if press_icon(
            paint,
            input,
            Rect::new(rect.max[0] - pad - slot, title.min[1], slot, row),
            Icon::Close,
            true,
        ) {
            if self.cancel() {
                out.push(Action::History(4));
            }
            return;
        }
        let enabled = !locked && track.implemented;
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
            text(
                paint,
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
            #[cfg(test)]
            self.test_rects.insert(
                (popup.object, popup.effect, popup.param.clone(), axis as i64),
                number,
            );
            self.channel_field(
                paint,
                atlas,
                number,
                popup.object,
                popup.effect,
                &popup.param,
                axis,
                &mut value,
                0.,
                1.,
                frame,
                input,
                enabled,
                out,
            );
        }
    }
    /// Shared numeric channel editor used by the parameter rows and the color popup.
    #[allow(clippy::too_many_arguments)]
    fn channel_field(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        rect: Rect,
        object: u64,
        effect: u64,
        param: &str,
        component: usize,
        value: &mut [f32; 4],
        min: f64,
        max: f64,
        frame: u32,
        input: &Input,
        enabled: bool,
        out: &mut Vec<Action>,
    ) {
        #[cfg(test)]
        self.test_rects
            .insert((object, effect, param.into(), component as i64), rect);
        let active = self.field.as_ref().is_some_and(|f| {
            f.object == object && f.effect == effect && f.param == param && f.component == component
        });
        if clicked(input, rect) && !active && enabled {
            if self.field.take().is_some_and(|mut f| f.number.cancel()) {
                out.push(Action::History(4));
            }
            self.field = Some(Field {
                object,
                effect,
                param: param.into(),
                component,
                number: Default::default(),
                rect,
            });
        }
        if let Some(field) = self.field.as_mut().filter(|f| {
            f.object == object && f.effect == effect && f.param == param && f.component == component
        }) {
            field.rect = rect;
            self.cursor |= field.number.editor.dragging() || rect.contains(input.mouse);
            let speed = if (max - min).is_finite() && (max - min) <= 2. {
                0.005
            } else {
                0.5
            };
            let options = motion_ui::widgets::NumberOptions {
                min,
                max,
                speed,
                decimals: 3,
            };
            let events =
                field
                    .number
                    .draw(list, atlas, rect, value[component] as f64, options, input);
            if let Some(note) = field.number.readout() {
                self.drag_readout = Some((input.mouse, note));
            }
            if super::numeric::apply(events, out, |v| {
                value[component] = v;
                Command::Effect {
                    object,
                    action: EffectAction::Set {
                        effect,
                        param: param.into(),
                        frame,
                        value: *value,
                    },
                }
            }) {
                self.field = None;
            }
        } else {
            self.cursor |= enabled && rect.contains(input.mouse);
            if enabled && rect.contains(input.mouse) {
                self.drag_readout = Some((input.mouse, format!("{:.3}", value[component])));
            }
            super::numeric::passive(list, atlas, rect, value[component] as f64, input, enabled);
        }
    }
}

#[cfg(test)]
mod application_reveal_tests {
    use super::*;
    #[test]
    fn reveal_reopens_reused_ids_clears_filters_and_scrolls_only_the_target_layer_once() {
        let styles = PaintList::default().styles();
        let layer = serde_json::json!({"id":7,"effects":[{"id":1},{"id":2}]});
        let mut controls = Controls::default();
        controls.collapsed_effects.insert(2);
        controls.property_filter = super::super::types::PropertyFilter::Animated;
        controls.choosing = true;
        controls.reveal_effect(7,2);
        assert!(!controls.choosing && !controls.collapsed_effects.contains(&2));
        assert_eq!(controls.property_filter,super::super::types::PropertyFilter::All);
        assert_eq!(controls.take_reveal_offset(&serde_json::json!({"id":8}),&styles,Locale::En,360.),None);
        assert!(controls.take_reveal_offset(&layer,&styles,Locale::En,360.).unwrap() > 0.);
        assert_eq!(controls.take_reveal_offset(&layer,&styles,Locale::En,360.),None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Engine;
    use motion_ui::input::MouseButton;

    fn run(engine: &Engine, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::History(op) => {
                    engine.history(op).unwrap();
                }
                Action::Edit(command) => {
                    engine
                        .edit(serde_json::to_string(&command).unwrap())
                        .unwrap();
                }
                _ => panic!("unexpected action"),
            }
        }
    }
    fn input(x: f32, y: f32, pressed: bool, released: bool) -> Input {
        Input {
            mouse: [x, y],
            held: (!released).then_some(MouseButton::Left),
            pressed: pressed.then_some((MouseButton::Left, [x, y])),
            released: released.then_some((MouseButton::Left, [x, y])),
            ..Default::default()
        }
    }
    #[test]
    fn inline_rows_keep_vectors_together_and_real_discrete_and_curve_edits_undo() {
        let folder = tempfile::tempdir().unwrap();
        let engine = Engine::start(
            folder.path().join("project"),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        engine
            .edit(
                serde_json::to_string(&Command::Add {
                    layer: motion_core::Layer::solid(
                        7,
                        "Controls",
                        [100.; 2],
                        [960., 540., 0.],
                        [1.; 4],
                    ),
                })
                .unwrap(),
            )
            .unwrap();
        let catalogue = engine.plugin("{\"op\":\"catalogue\"}".into()).unwrap();
        for name in ["drop_shadow", "curves", "brightness_contrast"] {
            let package = catalogue["packages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|package| {
                    package["manifest"]["effects"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|effect| effect["id"] == name)
                })
                .unwrap();
            engine.plugin(serde_json::json!({"op":"add","object":7,"plugin":package["manifest"]["id"],"version":package["manifest"]["version"],"hash":package["hash"],"effect":name}).to_string()).unwrap();
        }
        let original = engine.state().unwrap();
        let effect_id = |name: &str| {
            original["project"]["layers"][0]["effects"]
                .as_array()
                .unwrap()
                .iter()
                .find(|effect| effect["effect"] == name)
                .unwrap()["id"]
                .as_u64()
                .unwrap()
        };
        let shadow = effect_id("drop_shadow");
        let curves = effect_id("curves");
        let brightness = effect_id("brightness_contrast");
        for (width, scale, density) in [
            (220., 1., 22f32),
            (220., 1.5, 32.),
            (220., 2., 32.),
            (360., 1., 22.),
            (360., 1.5, 32.),
            (360., 2., 32.),
        ] {
            let mut controls = Controls {
                catalogue: catalogue.clone().into(),
                ..Default::default()
            };
            let mut atlas = TextAtlas::from_system(12.).unwrap();
            atlas.set_pixel_ratio(scale);
            let area = Rect::new(0., 0., width, 1000.);
            let mut tokens: Value =
                serde_json::from_str(include_str!("../../../ui/styles/dark.json")).unwrap();
            tokens["tokens"]["sizes"]["row.height"] = density.into();
            let theme = std::sync::Arc::new(
                motion_ui::style::StyleSheet::from_json(&serde_json::to_vec(&tokens).unwrap())
                    .unwrap(),
            );
            let mut draw = |controls: &mut Controls, input: &Input| {
                let state = engine.state().unwrap();
                let mut actions = vec![];
                let mut y = 0.;
                let mut paint = PaintList::with_styles(theme.clone());
                controls.render(
                    &mut paint,
                    &mut atlas,
                    area,
                    &mut y,
                    &state["project"]["layers"][0],
                    &state,
                    input,
                    Locale::En,
                    &mut actions,
                );
                controls.finish_context_menu(
                    &mut paint,
                    &mut atlas,
                    area,
                    &state,
                    input,
                    Locale::En,
                    &mut actions,
                );
                actions
            };
            assert!(draw(&mut controls, &Input::default()).is_empty());
            let key = |effect, param: &str, axis| (7, effect, param.to_string(), axis);
            let x = controls.test_rects[&key(shadow, "offset", 0)];
            let y = controls.test_rects[&key(shadow, "offset", 1)];
            assert_eq!(x.min[1], y.min[1]);
            assert_eq!(x.max[1], y.max[1]);
            assert!(x.max[0] <= y.min[0] && y.max[0] <= area.max[0]);
            for (index, rect) in &controls.test_rects {
                if index.3 == -3 {
                    assert!(rect.height() >= density.max(24.) && rect.max[0] <= area.max[0]);
                }
            }
            assert!(x.width() >= TextAtlas::from_system(12.).unwrap().measure("12.000"));
            let title = controls.test_rects[&key(shadow, "__title", 0)];
            assert!(controls.test_rects[&key(shadow, "offset", -7)].min[0] > title.min[0]);
            assert_eq!(
                controls.test_rects[&key(shadow, "offset", -7)].min[0],
                controls.test_rects[&key(brightness, "p0003", -7)].min[0]
            );
            let more = controls.test_rects[&key(shadow, "__actions", 0)];
            assert!(draw(
                &mut controls,
                &input(more.min[0] + 2., more.min[1] + 2., true, false)
            )
            .is_empty());
            assert!(controls.context_menu.is_some());
            let duplicate = controls.test_rects[&key(shadow, "__menu", 3)];
            run(
                &engine,
                draw(
                    &mut controls,
                    &input(duplicate.min[0] + 2., duplicate.min[1] + 2., true, false),
                ),
            );
            assert_eq!(
                engine.state().unwrap()["project"]["layers"][0]["effects"]
                    .as_array()
                    .unwrap()
                    .len(),
                4
            );
            engine.history(0).unwrap();
            draw(&mut controls, &Input::default());
            let boolean = controls.test_rects[&key(brightness, "p0003", -4)];
            run(
                &engine,
                draw(
                    &mut controls,
                    &input(
                        boolean.min[0] + 2.,
                        boolean.min[1] + boolean.height() * 0.5,
                        true,
                        false,
                    ),
                ),
            );
            assert_ne!(engine.state().unwrap()["project"], original["project"]);
            assert_eq!(engine.history(0).unwrap()["project"], original["project"]);
            draw(&mut controls, &Input::default());
            let dropdown = controls.test_rects[&key(shadow, "mode", -4)];
            assert!(draw(
                &mut controls,
                &input(
                    dropdown.min[0] + 2.,
                    dropdown.min[1] + dropdown.height() * 0.5,
                    true,
                    false
                )
            )
            .is_empty());
            assert!(controls.enum_owner.is_some());
            assert_eq!(
                engine.state().unwrap()["project"],
                original["project"],
                "opening an enum must not cycle its value"
            );
            assert!(draw(&mut controls, &input(width - 1., 990., true, false)).is_empty());
            assert!(controls.enum_owner.is_none());
            draw(&mut controls, &Input::default());
            let channel = controls.test_rects[&key(curves, "p0002", -4)];
            assert!(draw(
                &mut controls,
                &input(channel.min[0] + 2., channel.min[1] + 2., true, false)
            )
            .is_empty());
            assert!(draw(
                &mut controls,
                &Input {
                    keys: vec![(Key::Down, Default::default())],
                    ..Default::default()
                }
            )
            .is_empty());
            run(
                &engine,
                draw(
                    &mut controls,
                    &Input {
                        keys: vec![(Key::Enter, Default::default())],
                        ..Default::default()
                    },
                ),
            );
            let state = engine.state().unwrap();
            let channel_value = state["project"]["layers"][0]["effects"]
                .as_array()
                .unwrap()
                .iter()
                .find(|effect| effect["id"].as_u64() == Some(curves))
                .unwrap()["params"]["p0002"]["track"]["value"][0]
                .as_f64()
                .unwrap();
            assert_eq!(channel_value, 2.);
            draw(&mut controls, &Input::default());
            let red_graph = controls.curve_editors[&(7, curves, "p0001".into())]
                .test_graph
                .unwrap();
            let middle = [
                (red_graph.min[0] + red_graph.max[0]) * 0.5,
                (red_graph.min[1] + red_graph.max[1]) * 0.5,
            ];
            run(
                &engine,
                draw(&mut controls, &input(middle[0], middle[1], true, false)),
            );
            run(
                &engine,
                draw(&mut controls, &input(middle[0], middle[1], false, true)),
            );
            let state = engine.state().unwrap();
            let red_curve = &state["project"]["layers"][0]["effects"]
                .as_array()
                .unwrap()
                .iter()
                .find(|effect| effect["id"].as_u64() == Some(curves))
                .unwrap()["params"]["p0001"]["curve"]["value"]["channels"];
            assert_eq!(red_curve[0].as_array().unwrap().len(), 2);
            assert_eq!(red_curve[1].as_array().unwrap().len(), 3);
            engine.history(0).unwrap();

            assert_eq!(engine.history(0).unwrap()["project"], original["project"]);
            draw(&mut controls, &Input::default());
            let edit = controls.test_rects[&key(curves, "p0001", -4)];
            assert!(draw(
                &mut controls,
                &input(
                    edit.min[0] + 2.,
                    edit.min[1] + edit.height() * 0.5,
                    true,
                    false
                )
            )
            .is_empty());
            assert!(controls
                .collapsed_curves
                .contains(&(7, curves, "p0001".into())));
            draw(&mut controls, &Input::default());
            assert!(draw(
                &mut controls,
                &input(
                    edit.min[0] + 2.,
                    edit.min[1] + edit.height() * 0.5,
                    true,
                    false
                )
            )
            .is_empty());
            draw(&mut controls, &Input::default());
            let popup = controls.test_rects[&key(curves, "p0001", -5)];
            assert!(popup.min[0] >= area.min[0] && popup.max[0] <= area.max[0]);
            let graph = controls.curve_editors[&(7, curves, "p0001".into())]
                .test_graph
                .unwrap();
            let point = [
                (graph.min[0] + graph.max[0]) * 0.5,
                (graph.min[1] + graph.max[1]) * 0.5,
            ];
            run(
                &engine,
                draw(&mut controls, &input(point[0], point[1], true, false)),
            );
            run(
                &engine,
                draw(
                    &mut controls,
                    &input(point[0] + 8., point[1] - 10., false, true),
                ),
            );
            assert_ne!(engine.state().unwrap()["project"], original["project"]);
            assert_eq!(engine.history(0).unwrap()["project"], original["project"]);
            assert!(draw(
                &mut controls,
                &Input {
                    keys: vec![(Key::Escape, Default::default())],
                    ..Default::default()
                }
            )
            .is_empty());
            assert!(!controls.editing());
        }
    }

    #[test]
    fn color_row_opens_popup_and_channel_scrub_commits_one_undo() {
        let folder = tempfile::tempdir().unwrap();
        let engine = Engine::start(
            folder.path().join("project"),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        engine
            .edit(
                serde_json::to_string(&Command::Add {
                    layer: motion_core::Layer::solid(
                        7,
                        "Color",
                        [100., 100.],
                        [960., 540., 0.],
                        [1.; 4],
                    ),
                })
                .unwrap(),
            )
            .unwrap();
        let catalogue = engine.plugin("{\"op\":\"catalogue\"}".into()).unwrap();
        let package = catalogue["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| {
                p["manifest"]["effects"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|e| e["id"] == "drop_shadow")
            })
            .unwrap();
        engine.plugin(serde_json::json!({"op":"add","object":7,"plugin":package["manifest"]["id"],"version":package["manifest"]["version"],"hash":package["hash"],"effect":"drop_shadow"}).to_string()).unwrap();
        let before = engine.state().unwrap();
        let effect = before["project"]["layers"][0]["effects"][0]["id"]
            .as_u64()
            .unwrap();
        let mut panels = crate::panels::Panels::default();
        panels.selected = Some(7);
        panels.effect_controls.catalogue = catalogue.into();
        let id = motion_ui::dock::Panel::EffectControls.dock();
        let layout = motion_ui::dock::Node::Dock {
            id,
            tabs: vec![id],
            active: 0,
        };
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let mut draw = |panels: &mut crate::panels::Panels, input: &Input| {
            let state = engine.state().unwrap();
            crate::panels::chrome(
                &mut PaintList::default(),
                &mut atlas,
                [400., 1000.],
                &layout,
                panels,
                &state,
                input,
            )
        };
        draw(&mut panels, &Input::default());
        let key = |axis: i64| (7, effect, "color".to_string(), axis);
        let swatch = panels.effect_controls.test_rects[&key(-1)];
        assert!(swatch.width() > 0.);
        let actions = draw(
            &mut panels,
            &input(swatch.min[0] + 4., swatch.min[1] + 4., true, false),
        );
        assert!(actions.is_empty());
        assert!(panels.effect_controls.editing());
        let popup = panels.effect_controls.test_rects[&key(-2)];
        let channel = panels.effect_controls.test_rects[&key(0)];
        assert!(popup.contains([channel.min[0] + 1., channel.min[1] + 1.]));
        let start = [
            channel.min[0] + channel.width() * 0.5,
            channel.min[1] + channel.height() * 0.5,
        ];
        assert!(
            draw(&mut panels, &input(start[0], start[1], true, false)).is_empty(),
            "press only arms the field"
        );
        run(
            &engine,
            draw(
                &mut panels,
                &Input {
                    mouse: [start[0] + 10., start[1]],
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
            ),
        );
        run(
            &engine,
            draw(&mut panels, &input(start[0] + 20., start[1], false, true)),
        );
        let after = engine.state().unwrap();
        assert_ne!(after["project"], before["project"]);
        let changed = after["project"]["layers"][0]["effects"][0]["params"]["color"].clone();
        let initial = before["project"]["layers"][0]["effects"][0]["params"]["color"].clone();
        assert_ne!(changed, initial);
        assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
        assert!(panels.effect_controls.editing());
        draw(&mut panels, &input(10., 960., true, false));
        assert!(
            !panels.effect_controls.editing(),
            "clicking outside dismisses the popup"
        );
    }
}

#[cfg(test)]
mod color_observation_tests {
    use super::*;
    #[test]
    fn invalid_color_track_still_closes_popup_and_reports_the_object() {
        const CHILD: &str = "MOTION_EFFECT_COLOR_TRACK_CHILD";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = std::path::PathBuf::from(root);
            let _logger = crate::logging::init(root.clone()).unwrap();
            let mut controls = Controls::default();
            let mut atlas = TextAtlas::from_system(12.).unwrap();
            let layer = serde_json::json!({"id":7,"effects":[{"id":11,"params":{"color":17}}]});
            let state = serde_json::json!({"frame":0});
            for _ in 0..120 {
                controls.color_popup = Some(ColorPopup { object:7,effect:11,param:"color".into(),anchor:Rect::new(0.,0.,20.,20.),opened:false });
                let mut actions = Vec::new();
                controls.color_overlay(&mut PaintList::default(),&mut atlas,Rect::new(0.,0.,640.,480.),&layer,&state,&Input::default(),Locale::En,&mut actions);
                assert!(controls.color_popup.is_none() && actions.is_empty());
            }
            let text = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
            let records = text.lines().filter(|l| l.contains("site=effect.color_track ")).collect::<Vec<_>>();
            assert_eq!(records.len(),2);
            assert!(records[1].contains("ERROR") && records[1].contains("object=7 effect=11 param=color"));
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","panels::effect_controls::color_observation_tests::invalid_color_track_still_closes_popup_and_reports_the_object"]).env_remove("MOTION_LOG").env(CHILD,temp.path()).output().unwrap();
        assert!(output.status.success(),"{}\n{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stderr).contains("site=effect.color_track"));
    }
}
