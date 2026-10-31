//! Revision-scoped, model-validated plans; no geometry preparation on idle menus.
use super::super::key_selection::{self, SpatialKey};
use super::*;
use motion_core::{KeyframeTarget, SpatialKeyEdit, Track, Tween};
use serde::de::DeserializeOwned;
use std::sync::Arc;

#[derive(Default)]
pub(super) struct Cache {
    entry: Option<(MenuScope, Arc<Plans>)>,
    pending: Option<Pending>,
    #[cfg(test)]
    solves: usize,
}

struct Pending {
    before: key_selection::Set,
    revision: u64,
    plan: Arc<Plans>,
}

#[derive(Default)]
pub(super) struct Plans {
    count: usize,
    spatial: usize,
    roving: usize,
    enable: Option<Vec<Command>>,
    freeze: Option<Vec<Command>>,
    // Expected authoritative channels and identities after initial conversion.
    // Together bounded by the existing typed metadata budget.
    converted: Vec<SpatialKey>,
    expected: Vec<(KeyframeTarget, Value)>,
}
impl Plans {
    pub(super) fn commands(&self, enabled: bool) -> Option<&[Command]> {
        if enabled {
            self.enable.as_deref()
        } else {
            self.freeze.as_deref()
        }
    }
    pub(super) fn all_roving(&self) -> bool {
        self.count > 0 && self.roving == self.count
    }
    pub(super) fn partial(&self) -> bool {
        self.roving > 0 && self.roving < self.count
    }
    pub(super) fn title(&self, locale: Locale) -> Option<String> {
        if self.spatial == 0 {
            return None;
        }
        let zh = locale == Locale::Zh;
        Some(if self.all_roving() {
            if zh {
                "空间时间约束：Roving"
            } else {
                "Spatial timing: Roving"
            }
            .into()
        } else if self.spatial == self.count && self.roving == 0 {
            if zh {
                "空间时间约束：固定"
            } else {
                "Spatial timing: fixed"
            }
            .into()
        } else if zh {
            format!(
                "时间约束：混合（{} / {} 个 Roving）",
                self.roving, self.count
            )
        } else {
            format!(
                "Mixed timing ({} / {} Roving keys)",
                self.roving, self.count
            )
        })
    }
}

pub(super) fn writable(panels: &Panels, state: &Value) -> bool {
    state["root"].is_string()
        && state["document_generation"].is_u64()
        && state["revision"].is_u64()
        && state["composition"]
            .as_str()
            .or_else(|| state["project"]["composition_id"].as_str())
            .is_some()
        && if panels.selected_keys.owns_keys() {
            panels.selected_keys.writable(state)
        } else {
            super::interpolation_selection(panels, state).is_some()
        }
}

pub(super) fn plans(panels: &Panels, state: &Value) -> Arc<Plans> {
    let scope = MenuScope::of(panels, state);
    let versioned = state["revision"].is_u64()
        && state["document_generation"].is_u64()
        && state["root"].is_string()
        && state["composition"]
            .as_str()
            .or_else(|| state["project"]["composition_id"].as_str())
            .is_some();
    if versioned {
        if let Some((old, plan)) = &panels.animation_type_cache.borrow().roving.entry {
            if old == &scope {
                return plan.clone();
            }
        }
    }
    // Build outside the RefCell borrow: fault observations use the logger.
    let prepared = build(panels, state, &scope);
    let healthy = prepared.is_some();
    let plan = Arc::new(prepared.unwrap_or_default());
    let mut cache = panels.animation_type_cache.borrow_mut();
    #[cfg(test)]
    {
        cache.roving.solves += 1;
    }
    cache.roving.entry = (versioned && healthy).then(|| (scope, plan.clone()));
    plan
}

#[derive(Default)]
struct Channel {
    count: usize,
    spatial: usize,
    roving: usize,
    enable: Option<(SpatialKeyEdit, Value)>,
    freeze: Option<SpatialKeyEdit>,
    converted: Vec<SpatialKey>,
}

fn channel<T: Tween + DeserializeOwned + serde::Serialize>(
    state: &Value,
    target: &KeyframeTarget,
    source: &Value,
    offset: i64,
    frames: &[u32],
    ids: &[u64],
) -> Option<Channel> {
    let original: Track<T> = observe!(
        "animation.roving.track",
        serde_json::from_value(source.clone()),
        "object={} phase=roving_capability",
        target.object()
    )
    .ok()?;
    observe!(
        "animation.roving.validity",
        original.validate_local(),
        "object={} phase=roving_capability",
        target.object()
    )
    .ok()?;
    let mut result = Channel {
        count: frames.len() + ids.len(),
        spatial: ids.len(),
        ..Default::default()
    };
    let valid = |track: &Track<T>| {
        let range = if target.object() == 0 {
            state["project"]["frames"]
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .is_some_and(|frames| track.validate(frames).is_ok())
        } else {
            track.validate_local().is_ok()
        };
        let bound = if matches!(
            target,
            KeyframeTarget::Property {
                property: Property::Anchor,
                ..
            }
        ) {
            1000.
        } else {
            10_000_000.
        };
        range && track.validate_bound(bound).is_ok()
    };
    let encode = |track: &Track<T>| {
        observe!(
            "animation.roving.candidate",
            serde_json::to_value(track),
            "object={} phase=roving_candidate",
            target.object()
        )
        .ok()
    };
    if !frames.is_empty() {
        if !ids.is_empty() || original.spatial_animation.is_some() {
            return Some(result);
        }
        let locals = frames
            .iter()
            .map(|frame| {
                (u64::from(*frame) < state["project"]["frames"].as_u64()?)
                    .then(|| i32::try_from(i64::from(*frame) - offset).ok())
                    .flatten()
            })
            .collect::<Option<Vec<_>>>();
        let Some(locals) = locals else {
            return Some(result);
        };
        let mut candidate = original;
        if candidate.enable_roving(&locals).is_ok() && valid(&candidate) {
            let animation = candidate.spatial_animation.as_ref()?;
            for local in locals {
                let key = animation
                    .keys()
                    .iter()
                    .find(|key| key.reference == f64::from(local))?;
                result.converted.push(SpatialKey {
                    target: target.clone(),
                    id: key.id,
                });
            }
            result.enable = Some((
                SpatialKeyEdit::Enable {
                    frames: frames.to_vec(),
                },
                encode(&candidate)?,
            ));
        }
        return Some(result);
    }
    let Some(animation) = original.spatial_animation.as_ref() else {
        return Some(result);
    };
    for id in ids {
        let Some(key) = animation.keys().iter().find(|key| key.id == *id) else {
            return Some(result);
        };
        result.roving += usize::from(key.fixed.is_none());
    }
    for enabled in [true, false] {
        let mut candidate = original.clone();
        if candidate
            .spatial_animation
            .as_mut()?
            .set_roving(ids, enabled)
            .is_ok()
            && valid(&candidate)
        {
            let edit = SpatialKeyEdit::Roving {
                keys: ids.to_vec(),
                enabled,
            };
            if enabled {
                result.enable = Some((edit, encode(&candidate)?));
            } else {
                result.freeze = Some(edit);
            }
        }
    }
    result.converted = ids
        .iter()
        .map(|id| SpatialKey {
            target: target.clone(),
            id: *id,
        })
        .collect();
    Some(result)
}

fn build(panels: &Panels, state: &Value, scope: &MenuScope) -> Option<Plans> {
    if panels.selected_keys.owns_keys()
        && panels.selected_keys.status(state) != key_selection::Status::Current
    {
        return Some(Plans::default());
    }
    let mut channels = scope
        .keys
        .iter()
        .map(|selected| {
            (
                selected.target.clone(),
                selected.frames.clone(),
                Vec::<u64>::new(),
            )
        })
        .collect::<Vec<_>>();
    for (target, ids) in &scope.spatial_keys {
        if let Some((_, _, keys)) = channels.iter_mut().find(|(key, _, _)| key == target) {
            keys.extend(ids);
        } else {
            channels.push((target.clone(), vec![], ids.clone()));
        }
    }
    let mut result = Plans {
        enable: Some(vec![]),
        freeze: Some(vec![]),
        ..Default::default()
    };
    let mut bytes = 0usize;
    for (target, frames, ids) in channels {
        let supported = matches!(
            target,
            KeyframeTarget::Property {
                axis: None,
                property: Property::Position | Property::Anchor | Property::Target,
                ..
            }
        );
        let part = if supported {
            let Some((source, offset)) = key_selection::authored_track(state, &target) else {
                return Some(Plans::default());
            };
            if matches!(
                target,
                KeyframeTarget::Property {
                    property: Property::Anchor,
                    ..
                }
            ) {
                channel::<[f32; 2]>(state, &target, source, offset, &frames, &ids)?
            } else {
                channel::<[f32; 3]>(state, &target, source, offset, &frames, &ids)?
            }
        } else {
            Channel {
                count: frames.len() + ids.len(),
                ..Default::default()
            }
        };
        result.count += part.count;
        result.spatial += part.spatial;
        result.roving += part.roving;
        if let Some((edit, next)) = part.enable {
            bytes = bytes.saturating_add(motion_core::serialized_bytes(&next));
            if let Some(commands) = result.enable.as_mut() {
                commands.push(Command::SpatialKeyframes {
                    target: target.clone(),
                    edit,
                });
                result.expected.push((target.clone(), next));
            }
        } else {
            result.enable = None;
            result.expected.clear();
        }
        if let Some(edit) = part.freeze {
            if let Some(commands) = result.freeze.as_mut() {
                commands.push(Command::SpatialKeyframes {
                    target: target.clone(),
                    edit,
                });
            }
        } else {
            result.freeze = None;
        }
        result.converted.extend(part.converted);
    }
    bytes = bytes.saturating_add(motion_core::serialized_bytes(
        &result
            .converted
            .iter()
            .map(|key| (&key.target, key.id))
            .collect::<Vec<_>>(),
    ));
    if result.count == 0 || bytes > motion_core::MAX_KEYFRAME_CLIPBOARD_BYTES {
        result.enable = None;
        result.freeze = None;
        result.expected.clear();
        result.converted.clear();
    }
    if result.roving == result.count {
        result.enable = None;
    }
    if result.roving == 0 {
        result.freeze = None;
    }
    if result.enable.is_none() {
        result.expected.clear();
    }
    Some(result)
}

pub(super) fn apply(panels: &mut Panels, state: &Value, enabled: bool, out: &mut Vec<Action>) {
    let plan = plans(panels, state);
    let Some(commands) = plan.commands(enabled) else {
        return;
    };
    if enabled && plan.spatial < plan.count {
        let frames = MenuScope::of(panels, state)
            .keys
            .into_iter()
            .flat_map(|selected| {
                selected
                    .frames
                    .into_iter()
                    .map(move |frame| key_selection::Key {
                        target: selected.target.clone(),
                        frame,
                    })
            });
        // Claim the legacy property selection before the asynchronous commit.
        // Mixed spatial selections are already owned and must remain intact.
        if !panels.selected_keys.owns_keys()
            && !key_selection::choose(panels, state, frames, key_selection::Mode::Replace)
        {
            return;
        }
        panels.animation_type_cache.borrow_mut().roving.pending = Some(Pending {
            before: panels.selected_keys.clone(),
            revision: state["revision"].as_u64().unwrap_or(0),
            plan: plan.clone(),
        });
    }
    out.push(Action::EditBatch(commands.to_vec()));
}

pub(super) fn reconcile(panels: &mut Panels, state: &Value) {
    let pending = panels
        .animation_type_cache
        .borrow_mut()
        .roving
        .pending
        .take();
    let Some(pending) = pending else {
        return;
    };
    if !panels.selected_keys.same_selection(&pending.before)
        || !pending.before.same_document(state)
        || state["editorAction"]["cancelled"] == true
    {
        return;
    }
    let revision = state["revision"].as_u64();
    if revision == Some(pending.revision) {
        panels.animation_type_cache.borrow_mut().roving.pending = Some(pending);
        return;
    }
    if revision != Some(pending.revision.wrapping_add(1))
        || pending.plan.expected.iter().any(|(target, expected)| {
            key_selection::authored_track(state, target)
                .is_none_or(|(actual, _)| actual != expected)
        })
    {
        return;
    }
    if let Err(error) = panels.selected_keys.apply_addressed(
        state,
        pending.plan.converted.iter().cloned().map(Into::into),
        key_selection::Mode::Replace,
    ) {
        log::error!("Roving selection failed operation=convert_selection revision={revision:?} phase=commit cause={error}");
        panels.message = Some(format!(
            "{}: {error}",
            if panels.locale == Locale::Zh {
                "Roving 选区更新失败"
            } else {
                "Roving selection update failed"
            }
        ));
    }
}

#[cfg(test)]
mod tests;

#[cfg(all(test, windows, feature = "gpu-tests"))]
mod native_tests;

#[cfg(all(test, windows, feature = "gpu-tests", feature = "ffmpeg"))]
mod export_tests;
