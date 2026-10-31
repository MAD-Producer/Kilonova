//! Authored key identities are channel + composition time, independent of the
//! displayed property, graph component, playhead and primary layer selection.
use motion_core::{Error, KeyframeSelection, KeyframeTarget, MAX_KEYFRAME_CLIPBOARD_BYTES};
use serde_json::Value;
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
mod movement;
mod source;
mod transform;
pub(super) fn authored_track<'a>(
    state: &'a Value,
    target: &KeyframeTarget,
) -> Option<(&'a Value, i64)> {
    source::track(state, target)
}
pub(super) mod ui;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Key {
    pub target: KeyframeTarget,
    pub frame: u32,
}

/// Persistent identity never aliases a rounded composition frame. Distinct
/// spatial keys may legitimately have the same solved time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SpatialKey {
    pub target: KeyframeTarget,
    pub id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AddressedKey {
    Frame(Key),
    Spatial(SpatialKey),
}
impl From<Key> for AddressedKey {
    fn from(key: Key) -> Self {
        Self::Frame(key)
    }
}
impl From<SpatialKey> for AddressedKey {
    fn from(key: SpatialKey) -> Self {
        Self::Spatial(key)
    }
}
impl AddressedKey {
    fn target(&self) -> &KeyframeTarget {
        match self {
            Self::Frame(k) => &k.target,
            Self::Spatial(k) => &k.target,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Replace,
    Add,
    Toggle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Unowned,
    Empty,
    Current,
    Stale,
}

#[derive(Clone, PartialEq, Eq)]
struct Scope {
    root: String,
    composition: String,
    generation: u64,
}
impl Scope {
    fn of(state: &Value) -> Option<Self> {
        Some(Self {
            root: state["root"].as_str()?.to_owned(),
            composition: state["composition"]
                .as_str()
                .or_else(|| state["project"]["composition_id"].as_str())?
                .to_owned(),
            generation: state["document_generation"].as_u64()?,
        })
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
struct Channel {
    target: Option<KeyframeTarget>,
    frames: BTreeSet<u32>,
    ids: BTreeSet<u64>,
}
impl Channel {
    // Worst-case serialized u32 is ten digits plus a comma. The channel
    // envelope fits in 32 bytes beyond the already serialized typed address.
    fn bytes(address: &str, count: usize) -> usize {
        address
            .len()
            .saturating_add(32)
            .saturating_add(count.saturating_mul(11))
    }
    fn retained_bytes(&self, address: &str) -> usize {
        Self::bytes(address, self.frames.len()).saturating_add(self.ids.len().saturating_mul(21))
    }
    fn insert(&mut self, key: &AddressedKey, toggle: bool) {
        match key {
            AddressedKey::Frame(k) => {
                if !toggle || !self.frames.remove(&k.frame) {
                    self.frames.insert(k.frame);
                }
            }
            AddressedKey::Spatial(k) => {
                if !toggle || !self.ids.remove(&k.id) {
                    self.ids.insert(k.id);
                }
            }
        }
    }
    fn empty(&self) -> bool {
        self.frames.is_empty() && self.ids.is_empty()
    }
    fn keys(&self) -> impl Iterator<Item = AddressedKey> + '_ {
        self.frames
            .iter()
            .filter_map(|frame| {
                Some(
                    Key {
                        target: self.target.clone()?,
                        frame: *frame,
                    }
                    .into(),
                )
            })
            .chain(self.ids.iter().filter_map(|id| {
                Some(
                    SpatialKey {
                        target: self.target.clone()?,
                        id: *id,
                    }
                    .into(),
                )
            }))
    }
}

#[derive(Clone, Default)]
pub(crate) struct Set {
    claimed: bool,
    intent: bool,
    scope: Option<Scope>,
    channels: BTreeMap<String, Channel>,
    primary: Option<Key>,
    spatial_primary: Option<SpatialKey>,
    // A valid authoritative document revision permits reusing presence checks.
    // Missing revision disables caching; view-only changes do not invalidate it.
    validated: Cell<Option<(u64, Status)>>,
}
impl Set {
    /// Popup identity excludes the mutable presence cache and one-frame intent.
    pub(crate) fn same_selection(&self, other: &Self) -> bool {
        self.claimed == other.claimed
            && self.scope == other.scope
            && self.channels == other.channels
            && self.primary == other.primary
            && self.spatial_primary == other.spatial_primary
    }
    pub fn owns_keys(&self) -> bool {
        self.claimed
    }

    pub fn status(&self, state: &Value) -> Status {
        self.validate(state, |target, frame| {
            source::contains(state, target, frame)
        })
    }

    pub fn take_intent(&mut self) -> bool {
        std::mem::take(&mut self.intent)
    }

    pub fn writable(&self, state: &Value) -> bool {
        self.status(state) == Status::Current
            && state["observing"] != true
            && self.channels.values().all(|channel| {
                channel.target.as_ref().is_some_and(|target| {
                    target.object() == 0
                        || super::keyframe_clipboard::layer(state, target.object())
                            .is_some_and(|layer| layer["locked"] != true)
                })
            })
    }

    pub fn primary(&self) -> Option<&Key> {
        self.primary.as_ref()
    }

    fn addressed_primary(&self) -> Option<AddressedKey> {
        self.spatial_primary
            .clone()
            .map(AddressedKey::Spatial)
            .or_else(|| self.primary.clone().map(AddressedKey::Frame))
    }
    fn set_primary(&mut self, key: Option<AddressedKey>) {
        self.primary = None;
        self.spatial_primary = None;
        match key {
            Some(AddressedKey::Frame(k)) => self.primary = Some(k),
            Some(AddressedKey::Spatial(k)) => self.spatial_primary = Some(k),
            None => {}
        }
    }
    pub fn has_spatial(&self) -> bool {
        self.channels.values().any(|c| !c.ids.is_empty())
    }
    pub fn spatial_selections(&self) -> Vec<(KeyframeTarget, Vec<u64>)> {
        self.channels
            .values()
            .filter_map(|c| {
                (!c.ids.is_empty())
                    .then(|| Some((c.target.clone()?, c.ids.iter().copied().collect())))?
            })
            .collect()
    }
    fn contains_addressed(&self, key: &AddressedKey) -> bool {
        match key {
            AddressedKey::Frame(k) => self.contains(k),
            AddressedKey::Spatial(k) => self
                .channels
                .values()
                .any(|c| c.target.as_ref() == Some(&k.target) && c.ids.contains(&k.id)),
        }
    }

    pub fn contains(&self, key: &Key) -> bool {
        self.channels.values().any(|channel| {
            channel.target.as_ref() == Some(&key.target) && channel.frames.contains(&key.frame)
        })
    }

    fn translated(&self, delta: i32) -> motion_core::Result<Self> {
        let shifted = |frame: u32| {
            u32::try_from(i64::from(frame) + i64::from(delta))
                .map_err(|_| Error::Invalid("key selection time overflow".into()))
        };
        let mut set = self.clone();
        for channel in set.channels.values_mut() {
            channel.frames = channel
                .frames
                .iter()
                .map(|f| shifted(*f))
                .collect::<motion_core::Result<_>>()?;
        }
        if let Some(primary) = &mut set.primary {
            primary.frame = shifted(primary.frame)?;
        }
        set.validated.set(None);
        set.intent = true;
        Ok(set)
    }

    /// Explicit layer/property navigation relinquishes ownership. Clearing only
    /// the selected keys must instead keep ownership, preventing layer fallback.
    pub fn release(&mut self) {
        *self = Self::default();
    }

    pub fn clear(&mut self, state: &Value) {
        self.claimed = true;
        self.intent = true;
        self.scope = Scope::of(state);
        self.channels.clear();
        self.primary = None;
        self.spatial_primary = None;
        self.validated.set(None);
    }

    pub fn selections(&self) -> Vec<KeyframeSelection> {
        self.channels
            .values()
            .filter_map(|channel| {
                if channel.frames.is_empty() {
                    return None;
                }
                Some(KeyframeSelection {
                    target: channel.target.clone()?,
                    frames: channel.frames.iter().copied().collect(),
                })
            })
            .collect()
    }

    /// Preflight the entire incoming gesture before changing the old set. A
    /// limit failure cannot leave half a marquee or toggle operation selected.
    pub fn apply(
        &mut self,
        state: &Value,
        keys: impl IntoIterator<Item = Key>,
        mode: Mode,
    ) -> motion_core::Result<()> {
        self.apply_addressed(state, keys.into_iter().map(AddressedKey::Frame), mode)
    }

    fn apply_addressed(
        &mut self,
        state: &Value,
        keys: impl IntoIterator<Item = AddressedKey>,
        mode: Mode,
    ) -> motion_core::Result<()> {
        let Some(scope) = Scope::of(state) else {
            self.claimed = true;
            self.intent = true;
            self.validated.set(None);
            return Err(Error::Invalid(
                "key selection document identity missing".into(),
            ));
        };
        let mut incoming: BTreeMap<String, Channel> = BTreeMap::new();
        let mut last = None;
        let mut bytes = 2usize;
        for key in keys {
            let address = serde_json::to_string(key.target())?;
            let channel = incoming.entry(address.clone()).or_default();
            if channel.target.is_none() {
                bytes = bytes.saturating_add(Channel::bytes(&address, 0));
                channel.target = Some(key.target().clone());
            }
            let old = channel.retained_bytes(&address);
            channel.insert(&key, false);
            bytes = bytes.saturating_add(channel.retained_bytes(&address).saturating_sub(old));
            if bytes > MAX_KEYFRAME_CLIPBOARD_BYTES {
                return Err(Error::Invalid(
                    "key selection exceeds metadata limit".into(),
                ));
            }
            last = Some(key);
        }
        let replace = mode == Mode::Replace || self.scope.as_ref() != Some(&scope);
        let mut bytes = if replace {
            2
        } else {
            self.channels
                .iter()
                .fold(2usize, |sum, (address, channel)| {
                    sum.saturating_add(channel.retained_bytes(address))
                })
        };
        for (address, channel) in &incoming {
            let old = (!replace).then(|| self.channels.get(address)).flatten();
            let mut merged = old.cloned().unwrap_or_default();
            if let Some(old) = old {
                bytes = bytes.saturating_sub(old.retained_bytes(address));
            }
            for key in channel.keys() {
                merged.insert(&key, mode == Mode::Toggle);
            }
            if !merged.empty() {
                bytes = bytes.saturating_add(merged.retained_bytes(address));
            }
        }
        if bytes > MAX_KEYFRAME_CLIPBOARD_BYTES {
            return Err(Error::Invalid(
                "key selection exceeds metadata limit".into(),
            ));
        }
        if replace {
            self.channels.clear();
        }
        for (address, incoming) in incoming {
            let channel = self
                .channels
                .entry(address.clone())
                .or_insert_with(|| Channel {
                    target: incoming.target.clone(),
                    frames: BTreeSet::new(),
                    ids: BTreeSet::new(),
                });
            for key in incoming.keys() {
                channel.insert(&key, mode == Mode::Toggle);
            }
            if channel.empty() {
                self.channels.remove(&address);
            }
        }
        self.claimed = true;
        self.intent = true;
        self.scope = Some(scope);
        self.validated.set(None);
        let candidate = last.or_else(|| (!replace).then(|| self.addressed_primary()).flatten());
        let primary = candidate
            .filter(|key| self.contains_addressed(key))
            .or_else(|| self.channels.values().next_back()?.keys().last());
        self.set_primary(primary);
        Ok(())
    }

    /// `present` must inspect authored channel data, including its layer offset,
    /// never just the currently visible rows. A stale/empty set keeps ownership.
    pub fn validate(
        &self,
        state: &Value,
        mut present: impl FnMut(&KeyframeTarget, u32) -> bool,
    ) -> Status {
        if !self.claimed {
            return Status::Unowned;
        }
        if self.scope.is_none() || self.scope != Scope::of(state) {
            return Status::Stale;
        }
        if self.channels.is_empty() {
            return Status::Empty;
        }
        let revision = state["revision"].as_u64();
        if let Some((_, status)) = self
            .validated
            .get()
            .filter(|(old, _)| Some(*old) == revision)
        {
            return status;
        }
        let current = self.channels.values().all(|channel| {
            channel.target.as_ref().is_some_and(|target| {
                channel.frames.iter().all(|&frame| present(target, frame))
                    && (channel.ids.is_empty()
                        || source::contains_spatial(state, target, &channel.ids))
            })
        });
        let status = if current {
            Status::Current
        } else {
            Status::Stale
        };
        self.validated
            .set(revision.map(|revision| (revision, status)));
        status
    }
}

pub(super) fn choose(
    panels: &mut super::Panels,
    state: &Value,
    keys: impl IntoIterator<Item = Key>,
    mode: Mode,
) -> bool {
    choose_addressed(
        panels,
        state,
        keys.into_iter().map(AddressedKey::Frame),
        mode,
    )
}
fn choose_addressed(
    panels: &mut super::Panels,
    state: &Value,
    keys: impl IntoIterator<Item = AddressedKey>,
    mode: Mode,
) -> bool {
    match panels.selected_keys.apply_addressed(state, keys, mode) {
        Ok(()) => true,
        Err(error) => {
            log::error!("key selection failed operation=select phase=preflight revision={:?} object={:?} cause={error}", state["revision"].as_u64(), panels.selected);
            panels.message = Some(format!(
                "{}: {error}",
                if panels.locale == crate::i18n::Locale::Zh {
                    "关键帧选择失败"
                } else {
                    "Keyframe selection failed"
                }
            ));
            false
        }
    }
}

pub(super) fn ease_command(
    panels: &super::Panels,
    state: &Value,
    command: crate::shortcuts::Command,
) -> Option<motion_core::Command> {
    use crate::shortcuts::Command as Shortcut;
    if !matches!(
        command,
        Shortcut::EaseBoth | Shortcut::EaseIncoming | Shortcut::EaseOutgoing
    ) || !panels.selected_keys.writable(state)
        || panels.selected_keys.has_spatial()
    {
        return None;
    }
    Some(motion_core::Command::EaseKeyframes {
        selections: panels.selected_keys.selections(),
        incoming: command != Shortcut::EaseOutgoing,
        outgoing: command != Shortcut::EaseIncoming,
    })
}

pub(super) fn delete_available(panels: &super::Panels, state: &Value) -> bool {
    panels.selected_keys.writable(state)
        && panels.selected_keys.channels.values().all(|channel| {
            if channel.ids.is_empty() {
                return true;
            }
            channel
                .target
                .as_ref()
                .and_then(|target| source::track(state, target))
                .and_then(|(track, _)| track["spatial_animation"]["keys"].as_array())
                .is_some_and(|keys| keys.len().saturating_sub(channel.ids.len()) >= 2)
        })
}

pub(super) fn delete_commands(
    panels: &super::Panels,
    state: &Value,
) -> Option<Vec<motion_core::Command>> {
    if !delete_available(panels, state) {
        return None;
    }
    let mut commands = Vec::new();
    let selections = panels.selected_keys.selections();
    if !selections.is_empty() {
        commands.push(motion_core::Command::DeleteKeyframes { selections });
    }
    for (target, ids) in panels.selected_keys.spatial_selections() {
        let (track, _) = source::track(state, &target)?;
        let keys = track["spatial_animation"]["keys"].as_array()?;
        // Spatial storage must retain at least two keys. This unsupported edit
        // is a normal disabled capability, not an engine error or partial delete.
        if keys.len().saturating_sub(ids.len()) < 2 {
            return None;
        }
        commands.extend(
            ids.into_iter()
                .map(|id| motion_core::Command::SpatialKeyframes {
                    target: target.clone(),
                    edit: motion_core::SpatialKeyEdit::Delete { key: id },
                }),
        );
    }
    (!commands.is_empty()).then_some(commands)
}

pub(super) fn shortcut(
    panels: &mut super::Panels,
    state: &Value,
    command: crate::shortcuts::Command,
    out: &mut Vec<super::Action>,
) -> bool {
    use crate::shortcuts::Command as Shortcut;
    use motion_ui::dock::Panel;
    if transform::nudge(panels, state, command, out) {
        return true;
    }
    if command == Shortcut::SelectAll
        && panels.focused == Some(Panel::Timeline)
        && (panels.selected_keys.owns_keys() || panels.timeline.graph_mode)
    {
        if panels.timeline.graph_mode
            && !panels.animation_controls.foreign_track_selected()
            && super::graph_box::select_all(panels, state)
        {
            if let Some(object) = panels.selected {
                let property = panels
                    .animation_controls
                    .property
                    .unwrap_or(motion_core::Property::Position);
                let axis = panels.animation_controls.axis;
                if let Some(source) = super::animation_data::channel(state, object, property, axis)
                {
                    choose(
                        panels,
                        state,
                        source["keys"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|k| k["frame"].as_u64())
                            .filter(|f| *f as f64 <= super::last_frame(state))
                            .map(|frame| Key {
                                target: KeyframeTarget::Property {
                                    object,
                                    property,
                                    axis,
                                },
                                frame: frame as u32,
                            }),
                        Mode::Replace,
                    );
                }
            }
            return true;
        }
        panels.key_gestures.select_all = true;
        return true;
    }
    if !panels.selected_keys.owns_keys() || !super::property_focus::navigation_scope(panels.focused)
    {
        return false;
    }
    if command == Shortcut::DeselectAll
        && matches!(panels.focused, Some(Panel::Timeline | Panel::Curves))
    {
        panels.selected_keys.clear(state);
        ui::project_primary(panels, state);
        return true;
    }
    let delete = command == Shortcut::Delete
        && matches!(panels.focused, Some(Panel::Timeline | Panel::Curves));
    if !delete
        && !matches!(
            command,
            Shortcut::EaseBoth | Shortcut::EaseIncoming | Shortcut::EaseOutgoing
        )
    {
        return false;
    }
    if !panels.selected_keys.writable(state) {
        panels.message = Some(
            panels
                .locale
                .t(crate::i18n::Text::KeySelectionUnavailable)
                .into(),
        );
        return true;
    }
    let commands = if delete {
        delete_commands(panels, state)
    } else {
        ease_command(panels, state, command).map(|command| vec![command])
    };
    if let Some(commands) = commands {
        out.push(super::Action::EditorAction {
            source: crate::layer_clipboard::KeyframeSource::of(state),
            commands,
        });
    } else {
        transform::unavailable(panels);
    }
    true
}

#[cfg(test)]
mod tests;
