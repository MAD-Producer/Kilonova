//! Stereo level envelopes in decibels; key times remain layer-local frames.
use crate::{ensure, Easing, Layer, Result, Track};
use serde::{Deserialize, Serialize};

pub const MIN_AUDIO_DB: f32 = -192.;
pub const MAX_AUDIO_DB: f32 = 24.;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioLevels {
    pub left: Track<f32>,
    pub right: Track<f32>,
}
impl Default for AudioLevels {
    fn default() -> Self {
        Self {
            left: Track::constant(0.),
            right: Track::constant(0.),
        }
    }
}
impl AudioLevels {
    pub fn validate(&self) -> Result<()> {
        for track in [&self.left, &self.right] {
            track.validate_local()?;
            ensure(
                track.axes.is_none() && track.keys.iter().all(|k| k.spatial.is_none()),
                "audio levels are scalar temporal tracks",
            )?;
            ensure(
                std::iter::once(track.value)
                    .chain(track.keys.iter().map(|k| k.value))
                    .all(|v| (MIN_AUDIO_DB..=MAX_AUDIO_DB).contains(&v)),
                "audio levels must be -192..24 dB",
            )?;
        }
        Ok(())
    }
    pub fn sample(&self, local_frame: f64) -> [f32; 2] {
        [&self.left, &self.right].map(|t| t.sample(local_frame).clamp(MIN_AUDIO_DB, MAX_AUDIO_DB))
    }
    pub fn animated(&self) -> bool {
        !self.left.keys.is_empty() || !self.right.keys.is_empty()
    }
}
pub fn db_gain(db: f32) -> f32 {
    10f32.powf(db / 20.)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioChannel {
    Left,
    Right,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AudioLevelsAction {
    Set {
        channel: AudioChannel,
        frame: u32,
        value: f32,
        #[serde(default)]
        animated: Option<bool>,
    },
    Animate {
        channel: AudioChannel,
        frame: u32,
        enabled: bool,
    },
    DeleteKey {
        channel: AudioChannel,
        frame: u32,
    },
    MoveKey {
        channel: AudioChannel,
        from: u32,
        to: u32,
    },
    CopyKey {
        channel: AudioChannel,
        from: u32,
        to: u32,
    },
    Curve {
        channel: AudioChannel,
        frame: u32,
        easing: Easing,
    },
    Clear,
}
pub fn edit(layer: &mut Layer, frames: u32, action: AudioLevelsAction) -> Result<()> {
    if layer.locked {
        return Err(crate::Error::Locked(layer.id));
    }
    ensure(
        matches!(
            layer.content,
            crate::Content::Audio { .. }
                | crate::Content::Video { .. }
                | crate::Content::Composition { .. }
        ),
        "audio levels require an audio, video or composition layer",
    )?;
    let local = |frame: u32| {
        ensure(frame < frames, "audio key outside composition")?;
        layer.clip(frames).edit_frame(frame)
    };
    if matches!(action, AudioLevelsAction::Clear) {
        layer.audio_levels = None;
        return Ok(());
    }
    let channel = match &action {
        AudioLevelsAction::Set { channel, .. }
        | AudioLevelsAction::Animate { channel, .. }
        | AudioLevelsAction::DeleteKey { channel, .. }
        | AudioLevelsAction::MoveKey { channel, .. }
        | AudioLevelsAction::CopyKey { channel, .. }
        | AudioLevelsAction::Curve { channel, .. } => *channel,
        AudioLevelsAction::Clear => unreachable!(),
    };
    let mut levels = layer.audio_levels.clone().unwrap_or_default();
    let track = match channel {
        AudioChannel::Left => &mut levels.left,
        AudioChannel::Right => &mut levels.right,
    };
    match action {
        AudioLevelsAction::Set {
            frame,
            value,
            animated,
            ..
        } => {
            ensure(
                (MIN_AUDIO_DB..=MAX_AUDIO_DB).contains(&value),
                "audio levels must be -192..24 dB",
            )?;
            if let Some(enabled) = animated {
                track.set_animated(local(frame)?, enabled)?;
            }
            track.set_at(local(frame)?, value)?;
        }
        AudioLevelsAction::Animate { frame, enabled, .. } => {
            track.set_animated(local(frame)?, enabled)?
        }
        AudioLevelsAction::DeleteKey { frame, .. } => track.delete_key(local(frame)?)?,
        AudioLevelsAction::MoveKey { from, to, .. } => track.move_key(local(from)?, local(to)?)?,
        AudioLevelsAction::CopyKey { from, to, .. } => track.copy_key(local(from)?, local(to)?)?,
        AudioLevelsAction::Curve { frame, easing, .. } => track.set_curve(local(frame)?, easing)?,
        AudioLevelsAction::Clear => unreachable!(),
    }
    levels.validate()?;
    layer.audio_levels = Some(levels);
    Ok(())
}
