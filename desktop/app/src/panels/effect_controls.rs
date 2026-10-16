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
mod library;
pub(super) use library::entries;

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
    pub catalogue: std::sync::Arc<Value>,
    pub cursor: bool,
    pub drag_readout: Option<([f32; 2], String)>,
    pub choosing: bool,
    library_tab: usize,
    library_initialized: bool,
    library_scroll: motion_ui::widgets::ScrollArea,
    library_selected: Option<String>,
    library_last_click: Option<(String, std::time::Instant)>,
    pub importing_package: bool,
    recent: Vec<String>,
    query: String,
    search: motion_ui::widgets::SearchField,
    dropdowns: HashMap<(u64, String), motion_ui::widgets::Dropdown>,
    enum_owner: Option<(u64, String)>,
    pub favorites: std::collections::BTreeSet<String>,
    field: Option<Field>,
    revision: Option<(String, String, u64)>,
    tracks: HashMap<(u64, u64, String), EffectParam>,
    curve_editors: HashMap<(u64, u64, String), curve::Editor>,
    collapsed_categories: std::collections::HashSet<String>,
    collapsed_effects: std::collections::HashSet<u64>,
    context_menu: Option<EffectContextMenu>,
    color_popup: Option<ColorPopup>,
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
    value: &str,
    enabled: bool,
) -> bool {
    let styles = list.styles();
    let colors = styles.colors();

    paint::rect(
        list,
        rect,
        if rect.contains(input.mouse) && enabled {
            colors.SUNKEN
        } else {
            colors.CHROME
        },
    );
    ui::text(
        list,
        atlas,
        rect.shrink(4.),
        value,
        if enabled {
            colors.TEXT
        } else {
            colors.TEXT_MUTED.with_alpha(0.4)
        },
        Align::Center,
    );
    enabled && clicked(input, rect)
}
fn press_icon(
    list: &mut PaintList,
    input: &Input,
    rect: Rect,
    icon: Icon,
    enabled: bool,
) -> bool {
    let styles = list.styles();
    let colors = styles.colors();

    paint::rect(
        list,
        rect,
        if rect.contains(input.mouse) && enabled {
            colors.SUNKEN
        } else {
            colors.CHROME
        },
    );
    icons::draw(
        list,
        rect,
        icon,
        14.,
        if enabled {
            colors.TEXT
        } else {
            colors.TEXT_MUTED.with_alpha(0.4)
        },
    );
    enabled && clicked(input, rect)
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
    pub fn editing(&self) -> bool {
        self.field.is_some()
            || self.choosing
            || self.enum_owner.is_some()
            || self.context_menu.is_some()
            || self.color_popup.is_some()
            || self.curve_editors.values().any(curve::Editor::dragging)
    }
    pub fn cancel(&mut self) -> bool {
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
    fn definition(&self, effect: &Value) -> Option<Value> {
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
    pub fn height(&self, layer: &Value) -> f32 {
        if self.choosing {
            return 70.
                + self.catalogue["packages"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|p| p["manifest"]["effects"].as_array().map_or(0, Vec::len))
                    .sum::<usize>() as f32
                    * 28.;
        }
        layer["effects"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|e| {
                40. + if e["node_graph"].is_object() { 28. } else { 0. }
                    + self.definition(e).map_or(0., |d| {
                        d["params"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|p| {
                                let kind = p["kind"].as_str().unwrap_or("float");
                                if kind == "curve" {
                                    20. + curve::HEIGHT
                                } else if kind == "color" {
                                    // One swatch row; channels live in the popup.
                                    26. + 20.
                                } else {
                                    dimensions(kind) as f32 * 26. + 20.
                                }
                            })
                            .sum::<f32>()
                    })
            })
            .sum::<f32>()
            + 94.
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
        let categories = self.context_categories(locale);
        let Some(menu) = self.context_menu.as_ref() else {
            return;
        };
        let anchor = menu.anchor;
        let object = menu.object;
        let control = menu.control.clone();
        let effect = menu.effect.clone();
        let mut active_category = menu.category.clone();
        let styles = list.styles();
        let row = styles.metric("context.row.height", 20.).clamp(18., 24.);
        let width = 250f32.min(bounds.width());
        let height = (4 + categories.len()) as f32 * row + 6.;
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
        let clear_effects = word(locale, "清除所有效果", "Remove All Effects");
        let header = [
            (control.as_str(), false, false),
            (effect.as_str(), false, false),
            (clear_effects.as_str(), true, false),
        ];
        for (index, (label, enabled, _)) in header.iter().enumerate() {
            let area = context_row(card, width, row, index);
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
                [card.min[0] + 4., card.min[1] + 3. * row + 3.],
                [card.max[0] - 4., card.min[1] + 3. * row + 3.],
            ],
            styles.color("context.border"),
        );
        let mut hovered = None;
        for (offset, category) in categories.iter().enumerate() {
            let index = offset + 3;
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
                style.foreground.with_alpha(0.82),
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
        if chosen == Some(2) {
            let commands = state["project"]["layers"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|layer| layer["id"].as_u64() == Some(object))
                .and_then(|layer| layer["effects"].as_array())
                .into_iter()
                .flatten()
                .filter_map(|effect| effect["id"].as_u64())
                .map(|effect| Command::Effect {
                    object,
                    action: EffectAction::Remove { effect },
                })
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

        let original_input = input;
        let neutral = Input::default();
        let modal_owner = self.enum_owner.clone();
        let input = if modal_owner.is_some() || self.color_popup.is_some() {
            &neutral
        } else {
            input
        };
        let mut overlays = list.child();
        self.cursor = false;
        self.drag_readout = None;
        let object = layer["id"].as_u64().unwrap();
        let locked = layer["locked"].as_bool().unwrap_or(false);
        let frame = state["frame"].as_f64().unwrap_or(0.0).floor() as u32;
        let local = state["frame"].as_f64().unwrap_or(0.0)
            - layer["timeline"]["offset_frame"].as_f64().unwrap_or(0.0);
        if self.field.as_ref().is_some_and(|f| {
            f.object != object || locked || input.pressed.is_some_and(|(_, p)| !f.rect.contains(p))
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
            if press(
                list,
                atlas,
                input,
                Rect::new(area.min[0] + 20., *y, area.width() - 40., 28.),
                locale.t(T::AddEffect),
                !locked,
            ) {
                self.choosing = !self.choosing;
                self.query.clear();
                if self.choosing {
                    self.color_popup = None;
                }
            }
            *y += 34.;
            if ui::button_styled(
                list,
                atlas,
                Rect::new(area.min[0] + 20., *y, area.width() - 40., 24.),
                if locale == Locale::Zh {
                    "新建节点效果"
                } else {
                    "New Node Effect"
                },
                input,
                !locked
                    && object != 0
                    && !matches!(layer["content"]["kind"].as_str(), Some("audio" | "null")),
                &styles,
                "button.icon",
            )
            .clicked
            {
                out.push(Action::NewNodeEffect(object));
            }
            *y += 28.;
        }
        if layer["effects"].as_array().is_none_or(Vec::is_empty) && !self.choosing {
            text(
                list,
                atlas,
                Rect::new(area.min[0] + 24., *y, area.width() - 40., 24.),
                locale.t(T::NoEffects),
                true,
            );
            *y += 28.;
        }
        if self.choosing {
            let search = Rect::new(area.min[0] + 20., *y, area.width() - 40., 28.);
            self.search.draw(
                list,
                atlas,
                search,
                &mut self.query,
                locale.t(T::SearchEffects),
                input,
                true,
            );
            *y += 34.;

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
                let rect = Rect::new(area.min[0] + 12. + index as f32 * 58., *y, 54., 22.);
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
            *y += 28.;
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
                let header_rect = Rect::new(area.min[0] + 16., *y, area.width() - 32., 28.);

                // Category header background
                paint::rect(list, header_rect, colors.SUNKEN.with_alpha(0.3));

                // Collapse/expand chevron
                let chevron_rect = Rect::new(area.min[0] + 20., *y + 4., 16., 20.);
                icons::draw(
                    list,
                    chevron_rect,
                    if is_collapsed {
                        Icon::ChevronRight
                    } else {
                        Icon::ChevronDown
                    },
                    14.,
                    colors.TEXT,
                );

                // Category name
                ui::text(
                    list,
                    atlas,
                    Rect::new(area.min[0] + 40., *y + 4., area.width() - 60., 20.),
                    category_name,
                    colors.TEXT,
                    Align::Left,
                );

                // Effect count badge
                let count_text = format!("{}", effects.len());
                ui::text(
                    list,
                    atlas,
                    Rect::new(area.max[0] - 60., *y + 4., 40., 20.),
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

                *y += 32.;

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
            let Some(def) = self.definition(effect) else {
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

            // Effect header with collapse triangle
            let header_rect = Rect::new(area.min[0] + 14., *y, area.width() - 28., 30.);

            if input.pressed.is_some_and(|(button, point)| {
                button == MouseButton::Right && header_rect.contains(point)
            }) {
                self.context_menu = Some(EffectContextMenu {
                    anchor: input.mouse,
                    object,
                    control: word(locale, "效果控件", "Effect Controls"),
                    effect: title.to_string(),
                    category: None,
                });
            }

            // Background highlight on hover
            if header_rect.contains(input.mouse) {
                box_style(
                    list,
                    header_rect,
                    styles.resolve("row", States::default().with(State::Hovered)).unwrap(),
                );
            }

            // Collapse/expand chevron
            let chevron_rect = Rect::new(area.min[0] + 18., *y + 5., 16., 20.);
            if press_icon(
                list,
                input,
                chevron_rect,
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

            // Enable/disable button
            if press(
                list,
                atlas,
                input,
                Rect::new(area.min[0] + 38., *y, 28., 26.),
                if enabled { "fx" } else { "—" },
                !locked,
            ) {
                out.push(Action::Edit(Command::Effect {
                    object,
                    action: EffectAction::Enable {
                        effect: id,
                        enabled: !enabled,
                    },
                }));
            }

            // Effect title
            text(
                list,
                atlas,
                Rect::new(area.min[0] + 72., *y, (area.width() - 179.).max(1.), 26.),
                title,
                !enabled,
            );

            // Action buttons: Move Up, Duplicate, Remove
            for (i, (icon, action, allowed)) in [
                (
                    Icon::ChevronUp,
                    EffectAction::Move {
                        effect: id,
                        index: index.saturating_sub(1),
                    },
                    index > 0,
                ),
                (Icon::Plus, EffectAction::Duplicate { effect: id }, true),
                (Icon::Close, EffectAction::Remove { effect: id }, true),
            ]
            .into_iter()
            .enumerate()
            {
                if press_icon(
                    list,
                    input,
                    Rect::new(area.max[0] - 94. + i as f32 * 26., *y, 24., 26.),
                    icon,
                    !locked && allowed,
                ) {
                    out.push(Action::Edit(Command::Effect { object, action }));
                }
            }
            *y += 34.;

            if effect["node_graph"].is_object() {
                if ui::button_styled(
                    list,
                    atlas,
                    Rect::new(area.min[0] + 24., *y, area.width() - 48., 22.),
                    if locale == Locale::Zh {
                        "编辑节点"
                    } else {
                        "Edit Nodes"
                    },
                    input,
                    !locked,
                    &styles,
                    "button.icon",
                )
                .clicked
                {
                    out.push(Action::EditNodeEffect { object, effect: id });
                }
                *y += 26.;
            }
            // Skip parameters if collapsed
            if is_collapsed {
                continue;
            }

            // Render effect parameters
            for parameter in def["params"].as_array().into_iter().flatten() {
                let param = parameter["id"].as_str().unwrap();
                let kind = parameter["kind"].as_str().unwrap_or("float");
                let implemented = parameter["implemented"].as_bool().unwrap_or(true);
                let name = parameter["name"].as_str().unwrap_or(param);
                text(
                    list,
                    atlas,
                    Rect::new(area.min[0] + 24., *y, area.width() - 40., 20.),
                    name,
                    !implemented,
                );
                let parameter_row = Rect::new(area.min[0] + 18., *y, area.width() - 36., 24.);
                if input.pressed.is_some_and(|(button, point)| {
                    button == MouseButton::Right && parameter_row.contains(point)
                }) {
                    self.context_menu = Some(EffectContextMenu {
                        anchor: input.mouse,
                        object,
                        control: name.to_string(),
                        effect: title.to_string(),
                        category: None,
                    });
                }
                *y += 20.;
                if kind == "curve" {
                    let key = (object, id, param.to_string());
                    let Ok(value) =
                        serde_json::from_value::<EffectParam>(effect["params"][param].clone())
                    else {
                        continue;
                    };
                    let Some(track) = value.curve else {
                        continue;
                    };
                    let current = curve::value_at(&track, local);
                    let editor = self.curve_editors.entry(key).or_default();
                    editor.draw(
                        list,
                        atlas,
                        Rect::new(area.min[0] + 24., *y, area.width() - 48., curve::HEIGHT),
                        input,
                        !locked && implemented,
                        &current,
                        |value| {
                            Command::Effect {
                                object,
                                action: EffectAction::SetCurveObject {
                                    effect: id,
                                    param: param.into(),
                                    frame,
                                    value,
                                },
                            }
                        },
                        out,
                        &mut self.drag_readout,
                    );
                    *y += curve::HEIGHT + 8.;
                    continue;
                }
                let key = (object, id, param.to_string());
                if !self.tracks.contains_key(&key) {
                    let Ok(parameter) =
                        serde_json::from_value::<EffectParam>(effect["params"][param].clone())
                    else {
                        continue;
                    };
                    self.tracks.insert(key, parameter);
                }
                let track = &self.tracks[&(object, id, param.to_string())];
                let animated = !track.track.keys.is_empty();
                let mut value = track.sample(local);
                let animate = Rect::new(area.min[0] + 24., *y, 24., 24.);
                if press(
                    list,
                    atlas,
                    input,
                    animate,
                    if animated { "●" } else { "○" },
                    !locked && implemented && parameter["animatable"].as_bool().unwrap_or(false),
                ) {
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
                if kind == "color" {
                    self.color_row(
                        list,
                        atlas,
                        area,
                        *y,
                        object,
                        id,
                        param,
                        &value,
                        input,
                        !locked && implemented,
                        out,
                    );
                    *y += 26. + 8.;
                    continue;
                }
                for component in 0..dimensions(kind) {
                    let inset = if dimensions(kind) > 1 { 20. } else { 0. };
                    if dimensions(kind) > 1 {
                        let axis = ["X", "Y", "Z", "W"][component];
                        ui::text(
                            list,
                            atlas,
                            Rect::new(area.min[0] + 62., *y, 18., 24.),
                            axis,
                            colors.TEXT_MUTED,
                            Align::Left,
                        );
                    }
                    let rect = Rect::new(
                        area.min[0] + 62. + inset,
                        *y,
                        area.width() - 82. - inset,
                        24.,
                    );
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
                                let items: Vec<_> = parameter["options"]
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                                    .map(|o| motion_ui::widgets::DropdownItem {
                                        label: o.as_str().unwrap_or(""),
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
                                    value[0] as usize,
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
                                result.map(|i| i as f32)
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
                    *y += 26.;
                }
                *y += 8.;
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
        let edge = styles.metric("field.height", 18.).min(24.);
        let preview = Rect::new(
            area.min[0] + 62.,
            y + (24. - edge) * 0.5,
            edge + styles.metric("icon.slot", 20.),
            edge,
        );
        color_well::swatch(list, preview, values, input, enabled);
        #[cfg(test)]
        self.test_rects
            .insert((object, effect, param.into(), -1), preview);
        let text_area = Rect::new(
            preview.max[0] + styles.metric("padding", 5.),
            y,
            (area.max[0]
                - styles.metric("padding.panel", 8.)
                - preview.max[0]
                - styles.metric("padding", 5.))
            .max(0.),
            24.,
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
                serde_json::from_value::<EffectParam>(e["params"][&popup.param].clone()).ok()
            });
        let Some(track) = track else {
            self.color_popup = None;
            return;
        };
        let mut value = track.sample(local);
        let styles = paint.styles();
        let pad = styles.metric("padding.panel", 8.);
        let row = styles.metric("row.height", 22.);
        let slot = styles.metric("icon.slot", 20.);
        let width = (styles.metric("label.column", 72.)
            + styles.metric("value.min_width", 64.)
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
            super::numeric::passive(list, atlas, rect, value[component] as f64, input, enabled);
        }
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
            draw(
                &mut panels,
                &input(start[0] + 20., start[1], false, true),
            ),
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
