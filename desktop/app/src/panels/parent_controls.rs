use crate::{
    i18n::{Locale, Text as T},
    panels::Action,
};
use motion_core::Command;
use motion_ui::{
    input::{Input, MouseButton, Rect},
    paint::{self, PaintList},
    text::TextAtlas,
    ui::{self, Align},
};
use serde_json::Value;

/// Spatial parent candidates shared by the inspector and timeline.
/// Object IDs, not display names or row indices, identify the destination.
pub(super) fn candidates(state: &Value, object: u64, locale: Locale) -> Vec<(u64, String)> {
    use std::collections::{HashMap, HashSet};
    let layers = state["project"]["layers"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let camera = &state["project"]["camera"];
    let mut parents: HashMap<u64, Option<u64>> = layers
        .iter()
        .filter_map(|l| Some((l["id"].as_u64()?, l["parent"]["object"].as_u64())))
        .collect();
    if camera["created"] == true {
        parents.insert(0, camera["parent"]["object"].as_u64());
    }
    let allowed = |id: u64| {
        let mut current = Some(id);
        let mut seen = HashSet::new();
        while let Some(next) = current {
            if next == object || !seen.insert(next) {
                return false;
            }
            let Some(parent) = parents.get(&next) else {
                return false;
            };
            current = *parent;
        }
        true
    };
    let mut choices = Vec::new();
    if camera["created"] == true && allowed(0) {
        choices.push((
            0,
            format!(
                "1. {}",
                if locale == Locale::Zh {
                    "摄像机"
                } else {
                    "Camera"
                }
            ),
        ));
    }
    choices.extend(layers.iter().rev().enumerate().filter_map(|(index, l)| {
        let id = l["id"].as_u64()?;
        if l["content"]["kind"] == "audio" || !allowed(id) {
            return None;
        }
        Some((
            id,
            format!(
                "{}. {}",
                index + 1 + usize::from(camera["created"] == true),
                l["name"].as_str().unwrap_or("Layer")
            ),
        ))
    }));
    choices
}

pub(super) fn menu_entries(
    state: &Value,
    object: u64,
    locale: Locale,
) -> Vec<(Option<u64>, String, bool)> {
    let allowed: std::collections::HashSet<_> = candidates(state, object, locale)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let camera = state["project"]["camera"]["created"] == true;
    let mut entries = vec![(None, locale.t(T::None).to_string(), true)];
    if camera {
        entries.push((
            Some(0),
            format!(
                "1. {}",
                if locale == Locale::Zh {
                    "摄像机"
                } else {
                    "Camera"
                }
            ),
            allowed.contains(&0),
        ));
    }
    entries.extend(
        state["project"]["layers"]
            .as_array()
            .into_iter()
            .flatten()
            .rev()
            .enumerate()
            .filter_map(|(index, l)| {
                let id = l["id"].as_u64()?;
                Some((
                    Some(id),
                    format!(
                        "{}. {}",
                        index + 1 + usize::from(camera),
                        l["name"].as_str().unwrap_or("Layer")
                    ),
                    allowed.contains(&id),
                ))
            }),
    );
    entries
}

#[derive(Default)]
pub struct Controls {
    pub open: bool,
    object: Option<u64>,
    scroll: usize,
}
fn clicked(input: &Input, rect: Rect) -> bool {
    input
        .pressed
        .is_some_and(|(b, p)| b == MouseButton::Left && rect.contains(p))
}
impl Controls {
    pub fn render(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: &mut f32,
        object: u64,
        owner: &Value,
        state: &Value,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        let styles = list.styles();
        let colors = styles.colors();

        if self.object != Some(object) {
            self.object = Some(object);
            self.open = false;
            self.scroll = 0;
        }
        let layers = state["project"]["layers"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let current = owner["parent"]["object"].as_u64();
        let name = if current == Some(0) {
            if locale == Locale::Zh {
                "摄像机"
            } else {
                "Camera"
            }
        } else {
            current
                .and_then(|id| layers.iter().find(|l| l["id"].as_u64() == Some(id)))
                .and_then(|l| l["name"].as_str())
                .unwrap_or(locale.t(T::None))
        };
        let rect = Rect::new(area.min[0] + 16., *y, area.width() - 32., 26.);
        paint::rect(list, rect, colors.SUNKEN);
        ui::text(
            list,
            atlas,
            rect.shrink(4.),
            &format!("{}  ›  {}", locale.t(T::ParentLink), name),
            colors.TEXT,
            Align::Left,
        );
        let locked = owner["locked"].as_bool().unwrap_or(false);
        if clicked(input, rect) && !locked {
            self.open = !self.open;
        }
        *y += 32.;
        if !self.open {
            return;
        }
        let mut options = vec![(None, locale.t(T::None))];
        let choices = candidates(state, object, locale);
        for (id, name) in &choices {
            options.push((Some(*id), name.as_str()));
        }
        if area.contains(input.mouse) {
            if input.wheel[1] > 0. {
                self.scroll = (self.scroll + 1).min(options.len().saturating_sub(8));
            } else if input.wheel[1] < 0. {
                self.scroll = self.scroll.saturating_sub(1);
            }
        }
        for (parent, name) in options.into_iter().skip(self.scroll).take(8) {
            let row = Rect::new(rect.min[0], *y, rect.width(), 24.);
            paint::rect(
                list,
                row,
                if parent == current {
                    colors.ACCENT.with_alpha(0.2)
                } else {
                    colors.CHROME
                },
            );
            ui::text(list, atlas, row.shrink(4.), name, colors.TEXT, Align::Left);
            if clicked(input, row) && parent != current {
                out.push(Action::Edit(Command::Parent {
                    object,
                    parent,
                    frame: state["frame"].as_f64().unwrap_or(0.0).floor() as u32,
                }));
                self.open = false;
            }
            *y += 26.;
        }
    }
}
