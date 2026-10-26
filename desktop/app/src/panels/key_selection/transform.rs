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
}
impl Plan {
    pub fn capture(selected: &Set, state: &Value) -> motion_core::Result<Self> {
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
        Ok(Self {
            selected: selected.clone(),
            channels,
            total,
        })
    }
    pub fn allows(&self, delta: i32, copy: bool) -> bool {
        self.channels.iter().all(|c| {
            c.selected.iter().all(|from| {
                let to = i64::from(*from) + i64::from(delta);
                (0..i64::from(self.total)).contains(&to)
                    && (!c.authored.contains(&to)
                        || !copy && u32::try_from(to).is_ok_and(|to| c.selected.contains(&to)))
            })
        })
    }
    pub fn command(
        &self,
        delta: i32,
        value_offset: Option<motion_core::KeyframeValueOffset>,
        copy: bool,
    ) -> motion_core::Command {
        motion_core::Command::TransformKeyframes {
            selections: self.selected.selections(),
            delta_frame: delta,
            value_offset,
            copy,
        }
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
    if !panels.selected_keys.writable(state) {
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
        commands: vec![plan.command(delta, None, false)],
    });
    panels.selected_keys = moved;
    ui::project_primary(panels, state);
    true
}
