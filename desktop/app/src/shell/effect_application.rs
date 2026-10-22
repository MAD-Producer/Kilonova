//! Successful effect additions share feedback across search, library and menus.
use crate::*;
#[cfg(test)]
mod tests;

impl Shell {
    pub(super) fn reveal_effect_application(&mut self, request: &Value, next: &Value) {
        let mut added = Vec::new();
        for layer in next["project"]["layers"].as_array().into_iter().flatten() {
            let Some(object) = layer["id"].as_u64() else {
                continue;
            };
            if request["object"] != object
                && !request["objects"]
                    .as_array()
                    .is_some_and(|ids| ids.iter().any(|id| *id == object))
            {
                continue;
            }
            let previous = self.state["project"]["layers"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|old| old["id"] == object);
            // An atomic add appends exactly one effect to each requested layer.
            // A stale UI snapshot can also contain unrelated edits from MCP;
            // do not attribute those earlier additions to this action.
            if let Some(effect) = layer["effects"]
                .as_array()
                .and_then(|effects| effects.last())
            {
                let Some(id) = effect["id"].as_u64() else {
                    continue;
                };
                if ["plugin", "version", "hash", "effect"]
                    .iter()
                    .any(|key| effect[*key] != request[*key])
                {
                    continue;
                }
                if previous.is_some_and(|old| {
                    old["effects"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|old_effect| old_effect["id"] == id)
                }) {
                    continue;
                }
                added.push((object, id, layer["name"].as_str().unwrap_or("")));
            }
        }
        let Some(&(object, effect, target)) = added
            .iter()
            .find(|(object, _, _)| self.panels.selected == Some(*object))
            .or_else(|| added.first())
        else {
            return;
        };
        let name = panels::effect_controls::entries(
            &self.panels.effect_controls.catalogue,
            self.workspace.locale,
        )
        .into_iter()
        .find(|entry| request["plugin"] == entry.plugin && request["effect"] == entry.id)
        .map(|entry| entry.name)
        .unwrap_or_else(|| request["effect"].as_str().unwrap_or("").into());
        let targets = if added.len() > 1 {
            if self.workspace.locale == Locale::Zh {
                format!("{target} 等 {} 个图层", added.len())
            } else {
                format!("{target} ({} layers)", added.len())
            }
        } else {
            target.to_owned()
        };
        self.panels.message = Some(if self.workspace.locale == Locale::Zh {
            format!("已添加「{name}」到「{targets}」")
        } else {
            format!("Added {name} to {targets}")
        });
        if !self.panels.selection_contains(object) {
            self.panels.select_only(Some(object));
        }
        self.panels.selected = Some(object);
        self.panels.effect_controls.reveal_effect(object, effect);
        let panel = dock::Panel::EffectControls.dock();
        // Keep an open search's owning native window focused. Showing a dock tab
        // is a view operation and must never interrupt continuous search input.
        if self.panels.effect_search.is_some() {
            if let Some(dock::Node::Dock { id, .. }) = self.workspace.root.find(panel) {
                let id = *id;
                self.workspace.root.activate(id, panel);
            } else if !self
                .floating
                .values()
                .any(|window| window.panel.dock() == panel)
            {
                self.workspace.redock(panel);
            }
        } else {
            self.actions.push_back(panels::Action::ShowPanel(panel));
        }
    }
}
