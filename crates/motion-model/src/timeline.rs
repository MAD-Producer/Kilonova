use crate::{Curve, Ease, Layer, Project, Result, Track, Tween};
use serde::Serialize;

fn serialize_frame<S: serde::Serializer>(frame: &f64, serializer: S) -> std::result::Result<S::Ok, S::Error> {
    if !frame.is_finite() {
        return Err(serde::ser::Error::custom("non-finite projected key time"));
    }
    if frame.fract() == 0. && (-9223372036854775808. ..9223372036854775808.).contains(frame) {
        serializer.serialize_i64(*frame as i64)
    } else {
        serializer.serialize_f64(*frame)
    }
}

/// Derived edit data, built only for state responses, never for frame sampling.
#[derive(Debug, Serialize)]
pub struct TimelineKey<T> {
    #[serde(serialize_with = "serialize_frame")]
    pub frame: f64,
    pub local_frame: i32,
    pub value: T,
    pub ease: Ease,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub curve: Option<Curve>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spatial: Option<crate::SpatialTangents<T>>,
}
#[derive(Debug, Serialize)]
pub struct TimelineTrack<T> {
    pub value: T,
    pub keys: Vec<TimelineKey<T>>,
    pub separated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub axes: Option<Box<TimelineAxes>>,
    /// Continuous-time keys have stable identities; they are not rounded into
    /// the ordinary frame-key protocol. Absent for all existing tracks.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub spatial_keys: Vec<TimelineSpatialKey<T>>,
}
#[derive(Debug, Serialize)]
pub struct TimelineSpatialKey<T> {
    pub id: u64,
    pub frame: f64,
    pub local_frame: f64,
    pub roving: bool,
    /// Cached incoming arc length in stored property units; zero for the first
    /// key. A zero-time span is valid only if every enclosed length is zero.
    pub incoming_length: f64,
    pub arc: bool,
    pub easing: crate::Easing,
    pub value: T,
    pub spatial: Option<crate::SpatialTangents<T>>,
}
#[derive(Debug, Serialize)]
pub struct TimelineAxes {
    pub x: TimelineTrack<f32>,
    pub y: TimelineTrack<f32>,
    pub z: TimelineTrack<f32>,
}
impl<T: Tween> Track<T> {
    pub fn timeline(&self, offset: i32) -> Result<TimelineTrack<T>> {
        self.timeline_projected(&|local| Ok(local + f64::from(offset)))
    }
    /// Keys keep their local order and interval metadata even under reverse.
    /// Composition coordinates can be fractional or outside the visible range.
    pub fn timeline_for_layer(&self, layer: &Layer) -> Result<TimelineTrack<T>> {
        self.timeline_projected(&|local| layer.property_composition_frame(local))
    }
    fn timeline_projected(&self, project: &impl Fn(f64) -> Result<f64>) -> Result<TimelineTrack<T>> {
        Ok(TimelineTrack {
            value: self.static_value()?,
            spatial_keys: if let Some(animation) = &self.spatial_animation {
                animation
                    .keys()
                    .iter()
                    .zip(animation.prepared().keys())
                    .enumerate()
                    .map(|(index, (key, (_, time)))| {
                        Ok(TimelineSpatialKey {
                            id: key.id,
                            frame: project(time)?,
                            local_frame: time,
                            roving: key.fixed.is_none(),
                            incoming_length: if index == 0 {
                                0.
                            } else {
                                animation.prepared().interval_length(index - 1)?
                            },
                            arc: key.arc,
                            easing: key.easing,
                            value: key.value,
                            spatial: key.spatial.clone(),
                        })
                    })
                    .collect::<Result<_>>()?
            } else {
                Vec::new()
            },
            separated: self.axes.is_some(),
            axes: self
                .axes
                .as_ref()
                .map(|a| {
                    Ok::<_, crate::Error>(Box::new(TimelineAxes {
                        x: a.x.timeline_projected(project)?,
                        y: a.y.timeline_projected(project)?,
                        z: a.z.timeline_projected(project)?,
                    }))
                })
                .transpose()?,
            keys: self
                .keys
                .iter()
                .map(|k| Ok(TimelineKey {
                    frame: project(f64::from(k.frame))?,
                    local_frame: k.frame,
                    value: k.value,
                    ease: k.ease,
                    curve: k.curve,
                    spatial: k.spatial.clone(),
                }))
                .collect::<Result<_>>()?,
        })
    }
}
#[derive(Debug, Serialize)]
pub struct TimelineProperties {
    pub anchor: TimelineTrack<[f32; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<TimelineTrack<[f32; 3]>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roll: Option<TimelineTrack<f32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fov: Option<TimelineTrack<f32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radius: Option<TimelineTrack<f32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub azimuth: Option<TimelineTrack<f32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elevation: Option<TimelineTrack<f32>>,
    pub position: TimelineTrack<[f32; 3]>,
    pub rotation: TimelineTrack<[f32; 3]>,
    pub scale: TimelineTrack<[f32; 3]>,
    pub opacity: TimelineTrack<f32>,
}
#[derive(Debug, Serialize)]
pub struct TimelineLayer {
    pub object: u64,
    pub in_frame: i32,
    pub out_frame: i32,
    pub offset_frame: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subframe_range: Option<crate::ClipRange>,
    pub active: bool,
    pub three_d: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<TimelineAudio>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<TimelineVideo>,
    pub properties: TimelineProperties,
    pub masks: Vec<TimelineMask>,
    pub blend: crate::compositing::LayerBlend,
    pub track_matte: Option<crate::compositing::TrackMatte>,
}
#[derive(Debug, Serialize)]
pub struct TimelineMask {
    pub id: u64,
    pub opacity: TimelineTrack<f32>,
    pub feather: TimelineTrack<[f32; 2]>,
    pub expansion: TimelineTrack<f32>,
    pub nodes: Vec<TimelineMaskNode>,
}
#[derive(Debug, Serialize)]
pub struct TimelineMaskNode {
    pub id: u64,
    pub geometry: TimelineTrack<[f32; 6]>,
}
#[derive(Debug, Serialize)]
pub struct TimelineAudio {
    pub asset: u64,
    pub source_offset_us: u64,
    pub source_time_us: i64,
    pub source_duration_us: u64,
    pub volume: f32,
    pub muted: bool,
    pub spatial_properties: bool,
}
#[derive(Debug, Serialize)]
pub struct TimelineVideo {
    pub asset: u64,
    pub source_time_us: i64,
    pub source_offset_us: u64,
    pub duration_us: u64,
    pub display_width: u32,
    pub display_height: u32,
    pub has_audio: bool,
}
impl Project {
    pub fn timeline_layers(&self, frame: f64) -> Result<Vec<TimelineLayer>> {
        if !frame.is_finite() {
            return Err(crate::Error::Invalid(format!(
                "timeline query requires finite time: frame={frame}"
            )));
        }
        self.layers
            .iter()
            .map(|l| {
                let result = (|| {
                let clip = l.clip(self.frames);
                let t = &l.transform;
                Ok(TimelineLayer {
                    object: l.id,
                    in_frame: clip.in_frame,
                    out_frame: clip.out_frame,
                    offset_frame: clip.offset_frame,
                    subframe_range:l.subframe_range,
                    active: if matches!(l.content, crate::Content::Audio { .. }) {
                        frame >= 0. && frame < f64::from(self.frames)
                            && frame >= l.interval(self.frames).in_frame && frame < l.interval(self.frames).out_frame
                    } else {
                        l.active(frame, self.frames)
                    },
                    three_d: l.three_d,
                    audio: if let Some(audio) = self.layer_audio(l) {
                        Some(TimelineAudio {
                            asset: audio.asset,
                            source_offset_us: audio.source_offset_us,
                            source_time_us: audio.source_offset_us as i64
                                + ((frame - f64::from(clip.offset_frame)) * 1_000_000.0
                                    / f64::from(self.fps))
                                .round() as i64,
                            source_duration_us: self
                                .audio_assets
                                .iter()
                                .find(|a| a.id == audio.asset)
                                .ok_or_else(|| crate::Error::Invalid(format!(
                                    "timeline audio asset missing: object={} asset={} frame={frame}",
                                    l.id, audio.asset,
                                )))?.duration_us,
                            volume: audio.volume,
                            muted: audio.muted,
                            spatial_properties: matches!(l.content, crate::Content::Video { .. }),
                        })
                    } else {
                        None
                    },
                    video: if let crate::Content::Video { video } = &l.content {
                        let a = self
                            .video_assets
                            .iter()
                            .find(|a| a.id == video.asset)
                            .ok_or_else(|| crate::Error::Invalid(format!(
                                "timeline video asset missing: object={} asset={} frame={frame}",
                                l.id, video.asset,
                            )))?;
                        Some(TimelineVideo {
                            asset: a.id,
                            source_time_us: video.source_time_us(l.local_frame(frame), self.fps),
                            source_offset_us: video.source_offset_us,
                            duration_us: a.duration_us,
                            display_width: a.display_width,
                            display_height: a.display_height,
                            has_audio: a.audio_asset.is_some(),
                        })
                    } else {
                        None
                    },
                    properties: TimelineProperties {
                        anchor: t.anchor.timeline_for_layer(l)?,
                        position: l.camera_data().map_or(&t.position, |c| &c.position).timeline_for_layer(l)?,
                        target: l.camera_data().map(|c| c.target.timeline_for_layer(l)).transpose()?,
                        roll: l.camera_data().map(|c| c.roll.timeline_for_layer(l)).transpose()?,
                        fov: l.camera_data().map(|c| c.fov.timeline_for_layer(l)).transpose()?,
                        radius: l.camera_data().map(|c| c.radius.timeline_for_layer(l)).transpose()?,
                        azimuth: l.camera_data().map(|c| c.azimuth.timeline_for_layer(l)).transpose()?,
                        elevation: l.camera_data().map(|c| c.elevation.timeline_for_layer(l)).transpose()?,
                        rotation: t.rotation.timeline_for_layer(l)?,
                        scale: t.scale.timeline_for_layer(l)?,
                        opacity: t.opacity.timeline_for_layer(l)?,
                    },
                    masks: l.masks.iter().map(|m| Ok(TimelineMask {
                        id: m.id, opacity: m.opacity.timeline_for_layer(l)?,
                        feather: m.feather.timeline_for_layer(l)?, expansion: m.expansion.timeline_for_layer(l)?,
                        nodes: m.path.nodes.iter().map(|n| Ok(TimelineMaskNode { id:n.id, geometry:n.geometry.timeline_for_layer(l)? })).collect::<Result<_>>()?,
                    })).collect::<Result<_>>()?,
                    blend:l.blend,track_matte:l.track_matte,
                })
                })();
                result.map_err(|source| crate::Error::Timeline {
                    object: l.id,
                    source: Box::new(source),
                })
            })
            .collect()
    }
}
