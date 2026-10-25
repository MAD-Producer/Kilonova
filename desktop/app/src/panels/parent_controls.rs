use crate::{
    i18n::{Locale, Text as T},
    panels::Action,
};
use motion_core::Command;
use motion_core::ParentMode;
use motion_ui::{
    input::{Input, MouseButton, Rect},
    paint::PaintList,
    text::TextAtlas,
    ui::Align,
};
use serde_json::Value;
mod chooser;
mod picking;
#[cfg(test)]
mod tests;
pub(super) use chooser::{dispatch, Chooser, Decision};

pub(super) fn targets(state: &Value, anchor: u64, selected: &[u64]) -> Option<Vec<u64>> {
    if state["observing"] == true {
        return None;
    }
    let mut targets = if selected.contains(&anchor) {
        selected.to_vec()
    } else {
        vec![anchor]
    };
    targets.sort_unstable();
    targets.dedup();
    targets
        .iter()
        .all(|object| {
            if *object == 0 {
                return state["project"]["camera"]["created"] == true;
            }
            super::timeline_columns::owner(state, *object)
                .is_some_and(|owner| owner["locked"] != true && owner["content"]["kind"] != "audio")
        })
        .then_some(targets)
}

/// Spatial parent candidates shared by the inspector and timeline.
/// Object IDs, not display names or row indices, identify the destination.
pub(super) fn candidates(state: &Value, object: u64, locale: Locale) -> Vec<(u64, String)> {
    candidates_for(state, &[object], locale)
}
fn candidates_for(state: &Value, objects: &[u64], locale: Locale) -> Vec<(u64, String)> {
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
            if objects.contains(&next) || !seen.insert(next) {
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

pub struct Controls {
    pub open: bool,
    object: Option<u64>,
    pub(super) mode: ParentMode,
    chooser: Option<Chooser>,
    window: Option<motion_ui::dock::Panel>,
    anchor: Option<(Option<motion_ui::dock::Panel>, Rect)>,
    seen: bool,
    deferred: bool,
    picker: Option<picking::Pick>,
    pick_input: Option<Input>,
    pick_owned: bool,
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            open: false,
            object: None,
            mode: ParentMode::PreserveWorld,
            chooser: None,
            window: None,
            anchor: None,
            seen: false,
            deferred: false,
            picker: None,
            pick_input: None,
            pick_owned: false,
        }
    }
}
fn clicked(input: &Input, rect: Rect) -> bool {
    input
        .pressed
        .is_some_and(|(b, p)| b == MouseButton::Left && rect.contains(p))
}
impl Controls {
    pub fn cancel(&mut self) {
        self.open = false;
        self.chooser = None;
        self.anchor = None;
        self.picker = None;
        self.pick_input = None;
    }
    pub(super) fn picking(&self) -> bool {
        self.picker.is_some() || self.pick_owned
    }
    pub(super) fn prepare_window(
        &mut self,
        window: Option<motion_ui::dock::Panel>,
        input: &Input,
    ) -> bool {
        self.pick_owned = false;
        self.pick_input = None;
        if let Some(picker) = &mut self.picker {
            if picker.window == window {
                self.pick_owned = true;
                self.pick_input = Some(input.clone());
                picker.rows.clear();
            } else if input.pressed.is_some() || !input.keys.is_empty() || !input.text.is_empty() {
                self.picker = None;
                self.pick_owned = true;
            }
        }
        if self.open
            && self
                .anchor
                .as_ref()
                .is_some_and(|(owner, _)| *owner != window)
            && (input.pressed.is_some() || !input.keys.is_empty() || !input.text.is_empty())
        {
            self.cancel();
        }
        self.window = window;
        self.deferred = true;
        if self
            .anchor
            .as_ref()
            .is_some_and(|(owner, _)| *owner == window)
        {
            self.seen = false;
        }
        self.pick_owned
    }
    pub(super) fn begin_timeline(&mut self, input: &Input) {
        if !self.deferred {
            self.prepare_window(self.window, input);
            self.deferred = false;
        }
    }
    pub(super) fn begin_pick(
        &mut self,
        state: &Value,
        object: u64,
        selected: &[u64],
        rect: Rect,
        input: &Input,
    ) {
        let Some(pick) = picking::Pick::new(state, object, selected, self.window, rect, self.mode)
        else {
            return;
        };
        self.cancel();
        self.picker = Some(pick);
        self.pick_input = Some(input.clone());
        self.pick_owned = true;
    }
    pub(super) fn pick_row(&mut self, object: u64, rect: Rect, link: Option<Rect>) {
        if let Some(picker) = self
            .picker
            .as_mut()
            .filter(|picker| picker.window == self.window)
        {
            picker.row(object, rect, link);
        }
    }
    pub(super) fn finish_timeline(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        bounds: Rect,
        state: &Value,
        selected: &[u64],
        input: &Input,
        locale: Locale,
        suspended: bool,
        out: &mut Vec<Action>,
    ) {
        if !self.deferred {
            self.finish_window(
                list, atlas, bounds, state, selected, input, locale, suspended, out,
            );
        }
    }
    pub(super) fn finish_window(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        bounds: Rect,
        state: &Value,
        selected: &[u64],
        input: &Input,
        locale: Locale,
        suspended: bool,
        out: &mut Vec<Action>,
    ) {
        self.deferred = false;
        if self
            .picker
            .as_ref()
            .is_some_and(|picker| picker.window == self.window)
        {
            if let Some(input) = self.pick_input.take() {
                let decision = self.picker.as_mut().map_or(Decision::Close, |picker| {
                    picker.draw(list, bounds, state, selected, &input)
                });
                match decision {
                    Decision::Open => {}
                    Decision::Close => self.picker = None,
                    Decision::Apply(commands) => {
                        dispatch(commands, out);
                        self.picker = None;
                    }
                }
            }
        }
        if !self.open {
            self.chooser = None;
            return;
        }
        let Some((owner, rect)) = self.anchor else {
            self.cancel();
            return;
        };
        if owner != self.window {
            return;
        }
        if !self.seen {
            self.cancel();
            return;
        }
        let decision = self.chooser.as_mut().map_or(Decision::Close, |chooser| {
            chooser.draw(
                list,
                atlas,
                rect,
                bounds,
                state,
                selected,
                &mut self.mode,
                input,
                locale,
                suspended,
            )
        });
        match decision {
            Decision::Open => {}
            Decision::Close => self.cancel(),
            Decision::Apply(commands) => {
                dispatch(commands, out);
                self.cancel();
            }
        }
    }
    pub(super) fn text_menu(&mut self, request: &mut super::text_menu::Request<'_>) {
        if let Some(chooser) = self.chooser.as_mut().filter(|_| self.open) {
            chooser.text_menu(request);
        }
    }
    pub fn render(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: &mut f32,
        object: u64,
        owner: &Value,
        state: &Value,
        selected: &[u64],
        suspended: bool,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        let styles = list.styles();
        if self.object != Some(object) {
            self.object = Some(object);
            self.open = false;
            self.chooser = None;
            self.anchor = None;
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
        let padding = styles.metric("padding.panel", f32::default());
        let rect = Rect::new(
            area.min[0] + padding * 2.,
            *y,
            (area.width() - padding * 4.).max(0.),
            styles.metric("parent.field.height", f32::default()),
        );
        let enabled = targets(state, object, selected).is_some();
        let mut states = motion_ui::style::States::default();
        if !enabled {
            states = states.with(motion_ui::style::State::Disabled);
        } else if self.open {
            states = states.with(motion_ui::style::State::Focused);
        } else if rect.contains(input.mouse) {
            states = states.with(motion_ui::style::State::Hovered);
        }
        let style = styles.resolve("field.number", states).unwrap();
        motion_ui::style::box_style(list, rect, style);
        motion_ui::style::text_style(
            list,
            atlas,
            Rect::new(
                rect.min[0] + padding,
                rect.min[1],
                (rect.width() - padding * 2.).max(0.),
                rect.height(),
            ),
            &format!("{}  ›  {}", locale.t(T::ParentLink), name),
            style,
            Align::Left,
        );
        if !self.open && clicked(input, rect) && enabled {
            self.open = !self.open;
            self.chooser = Chooser::new(state, object, selected);
            self.anchor = Some((self.window, rect));
        }
        *y += styles.metric("parent.row.height", f32::default());
        if !self.open {
            self.chooser = None;
            return;
        }
        if self
            .anchor
            .as_ref()
            .is_some_and(|(owner, _)| *owner == self.window)
        {
            self.anchor = Some((self.window, rect));
            self.seen = true;
        }
        if !self.deferred {
            self.finish_window(
                list, atlas, area, state, selected, input, locale, suspended, out,
            );
        }
    }
}
