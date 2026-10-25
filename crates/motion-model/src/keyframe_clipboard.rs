//! Self-contained authored keys. Composition time is the transfer coordinate;
//! each destination converts it to its own signed layer-local time.
use crate::{
    ensure, Axis, CameraMode, Content, Error, Keyframe, Project, Property, Result, Track, Tween,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

mod channels;
mod interpolation;
pub use interpolation::set_interpolation;
use channels::{read, write, ReadChannel, WriteChannel};

pub const MAX_KEYFRAME_CLIPBOARD_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum KeyframeTarget {
    Property {
        object: u64,
        property: Property,
        axis: Option<Axis>,
    },
    Effect {
        object: u64,
        effect: u64,
        param: String,
    },
    Mask {
        object: u64,
        mask: u64,
        property: crate::masks::MaskProperty,
    },
    Audio {
        object: u64,
        channel: crate::AudioChannel,
    },
    /// None addresses a legacy vector's own channels; Some addresses a group item.
    Vector {
        object: u64,
        item: Option<u64>,
        parameter: String,
    },
}
impl KeyframeTarget {
    fn failure(&self, phase: &'static str, source: Error) -> Error {
        let channel = match self {
            Self::Property { property, axis, .. } => format!("{property:?}/{axis:?}"),
            Self::Effect { effect, param, .. } => format!(
                "effect={effect}/{}",
                param.chars().take(128).collect::<String>()
            ),
            Self::Mask { mask, property, .. } => format!("mask={mask}/{property:?}"),
            Self::Audio { channel, .. } => format!("audio/{channel:?}"),
            Self::Vector {
                item, parameter, ..
            } => format!(
                "vector={item:?}/{}",
                parameter.chars().take(128).collect::<String>()
            ),
        };
        Error::KeyframeClipboard {
            phase,
            object: self.object(),
            channel,
            source: Box::new(source),
        }
    }
    fn address(&self) -> Result<String> {
        let mut canonical = self.clone();
        if let Self::Vector { parameter, .. } = &mut canonical {
            ensure(parameter.len() <= 256, "clipboard parameter name too long")?;
            if let Some(node) = parameter.strip_prefix("node:") {
                let (path, node) = node
                    .split_once(':')
                    .ok_or_else(|| Error::Invalid("invalid clipboard node address".into()))?;
                let path: u64 = path
                    .parse()
                    .map_err(|_| Error::Invalid("invalid clipboard path ID".into()))?;
                let node: u64 = node
                    .parse()
                    .map_err(|_| Error::Invalid("invalid clipboard node ID".into()))?;
                *parameter = format!("node:{path}:{node}");
            } else if let Some(index) = parameter.strip_prefix("dash_").filter(|v| *v != "offset") {
                let index: usize = index
                    .parse()
                    .map_err(|_| Error::Invalid("invalid clipboard dash index".into()))?;
                *parameter = format!("dash_{index}");
            }
        }
        Ok(serde_json::to_string(&canonical)?)
    }
    pub fn object(&self) -> u64 {
        match self {
            Self::Property { object, .. }
            | Self::Effect { object, .. }
            | Self::Mask { object, .. }
            | Self::Audio { object, .. }
            | Self::Vector { object, .. } => *object,
        }
    }
    fn signature(&self, project: &Project) -> Result<String> {
        Ok(match self {
            Self::Property { property, axis, .. } => {
                format!("property:{property:?}:{}", axis.is_some())
            }
            Self::Effect {
                object,
                effect,
                param,
            } => {
                ensure(param.len() <= 256, "clipboard parameter name too long")?;
                let effect = project
                    .layers
                    .iter()
                    .find(|l| l.id == *object)
                    .and_then(|l| l.effects.iter().find(|e| e.id == *effect))
                    .ok_or_else(|| Error::Invalid("clipboard effect missing".into()))?;
                let p = effect
                    .params
                    .get(param)
                    .ok_or_else(|| Error::Invalid("clipboard effect parameter missing".into()))?;
                ensure(
                    p.animatable && p.implemented,
                    "clipboard effect parameter is not editable",
                )?;
                serde_json::to_string(&(
                    &effect.plugin,
                    &effect.effect,
                    &effect.version,
                    &effect.hash,
                    param,
                    p.kind,
                ))?
            }
            Self::Mask { property, .. } => match property {
                crate::masks::MaskProperty::Node(_) => "mask:node".into(),
                _ => format!("mask:{property:?}"),
            },
            Self::Audio { .. } => "audio:levels".into(),
            Self::Vector { parameter, .. } => {
                ensure(parameter.len() <= 256, "clipboard parameter name too long")?;
                if parameter.starts_with("node:") {
                    "vector:node".into()
                } else {
                    format!("vector:{parameter}")
                }
            }
        })
    }
    fn editable(&self, project: &Project) -> Result<()> {
        let object = self.object();
        if object == 0 {
            project.camera_for(0)?;
        } else {
            let layer = project
                .layers
                .iter()
                .find(|l| l.id == object)
                .ok_or(Error::Missing(object))?;
            if layer.locked {
                return Err(Error::Locked(object));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyframeSelection {
    pub target: KeyframeTarget,
    pub frames: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "keys",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum KeyframeData {
    Scalar(Vec<Keyframe<f32>>),
    Vec2(Vec<Keyframe<[f32; 2]>>),
    Vec3(Vec<Keyframe<[f32; 3]>>),
    Vec4(Vec<Keyframe<[f32; 4]>>),
    Geometry(Vec<Keyframe<[f32; 6]>>),
    ColorCurve(Vec<Keyframe<crate::CurveObject>>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyframeClipTrack {
    pub source: KeyframeTarget,
    pub signature: String,
    pub data: KeyframeData,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyframeClipboard {
    pub tracks: Vec<KeyframeClipTrack>,
}

macro_rules! key_data {
    ($self:expr, $keys:ident => $body:expr) => {
        match $self {
            KeyframeData::Scalar($keys) => $body,
            KeyframeData::Vec2($keys) => $body,
            KeyframeData::Vec3($keys) => $body,
            KeyframeData::Vec4($keys) => $body,
            KeyframeData::Geometry($keys) => $body,
            KeyframeData::ColorCurve($keys) => $body,
        }
    };
}

fn capture<T: Clone>(
    keys: &[Keyframe<T>],
    selection: &KeyframeSelection,
    project: &Project,
) -> Result<Vec<Keyframe<T>>> {
    let mut frames = selection.frames.clone();
    frames.sort_unstable();
    ensure(
        !frames.is_empty() && frames.windows(2).all(|w| w[0] != w[1]),
        "clipboard keys must be nonempty and unique",
    )?;
    frames
        .into_iter()
        .map(|frame| {
            ensure(frame < project.frames, "clipboard key outside composition")?;
            let local = project.edit_frame(selection.target.object(), frame)?;
            let mut key = keys
                .binary_search_by_key(&local, |k| k.frame)
                .ok()
                .and_then(|index| keys.get(index))
                .cloned()
                .ok_or_else(|| {
                    Error::Invalid(format!(
                        "clipboard source key missing: object={} frame={frame}",
                        selection.target.object()
                    ))
                })?;
            key.frame = i32::try_from(frame)
                .map_err(|_| Error::Invalid("clipboard key time overflow".into()))?;
            Ok(key)
        })
        .collect()
}

impl KeyframeClipboard {
    pub fn capture(project: &Project, selection: &[KeyframeSelection]) -> Result<Self> {
        ensure(!selection.is_empty(), "invalid clipboard track count")?;
        let mut seen = BTreeSet::new();
        let mut tracks = vec![];
        let mut bytes = 0usize;
        for selected in selection {
            let track = (|| -> Result<KeyframeClipTrack> {
                ensure(
                    seen.insert(selected.target.address()?),
                    "duplicate clipboard source channel",
                )?;
                let signature = selected.target.signature(project)?;
                macro_rules! data {
                    ($variant:ident, $keys:expr) => {
                        KeyframeData::$variant(capture($keys, selected, project)?)
                    };
                }
                let data = match read(project, &selected.target)? {
                    ReadChannel::Scalar(t) => data!(Scalar, &t.keys),
                    ReadChannel::Vec2(t) => data!(Vec2, &t.keys),
                    ReadChannel::Vec3(t) => data!(Vec3, &t.keys),
                    ReadChannel::Vec4(t) => data!(Vec4, &t.keys),
                    ReadChannel::Geometry(t) => data!(Geometry, &t.keys),
                    ReadChannel::ColorCurve(t) => data!(ColorCurve, &t.keys),
                };
                Ok(KeyframeClipTrack {
                    source: selected.target.clone(),
                    signature,
                    data,
                })
            })()
            .map_err(|error| selected.target.failure("capture", error))?;
            bytes = bytes.saturating_add(crate::serialized_bytes(&track));
            ensure(
                bytes <= MAX_KEYFRAME_CLIPBOARD_BYTES,
                "keyframe clipboard exceeds metadata limit",
            )?;
            tracks.push(track);
        }
        let result = Self { tracks };
        result.validate()?;
        Ok(result)
    }

    pub fn validate(&self) -> Result<()> {
        ensure(!self.tracks.is_empty(), "invalid clipboard track count")?;
        let bytes = crate::serialized_bytes(self);
        ensure(
            bytes <= MAX_KEYFRAME_CLIPBOARD_BYTES,
            "keyframe clipboard exceeds metadata limit",
        )?;
        let mut seen = BTreeSet::new();
        for track in &self.tracks {
            (|| -> Result<()> {
            ensure(
                track.signature.len() <= 4096,
                "clipboard channel signature too long",
            )?;
            ensure(
                seen.insert(track.source.address()?),
                "duplicate clipboard source channel",
            )?;
            let frames =
                key_data!(&track.data, keys => keys.iter().map(|k| k.frame).collect::<Vec<_>>());
            ensure(
                !frames.is_empty()
                    && frames.len() <= crate::MAX_FRAMES as usize
                    && frames
                        .iter()
                        .all(|f| *f >= 0 && (*f as u32) < crate::MAX_FRAMES)
                    && frames.windows(2).all(|w| w[0] < w[1]),
                "clipboard keys must have unique sorted composition times",
            )?;
            fn validate_keys<T: Tween>(keys: &[Keyframe<T>]) -> Result<()> {
                Track {
                    value: keys[0].value,
                    keys: keys.to_vec(),
                    axes: None,
                }
                .validate_local()
            }
            match &track.data {
                KeyframeData::Scalar(k) => validate_keys(k)?,
                KeyframeData::Vec2(k) => validate_keys(k)?,
                KeyframeData::Vec3(k) => validate_keys(k)?,
                KeyframeData::Vec4(k) => validate_keys(k)?,
                KeyframeData::Geometry(k) => validate_keys(k)?,
                KeyframeData::ColorCurve(keys) => {
                    for key in keys {
                        key.value.validate()?;
                        ensure(
                            key.spatial.is_none(),
                            "color transfer curves do not support spatial tangents",
                        )?;
                        if let Some(curve) = key.curve {
                            curve.validate()?;
                        }
                    }
                }
            }
            Ok(())
            })().map_err(|error| track.source.failure("validate", error))?;
        }
        Ok(())
    }

    /// Use through the core command transaction, which validates the resulting
    /// project and restores model/history if any later channel fails.
    pub fn paste(
        &self,
        project: &mut Project,
        targets: &[KeyframeTarget],
        at_frame: Option<u32>,
    ) -> Result<()> {
        self.validate()?;
        ensure(
            targets.len() == self.tracks.len(),
            "clipboard destination channel count differs",
        )?;
        let earliest = self
            .tracks
            .iter()
            .map(|t| key_data!(&t.data, k => k[0].frame))
            .min()
            .unwrap_or(0);
        let delta = at_frame.map_or(0, |frame| i64::from(frame) - i64::from(earliest));
        let mut seen = BTreeSet::new();
        for (source, target) in self.tracks.iter().zip(targets) {
            (|| -> Result<()> {
                ensure(
                    seen.insert(target.address()?),
                    "duplicate clipboard destination channel",
                )?;
                target.editable(project)?;
                ensure(
                    source.signature == target.signature(project)?,
                    "clipboard source and destination properties differ",
                )?;
                let offset = if target.object() == 0 {
                    0
                } else {
                    project
                        .layers
                        .iter()
                        .find(|l| l.id == target.object())
                        .ok_or(Error::Missing(target.object()))?
                        .clip(project.frames)
                        .offset_frame
                };
                let frames = project.frames;
                macro_rules! insert {
                    ($keys:expr, $data:expr) => {{
                        let mut moved = $data.clone();
                        for key in &mut moved {
                            let composition = i64::from(key.frame) + delta;
                            ensure(
                                (0..i64::from(frames)).contains(&composition),
                                "clipboard destination key outside composition",
                            )?;
                            key.frame =
                                i32::try_from(composition - i64::from(offset)).map_err(|_| {
                                    Error::Invalid(
                                        "clipboard destination local time overflow".into(),
                                    )
                                })?;
                            crate::animation::vacant_key_index($keys, key.frame)?;
                        }
                        $keys.extend(moved);
                        $keys.sort_by_key(|k| k.frame);
                    }};
                }
                macro_rules! track {
                    ($dest:expr, $data:expr) => {{
                        ensure(
                            $dest.axes.is_none(),
                            "choose a separated clipboard axis explicitly",
                        )?;
                        insert!(&mut $dest.keys, $data);
                        $dest.validate_local()?;
                    }};
                }
                match (write(project, target)?, &source.data) {
                    (WriteChannel::Scalar(t), KeyframeData::Scalar(k)) => track!(t, k),
                    (WriteChannel::Vec2(t), KeyframeData::Vec2(k)) => track!(t, k),
                    (WriteChannel::Vec3(t), KeyframeData::Vec3(k)) => track!(t, k),
                    (WriteChannel::Vec4(t), KeyframeData::Vec4(k)) => track!(t, k),
                    (WriteChannel::Geometry(t), KeyframeData::Geometry(k)) => track!(t, k),
                    (WriteChannel::ColorCurve(t), KeyframeData::ColorCurve(k)) => {
                        insert!(&mut t.keys, k)
                    }
                    _ => {
                        return Err(Error::Invalid(
                            "clipboard channel value types differ".into(),
                        ))
                    }
                }
                Ok(())
            })()
            .map_err(|error| target.failure("paste", error))?;
        }
        Ok(())
    }
}

/// A cut checks every frozen key against the current source before deleting.
/// Stale requests cannot delete a key that another edit has changed.
pub fn remove(project: &mut Project, clipboard: &KeyframeClipboard) -> Result<()> {
    clipboard.validate()?;
    let selections = clipboard
        .tracks
        .iter()
        .map(|track| KeyframeSelection {
            target: track.source.clone(),
            frames: key_data!(&track.data, k => k.iter().map(|k| k.frame as u32).collect()),
        })
        .collect::<Vec<_>>();
    let current = KeyframeClipboard::capture(project, &selections)?;
    for (old, current) in clipboard.tracks.iter().zip(&current.tracks) {
        if old != current {
            return Err(old
                .source
                .failure("cut", Error::Invalid("clipboard cut source changed".into())));
        }
    }
    for selected in selections {
        (|| -> Result<()> {
            selected.target.editable(project)?;
            let local = selected
                .frames
                .iter()
                .map(|frame| project.edit_frame(selected.target.object(), *frame))
                .collect::<Result<Vec<_>>>()?;
            macro_rules! delete {
                ($track:expr) => {{
                    ensure(
                        $track.axes.is_none(),
                        "choose a separated clipboard axis explicitly",
                    )?;
                    ensure(
                        local.iter().all(|frame| {
                            $track.keys.binary_search_by_key(frame, |k| k.frame).is_ok()
                        }),
                        "clipboard cut source key missing",
                    )?;
                    if $track.keys.len() == local.len() {
                        $track.value = $track
                            .keys
                            .last()
                            .ok_or_else(|| {
                                Error::Invalid("clipboard cut source key missing".into())
                            })?
                            .value;
                    }
                    $track
                        .keys
                        .retain(|k| local.binary_search(&k.frame).is_err());
                }};
            }
            match write(project, &selected.target)? {
                WriteChannel::Scalar(t) => delete!(t),
                WriteChannel::Vec2(t) => delete!(t),
                WriteChannel::Vec3(t) => delete!(t),
                WriteChannel::Vec4(t) => delete!(t),
                WriteChannel::Geometry(t) => delete!(t),
                WriteChannel::ColorCurve(t) => {
                    // Deleting all keys in ascending order leaves the final key's
                    // value, matching Track::delete_key's last-key fallback.
                    ensure(
                        local
                            .iter()
                            .all(|frame| t.keys.binary_search_by_key(frame, |k| k.frame).is_ok()),
                        "clipboard cut source key missing",
                    )?;
                    if t.keys.len() == local.len() {
                        t.value = t
                            .keys
                            .last()
                            .ok_or_else(|| {
                                Error::Invalid("clipboard cut source key missing".into())
                            })?
                            .value
                            .clone();
                    }
                    t.keys.retain(|k| local.binary_search(&k.frame).is_err());
                }
            }
            Ok(())
        })()
        .map_err(|error| selected.target.failure("cut", error))?;
    }
    Ok(())
}
