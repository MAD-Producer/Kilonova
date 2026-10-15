//! Layer-local audio reference markers and source channel isolation.
use crate::{ensure, Content, Layer, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_AUDIO_MARKERS: usize = 4096;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioSourceChannel {
    #[default]
    Stereo,
    Left,
    Right,
}
pub fn stereo(channel: &AudioSourceChannel) -> bool {
    *channel == AudioSourceChannel::Stereo
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioMarker {
    pub id: u64,
    /// Source/layer time, so moving and trimming a clip preserve its references.
    pub frame: i32,
    pub label: String,
    #[serde(default)]
    pub generated: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AudioMarkersAction {
    ReplaceGenerated { frames: Vec<u32> },
    Add { frame: u32, label: String },
    Move { id: u64, frame: u32 },
    Remove { id: u64 },
    ClearGenerated,
}

pub fn validate(layer: &Layer) -> Result<()> {
    if layer.audio_markers.is_empty() && stereo(&layer.audio_source_channel) {
        return Ok(());
    }
    ensure(
        matches!(
            layer.content,
            Content::Audio { .. } | Content::Video { .. } | Content::Composition { .. }
        ),
        "audio metadata requires an audio, video or composition layer",
    )?;
    ensure(
        stereo(&layer.audio_source_channel)
            || !matches!(layer.content, Content::Composition { .. }),
        "channel isolation requires a single audio source",
    )?;
    ensure(
        layer.audio_markers.len() <= MAX_AUDIO_MARKERS,
        "too many audio reference markers",
    )?;
    let mut ids = HashSet::new();
    for marker in &layer.audio_markers {
        ensure(
            marker.id != 0 && ids.insert(marker.id),
            "audio marker IDs must be unique and nonzero",
        )?;
        ensure(
            marker.label.len() <= 256 && !marker.label.chars().any(char::is_control),
            "invalid audio marker label",
        )?;
    }
    Ok(())
}

pub fn edit(layer: &mut Layer, frames: u32, action: AudioMarkersAction) -> Result<()> {
    ensure(!layer.locked, "audio layer is locked")?;
    ensure(
        matches!(
            layer.content,
            Content::Audio { .. } | Content::Video { .. } | Content::Composition { .. }
        ),
        "object has no audio",
    )?;
    let clip = layer.clip(frames);
    let interval = layer.interval(frames);
    let local = |frame: u32| {
        ensure(
            frame < frames
                && f64::from(frame) >= interval.in_frame
                && f64::from(frame) < interval.out_frame,
            "audio marker outside visible clip",
        )?;
        clip.edit_frame(frame)
    };
    let mut markers = layer.audio_markers.clone();
    let mut next = markers.iter().map(|m| m.id).max().unwrap_or(0);
    let mut new_marker = |frame, label, generated| -> Result<AudioMarker> {
        next = next
            .checked_add(1)
            .ok_or_else(|| crate::Error::Invalid("audio marker ID space exhausted".into()))?;
        Ok(AudioMarker {
            id: next,
            frame: local(frame)?,
            label,
            generated,
        })
    };
    match action {
        AudioMarkersAction::ReplaceGenerated { mut frames } => {
            ensure(
                frames.len() <= MAX_AUDIO_MARKERS,
                "too many detected markers",
            )?;
            frames.sort_unstable();
            frames.dedup();
            markers.retain(|m| !m.generated);
            for frame in frames {
                markers.push(new_marker(frame, String::new(), true)?);
            }
        }
        AudioMarkersAction::Add { frame, label } => markers.push(new_marker(frame, label, false)?),
        AudioMarkersAction::Move { id, frame } => {
            let marker = markers
                .iter_mut()
                .find(|m| m.id == id)
                .ok_or_else(|| crate::Error::Invalid("audio marker does not exist".into()))?;
            marker.frame = local(frame)?;
            // A corrected reference becomes manual and survives subsequent analysis.
            marker.generated = false;
        }
        AudioMarkersAction::Remove { id } => {
            ensure(
                markers.iter().any(|m| m.id == id),
                "audio marker does not exist",
            )?;
            markers.retain(|m| m.id != id);
        }
        AudioMarkersAction::ClearGenerated => markers.retain(|m| !m.generated),
    }
    markers.sort_by_key(|m| (m.frame, m.id));
    let mut candidate = layer.clone();
    candidate.audio_markers = markers;
    validate(&candidate)?;
    layer.audio_markers = candidate.audio_markers;
    Ok(())
}
