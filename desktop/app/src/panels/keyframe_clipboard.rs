//! Clipboard routing projects existing editor selections; it owns no selection.
use super::{animation_controls::TrackSelection, Action, Panels};
use crate::layer_clipboard::controller::Intent;
use motion_core::{KeyframeSelection, KeyframeTarget};
use serde_json::Value;

#[derive(Clone, Debug)]
pub(crate) struct Selection {
    scope: crate::layer_clipboard::KeyframeSource,
    pub selections: Vec<KeyframeSelection>,
    pub targets: Vec<KeyframeTarget>,
    pub writable: bool,
}
fn scope(state: &Value) -> crate::layer_clipboard::KeyframeSource {
    crate::layer_clipboard::KeyframeSource::of(state)
}
pub(super) fn frames(state: &Value, keys: impl Iterator<Item = i64>, offset: i64) -> Vec<u32> {
    let last = super::last_frame(state);
    let mut frames: Vec<_> = keys
        .filter_map(|f| f.checked_add(offset))
        .filter(|f| *f >= 0 && (*f as f64) <= last)
        .filter_map(|f| u32::try_from(f).ok())
        .collect();
    frames.sort_unstable();
    frames.dedup();
    frames
}
pub(super) fn json_frames(
    state: &Value,
    track: &Value,
    offset: i64,
    object: u64,
) -> Option<Vec<u32>> {
    // The shared panel observer counts malformed snapshots per site for its process
    // lifetime; healthy neighbouring rows do not reset that bounded evidence.
    let keys = track["keys"]
        .as_array()
        .ok_or("key array missing")
        .and_then(|keys| {
            keys.iter()
                .map(|key| key["frame"].as_i64().ok_or("key frame is not an integer"))
                .collect::<Result<Vec<_>, _>>()
        });
    let keys = observe!(
        "clipboard.key_frames",
        keys,
        "phase=select object={object} fallback=disable_property_clipboard"
    )
    .ok()?;
    Some(frames(state, keys.into_iter(), offset))
}
pub(super) fn layer(state: &Value, object: u64) -> Option<&Value> {
    state["project"]["layers"]
        .as_array()?
        .iter()
        .find(|l| l["id"] == object)
}
pub(super) fn offset(layer: &Value) -> i64 {
    layer["timeline"]["offset_frame"].as_i64().unwrap_or(0)
}
impl Selection {
    pub(super) fn new(
        state: &Value,
        targets: Vec<KeyframeTarget>,
        frames: Vec<Vec<u32>>,
        writable: bool,
    ) -> Self {
        let selections = targets
            .iter()
            .cloned()
            .zip(frames)
            .filter(|(_, frames)| !frames.is_empty())
            .map(|(target, frames)| KeyframeSelection { target, frames })
            .collect();
        Self {
            scope: scope(state),
            selections,
            targets,
            writable,
        }
    }
    pub(super) fn single(
        state: &Value,
        target: KeyframeTarget,
        frames: Vec<u32>,
        writable: bool,
    ) -> Self {
        Self::new(state, vec![target], vec![frames], writable)
    }
    pub(super) fn current(&self, state: &Value) -> bool {
        self.scope == scope(state)
    }
    pub(super) fn action(&self, intent: Intent) -> Action {
        Action::KeyframeClipboard {
            intent,
            source: self.scope.clone(),
            selections: self.selections.clone(),
            targets: self.targets.clone(),
        }
    }
    pub(super) fn enabled(&self, state: &Value, intent: Intent, available: bool) -> bool {
        self.current(state)
            && match intent {
                Intent::Copy => !self.selections.is_empty(),
                Intent::Cut => self.writable && !self.selections.is_empty(),
                Intent::Paste { .. } => self.writable && available && !self.targets.is_empty(),
            }
    }
}

/// Unselected returns None; an explicit but invalid property still prevents
/// layer fallback through TrackSelection::Invalid in the caller.
pub(super) fn selection(panels: &Panels, state: &Value) -> Option<Selection> {
    if !super::property_focus::navigation_scope(panels.focused) {
        return None;
    }
    if panels.selected_keys.owns_keys() {
        // The integer clipboard cannot represent local addresses or spatial IDs. Keep
        // ownership and disable the whole mixed set, never copy only a subset
        // or fall through to copying layers.
        if panels.selected_keys.has_spatial() || panels.selected_keys.has_local() {
            return Some(Selection::new(state, vec![], vec![], false));
        }
        let selections = panels.selected_keys.selections();
        let status = panels.selected_keys.status(state);
        let mut selected = Selection::new(state,
            selections.iter().map(|s| s.target.clone()).collect(),
            selections.iter().map(|s| s.frames.clone()).collect(),
            panels.selected_keys.writable(state));
        // Even an empty or invalid owned set returns Some: clipboard shortcuts
        // must never reinterpret it as a request to copy/cut whole layers.
        if status == super::key_selection::Status::Stale { selected.selections.clear(); }
        return Some(selected);
    }
    let object = panels.selected?;
    if let Some(keys) = panels.path_controls.clipboard_selection(state, object) {
        return Some(keys);
    }
    if let Some(keys) = panels.audio_controls.clipboard_selection(state, object) {
        return Some(keys);
    }
    let controls = if panels.effect_controls.has_key_channel() {
        &panels.effect_controls
    } else {
        &panels.properties.effects
    };
    if let Some((owner, effect, param)) = controls
        .selected_parameter(state)
        .filter(|(owner, _, _)| *owner == object)
    {
        let layer = layer(state, owner)?;
        let parameter = layer["effects"]
            .as_array()?
            .iter()
            .find(|e| e["id"] == effect)?["params"]
            .get(param)?;
        if parameter["animatable"] != true || parameter["implemented"] != true {
            return None;
        }
        let track = if parameter["curve"].is_object() {
            &parameter["curve"]
        } else {
            &parameter["track"]
        };
        let mut frames = json_frames(state, track, offset(layer), owner)?;
        if let Some(frame) = panels
            .effect_timeline
            .clipboard_frame(state, owner, effect, param)
            .filter(|_| {
                matches!(
                    panels.focused,
                    Some(motion_ui::dock::Panel::Timeline | motion_ui::dock::Panel::Curves)
                )
            })
        {
            frames.retain(|f| *f == frame);
        }
        return Some(Selection::single(
            state,
            KeyframeTarget::Effect {
                object: owner,
                effect,
                param: param.into(),
            },
            frames,
            layer["locked"] != true,
        ));
    }
    let TrackSelection::Current(track) = panels
        .animation_controls
        .track_selection(state, panels.selected)
    else {
        return None;
    };
    let source = super::animation_data::channel(state, track.object, track.property, track.axis)?;
    let mut frames = json_frames(state, source, 0, track.object)?;
    if let Some(key) = panels.animation_controls.user_key() {
        frames.retain(|f| *f == key.frame);
    }
    let writable = if track.object == 0 {
        true
    } else {
        layer(state, track.object).is_some_and(|l| l["locked"] != true)
    };
    Some(Selection::single(
        state,
        KeyframeTarget::Property {
            object: track.object,
            property: track.property,
            axis: track.axis,
        },
        frames,
        writable,
    ))
}

#[cfg(test)]
mod tests;
