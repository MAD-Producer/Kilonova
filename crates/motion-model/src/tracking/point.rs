//! Independent point displacement in the target's local pixel axes.
//! Ordinary position tracks remain editable corrections. No keys are baked.
use super::{
    asset::{Kind, Observation, Rate, Visibility},
    plate::{Extent, PlateTransform, Rotation},
    time::{Location, Mapping, Speed, Time, Timeline},
};
use crate::{ensure, Error, Layer, Project, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Interpolation {
    Hold,
    Linear,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Missing {
    Error,
    AuthoredOnly,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointBinding {
    pub asset: u64,
    pub track: String,
    /// Anchor on the target's local composition-frame clock, including clip offsets.
    pub anchor_local_frame: i32,
    pub source_anchor_pts: i64,
    pub speed_numerator: i32,
    pub speed_denominator: u32,
    /// Explicit source-pixel reference; its displacement is added to authored position.
    pub reference_pixels: [f64; 2],
    pub crop: [f64; 4],
    pub rotation: Rotation,
    pub interpolation: Interpolation,
    /// Maximum original-PTS span for hold/interpolation. Zero allows exact samples only.
    pub max_gap_ticks: u64,
    pub minimum_confidence: f64,
    pub missing: Missing,
}
impl PointBinding {
    pub fn validate(&self, project: &Project) -> Result<()> {
        ensure(
            self.asset != 0 && !self.track.is_empty() && self.track.len() <= 128,
            "invalid tracking point identity",
        )?;
        ensure(
            self.anchor_local_frame.unsigned_abs() <= crate::MAX_FRAMES,
            "tracking point anchor frame out of range",
        )?;
        ensure(
            self.reference_pixels
                .into_iter()
                .all(|x| x.is_finite() && x.abs() <= 1e7),
            "tracking reference pixels out of range",
        )?;
        ensure(
            self.minimum_confidence.is_finite() && (0. ..=1.).contains(&self.minimum_confidence),
            "tracking confidence threshold out of range",
        )?;
        Speed::new(self.speed_numerator, self.speed_denominator)
            .map_err(|cause| Error::Invalid(cause.to_string()))?;
        let asset = project
            .tracking_assets
            .iter()
            .find(|a| a.id == self.asset)
            .ok_or_else(|| Error::Invalid("tracking point asset missing".into()))?;
        asset.validate_source_digest()?;
        let track = asset
            .data
            .tracks
            .iter()
            .find(|t| t.id == self.track)
            .ok_or_else(|| Error::Invalid("tracking point track missing".into()))?;
        ensure(
            track.kind == Kind::Point,
            "point binding requires a point track",
        )?;
        self.plate(asset.data.source_extent, [project.width, project.height])?;
        Ok(())
    }
    fn plate(&self, source: Extent, output: [u32; 2]) -> Result<PlateTransform> {
        let output =
            Extent::new(output[0], output[1]).map_err(|cause| Error::Invalid(cause.to_string()))?;
        PlateTransform::new(source, self.crop, self.rotation, output)
            .map_err(|cause| Error::Invalid(cause.to_string()))
    }
    /// No IO, mutation, logging, allocation on healthy lookup, or full-document scan.
    pub fn displacement(&self, project: &Project, local_frame: f64) -> Result<[f32; 2]> {
        let data = &project
            .tracking_assets
            .iter()
            .find(|a| a.id == self.asset)
            .ok_or_else(|| Error::Invalid("tracking point asset missing".into()))?
            .data;
        let track = data
            .tracks
            .iter()
            .find(|t| t.id == self.track)
            .ok_or_else(|| Error::Invalid("tracking point track missing".into()))?;
        let clock = Rate::new(project.fps, 1).map_err(|cause| Error::Invalid(cause.to_string()))?;
        let mapping = Mapping::new(
            Time::frame(i64::from(self.anchor_local_frame), clock),
            self.source_anchor_pts,
            Speed::new(self.speed_numerator, self.speed_denominator)
                .map_err(|cause| Error::Invalid(cause.to_string()))?,
        );
        let time = Time::fractional_frame(local_frame, clock)
            .map_err(|cause| Error::Invalid(cause.to_string()))?;
        let location = Timeline::from_shared(data, mapping)
            .locate(time)
            .map_err(|cause| Error::Invalid(cause.to_string()))?;
        let point = |index: u32| -> Result<[f64; 2]> {
            let found = track
                .samples
                .binary_search_by_key(&index, |s| s.source_index)
                .ok()
                .map(|i| &track.samples[i]);
            let sample = found.ok_or_else(|| {
                Error::Invalid(format!("point observation absent source_index={index}"))
            })?;
            if sample.visibility != Visibility::Visible {
                return Err(Error::Invalid(format!(
                    "point not visible source_index={index} state={:?}",
                    sample.visibility
                )));
            }
            if sample.confidence < self.minimum_confidence {
                return Err(Error::Invalid(format!(
                    "point below confidence threshold source_index={index} confidence={}",
                    sample.confidence
                )));
            }
            match sample.observation {
                Some(Observation::Point(point)) => Ok(point),
                _ => Err(Error::Invalid(format!(
                    "point payload missing source_index={index}"
                ))),
            }
        };
        let selected = (|| match location {
            Location::At(index) => point(index),
            Location::Between {
                before,
                after,
                numerator,
                denominator,
            } => {
                let span = i128::from(data.source_pts[after as usize])
                    - i128::from(data.source_pts[before as usize]);
                ensure(
                    span <= i128::from(self.max_gap_ticks),
                    "tracking interpolation gap exceeds policy",
                )?;
                let a = point(before)?;
                if self.interpolation == Interpolation::Hold {
                    return Ok(a);
                }
                let b = point(after)?;
                let weight = numerator as f64 / denominator as f64;
                Ok([a[0] + (b[0] - a[0]) * weight, a[1] + (b[1] - a[1]) * weight])
            }
            _ => Err(Error::Invalid(format!(
                "tracking sample outside source PTS range location={location:?}"
            ))),
        })();
        let point = match selected {
            Ok(point) => point,
            Err(_) if self.missing == Missing::AuthoredOnly => return Ok([0.; 2]),
            Err(cause) => return Err(cause),
        };
        let plate = self.plate(data.source_extent, [project.width, project.height])?;
        let mapped = plate
            .apply(point)
            .map_err(|cause| Error::Invalid(cause.to_string()))?;
        let reference = plate
            .apply(self.reference_pixels)
            .map_err(|cause| Error::Invalid(cause.to_string()))?;
        let delta = [
            (mapped[0] - reference[0]) as f32,
            (mapped[1] - reference[1]) as f32,
        ];
        ensure(
            delta.iter().all(|v| v.is_finite() && v.abs() <= 1e7),
            "tracking displacement out of range",
        )?;
        Ok(delta)
    }
}
impl Layer {
    pub fn tracking_displacement(&self, project: &Project, frame: f64) -> Result<[f32; 2]> {
        let Some(binding) = &self.tracking_point else {
            return Ok([0.; 2]);
        };
        binding
            .displacement(project, self.local_frame(frame))
            .map_err(|source| Error::TrackingPoint {
                object: self.id,
                asset: binding.asset,
                track: binding.track.clone(),
                frame,
                source: Box::new(source),
            })
    }
    pub fn sampled_position(&self, project: &Project, frame: f64) -> Result<[f32; 3]> {
        let mut result = self.transform.position.sample(self.local_frame(frame))?;
        let delta = self.tracking_displacement(project, frame)?;
        result[0] += delta[0];
        result[1] += delta[1];
        ensure(
            result.iter().all(|v| v.is_finite() && v.abs() <= 1e7),
            "tracked position out of range",
        )?;
        Ok(result)
    }
}
