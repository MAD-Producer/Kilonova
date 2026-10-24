//! Recent menu metadata is cached; target scope and commands remain current.
use super::*;
use std::sync::Arc;

#[derive(Default)]
pub(crate) struct Cache {
    catalogue: Option<Arc<Value>>,
    locale: Option<Locale>,
    keys: Vec<String>,
    entries: Vec<effect_controls::Entry>,
}

impl Cache {
    fn refresh(&mut self, catalogue: &Arc<Value>, locale: Locale, keys: &[String]) {
        // Match the existing library's recent-history count, not a UI dimension.
        let keys = &keys[..keys.len().min(24)];
        if keys.is_empty() {
            self.catalogue = None;
            self.locale = None;
            self.keys.clear();
            self.entries.clear();
            return;
        }
        if self.locale == Some(locale)
            && self.keys == keys
            && self
                .catalogue
                .as_ref()
                .is_some_and(|old| Arc::ptr_eq(old, catalogue))
        {
            return;
        }
        let available = effect_controls::entries(catalogue, locale);
        self.entries = keys
            .iter()
            .filter_map(|key| available.iter().find(|entry| &entry.key == key).cloned())
            .collect();
        self.keys = keys.to_vec();
        self.locale = Some(locale);
        self.catalogue = Some(Arc::clone(catalogue));
    }
}

fn refresh(panels: &mut Panels) {
    panels.recent_effect_menu.refresh(
        &panels.effect_controls.catalogue,
        panels.locale,
        panels.browser.recent_keys(),
    );
}

fn targets<'a>(ids: &[u64], state: &'a Value) -> Option<Vec<&'a Value>> {
    if ids.is_empty()
        || state["observing"] == true
        || state["composition"].as_str().is_none_or(str::is_empty)
    {
        return None;
    }
    let layers = state["project"]["layers"].as_array()?;
    let mut targets = std::collections::HashMap::with_capacity(ids.len());
    for id in ids {
        if *id == 0 || targets.insert(*id, None).is_some() {
            return None;
        }
    }
    // Retain only selected targets and their first occurrence. Output still
    // follows selection order, without scanning every layer for every target.
    for layer in layers {
        if let Some(target) = layer["id"].as_u64().and_then(|id| targets.get_mut(&id)) {
            target.get_or_insert(layer);
        }
    }
    ids.iter()
        .map(|id| targets.get(id).copied().flatten())
        .collect()
}

pub(super) fn rows(panels: &mut Panels, state: &Value) -> Vec<(String, usize, bool)> {
    refresh(panels);
    if panels.recent_effect_menu.entries.is_empty() {
        return Vec::new();
    }
    let ids = panels.selection_ids();
    let targets = targets(&ids, state);
    panels
        .recent_effect_menu
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let enabled = targets
                .as_ref()
                .is_some_and(|layers| layers.iter().all(|layer| entry.can_apply(layer)));
            let label = if panels.locale == Locale::Zh {
                format!("最近 · {} 个图层：{}", ids.len(), entry.name)
            } else {
                format!(
                    "Recent · {} {}: {}",
                    ids.len(),
                    if ids.len() == 1 { "layer" } else { "layers" },
                    entry.name
                )
            };
            (label, index, enabled)
        })
        .collect()
}

pub(super) fn apply(index: usize, panels: &mut Panels, state: &Value) -> Option<Action> {
    refresh(panels);
    let ids = panels.selection_ids();
    let targets = targets(&ids, state)?;
    let entry = panels.recent_effect_menu.entries.get(index)?;
    if !targets.iter().all(|layer| entry.can_apply(layer)) {
        return None;
    }
    // The existing plugin protocol applies this whole scope atomically. Building
    // it only on activation avoids copying every target into every menu row.
    let request = serde_json::json!({"op":"add","objects":ids,
        "plugin":entry.plugin,"version":entry.version,"hash":entry.hash,
        "effect":entry.id,"composition":state["composition"]});
    let key = entry.key.clone();
    panels.browser.record_recent(key);
    Some(Action::Plugin(request))
}
