//! Clipboard routing projects existing editor selections; it owns no selection.
use super::{animation_controls::TrackSelection, Action, Panels};
use crate::layer_clipboard::controller::Intent;
use motion_core::{KeyframeSelection, KeyframeTarget};
use serde_json::Value;

#[derive(Clone, Debug)]
pub(crate) struct Selection {
    scope: crate::layer_clipboard::KeyframeSource,
    pub selections: Vec<KeyframeSelection>,
    pub local_selections: Vec<motion_core::LocalKeyframeSelection>,
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
    pub(in crate::panels) fn mapped(
        state: &Value,
        targets: Vec<KeyframeTarget>,
        selected: Option<u32>,
        pose: bool,
        writable: bool,
    ) -> Option<Self> {
        let mut local_selections = vec![];
        for target in &targets {
            let layer = layer(state, target.object())?;
            let Some((track, _)) = super::key_selection::authored_track(state, target) else {
                if matches!(target, KeyframeTarget::Audio { .. }) && layer["audio_levels"].is_null()
                {
                    local_selections.push(motion_core::LocalKeyframeSelection {
                        target: target.clone(),
                        frames: vec![],
                    });
                    continue;
                }
                return Some(Self::new(state, vec![], vec![], false));
            };
            if track["spatial_animation"].is_object() {
                return Some(Self::new(state, vec![], vec![], false));
            }
            let frames = (|| -> motion_core::Result<Vec<i32>> {
                let clock = super::key_selection::property_clock(state, target.object())?;
                let offset = i32::try_from(offset(layer)).map_err(|_| {
                    motion_core::Error::Invalid("clipboard offset out of range".into())
                })?;
                let keys = track["keys"].as_array().ok_or_else(|| {
                    motion_core::Error::Invalid("clipboard authored key array missing".into())
                })?;
                if keys.len() > motion_core::MAX_FRAMES as usize {
                    return Err(motion_core::Error::Invalid(
                        "clipboard key count exceeds limit".into(),
                    ));
                }
                let mut all = vec![];
                let mut visible = vec![];
                for key in keys {
                    let frame = key["frame"]
                        .as_i64()
                        .and_then(|f| i32::try_from(f).ok())
                        .ok_or_else(|| {
                            motion_core::Error::Invalid(
                                "clipboard authored key frame invalid".into(),
                            )
                        })?;
                    if all.last().is_some_and(|previous| *previous >= frame) {
                        return Err(motion_core::Error::Invalid(
                            "clipboard authored keys not strictly sorted".into(),
                        ));
                    }
                    all.push(frame);
                    let composition =
                        clock.projected_composition_frame(f64::from(frame), offset)?;
                    if composition >= 0.
                        && composition <= super::last_frame(state)
                        && selected.is_none_or(|selected| composition == f64::from(selected))
                    {
                        visible.push(frame);
                    }
                }
                Ok(visible)
            })();
            let frames = observe!(
                "clipboard.mapped_keys",
                frames,
                "object={} phase=select fallback=disable_property_clipboard",
                target.object()
            )
            .ok()?;
            local_selections.push(motion_core::LocalKeyframeSelection {
                target: target.clone(),
                frames,
            });
        }
        if pose {
            let union = local_selections
                .iter()
                .flat_map(|s| s.frames.iter().copied())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            for selection in &mut local_selections {
                selection.frames = union.clone();
            }
        }
        local_selections.retain(|s| !s.frames.is_empty());
        Some(Self {
            scope: scope(state),
            selections: vec![],
            local_selections,
            targets,
            writable,
        })
    }
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
            local_selections: vec![],
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
            local_selections: self.local_selections.clone(),
            targets: self.targets.clone(),
        }
    }
    pub(super) fn enabled(&self, state: &Value, intent: Intent, available: bool) -> bool {
        self.current(state)
            && match intent {
                Intent::Copy => !self.selections.is_empty() || !self.local_selections.is_empty(),
                Intent::Cut => self.writable && (!self.selections.is_empty() || !self.local_selections.is_empty()),
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
        // Stable spatial IDs still require a separate transfer representation.
        // Keep ownership and disable the entire mixed set; never copy a subset
        // or reinterpret the request as copying whole layers.
        if panels.selected_keys.has_spatial() {
            return Some(Selection::new(state, vec![], vec![], false));
        }
        let selections = panels.selected_keys.selections();
        let local=panels.selected_keys.local_selections().into_iter().map(|(target,frames)|motion_core::LocalKeyframeSelection {target,frames}).collect::<Vec<_>>();
        let status = panels.selected_keys.status(state);
        let mut selected = Selection::new(state,
            selections.iter().map(|s| s.target.clone()).collect(),
            selections.iter().map(|s| s.frames.clone()).collect(),
            panels.selected_keys.writable(state));
        // Even an empty or invalid owned set returns Some: clipboard shortcuts
        // must never reinterpret it as a request to copy/cut whole layers.
        selected.targets.extend(local.iter().map(|s|s.target.clone()));
        selected.local_selections=local;
        if status == super::key_selection::Status::Stale { selected.selections.clear();selected.local_selections.clear(); }
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
        if !layer["time_mapping"].is_null() {
            let selected=panels.effect_timeline.clipboard_frame(state,owner,effect,param)
                .filter(|_|matches!(panels.focused,Some(motion_ui::dock::Panel::Timeline|motion_ui::dock::Panel::Curves)));
            return Selection::mapped(state,vec![KeyframeTarget::Effect {object:owner,effect,param:param.into()}],selected,false,layer["locked"]!=true);
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
    if layer(state,track.object).is_some_and(|l|!l["time_mapping"].is_null()) {
        return Selection::mapped(state,vec![KeyframeTarget::Property {object:track.object,property:track.property,axis:track.axis}],
            panels.animation_controls.user_key().map(|k|k.frame),false,
            layer(state,track.object).is_some_and(|l|l["locked"]!=true));
    }
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
