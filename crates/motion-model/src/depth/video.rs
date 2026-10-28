//! Explicit calibration for indexed video samples; never interpolate relative AI
//! values or stretch a calibration across an unknown source-time gap.
use super::SavedDepthCalibration;
use crate::{ensure, Result, VideoAsset};
use serde::{Deserialize, Serialize};

pub const MAX_VIDEO_DEPTH_SAMPLES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SampleData {
    calibration: SavedDepthCalibration,
    end_us: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "SampleData")]
pub struct VideoDepthSample {
    calibration: SavedDepthCalibration,
    end_us: u64,
}
impl TryFrom<SampleData> for VideoDepthSample {
    type Error = crate::Error;
    fn try_from(data: SampleData) -> Result<Self> {
        Self::new(data.calibration, data.end_us)
    }
}
impl VideoDepthSample {
    pub fn new(calibration: SavedDepthCalibration, end_us: u64) -> Result<Self> {
        let sample = Self {
            calibration,
            end_us,
        };
        sample.validate()?;
        Ok(sample)
    }
    pub fn validate(&self) -> Result<()> {
        self.calibration.validate()?;
        let pts = self.calibration.source_time();
        ensure(
            pts.timescale == 1_000_000 && pts.ticks >= 0,
            "video depth samples require nonnegative indexed microsecond PTS",
        )?;
        ensure(
            self.end_us > pts.ticks as u64 && self.end_us <= 3_600_000_000,
            "video depth sample end must follow its PTS within one hour",
        )
    }
    pub fn start_us(&self) -> u64 {
        self.calibration.source_time().ticks as u64
    }
    pub fn end_us(&self) -> u64 {
        self.end_us
    }
    pub fn calibration(&self) -> &SavedDepthCalibration {
        &self.calibration
    }
}

/// Sorted nonoverlapping original-source intervals. Bounds apply to retained
/// metadata and deserialization, independently of the live depth-result cache.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(transparent)]
pub struct VideoDepthTrack(Vec<VideoDepthSample>);
impl<'de> Deserialize<'de> for VideoDepthTrack {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct Samples;
        impl<'de> serde::de::Visitor<'de> for Samples {
            type Value = VideoDepthTrack;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("at most 4096 ordered nonoverlapping video depth samples")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut samples = Vec::new();
                while let Some(sample) = seq.next_element()? {
                    if samples.len() == MAX_VIDEO_DEPTH_SAMPLES {
                        return Err(serde::de::Error::custom(
                            "video depth sample limit exceeded",
                        ));
                    }
                    samples.push(sample);
                }
                let track = VideoDepthTrack(samples);
                track.validate().map_err(serde::de::Error::custom)?;
                Ok(track)
            }
        }
        deserializer.deserialize_seq(Samples)
    }
}
impl VideoDepthTrack {
    pub fn samples(&self) -> &[VideoDepthSample] {
        &self.0
    }
    pub fn at(&self, source_time_us: u64) -> Option<&VideoDepthSample> {
        let index = self
            .0
            .partition_point(|sample| sample.start_us() <= source_time_us)
            .checked_sub(1)?;
        self.0
            .get(index)
            .filter(|sample| source_time_us < sample.end_us())
    }
    pub fn validate(&self) -> Result<()> {
        ensure(
            self.0.len() <= MAX_VIDEO_DEPTH_SAMPLES,
            "video depth sample limit exceeded",
        )?;
        for (index, sample) in self.0.iter().enumerate() {
            sample.validate()?;
            if let Some(first) = self.0.first() {
                ensure(
                    sample.calibration.asset() == first.calibration.asset()
                        && sample.calibration.source_size() == first.calibration.source_size(),
                    "video depth samples must refer to one original source geometry",
                )?;
            }
            if index != 0 {
                ensure(
                    self.0[index - 1].end_us <= sample.start_us(),
                    "video depth sample intervals must be ordered and nonoverlapping",
                )?;
            }
        }
        Ok(())
    }
    pub fn validate_asset(&self, asset: &VideoAsset) -> Result<()> {
        self.validate()?;
        ensure(
            !self.0.is_empty() && self.0.len() <= asset.frame_count as usize,
            "video depth track requires samples within the registered frame count",
        )?;
        for sample in &self.0 {
            ensure(
                sample.calibration.asset() == asset.id
                    && sample.calibration.source_size()
                        == [asset.display_width, asset.display_height]
                    && sample.start_us() >= asset.video_start_us
                    && sample.end_us <= asset.video_end_us,
                "video depth sample differs from registered source geometry or interval",
            )?;
        }
        Ok(())
    }
    /// Replace one exact indexed sample atomically; preserve all other source
    /// times. Overlaps, different source geometry and capacity refusal do not
    /// mutate the prior authored track.
    pub fn upsert(&mut self, sample: VideoDepthSample) -> Result<()> {
        sample.validate()?;
        let index = self.0.partition_point(|s| s.start_us() < sample.start_us());
        let replacing = self
            .0
            .get(index)
            .is_some_and(|s| s.start_us() == sample.start_us());
        ensure(
            replacing || self.0.len() < MAX_VIDEO_DEPTH_SAMPLES,
            "video depth sample limit exceeded",
        )?;
        if let Some(first) = self.0.first() {
            ensure(
                first.calibration.asset() == sample.calibration.asset()
                    && first.calibration.source_size() == sample.calibration.source_size(),
                "video depth samples must refer to one original source geometry",
            )?;
        }
        if index != 0 {
            ensure(
                self.0[index - 1].end_us <= sample.start_us(),
                "video depth sample overlaps previous interval",
            )?;
        }
        if let Some(next) = self.0.get(index + usize::from(replacing)) {
            ensure(
                sample.end_us <= next.start_us(),
                "video depth sample overlaps next interval",
            )?;
        }
        if replacing {
            self.0[index] = sample;
        } else {
            self.0.insert(index, sample);
        }
        Ok(())
    }
    pub fn remap_asset(&mut self, asset: u64) {
        for sample in &mut self.0 {
            sample.calibration.remap_asset(asset);
        }
    }
}
