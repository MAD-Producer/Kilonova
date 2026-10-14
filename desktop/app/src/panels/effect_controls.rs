//! Catalogue and parameter controls share the core effect command contracts.
use crate::{
    i18n::{Locale, Text as T},
    panels::Action,
};
use motion_core::EffectParam;
use motion_core::{Command, EffectAction};
use motion_ui::{
    input::{Input, MouseButton, Rect},
    paint::{self, PaintList},
    text::TextAtlas,
    ui::{self, Align},
};
use serde_json::Value;
use std::collections::HashMap;
mod library;
pub(super) use library::entries;

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
    collapsed_categories: std::collections::HashSet<String>,
    collapsed_effects: std::collections::HashSet<u64>,
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
        self.field.is_some() || self.choosing || self.enum_owner.is_some()
    }
    pub fn cancel(&mut self) -> bool {
        let begun = self.field.take().is_some_and(|mut f| f.number.cancel());
        self.choosing = false;
        self.enum_owner = None;
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
                40. + self.definition(e).map_or(0., |d| {
                    d["params"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|p| {
                            dimensions(p["kind"].as_str().unwrap_or("float")) as f32 * 26. + 20.
                        })
                        .sum::<f32>()
                })
            })
            .sum::<f32>()
            + 38.
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
        let input = if modal_owner.is_some() {
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
            }
            *y += 34.;
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

                // Collapse/expand triangle
                let triangle_rect = Rect::new(area.min[0] + 20., *y + 4., 16., 20.);
                ui::text(
                    list,
                    atlas,
                    triangle_rect,
                    if is_collapsed { "▶" } else { "▼" },
                    colors.TEXT,
                    Align::Left,
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

            // Background highlight on hover
            if header_rect.contains(input.mouse) {
                paint::rect(list, header_rect, colors.SUNKEN.with_alpha(0.2));
            }

            // Collapse/expand triangle
            let triangle_rect = Rect::new(area.min[0] + 18., *y + 5., 16., 20.);
            if press(
                list,
                atlas,
                input,
                triangle_rect,
                if is_collapsed { "▶" } else { "▼" },
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
            for (i, (symbol, action, allowed)) in [
                (
                    "↑",
                    EffectAction::Move {
                        effect: id,
                        index: index.saturating_sub(1),
                    },
                    index > 0,
                ),
                ("+", EffectAction::Duplicate { effect: id }, true),
                ("×", EffectAction::Remove { effect: id }, true),
            ]
            .into_iter()
            .enumerate()
            {
                if press(
                    list,
                    atlas,
                    input,
                    Rect::new(area.max[0] - 94. + i as f32 * 26., *y, 24., 26.),
                    symbol,
                    !locked && allowed,
                ) {
                    out.push(Action::Edit(Command::Effect { object, action }));
                }
            }
            *y += 34.;

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
                *y += 20.;
                if kind == "curve" {
                    text(
                        list,
                        atlas,
                        Rect::new(area.min[0] + 24., *y, area.width() - 40., 24.),
                        locale.t(T::ColorCurve),
                        true,
                    );
                    *y += 28.;
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
                for component in 0..dimensions(kind) {
                    let inset = if dimensions(kind) > 1 { 20. } else { 0. };
                    if dimensions(kind) > 1 {
                        let axis = if kind == "color" {
                            ["R", "G", "B", "A"][component]
                        } else {
                            ["X", "Y", "Z", "W"][component]
                        };
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
                    let active = self.field.as_ref().is_some_and(|f| {
                        f.object == object
                            && f.effect == id
                            && f.param == param
                            && f.component == component
                    });
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
                        if clicked(input, rect) && !active && !locked && implemented {
                            if self.field.take().is_some_and(|mut f| f.number.cancel()) {
                                out.push(Action::History(4));
                            }
                            self.field = Some(Field {
                                object,
                                effect: id,
                                param: param.into(),
                                component,
                                number: Default::default(),
                                rect,
                            });
                        }
                        if let Some(field) = self.field.as_mut().filter(|f| {
                            f.object == object
                                && f.effect == id
                                && f.param == param
                                && f.component == component
                        }) {
                            field.rect = rect;
                            self.cursor |=
                                field.number.editor.dragging() || rect.contains(input.mouse);
                            let min = parameter["min"].as_f64().unwrap_or(-1e9);
                            let max = parameter["max"].as_f64().unwrap_or(1e9);
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
                            let events = field.number.draw(
                                list,
                                atlas,
                                rect,
                                value[component] as f64,
                                options,
                                input,
                            );
                            if let Some(note) = field.number.readout() {
                                self.drag_readout = Some((input.mouse, note));
                            }
                            if super::numeric::apply(events, out, |v| {
                                value[component] = v;
                                Command::Effect {
                                    object,
                                    action: EffectAction::Set {
                                        effect: id,
                                        param: param.into(),
                                        frame,
                                        value,
                                    },
                                }
                            }) {
                                self.field = None;
                            }
                        } else {
                            self.cursor |= !locked && implemented && rect.contains(input.mouse);
                            super::numeric::passive(
                                list,
                                atlas,
                                rect,
                                value[component] as f64,
                                input,
                                !locked && implemented,
                            );
                        }
                    }
                    *y += 26.;
                }
                *y += 8.;
            }
        }
        list.append(overlays);
    }
}
