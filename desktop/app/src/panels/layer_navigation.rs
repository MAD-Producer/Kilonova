//! Layer selection is view state; ordering uses one existing engine batch.
use super::{types::Panels, Action, Command, Locale, Value};
use crate::shortcuts::Command as Shortcut;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    Up,
    Down,
    Top,
    Bottom,
}
impl Order {
    pub fn shortcut(self) -> Shortcut {
        match self {
            Self::Up => Shortcut::OrderLayersUp,
            Self::Down => Shortcut::OrderLayersDown,
            Self::Top => Shortcut::OrderLayersTop,
            Self::Bottom => Shortcut::OrderLayersBottom,
        }
    }
    pub fn label(self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::Up, Locale::Zh) => "上移一层",
            (Self::Down, Locale::Zh) => "下移一层",
            (Self::Top, Locale::Zh) => "置于顶层",
            (Self::Bottom, Locale::Zh) => "置于底层",
            (Self::Up, _) => "Move Up",
            (Self::Down, _) => "Move Down",
            (Self::Top, _) => "Move to Top",
            (Self::Bottom, _) => "Move to Bottom",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Unavailable {
    Empty,
    Locked,
    ReadOnly,
    Stale,
    InvalidSnapshot,
}
impl Unavailable {
    pub fn message(&self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::Empty, Locale::Zh) => "请选择要排列的图层",
            (Self::Locked, Locale::Zh) => "所选图层包含锁定图层，请先解锁",
            (Self::ReadOnly, Locale::Zh) => "当前处于只读模式",
            (Self::Stale, Locale::Zh) => "所选图层已变化，请重新选择",
            (Self::InvalidSnapshot, Locale::Zh) => "无法读取图层排列快照",
            (Self::Empty, _) => "Select layers to arrange",
            (Self::Locked, _) => "Unlock all selected layers before arranging",
            (Self::ReadOnly, _) => "This session is read-only",
            (Self::Stale, _) => "The selected layers changed; select them again",
            (Self::InvalidSnapshot, _) => "Cannot read the layer ordering snapshot",
        }
    }
}

/// Metadata only: do not rebuild authored property/key trees for a key press.
fn visible_layers(state: &Value, query: &str) -> Vec<u64> {
    let query = query.to_lowercase();
    state["project"]["layers"]
        .as_array()
        .into_iter()
        .flatten()
        .rev()
        .filter(|layer| {
            query.is_empty()
                || layer["name"]
                    .as_str()
                    .unwrap_or("Layer")
                    .to_lowercase()
                    .contains(&query)
        })
        .filter_map(|layer| layer["id"].as_u64().filter(|id| *id != 0))
        .collect()
}

pub(super) fn navigate(panels: &mut Panels, state: &Value, below: bool, extend: bool) {
    let visible = visible_layers(state, &panels.timeline.timeline_query);
    if visible.is_empty() {
        return;
    }
    let current = panels
        .selected
        .and_then(|id| visible.iter().position(|item| *item == id));
    let index = match current {
        Some(index) if below => (index + 1).min(visible.len() - 1),
        Some(index) => index.saturating_sub(1),
        None if below => 0,
        None => visible.len() - 1,
    };
    let anchor = panels
        .selection_anchor
        .and_then(|id| visible.iter().position(|item| *item == id))
        .or(current)
        .unwrap_or(index);
    panels.select_only(Some(visible[index]));
    if extend {
        panels.selection_anchor = Some(visible[anchor]);
        panels.selected_layers = visible[anchor.min(index)..=anchor.max(index)].to_vec();
    }
    panels.timeline.reveal_layer = Some(visible[index]);
}

pub fn plan(state: &Value, objects: &[u64], order: Order) -> Result<Vec<Command>, Unavailable> {
    if state["observing"] == true {
        return Err(Unavailable::ReadOnly);
    }
    if objects.is_empty() {
        return Err(Unavailable::Empty);
    }
    let layers = state["project"]["layers"]
        .as_array()
        .ok_or(Unavailable::InvalidSnapshot)?;
    let selected: std::collections::HashSet<_> = objects.iter().copied().collect();
    for id in &selected {
        let layer = layers
            .iter()
            .find(|layer| layer["id"].as_u64() == Some(*id))
            .ok_or(Unavailable::Stale)?;
        if layer["locked"] == true {
            return Err(Unavailable::Locked);
        }
    }
    let original = layers
        .iter()
        .map(|layer| layer["id"].as_u64().ok_or(Unavailable::InvalidSnapshot))
        .collect::<Result<Vec<_>, _>>()?;
    let mut desired = original.clone();
    match order {
        Order::Up => {
            for index in (0..desired.len().saturating_sub(1)).rev() {
                if selected.contains(&desired[index]) && !selected.contains(&desired[index + 1]) {
                    desired.swap(index, index + 1);
                }
            }
        }
        Order::Down => {
            for index in 1..desired.len() {
                if selected.contains(&desired[index]) && !selected.contains(&desired[index - 1]) {
                    desired.swap(index, index - 1);
                }
            }
        }
        Order::Top | Order::Bottom => {
            desired.sort_by_key(|id| selected.contains(id) == (order == Order::Top))
        }
    }
    let mut moving: Vec<_> = original
        .iter()
        .copied()
        .filter(|id| selected.contains(id))
        .collect();
    if matches!(order, Order::Up | Order::Top) {
        moving.reverse();
    }
    let mut working = original;
    let mut commands = vec![];
    for object in moving {
        let from = working
            .iter()
            .position(|id| *id == object)
            .ok_or(Unavailable::InvalidSnapshot)?;
        let index = desired
            .iter()
            .position(|id| *id == object)
            .ok_or(Unavailable::InvalidSnapshot)?;
        if from != index {
            working.remove(from);
            working.insert(index, object);
            commands.push(Command::Reorder { object, index });
        }
    }
    Ok(commands)
}

pub(super) fn menu_entries(panels: &Panels, state: &Value) -> Vec<(String, Action, bool)> {
    let objects = panels.selection_ids();
    [Order::Up, Order::Down, Order::Top, Order::Bottom]
        .into_iter()
        .map(|order| {
            let enabled = plan(state, &objects, order).is_ok_and(|commands| !commands.is_empty());
            (
                panels.shortcut_bindings.label(
                    order.label(panels.locale),
                    crate::shortcuts::Scope::Animation,
                    order.shortcut(),
                ),
                Action::OrderLayers {
                    objects: objects.clone(),
                    order,
                },
                enabled,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests;
