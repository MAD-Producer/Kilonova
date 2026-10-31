use super::{LayerClock, SourceTimeSample, TimeDirection, TimeRemap, TimeStretch};
use crate::{ClipRange, Content, Error, Layer, Result, MAX_FRAMES};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerTimeMapping {
    pub clock: LayerClock,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remap: Option<TimeRemap>,
}

fn deserialize_remap<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<TimeRemap>, D::Error> {
    Option::<TimeRemap>::deserialize(d)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LayerTimeAction {
    /// Absolute duration scale. Keep the current local time at anchor_frame;
    /// optionally scale the visible clip about that same composition instant.
    Stretch {
        stretch: TimeStretch,
        anchor_frame: f64,
        resize_clip: bool,
    },
    /// Reverse the continuous half-open clip about its center, retaining trim.
    Reverse {},
    Remap {
        #[serde(deserialize_with = "deserialize_remap")]
        remap: Option<TimeRemap>,
    },
    /// Clear the mapping; the current explicit clip interval stays authored.
    Reset {},
}

fn failure(object: u64, phase: &'static str, source: Error) -> Error {
    Error::LayerTimeObject {
        object,
        phase,
        source: Box::new(source),
    }
}

impl Layer {
    pub fn checked_local_frame(&self, composition_frame: f64) -> Result<f64> {
        self.time_mapping
            .as_ref()
            .map_or_else(LayerClock::default, |m| m.clock)
            .local_frame(
                composition_frame,
                self.timeline.map_or(0, |t| t.offset_frame),
            )
            .map_err(|source| failure(self.id, "property_sample", source))
    }
    pub fn source_time_sample(&self, composition_frame: f64) -> Result<SourceTimeSample> {
        let input = SourceTimeSample::new(composition_frame, 1.)
            .map_err(|source| failure(self.id, "source_input", source))?;
        let offset = self.timeline.map_or(0, |t| t.offset_frame);
        let result = match &self.time_mapping {
            Some(mapping) => input.through(mapping.clock, offset, mapping.remap.as_ref()),
            None => input.through(LayerClock::default(), offset, None),
        };
        result.map_err(|source| failure(self.id, "source_sample", source))
    }
    pub fn edit_frame(&self, frame: u32) -> Result<i32> {
        let offset = self.timeline.map_or(0, |t| t.offset_frame);
        self.time_mapping
            .as_ref()
            .map_or_else(LayerClock::default, |m| m.clock)
            .integer_key_frame(f64::from(frame), offset)
            .map_err(|source| failure(self.id, "integer_edit", source))
    }
    pub fn property_composition_frame(&self, local_frame: f64) -> Result<f64> {
        let offset = self.timeline.map_or(0, |t| t.offset_frame);
        if self.time_mapping.is_none() {
            // Old integer local keys may be outside the composition; retain
            // their full signed transfer coordinate instead of narrowing it.
            return if local_frame.is_finite() {
                Ok(local_frame + f64::from(offset))
            } else {
                Err(failure(
                    self.id,
                    "key_projection",
                    Error::Invalid("non-finite local key time".into()),
                ))
            };
        }
        self.time_mapping
            .as_ref()
            .map_or_else(LayerClock::default, |m| m.clock)
            .composition_frame(local_frame, offset)
            .map_err(|source| failure(self.id, "key_projection", source))
    }
    pub fn validate_time_mapping(&self, frames: u32) -> Result<()> {
        let Some(mapping) = &self.time_mapping else {
            return Ok(());
        };
        let result = (|| {
            if mapping.remap.is_some()
                && !matches!(
                    self.content,
                    Content::Video { .. }
                        | Content::Audio { .. }
                        | Content::Composition { .. }
                        | Content::Model { .. }
                )
            {
                return Err(Error::Invalid(
                    "source time remapping requires video, audio, composition or model content"
                        .into(),
                ));
            }
            let offset = self.timeline.map_or(0, |t| t.offset_frame);
            // The affine property map's extrema are its endpoints. Remap
            // curves have their own runtime error boundary and are not assumed
            // monotonic or bounded by just the source endpoint values.
            mapping.clock.local_frame(0., offset)?;
            mapping.clock.local_frame(f64::from(frames), offset)?;
            Ok(())
        })();
        result.map_err(|source| failure(self.id, "validate", source))
    }
}

/// Prepare a complete candidate first. A failed resize/type/bounds check must
/// not leave changed clocks, intervals or key metadata, even outside Engine.
pub fn apply(layer: &mut Layer, frames: u32, action: LayerTimeAction) -> Result<()> {
    let id = layer.id;
    let result = (|| {
        if layer.locked {
            return Err(Error::Locked(id));
        }
        let mut candidate = layer.clone();
        if matches!(action, LayerTimeAction::Reset {}) {
            candidate.time_mapping = None;
        } else {
            let mut mapping = candidate.time_mapping.clone().unwrap_or_default();
            let offset = candidate.timeline.map_or(0, |t| t.offset_frame);
            match action {
                LayerTimeAction::Stretch {
                    stretch,
                    anchor_frame,
                    resize_clip,
                } => {
                    if !anchor_frame.is_finite() || anchor_frame.abs() > f64::from(MAX_FRAMES) {
                        return Err(Error::Invalid(
                            "time stretch anchor is outside the composition time budget".into(),
                        ));
                    }
                    let old = mapping.clock;
                    if stretch != old.stretch() {
                        mapping.clock =
                            old.with_rate_at(stretch, old.direction(), anchor_frame, offset)?;
                        if resize_clip {
                            let factor = stretch.duration_scale() / old.stretch().duration_scale();
                            let range = candidate.interval(frames);
                            candidate.set_interval(
                                frames,
                                ClipRange {
                                    in_frame: anchor_frame
                                        + (range.in_frame - anchor_frame) * factor,
                                    out_frame: anchor_frame
                                        + (range.out_frame - anchor_frame) * factor,
                                },
                            )?;
                        }
                    }
                }
                LayerTimeAction::Reverse {} => {
                    let interval = candidate.interval(frames);
                    let center = interval.in_frame + (interval.out_frame - interval.in_frame) / 2.;
                    let direction = match mapping.clock.direction() {
                        TimeDirection::Forward => TimeDirection::Reverse,
                        TimeDirection::Reverse => TimeDirection::Forward,
                    };
                    mapping.clock = mapping.clock.with_rate_at(
                        mapping.clock.stretch(),
                        direction,
                        center,
                        offset,
                    )?;
                }
                LayerTimeAction::Remap { remap } => mapping.remap = remap,
                LayerTimeAction::Reset {} => unreachable!(),
            }
            candidate.time_mapping = if mapping == LayerTimeMapping::default() {
                None
            } else {
                Some(mapping)
            };
        }
        candidate.validate_time_mapping(frames)?;
        *layer = candidate;
        Ok(())
    })();
    result.map_err(|source| failure(id, "edit", source))
}
