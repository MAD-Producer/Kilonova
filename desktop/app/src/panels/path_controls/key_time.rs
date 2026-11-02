//! Navigation keeps authored identities separate from their display positions.
use super::*;
use crate::panels::key_selection::{AddressedKey, LocalKey, SpatialKey};
use crate::panels::Panels;
use motion_core::{Error, KeyframeTarget, LocalKeyEdit, SpatialKeyEdit, TrackEdit, Tween};

pub(super) struct Timing {
    pub entries: Vec<(f64, AddressedKey)>,
    pub targets: Vec<KeyframeTarget>,
    pub at: f64,
    pub editable: bool,
    integer: Option<i32>,
}
fn append<T: Tween>(
    target: KeyframeTarget,
    track: &Track<T>,
    entries: &mut Vec<(f64, AddressedKey)>,
) {
    entries.extend(track.keys.iter().map(|key| {
        (
            f64::from(key.frame),
            LocalKey {
                target: target.clone(),
                frame: key.frame,
            }
            .into(),
        )
    }));
    if let Some(animation) = &track.spatial_animation {
        entries.extend(animation.prepared().keys().map(|(id, time)| {
            (
                time,
                SpatialKey {
                    target: target.clone(),
                    id,
                }
                .into(),
            )
        }));
    }
}
fn timing(
    state: &Value,
    object: u64,
    targets: Vec<KeyframeTarget>,
    mut entries: Vec<(f64, AddressedKey)>,
) -> motion_core::Result<Timing> {
    let (clock, offset) = crate::panels::property_authoring::clock(state, object)?;
    let at = state["frame"]
        .as_f64()
        .filter(|f| f.is_finite())
        .ok_or_else(|| Error::Invalid("invalid path navigation playhead".into()))?;
    clock.key_edit_time(at, offset)?;
    for (time, _) in &mut entries {
        *time = clock.projected_composition_frame(*time, offset)?;
    }
    let integer = clock.integer_key_frame(at, offset).ok();
    let mut editable = !targets.is_empty()
        && crate::panels::keyframe_clipboard::layer(state, object)
            .is_some_and(|l| l["locked"] != true)
        && (0. ..=crate::panels::last_frame(state)).contains(&at);
    let current: Vec<_> = entries.iter().filter(|(time, _)| same(*time, at)).collect();
    for target in &targets {
        let (raw, _) = crate::panels::key_selection::authored_track(state, target)
            .ok_or_else(|| Error::Invalid("path navigation authored track missing".into()))?;
        if raw["spatial_animation"].is_object() {
            let count = raw["spatial_animation"]["keys"]
                .as_array()
                .ok_or_else(|| Error::Invalid("path navigation spatial keys missing".into()))?
                .len();
            let removed = current
                .iter()
                .filter(|(_, key)| key.target() == target)
                .count();
            editable &= count.saturating_sub(removed) >= 2;
        } else {
            editable &= integer.is_some();
        }
    }
    Ok(Timing {
        entries,
        targets,
        at,
        editable,
        integer,
    })
}
fn same(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6
}
impl Timing {
    fn existing(&self, time: f64) -> Vec<AddressedKey> {
        self.entries
            .iter()
            .filter(|(at, _)| same(*at, time))
            .map(|(_, key)| key.clone())
            .collect()
    }
    pub fn selected(&self, time: f64) -> Vec<AddressedKey> {
        let keys = self.existing(time);
        // A path-row selection denotes every vertex at one pose. Retain
        // missing authored identities too, so the shared selection disables
        // copying a partial pose instead of silently transferring a subset.
        if self.targets.len() > 1 {
            if let Some(AddressedKey::Local(first)) = keys.first() {
                if keys
                    .iter()
                    .all(|key| matches!(key, AddressedKey::Local(key) if key.frame == first.frame))
                {
                    return self
                        .targets
                        .iter()
                        .map(|target| {
                            LocalKey {
                                target: target.clone(),
                                frame: first.frame,
                            }
                            .into()
                        })
                        .collect();
                }
            }
        }
        keys
    }
    /// A whole-path pose removes only nodes that actually have a key. Toggling
    /// each node independently would add keys to the previously unkeyed nodes.
    pub fn toggle(&self) -> (Vec<Command>, Vec<AddressedKey>) {
        let current = self.existing(self.at);
        if !current.is_empty() {
            let commands = current
                .into_iter()
                .map(|key| match key {
                    AddressedKey::Local(key) => Command::LocalKeyframes {
                        target: key.target,
                        edit: LocalKeyEdit::Delete {
                            frames: vec![key.frame],
                        },
                    },
                    AddressedKey::Spatial(key) => Command::SpatialKeyframes {
                        target: key.target,
                        edit: SpatialKeyEdit::Delete { key: key.id },
                    },
                    AddressedKey::Frame(_) => {
                        unreachable!("path timing only contains authored identities")
                    }
                })
                .collect();
            (commands, vec![])
        } else {
            let commands = self
                .targets
                .iter()
                .map(|target| Command::TrackAt {
                    target: target.clone(),
                    frame: self.at,
                    edit: TrackEdit::ToggleKey,
                })
                .collect();
            // New spatial IDs belong to the worker. Do not guess a future ID.
            let keys = self
                .integer
                .into_iter()
                .flat_map(|frame| {
                    self.targets.iter().map(move |target| {
                        LocalKey {
                            target: target.clone(),
                            frame,
                        }
                        .into()
                    })
                })
                .filter(|key: &AddressedKey| {
                    !self.entries.iter().any(|(_, old)| {
                        matches!(old, AddressedKey::Spatial(_)) && old.target() == key.target()
                    })
                })
                .collect();
            (commands, keys)
        }
    }
}
pub(super) fn mask(
    state: &Value,
    object: u64,
    mask: &LayerMask,
    index: u8,
) -> motion_core::Result<Timing> {
    let mut entries = vec![];
    let target = |property| KeyframeTarget::Mask {
        object,
        mask: mask.id,
        property,
    };
    let targets = match index {
        mask_path::CHANNEL => mask
            .path
            .nodes
            .iter()
            .map(|node| {
                let target = target(MaskProperty::Node(node.id));
                append(target.clone(), &node.geometry, &mut entries);
                target
            })
            .collect(),
        0 => {
            let t = target(MaskProperty::Opacity);
            append(t.clone(), &mask.opacity, &mut entries);
            vec![t]
        }
        1 => {
            let t = target(MaskProperty::Feather);
            append(t.clone(), &mask.feather, &mut entries);
            vec![t]
        }
        _ => {
            let t = target(MaskProperty::Expansion);
            append(t.clone(), &mask.expansion, &mut entries);
            vec![t]
        }
    };
    timing(state, object, targets, entries)
}
pub(super) fn vector(
    state: &Value,
    object: u64,
    item: u64,
    p: &groups::parameters::Parameter,
) -> motion_core::Result<Timing> {
    let target = KeyframeTarget::Vector {
        object,
        item: Some(item),
        parameter: p.id.clone(),
    };
    let entries = p
        .keys
        .iter()
        .map(|(frame, _)| {
            (
                f64::from(*frame),
                LocalKey {
                    target: target.clone(),
                    frame: *frame,
                }
                .into(),
            )
        })
        .collect();
    timing(state, object, vec![target], entries)
}
pub(in crate::panels) fn apply_selection(panels: &mut Panels, state: &Value) {
    let pending = panels
        .path_controls
        .keys
        .navigation_selection
        .take()
        .or_else(|| panels.path_controls.groups.keys.navigation_selection.take());
    if let Some(keys) = pending {
        let _ = observe!(
            "path.navigation_selection",
            panels.selected_keys.apply_addressed(
                state,
                keys,
                crate::panels::key_selection::Mode::Replace
            ),
            "phase=navigation_selection object={:?}",
            panels.selected
        );
    }
}
