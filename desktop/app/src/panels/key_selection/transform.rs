//! Gesture-local occupancy shared by nudging and selection dragging. Normal
//! collisions and bounds are rejected before the serial worker receives edits.
use super::*;
use crate::panels::{Action, Panels};

pub(super) struct Occupancy {
    selected: BTreeSet<u32>,
    authored: BTreeSet<i64>,
}
pub(super) struct Plan {
    pub selected: Set,
    pub channels: Vec<Occupancy>,
    pub total: u32,
    spatial: Vec<SpatialOccupancy>,
}
struct SpatialOccupancy {
    target: KeyframeTarget,
    ids: Vec<u64>,
    // Authored order, effective composition time and ownership. Prepared only
    // once per gesture; pointer movement never rebuilds spatial geometry.
    keys: Vec<(u64, f64, bool, f64)>,
}
impl Plan {
    pub fn capture(selected: &Set, state: &Value) -> motion_core::Result<Self> {
        if selected.has_local() {
            return Err(Error::Invalid("local key timing gestures require a layer-clock transform plan".into()));
        }
        let total = state["project"]["frames"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| Error::Invalid("key transform composition duration missing".into()))?;
        let mut channels = Vec::new();
        let mut bytes = 0usize;
        for (channel_index, selection) in selected.selections().into_iter().enumerate() {
            let (track, offset) = source::track(state, &selection.target)
                .ok_or_else(|| Error::Invalid("key transform authored channel missing".into()))?;
            let keys = track["keys"]
                .as_array()
                .ok_or_else(|| Error::Invalid("key transform authored key array missing".into()))?;
            bytes = bytes.saturating_add(keys.len().saturating_mul(std::mem::size_of::<i64>()));
            if bytes > MAX_KEYFRAME_CLIPBOARD_BYTES {
                return Err(Error::Invalid(
                    "key transform occupancy exceeds metadata limit".into(),
                ));
            }
            let authored = keys
                .iter().enumerate()
                .map(|(key_index,key)| {
                    key["frame"]
                        .as_i64()
                        .and_then(|f| f.checked_add(offset))
                        .ok_or_else(|| {
                            Error::Invalid(format!("key transform authored time is not an integer object={} channel_index={channel_index} key_index={key_index}",selection.target.object()))
                        })
                })
                .collect::<motion_core::Result<_>>()?;
            channels.push(Occupancy {
                selected: selection.frames.into_iter().collect(),
                authored,
            });
        }
        let mut spatial = Vec::new();
        for (target, ids) in selected.spatial_selections() {
            let KeyframeTarget::Property {
                object,
                property,
                axis: None,
            } = &target
            else {
                return Err(Error::Invalid(
                    "spatial selection requires a coupled property".into(),
                ));
            };
            let (authored, offset) = source::track(state, &target).ok_or_else(|| {
                Error::Invalid("spatial transform authored channel missing".into())
            })?;
            let author_keys = authored["spatial_animation"]["keys"]
                .as_array()
                .ok_or_else(|| Error::Invalid("spatial transform authored keys missing".into()))?;
            let view = super::super::animation_data::channel(state, *object, *property, None)
                .ok_or_else(|| {
                    Error::Invalid("spatial transform timeline channel missing".into())
                })?;
            let view_keys = view["spatial_keys"]
                .as_array()
                .ok_or_else(|| Error::Invalid("spatial transform solved keys missing".into()))?;
            bytes = bytes.saturating_add(author_keys.len().saturating_mul(std::mem::size_of::<(
                u64,
                f64,
                bool,
                f64,
            )>()));
            if bytes > MAX_KEYFRAME_CLIPBOARD_BYTES || author_keys.len() > 16_385 {
                return Err(Error::Invalid(
                    "spatial transform occupancy exceeds metadata limit".into(),
                ));
            }
            if author_keys.len() != view_keys.len() {
                return Err(Error::Invalid(
                    "spatial transform authored and solved key counts differ".into(),
                ));
            }
            let mut keys = Vec::with_capacity(author_keys.len());
            let mut seen = BTreeSet::new();
            for (author, view) in author_keys.iter().zip(view_keys) {
                let id = author["id"].as_u64().filter(|id| *id > 0).ok_or_else(|| {
                    Error::Invalid("spatial transform key identity missing".into())
                })?;
                let time = view["frame"]
                    .as_f64()
                    .filter(|t| t.is_finite())
                    .ok_or_else(|| {
                        Error::Invalid("spatial transform solved time missing".into())
                    })?;
                let fixed = author.get("fixed").ok_or_else(|| {
                    Error::Invalid("spatial transform time ownership missing".into())
                })?;
                if !seen.insert(id)
                    || view["id"].as_u64() != Some(id)
                    || !fixed.is_null() && fixed.as_f64().is_none_or(|t| t + offset as f64 != time)
                    || view["roving"].as_bool() != Some(fixed.is_null())
                {
                    return Err(Error::Invalid(
                        "spatial transform authored and solved identities or timing differ".into(),
                    ));
                }
                let incoming = view["incoming_length"]
                    .as_f64()
                    .filter(|v| v.is_finite() && *v >= 0.)
                    .ok_or_else(|| {
                        Error::Invalid("spatial transform incoming metric missing".into())
                    })?;
                keys.push((id, time, !fixed.is_null(), incoming));
            }
            if ids.iter().any(|id| !seen.contains(id)) {
                return Err(Error::Invalid(
                    "spatial transform selected identity missing".into(),
                ));
            }
            spatial.push(SpatialOccupancy { target, ids, keys });
        }
        Ok(Self {
            selected: selected.clone(),
            channels,
            total,
            spatial,
        })
    }
    pub fn allows(&self, delta: i32, copy: bool) -> bool {
        if copy && !self.spatial.is_empty() {
            return false;
        }
        self.spatial.iter().all(|c| {
            let mut previous = None;
            let mut moving = false;
            c.keys.iter().all(|(id, time, fixed, incoming)| {
                moving |= *incoming > 0.;
                let chosen = c.ids.binary_search(id).is_ok();
                let next = *time + if chosen { f64::from(delta) } else { 0. };
                if chosen && !(next >= 0. && next < f64::from(self.total)) {
                    return false;
                }
                if chosen || *fixed {
                    if previous.is_some_and(|p| p > next || p == next && moving) {
                        return false;
                    }
                    previous = Some(next);
                    moving = false;
                }
                true
            })
        }) && self.channels.iter().all(|c| {
            c.selected.iter().all(|from| {
                let to = i64::from(*from) + i64::from(delta);
                (0..i64::from(self.total)).contains(&to)
                    && (!c.authored.contains(&to)
                        || !copy && u32::try_from(to).is_ok_and(|to| c.selected.contains(&to)))
            })
        })
    }
    pub fn commands(
        &self,
        delta: i32,
        value_offset: Option<motion_core::KeyframeValueOffset>,
        copy: bool,
    ) -> Vec<motion_core::Command> {
        let mut commands = Vec::new();
        if !self.channels.is_empty() {
            commands.push(motion_core::Command::TransformKeyframes {
                selections: self.selected.selections(),
                delta_frame: delta,
                value_offset,
                copy,
            });
        }
        commands.extend(
            self.spatial
                .iter()
                .map(|c| motion_core::Command::SpatialKeyframes {
                    target: c.target.clone(),
                    edit: motion_core::SpatialKeyEdit::Move {
                        keys: c.ids.clone(),
                        delta: f64::from(delta),
                    },
                }),
        );
        commands
    }
    pub fn times(&self) -> impl Iterator<Item = u32> + '_ {
        self.channels
            .iter()
            .flat_map(|c| c.selected.iter().copied())
    }
}
pub(super) fn unavailable(panels: &mut Panels) {
    panels.message = Some(
        panels
            .locale
            .t(crate::i18n::Text::KeySelectionUnavailable)
            .into(),
    );
}
pub(super) fn rejected(panels: &mut Panels) {
    panels.message = Some(
        if panels.locale == crate::i18n::Locale::Zh {
            "关键帧目标时间已占用或超出合成范围"
        } else {
            "Keyframe destination is occupied or outside the composition"
        }
        .into(),
    );
}
pub(super) fn failed(panels: &mut Panels, state: &Value, error: Error) {
    log::error!(
        "key transform failed operation=prepare phase=occupancy revision={:?} primary_object={:?} cause={error}",
        state["revision"].as_u64(),panels.selected
    );
    panels.message = Some(format!(
        "{}: {error}",
        if panels.locale == crate::i18n::Locale::Zh {
            "关键帧变换失败"
        } else {
            "Keyframe transform failed"
        }
    ));
}
pub(super) fn nudge(
    panels: &mut Panels,
    state: &Value,
    command: crate::shortcuts::Command,
    out: &mut Vec<Action>,
) -> bool {
    use crate::shortcuts::Command::*;
    let delta = match command {
        NudgeKeyEarlier => -1,
        NudgeKeyLater => 1,
        NudgeKeysEarlierTen => -10,
        NudgeKeysLaterTen => 10,
        _ => return false,
    };
    if !matches!(
        panels.focused,
        Some(motion_ui::dock::Panel::Timeline | motion_ui::dock::Panel::Curves)
    ) {
        return false;
    }
    if !panels.selected_keys.writable(state) || panels.selected_keys.has_local() {
        unavailable(panels);
        return true;
    }
    let plan = match Plan::capture(&panels.selected_keys, state) {
        Ok(plan) => plan,
        Err(error) => {
            failed(panels, state, error);
            return true;
        }
    };
    if !plan.allows(delta, false) {
        rejected(panels);
        return true;
    }
    let moved = match panels.selected_keys.translated(delta) {
        Ok(set) => set,
        Err(error) => {
            failed(panels, state, error);
            return true;
        }
    };
    out.push(Action::EditorAction {
        source: crate::layer_clipboard::KeyframeSource::of(state),
        commands: plan.commands(delta, None, false),
    });
    panels.selected_keys = moved;
    ui::project_primary(panels, state);
    true
}
