//! Catalogue and parameter controls share the core effect command contracts.
use crate::{
    i18n::{Locale, Text as T},
    panels::Action,
};
use motion_core::EffectParam;
use motion_core::{Command, EffectAction};
use motion_ui::{
    input::{Input, Key, MouseButton, Rect},
    paint::{self, PaintList},
    text::TextAtlas,
    theme::palette,
    ui::{self, Align},
};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Default)]
pub struct Controls {
    pub catalogue: Value,
    pub choosing: bool,
    query: String,
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
    text: String,
    replace: bool,
}
fn clicked(input: &Input, rect: Rect) -> bool {
    input
        .pressed
        .is_some_and(|(b, p)| b == MouseButton::Left && rect.contains(p))
}
fn text(list: &mut PaintList, atlas: &mut TextAtlas, rect: Rect, value: &str, muted: bool) {
    ui::text(
        list,
        atlas,
        rect,
        value,
        if muted {
            palette::TEXT_MUTED
        } else {
            palette::TEXT
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
    paint::rect(
        list,
        rect,
        if rect.contains(input.mouse) && enabled {
            palette::SUNKEN
        } else {
            palette::CHROME
        },
    );
    ui::text(
        list,
        atlas,
        rect.shrink(4.),
        value,
        if enabled {
            palette::TEXT
        } else {
            palette::TEXT_MUTED.with_alpha(0.4)
        },
        Align::Center,
    );
    enabled && clicked(input, rect)
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
        self.field.is_some() || self.choosing
    }
    pub fn cancel(&mut self) {
        self.field = None;
        self.choosing = false;
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
        self.choosing = true;
        self.render_mode(true, list, atlas, area, y, layer, state, input, locale, out);
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
        let object = layer["id"].as_u64().unwrap();
        let locked = layer["locked"].as_bool().unwrap_or(false);
        let frame = state["frame"].as_f64().unwrap_or(0.0).floor() as u32;
        let local = state["frame"].as_f64().unwrap_or(0.0)
            - layer["timeline"]["offset_frame"].as_f64().unwrap_or(0.0);
        if self.field.as_ref().is_some_and(|f| f.object != object) {
            self.field = None;
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
            paint::rect(list, search, palette::SUNKEN);
            paint::stroke(list, search, palette::ACCENT, 1.);
            self.query.push_str(&input.text);
            if input.keys.iter().any(|(k, _)| *k == Key::Backspace) {
                self.query.pop();
            }
            text(
                list,
                atlas,
                search.shrink(6.),
                if self.query.is_empty() {
                    locale.t(T::SearchEffects)
                } else {
                    &self.query
                },
                self.query.is_empty(),
            );
            *y += 34.;

            // Group effects by category for AE-style organization
            let mut categories: std::collections::HashMap<
                String,
                Vec<(String, String, String, String)>,
            > = std::collections::HashMap::new();

            for package in self.catalogue["packages"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|p| p["enabled"] != false)
            {
                let package_name = package["manifest"]["name"]
                    .as_str()
                    .unwrap_or("Effects")
                    .to_string();

                for effect in package["manifest"]["effects"]
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    let name = if locale == Locale::Zh {
                        effect["name"].as_str()
                    } else {
                        effect["english_name"].as_str()
                    }
                    .unwrap_or("Effect");

                    if !self.query.is_empty()
                        && !name.to_lowercase().contains(&self.query.to_lowercase())
                        && !effect["id"]
                            .as_str()
                            .unwrap_or("")
                            .contains(&self.query.to_lowercase())
                    {
                        continue;
                    }

                    categories.entry(package_name.clone()).or_default().push((
                        package["manifest"]["id"].as_str().unwrap_or("").to_string(),
                        package["manifest"]["version"]
                            .as_str()
                            .unwrap_or("")
                            .to_string(),
                        package["hash"].as_str().unwrap_or("").to_string(),
                        effect["id"].as_str().unwrap_or("").to_string(),
                    ));
                }
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
                paint::rect(list, header_rect, palette::SUNKEN.with_alpha(0.3));

                // Collapse/expand triangle
                let triangle_rect = Rect::new(area.min[0] + 20., *y + 4., 16., 20.);
                ui::text(
                    list,
                    atlas,
                    triangle_rect,
                    if is_collapsed { "▶" } else { "▼" },
                    palette::TEXT,
                    Align::Left,
                );

                // Category name
                ui::text(
                    list,
                    atlas,
                    Rect::new(area.min[0] + 40., *y + 4., area.width() - 60., 20.),
                    category_name,
                    palette::TEXT,
                    Align::Left,
                );

                // Effect count badge
                let count_text = format!("{}", effects.len());
                ui::text(
                    list,
                    atlas,
                    Rect::new(area.max[0] - 60., *y + 4., 40., 20.),
                    &count_text,
                    palette::TEXT_MUTED,
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

                        if press(
                            list,
                            atlas,
                            input,
                            Rect::new(area.min[0] + 32., *y, area.width() - 52., 26.),
                            effect_name,
                            !locked,
                        ) {
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
                        *y += 28.;
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
                paint::rect(list, header_rect, palette::SUNKEN.with_alpha(0.2));
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
                    let rect = Rect::new(area.min[0] + 62., *y, area.width() - 82., 24.);
                    let active = self.field.as_ref().is_some_and(|f| {
                        f.effect == id && f.param == param && f.component == component
                    });
                    let shown = if active {
                        self.field.as_ref().unwrap().text.clone()
                    } else if kind == "bool" {
                        if value[0] > 0.5 {
                            locale.t(T::Enabled)
                        } else {
                            locale.t(T::Disabled)
                        }
                        .into()
                    } else if kind == "enum" {
                        parameter["options"][value[0] as usize]
                            .as_str()
                            .unwrap_or("")
                            .to_string()
                    } else {
                        format!(
                            "{}{:.3}",
                            if dimensions(kind) > 1 {
                                ["X  ", "Y  ", "Z  ", "A  "][component]
                            } else {
                                ""
                            },
                            value[component]
                        )
                    };
                    paint::rect(list, rect, palette::SUNKEN);
                    text(list, atlas, rect.shrink(4.), &shown, !implemented);
                    if active {
                        paint::stroke(list, rect, palette::ACCENT, 1.);
                    }
                    if clicked(input, rect) && !locked && implemented {
                        if kind == "bool" || kind == "enum" {
                            value[0] = if kind == "bool" {
                                if value[0] > 0.5 {
                                    0.
                                } else {
                                    1.
                                }
                            } else {
                                let min = parameter["min"].as_f64().unwrap_or(0.) as f32;
                                let max = parameter["max"].as_f64().unwrap_or(1.) as f32;
                                if value[0] >= max {
                                    min
                                } else {
                                    value[0] + 1.
                                }
                            };
                            out.push(Action::Edit(Command::Effect {
                                object,
                                action: EffectAction::Set {
                                    effect: id,
                                    param: param.into(),
                                    frame,
                                    value,
                                },
                            }));
                        } else {
                            self.field = Some(Field {
                                object,
                                effect: id,
                                param: param.into(),
                                component,
                                text: format!("{}", value[component]),
                                replace: true,
                            });
                        }
                    }
                    *y += 26.;
                }
                if let Some(field) = self
                    .field
                    .as_mut()
                    .filter(|f| f.effect == id && f.param == param)
                {
                    if !input.text.is_empty() {
                        if field.replace {
                            field.text.clear();
                            field.replace = false;
                        }
                        field.text.push_str(&input.text);
                    }
                    if input.keys.iter().any(|(k, _)| *k == Key::Backspace) {
                        if field.replace {
                            field.text.clear();
                            field.replace = false;
                        } else {
                            field.text.pop();
                        }
                    }
                    if input.keys.iter().any(|(k, _)| *k == Key::Enter) {
                        if let Ok(number) = field.text.parse::<f32>() {
                            if number.is_finite() {
                                value[field.component] = number.clamp(
                                    parameter["min"].as_f64().unwrap_or(-1e9) as f32,
                                    parameter["max"].as_f64().unwrap_or(1e9) as f32,
                                );
                                out.push(Action::Edit(Command::Effect {
                                    object,
                                    action: EffectAction::Set {
                                        effect: id,
                                        param: param.into(),
                                        frame,
                                        value,
                                    },
                                }));
                                self.field = None;
                            }
                        }
                    }
                }
                *y += 8.;
            }
        }
    }
}
