//! Typed external-asset contract. Byte decoding and project mutation are separate
//! boundaries; validation here does not bound a decoder's earlier allocations.
use super::plate::{Calibration, Extent};
use std::{collections::HashSet, fmt};

pub const VERSION: u32 = 1;
pub const MAX_SOURCE_TIMESTAMPS: usize = 1_000_000;
pub const MAX_TRACKS: usize = 4096;
pub const MAX_OBSERVATIONS: usize = 1_000_000;
const MAX_ID_BYTES: usize = 128;
const MAX_TOTAL_ID_BYTES: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    Version,
    Timebase,
    Rate,
    Count,
    Identifier,
    DuplicateIdentifier,
    TimestampOrder,
    SampleOrder,
    SourceIndex,
    Confidence,
    MissingObservation,
    ObservationKind,
    NonFinite,
    PlaneCondition,
    PlaneReference,
    Quaternion,
    CalibrationExtent,
    WorldUnits,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Error {
    pub cause: Cause,
    pub track: Option<usize>,
    pub sample: Option<usize>,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "tracking asset cause={:?} track={:?} sample={:?}",
            self.cause, self.track, self.sample
        )
    }
}
impl std::error::Error for Error {}
fn error(cause: Cause) -> Error {
    Error {
        cause,
        track: None,
        sample: None,
    }
}

/// Nominal source-rate metadata only, not the project's fps representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rate {
    numerator: u32,
    denominator: u32,
}
impl Rate {
    pub fn new(numerator: u32, denominator: u32) -> Result<Self, Error> {
        if numerator == 0 || denominator == 0 || numerator > 1_000_000 || denominator > 1_000_000 {
            return Err(error(Cause::Rate));
        }
        let (mut a, mut b) = (numerator, denominator);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        Ok(Self {
            numerator: numerator / a,
            denominator: denominator / a,
        })
    }
    pub fn ratio(self) -> [u32; 2] {
        [self.numerator, self.denominator]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Point,
    Plane,
    Camera,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Visible,
    Occluded,
    Lost,
}
/// Fixed explicit canonical basis; decoding must reject other bases unless an
/// explicit conversion adapter is provided. No missing-field default is implied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coordinates {
    RightDownForward,
}

/// Camera-to-world transform, quaternion XYZW; translation uses the asset's
/// declared world unit. No automatic normalization or keyframe baking occurs.
#[derive(Clone, Copy, Debug)]
pub struct Pose {
    pub position: [f64; 3],
    pub orientation_xyzw: [f64; 4],
}
#[derive(Clone, Copy, Debug)]
pub enum Observation {
    Point([f64; 2]),
    /// Reference-image pixels to current source-image pixels, homogeneous 3x3.
    /// Track.reference_index identifies the reference image in the same source.
    /// Scale-equivalent matrices retain their bytes.
    Plane([[f64; 3]; 3]),
    /// Per-observation calibration permits animated focal/principal/lens data.
    Camera {
        camera_to_world: Pose,
        calibration: Calibration,
    },
}
#[derive(Clone, Debug)]
pub struct Sample {
    pub source_index: u32,
    pub visibility: Visibility,
    pub confidence: f64,
    pub observation: Option<Observation>,
    pub manual_correction: bool,
}
#[derive(Clone, Debug)]
pub struct Track {
    pub id: String,
    pub kind: Kind,
    pub reference_index: Option<u32>,
    pub samples: Vec<Sample>,
}
#[derive(Clone, Debug)]
pub struct Asset {
    pub version: u32,
    pub source_id: String,
    /// Content identity is supplied/verified by the eventual source connector.
    /// A path or the textual source_id alone is not content identity.
    pub source_sha256: [u8; 32],
    pub source_extent: Extent,
    pub coordinates: Coordinates,
    pub ticks_per_second: u32,
    pub nominal_rate: Rate,
    /// Exact signed PTS, including a nonzero/negative source origin and VFR.
    pub source_pts: Vec<i64>,
    pub meters_per_world_unit: Option<f64>,
    pub tracks: Vec<Track>,
}
impl Asset {
    /// Validates without repairing, resampling, sorting or dropping any data.
    pub fn validate(&self) -> Result<(), Error> {
        if self.version != VERSION {
            return Err(error(Cause::Version));
        }
        if !(1..=1_000_000_000).contains(&self.ticks_per_second) {
            return Err(error(Cause::Timebase));
        }
        if self.source_pts.is_empty()
            || self.source_pts.len() > MAX_SOURCE_TIMESTAMPS
            || self.tracks.is_empty()
            || self.tracks.len() > MAX_TRACKS
        {
            return Err(error(Cause::Count));
        }
        if self.source_pts.windows(2).any(|p| p[0] >= p[1]) {
            return Err(error(Cause::TimestampOrder));
        }
        if !valid_id(&self.source_id) {
            return Err(error(Cause::Identifier));
        }
        if self
            .meters_per_world_unit
            .is_some_and(|v| !v.is_finite() || v <= 0.)
        {
            return Err(error(Cause::WorldUnits));
        }
        let mut ids = HashSet::new();
        let mut labels = self.source_id.len();
        let mut observations = 0usize;
        for (index, track) in self.tracks.iter().enumerate() {
            let check = || -> Result<(), Error> {
                if !valid_id(&track.id) {
                    return Err(error(Cause::Identifier));
                }
                if track.kind == Kind::Camera && self.meters_per_world_unit.is_none() {
                    return Err(error(Cause::WorldUnits));
                }
                match (track.kind, track.reference_index) {
                    (Kind::Plane, Some(i)) if (i as usize) < self.source_pts.len() => {}
                    (Kind::Plane, _) | (_, Some(_)) => return Err(error(Cause::PlaneReference)),
                    _ => {}
                }
                Ok(())
            };
            check().map_err(|mut e| {
                e.track = Some(index);
                e
            })?;
            if !ids.insert(track.id.as_str()) {
                return Err(Error {
                    track: Some(index),
                    ..error(Cause::DuplicateIdentifier)
                });
            }
            labels = labels
                .checked_add(track.id.len())
                .ok_or_else(|| error(Cause::Count))?;
            observations = observations
                .checked_add(track.samples.len())
                .ok_or_else(|| error(Cause::Count))?;
            if labels > MAX_TOTAL_ID_BYTES
                || observations > MAX_OBSERVATIONS
                || track.samples.is_empty()
            {
                return Err(Error {
                    track: Some(index),
                    ..error(Cause::Count)
                });
            }
            let mut previous = None;
            for (sample_index, sample) in track.samples.iter().enumerate() {
                let result = validate_sample(
                    sample,
                    track.kind,
                    self.source_extent,
                    self.source_pts.len(),
                    previous,
                );
                result.map_err(|mut e| {
                    e.track = Some(index);
                    e.sample = Some(sample_index);
                    e
                })?;
                previous = Some(sample.source_index);
            }
        }
        Ok(())
    }
    /// Exact source seconds as a fraction. No project frame rounding or assumed
    /// constant cadence: the caller must validate the complete asset first.
    pub fn source_time(&self, index: usize) -> Option<(i64, u32)> {
        (self.ticks_per_second != 0)
            .then(|| self.source_pts.get(index).copied())
            .flatten()
            .map(|pts| (pts, self.ticks_per_second))
    }
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_BYTES
        && !id.trim().is_empty()
        && !id.chars().any(char::is_control)
}
fn validate_sample(
    sample: &Sample,
    kind: Kind,
    extent: Extent,
    count: usize,
    previous: Option<u32>,
) -> Result<(), Error> {
    if sample.source_index as usize >= count {
        return Err(error(Cause::SourceIndex));
    }
    if previous.is_some_and(|p| p >= sample.source_index) {
        return Err(error(Cause::SampleOrder));
    }
    if !sample.confidence.is_finite() || !(0.0..=1.0).contains(&sample.confidence) {
        return Err(error(Cause::Confidence));
    }
    let Some(observation) = sample.observation else {
        return if sample.visibility == Visibility::Visible {
            Err(error(Cause::MissingObservation))
        } else {
            Ok(())
        };
    };
    match (kind, observation) {
        (Kind::Point, Observation::Point(p)) => {
            if p.iter().any(|v| !v.is_finite()) {
                return Err(error(Cause::NonFinite));
            }
        }
        (Kind::Plane, Observation::Plane(matrix)) => {
            if matrix.iter().flatten().any(|v| !v.is_finite()) {
                return Err(error(Cause::NonFinite));
            }
            let scale = matrix.iter().flatten().fold(0f64, |s, v| s.max(v.abs()));
            if scale == 0. {
                return Err(error(Cause::PlaneCondition));
            }
            let m = matrix.map(|row| row.map(|v| v / scale));
            let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
                - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
                + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
            if det.abs() <= 1e-12 {
                return Err(error(Cause::PlaneCondition));
            }
        }
        (
            Kind::Camera,
            Observation::Camera {
                camera_to_world: pose,
                calibration,
            },
        ) => {
            if calibration.source() != extent {
                return Err(error(Cause::CalibrationExtent));
            }
            if pose
                .position
                .iter()
                .chain(&pose.orientation_xyzw)
                .any(|v| !v.is_finite())
            {
                return Err(error(Cause::NonFinite));
            }
            let norm: f64 = pose.orientation_xyzw.iter().map(|v| v * v).sum();
            if !norm.is_finite() || (norm - 1.).abs() > 1e-6 {
                return Err(error(Cause::Quaternion));
            }
        }
        _ => return Err(error(Cause::ObservationKind)),
    }
    Ok(())
}

#[cfg(test)]
#[path = "asset_tests.rs"]
mod tests;
